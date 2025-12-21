//! Metrics and tracing instrumentation for diesel-clickhouse.
//!
//! This module provides integration with the `tracing` crate for
//! observability, performance monitoring, and debugging.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::metrics::{QueryMetrics, MetricsCollector};
//! use tracing_subscriber::fmt;
//!
//! // Initialize tracing subscriber
//! fmt::init();
//!
//! // Create a metrics collector
//! let metrics = MetricsCollector::new();
//!
//! // Queries are automatically instrumented
//! let result = users::table
//!     .filter(users::id.eq(42))
//!     .instrument(&metrics)
//!     .load(&mut conn)
//!     .await?;
//!
//! // Get statistics
//! println!("Total queries: {}", metrics.query_count());
//! println!("Avg latency: {:?}", metrics.avg_latency());
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Query execution metrics.
#[derive(Debug, Clone)]
pub struct QueryMetrics {
    /// SQL query (possibly truncated).
    pub sql: String,
    /// Execution duration.
    pub duration: Duration,
    /// Number of rows affected/returned.
    pub rows: u64,
    /// Whether the query succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Timestamp when the query started.
    pub started_at: Instant,
}

impl QueryMetrics {
    /// Create metrics for a successful query.
    pub fn success(sql: impl Into<String>, duration: Duration, rows: u64) -> Self {
        Self {
            sql: sql.into(),
            duration,
            rows,
            success: true,
            error: None,
            started_at: Instant::now() - duration,
        }
    }

    /// Create metrics for a failed query.
    pub fn failure(sql: impl Into<String>, duration: Duration, error: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            duration,
            rows: 0,
            success: false,
            error: Some(error.into()),
            started_at: Instant::now() - duration,
        }
    }

    /// Log this metric using tracing.
    #[cfg(feature = "tracing")]
    pub fn log(&self) {
        if self.success {
            tracing::info!(
                target: "diesel_clickhouse::query",
                sql = %truncate_sql(&self.sql, 200),
                duration_ms = %self.duration.as_millis(),
                rows = %self.rows,
                "Query executed"
            );
        } else {
            tracing::error!(
                target: "diesel_clickhouse::query",
                sql = %truncate_sql(&self.sql, 200),
                duration_ms = %self.duration.as_millis(),
                error = %self.error.as_deref().unwrap_or("unknown"),
                "Query failed"
            );
        }
    }

    #[cfg(not(feature = "tracing"))]
    pub fn log(&self) {
        // No-op when tracing is disabled
    }
}

/// Truncate SQL for logging (avoid huge queries in logs).
#[cfg(feature = "tracing")]
fn truncate_sql(sql: &str, max_len: usize) -> &str {
    if sql.len() <= max_len {
        sql
    } else {
        &sql[..max_len]
    }
}

/// A collector for query metrics.
///
/// Thread-safe and lock-free for high performance.
#[derive(Debug, Default)]
pub struct MetricsCollector {
    /// Total number of queries executed.
    query_count: AtomicU64,
    /// Total number of successful queries.
    success_count: AtomicU64,
    /// Total number of failed queries.
    error_count: AtomicU64,
    /// Total execution time in nanoseconds.
    total_duration_ns: AtomicU64,
    /// Total rows processed.
    total_rows: AtomicU64,
    /// Minimum latency in nanoseconds.
    min_latency_ns: AtomicU64,
    /// Maximum latency in nanoseconds.
    max_latency_ns: AtomicU64,
}

