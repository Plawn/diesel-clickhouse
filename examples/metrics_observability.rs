//! Metrics and observability example for diesel-clickhouse.
//!
//! This example demonstrates how to collect and monitor query metrics
//! for performance observability.
//!
//! Run with: cargo run --example metrics_observability

use diesel_clickhouse::metrics::{MetricsCollector, QueryMetrics, QueryTimer, global_metrics};
use std::time::Duration;

fn main() {
    println!("=== Metrics & Observability Example ===\n");

    // -------------------------------------------------------------------------
    // 1. Creating a MetricsCollector
    // -------------------------------------------------------------------------
    println!("1. Creating a MetricsCollector:");

    let metrics = MetricsCollector::new();
    println!("   Created new MetricsCollector");
    println!("   Initial query count: {}", metrics.query_count());
    println!();

    // -------------------------------------------------------------------------
    // 2. Recording Query Metrics
    // -------------------------------------------------------------------------
    println!("2. Recording query metrics:");

    // Simulate successful queries
    metrics.record(&QueryMetrics::success(
        "SELECT * FROM users WHERE id = 42",
        Duration::from_millis(15),
        1
    ));
    metrics.record(&QueryMetrics::success(
        "SELECT count(*) FROM events",
        Duration::from_millis(23),
        1
    ));
    metrics.record(&QueryMetrics::success(
        "SELECT * FROM orders LIMIT 100",
        Duration::from_millis(8),
        100
    ));
    metrics.record(&QueryMetrics::success(
        "SELECT avg(value) FROM metrics",
        Duration::from_millis(45),
        1
    ));

    // Simulate a failed query
    metrics.record(&QueryMetrics::failure(
        "SELECT * FROM missing_table",
        Duration::from_millis(5),
        "Table 'missing_table' doesn't exist"
    ));

    println!("   Recorded 4 successful queries and 1 failure");
    println!();

    // -------------------------------------------------------------------------
    // 3. Reading Basic Metrics
    // -------------------------------------------------------------------------
    println!("3. Basic metrics:");

    println!("   Total queries: {}", metrics.query_count());
    println!("   Successful: {}", metrics.success_count());
    println!("   Failed: {}", metrics.error_count());
    println!("   Success rate: {:.1}%", metrics.success_rate() * 100.0);
    println!("   Total rows: {}", metrics.total_rows());
    println!();

    // -------------------------------------------------------------------------
    // 4. Latency Statistics
    // -------------------------------------------------------------------------
    println!("4. Latency statistics:");

    println!("   Average latency: {:?}", metrics.avg_latency());
    if let Some(min) = metrics.min_latency() {
        println!("   Min latency: {:?}", min);
    }
    if let Some(max) = metrics.max_latency() {
        println!("   Max latency: {:?}", max);
    }
    println!();

    // -------------------------------------------------------------------------
    // 5. Using QueryTimer
    // -------------------------------------------------------------------------
    println!("5. Using QueryTimer for automatic timing:");

    let metrics2 = MetricsCollector::new();

    // Method 1: Explicit success/failure
    {
        let timer = QueryTimer::start(&metrics2, "SELECT 1");
        std::thread::sleep(Duration::from_millis(10));
        timer.success(1);  // Records duration automatically
    }

    // Method 2: Explicit failure
    {
        let timer = QueryTimer::start(&metrics2, "SELECT * FROM bad_table");
        std::thread::sleep(Duration::from_millis(5));
        timer.failure("Connection refused");
    }

    println!("   After 2 timed queries:");
    println!("   - Total: {}", metrics2.query_count());
    println!("   - Successful: {}", metrics2.success_count());
    println!("   - Failed: {}", metrics2.error_count());
    println!();

    // -------------------------------------------------------------------------
    // 6. Metrics Snapshot
    // -------------------------------------------------------------------------
    println!("6. Getting a metrics snapshot:");

    let snapshot = metrics.snapshot();
    println!("   query_count: {}", snapshot.query_count);
    println!("   success_count: {}", snapshot.success_count);
    println!("   error_count: {}", snapshot.error_count);
    println!("   total_rows: {}", snapshot.total_rows);
    println!("   avg_latency: {:?}", snapshot.avg_latency);
    println!("   success_rate: {:.1}%", snapshot.success_rate * 100.0);
    println!();

    // Snapshot implements Display
    println!("   Display format: {}", snapshot);
    println!();

    // -------------------------------------------------------------------------
    // 7. Global Metrics
    // -------------------------------------------------------------------------
    println!("7. Using global metrics:");

    // Get the global metrics collector (lazy initialized)
    let global = global_metrics();

    // Record some metrics
    global.record(&QueryMetrics::success("global query 1", Duration::from_millis(10), 5));
    global.record(&QueryMetrics::success("global query 2", Duration::from_millis(20), 10));

    println!("   Global metrics:");
    println!("   - Total queries: {}", global.query_count());
    println!("   - Total rows: {}", global.total_rows());
    println!();

    // -------------------------------------------------------------------------
    // 8. Resetting Metrics
    // -------------------------------------------------------------------------
    println!("8. Resetting metrics:");

    let resettable = MetricsCollector::new();
    resettable.record(&QueryMetrics::success("test", Duration::from_millis(100), 50));

    println!("   Before reset: {} queries, {} rows",
        resettable.query_count(), resettable.total_rows());

    resettable.reset();

    println!("   After reset: {} queries, {} rows",
        resettable.query_count(), resettable.total_rows());
    println!();

    // -------------------------------------------------------------------------
    // 9. Simulating Production Workload
    // -------------------------------------------------------------------------
    println!("9. Simulating production workload:");

    let prod_metrics = MetricsCollector::new();

    // Simulate 1000 queries with varying latencies
    for i in 0..1000 {
        let latency = match i % 10 {
            0 => Duration::from_millis(100),  // Slow query
            1..=2 => Duration::from_millis(50),  // Medium
            _ => Duration::from_millis(10),   // Fast
        };

        let success = i % 50 != 0;  // 2% error rate

        if success {
            prod_metrics.record(&QueryMetrics::success(
                "simulated query",
                latency,
                (i % 100) as u64
            ));
        } else {
            prod_metrics.record(&QueryMetrics::failure(
                "simulated query",
                latency,
                "simulated error"
            ));
        }
    }

    let snap = prod_metrics.snapshot();
    println!("   Simulated 1000 queries:");
    println!("   - Success rate: {:.1}%", snap.success_rate * 100.0);
    println!("   - Avg latency: {:?}", snap.avg_latency);
    println!("   - Min latency: {:?}", prod_metrics.min_latency());
    println!("   - Max latency: {:?}", prod_metrics.max_latency());
    println!("   - Total rows: {}", snap.total_rows);
    println!();

    // -------------------------------------------------------------------------
    // 10. Alert Thresholds
    // -------------------------------------------------------------------------
    println!("10. Alert thresholds (example logic):");

    fn check_alerts(metrics: &MetricsCollector) {
        let snapshot = metrics.snapshot();

        // Success rate alert
        if snapshot.success_rate < 0.95 {
            println!("   [WARN] Success rate below 95%: {:.1}%",
                snapshot.success_rate * 100.0);
        } else {
            println!("   [OK] Success rate: {:.1}%", snapshot.success_rate * 100.0);
        }

        // Latency alert
        if snapshot.avg_latency > Duration::from_millis(50) {
            println!("   [WARN] Avg latency above 50ms: {:?}", snapshot.avg_latency);
        } else {
            println!("   [OK] Avg latency: {:?}", snapshot.avg_latency);
        }

        // Error rate alert
        let error_rate = if snapshot.query_count > 0 {
            snapshot.error_count as f64 / snapshot.query_count as f64
        } else {
            0.0
        };
        if error_rate > 0.01 {
            println!("   [WARN] Error rate above 1%: {:.2}%", error_rate * 100.0);
        } else {
            println!("   [OK] Error rate: {:.2}%", error_rate * 100.0);
        }
    }

    check_alerts(&prod_metrics);
    println!();

    // -------------------------------------------------------------------------
    // 11. Best Practices
    // -------------------------------------------------------------------------
    println!("11. Best practices:");
    println!();
    println!("   - Use global_metrics() for application-wide tracking");
    println!("   - Create separate collectors for different query types");
    println!("   - Export metrics to Prometheus/Grafana for visualization");
    println!("   - Set up alerts:");
    println!("     - Success rate < 95%");
    println!("     - P99 latency > threshold");
    println!("     - Error rate > 1%");
    println!("   - Reset metrics periodically or use rolling windows");
    println!("   - Log slow queries (e.g., > 100ms) for investigation");
    println!();

    println!("=== End of Metrics Example ===");
}