impl MetricsCollector {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        Self {
            query_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            total_duration_ns: AtomicU64::new(0),
            total_rows: AtomicU64::new(0),
            min_latency_ns: AtomicU64::new(u64::MAX),
            max_latency_ns: AtomicU64::new(0),
        }
    }

    /// Record a query execution.
    pub fn record(&self, metrics: &QueryMetrics) {
        self.query_count.fetch_add(1, Ordering::Relaxed);

        if metrics.success {
            self.success_count.fetch_add(1, Ordering::Relaxed);
            self.total_rows.fetch_add(metrics.rows, Ordering::Relaxed);
        } else {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        }

        let duration_ns = metrics.duration.as_nanos() as u64;
        self.total_duration_ns.fetch_add(duration_ns, Ordering::Relaxed);

        // Update min (using compare-exchange loop)
        let mut current = self.min_latency_ns.load(Ordering::Relaxed);
        while duration_ns < current {
            match self.min_latency_ns.compare_exchange_weak(
                current,
                duration_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }

        // Update max
        let mut current = self.max_latency_ns.load(Ordering::Relaxed);
        while duration_ns > current {
            match self.max_latency_ns.compare_exchange_weak(
                current,
                duration_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }

        // Log the query
        metrics.log();
    }

    /// Get total query count.
    pub fn query_count(&self) -> u64 {
        self.query_count.load(Ordering::Relaxed)
    }

    /// Get successful query count.
    pub fn success_count(&self) -> u64 {
        self.success_count.load(Ordering::Relaxed)
    }

    /// Get error count.
    pub fn error_count(&self) -> u64 {
        self.error_count.load(Ordering::Relaxed)
    }

    /// Get total rows processed.
    pub fn total_rows(&self) -> u64 {
        self.total_rows.load(Ordering::Relaxed)
    }

    /// Get average latency.
    pub fn avg_latency(&self) -> Duration {
        let count = self.query_count.load(Ordering::Relaxed);
        if count == 0 {
            return Duration::ZERO;
        }
        let total_ns = self.total_duration_ns.load(Ordering::Relaxed);
        Duration::from_nanos(total_ns / count)
    }

    /// Get minimum latency.
    pub fn min_latency(&self) -> Option<Duration> {
        let min_ns = self.min_latency_ns.load(Ordering::Relaxed);
        if min_ns == u64::MAX {
            None
        } else {
            Some(Duration::from_nanos(min_ns))
        }
    }

    /// Get maximum latency.
    pub fn max_latency(&self) -> Option<Duration> {
        let max_ns = self.max_latency_ns.load(Ordering::Relaxed);
        if max_ns == 0 {
            None
        } else {
            Some(Duration::from_nanos(max_ns))
        }
    }

    /// Get success rate (0.0 to 1.0).
    pub fn success_rate(&self) -> f64 {
        let total = self.query_count.load(Ordering::Relaxed);
        if total == 0 {
            return 1.0;
        }
        let success = self.success_count.load(Ordering::Relaxed);
        success as f64 / total as f64
    }

    /// Get a snapshot of all metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            query_count: self.query_count(),
            success_count: self.success_count(),
            error_count: self.error_count(),
            total_rows: self.total_rows(),
            avg_latency: self.avg_latency(),
            min_latency: self.min_latency(),
            max_latency: self.max_latency(),
            success_rate: self.success_rate(),
        }
    }

    /// Reset all metrics.
    pub fn reset(&self) {
        self.query_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
        self.total_duration_ns.store(0, Ordering::Relaxed);
        self.total_rows.store(0, Ordering::Relaxed);
        self.min_latency_ns.store(u64::MAX, Ordering::Relaxed);
        self.max_latency_ns.store(0, Ordering::Relaxed);
    }
}

/// A snapshot of metrics at a point in time.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub query_count: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub total_rows: u64,
    pub avg_latency: Duration,
    pub min_latency: Option<Duration>,
    pub max_latency: Option<Duration>,
    pub success_rate: f64,
}

impl std::fmt::Display for MetricsSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Queries: {} (success: {}, errors: {}), Rows: {}, Avg latency: {:?}, Success rate: {:.1}%",
            self.query_count,
            self.success_count,
            self.error_count,
            self.total_rows,
            self.avg_latency,
            self.success_rate * 100.0
        )
    }
}

/// A timer for measuring query duration.
///
/// Automatically records duration when dropped.
pub struct QueryTimer<'a> {
    collector: &'a MetricsCollector,
    sql: String,
    start: Instant,
    rows: u64,
    completed: bool,
}

impl<'a> QueryTimer<'a> {
    /// Start a new timer.
    pub fn start(collector: &'a MetricsCollector, sql: impl Into<String>) -> Self {
        Self {
            collector,
            sql: sql.into(),
            start: Instant::now(),
            rows: 0,
            completed: false,
        }
    }

    /// Set the number of rows (call before completing).
    pub fn set_rows(&mut self, rows: u64) {
        self.rows = rows;
    }

    /// Complete the timer with success.
    pub fn success(mut self, rows: u64) {
        self.rows = rows;
        self.completed = true;
        let duration = self.start.elapsed();
        let metrics = QueryMetrics::success(&self.sql, duration, rows);
        self.collector.record(&metrics);
    }

    /// Complete the timer with failure.
    pub fn failure(mut self, error: impl Into<String>) {
        self.completed = true;
        let duration = self.start.elapsed();
        let metrics = QueryMetrics::failure(&self.sql, duration, error);
        self.collector.record(&metrics);
    }
}

impl<'a> Drop for QueryTimer<'a> {
    fn drop(&mut self) {
        if !self.completed {
            // Timer dropped without explicit completion - record as success with current rows
            let duration = self.start.elapsed();
            let metrics = QueryMetrics::success(&self.sql, duration, self.rows);
            self.collector.record(&metrics);
        }
    }
}

/// Global metrics collector.
static GLOBAL_METRICS: std::sync::OnceLock<MetricsCollector> = std::sync::OnceLock::new();

/// Get or initialize the global metrics collector.
pub fn global_metrics() -> &'static MetricsCollector {
    GLOBAL_METRICS.get_or_init(MetricsCollector::new)
}

/// Record a query in the global metrics.
pub fn record_query(sql: &str, duration: Duration, rows: u64, success: bool, error: Option<&str>) {
    let metrics = if success {
        QueryMetrics::success(sql, duration, rows)
    } else {
        QueryMetrics::failure(sql, duration, error.unwrap_or("unknown"))
    };
    global_metrics().record(&metrics);
}

/// A wrapper that adds instrumentation to a connection.
pub struct InstrumentedConnection<'a, C> {
    inner: &'a C,
    collector: &'a MetricsCollector,
}

impl<'a, C> InstrumentedConnection<'a, C> {
    /// Create an instrumented wrapper around a connection.
    pub fn new(conn: &'a C, collector: &'a MetricsCollector) -> Self {
        Self {
            inner: conn,
            collector,
        }
    }

    /// Get the inner connection.
    pub fn inner(&self) -> &C {
        self.inner
    }

    /// Get the metrics collector.
    pub fn collector(&self) -> &MetricsCollector {
        self.collector
    }

    /// Start a timer for a query.
    pub fn timer(&self, sql: impl Into<String>) -> QueryTimer<'a> {
        QueryTimer::start(self.collector, sql)
    }
}

/// Extension trait for adding instrumentation.
pub trait InstrumentExt {
    /// Wrap with instrumentation.
    fn instrument<'a>(&'a self, collector: &'a MetricsCollector) -> InstrumentedConnection<'a, Self>
    where
        Self: Sized,
    {
        InstrumentedConnection::new(self, collector)
    }
}

// Implement for Connection
#[cfg(any(feature = "http", feature = "native"))]
impl InstrumentExt for crate::Connection {}

/// Tracing span helpers for structured logging.
#[cfg(feature = "tracing")]
pub mod spans {
    use tracing::{span, Level, Span};

    /// Create a span for a query operation.
    pub fn query_span(sql: &str) -> Span {
        span!(
            Level::INFO,
            "clickhouse.query",
            sql = %truncate(sql, 200),
            otel.kind = "client",
            db.system = "clickhouse"
        )
    }

    /// Create a span for a batch insert.
    pub fn insert_span(table: &str, row_count: usize) -> Span {
        span!(
            Level::INFO,
            "clickhouse.insert",
            table = %table,
            rows = %row_count,
            otel.kind = "client",
            db.system = "clickhouse"
        )
    }

    /// Create a span for a connection operation.
    pub fn connect_span(url: &str) -> Span {
        span!(
            Level::INFO,
            "clickhouse.connect",
            url = %truncate(url, 100),
            otel.kind = "client",
            db.system = "clickhouse"
        )
    }

    fn truncate(s: &str, max: usize) -> &str {
        if s.len() <= max {
            s
        } else {
            &s[..max]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        // Record a successful query
        let metrics = QueryMetrics::success("SELECT 1", Duration::from_millis(10), 1);
        collector.record(&metrics);

        assert_eq!(collector.query_count(), 1);
        assert_eq!(collector.success_count(), 1);
        assert_eq!(collector.error_count(), 0);
        assert_eq!(collector.total_rows(), 1);
    }

    #[test]
    fn test_metrics_error() {
        let collector = MetricsCollector::new();

        let metrics = QueryMetrics::failure("SELECT * FROM missing", Duration::from_millis(5), "table not found");
        collector.record(&metrics);

        assert_eq!(collector.query_count(), 1);
        assert_eq!(collector.success_count(), 0);
        assert_eq!(collector.error_count(), 1);
        assert!(collector.success_rate() < 0.01);
    }

    #[test]
    fn test_avg_latency() {
        let collector = MetricsCollector::new();

        collector.record(&QueryMetrics::success("SELECT 1", Duration::from_millis(10), 1));
        collector.record(&QueryMetrics::success("SELECT 2", Duration::from_millis(20), 1));
        collector.record(&QueryMetrics::success("SELECT 3", Duration::from_millis(30), 1));

        let avg = collector.avg_latency();
        assert!(avg >= Duration::from_millis(19) && avg <= Duration::from_millis(21));
    }

    #[test]
    fn test_min_max_latency() {
        let collector = MetricsCollector::new();

        collector.record(&QueryMetrics::success("q1", Duration::from_millis(50), 1));
        collector.record(&QueryMetrics::success("q2", Duration::from_millis(10), 1));
        collector.record(&QueryMetrics::success("q3", Duration::from_millis(100), 1));

        assert!(collector.min_latency().unwrap() <= Duration::from_millis(11));
        assert!(collector.max_latency().unwrap() >= Duration::from_millis(99));
    }

    #[test]
    fn test_query_timer() {
        let collector = MetricsCollector::new();

        {
            let timer = QueryTimer::start(&collector, "SELECT 1");
            std::thread::sleep(Duration::from_millis(5));
            timer.success(10);
        }

        assert_eq!(collector.query_count(), 1);
        assert_eq!(collector.total_rows(), 10);
        assert!(collector.avg_latency() >= Duration::from_millis(5));
    }

    #[test]
    fn test_metrics_snapshot() {
        let collector = MetricsCollector::new();

        collector.record(&QueryMetrics::success("q1", Duration::from_millis(10), 100));
        collector.record(&QueryMetrics::success("q2", Duration::from_millis(20), 200));

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.query_count, 2);
        assert_eq!(snapshot.total_rows, 300);
        assert_eq!(snapshot.success_rate, 1.0);
    }
}
