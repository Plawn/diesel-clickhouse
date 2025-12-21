//! Prepared statement cache for query reuse.
//!
//! This module provides a cache for prepared SQL statements, avoiding
//! the overhead of rebuilding the same query string multiple times.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::prepared::{PreparedCache, PreparedQuery};
//!
//! // Create a cache
//! let cache = PreparedCache::new(100);
//!
//! // Prepare and cache a query
//! let query = cache.prepare("user_by_id", || {
//!     users::table.filter(users::id.eq(placeholder::<u64>()))
//! });
//!
//! // Execute with different parameters
//! let user1 = conn.execute_prepared(&query, &[&42u64]).await?;
//! let user2 = conn.execute_prepared(&query, &[&123u64]).await?;
//! ```

use std::any::TypeId;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use crate::core::backend::ClickHouse;
use crate::core::query_builder::QueryFragment;

/// A cache for prepared SQL statements.
///
/// The cache stores compiled SQL strings keyed by a unique identifier,
/// avoiding repeated query building for frequently used queries.
///
/// Uses an LRU (Least Recently Used) eviction strategy when the cache
/// reaches its maximum size.
#[derive(Debug)]
pub struct PreparedCache {
    cache: RwLock<HashMap<CacheKey, CacheEntry>>,
    max_size: usize,
    hits: std::sync::atomic::AtomicU64,
    misses: std::sync::atomic::AtomicU64,
}

/// An entry in the prepared cache, including LRU metadata.
#[derive(Debug, Clone)]
struct CacheEntry {
    statement: Arc<PreparedStatement>,
    last_accessed: Instant,
}

/// Key for cache lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    name: String,
    type_id: TypeId,
}

impl PreparedCache {
    /// Create a new prepared cache with the given maximum size.
    ///
    /// When the cache exceeds this size, least recently used entries
    /// may be evicted.
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::with_capacity(max_size)),
            max_size,
            hits: std::sync::atomic::AtomicU64::new(0),
            misses: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Create a cache with default size (256 entries).
    pub fn default_size() -> Self {
        Self::new(256)
    }

    /// Prepare a query, using the cache if available.
    ///
    /// If the query is already in the cache, returns the cached version.
    /// Otherwise, builds the query and stores it in the cache.
    ///
    /// # Arguments
    ///
    /// - `name`: A unique name for this query type
    /// - `build`: A closure that builds the query fragment
    ///
    /// # Panics
    ///
    /// Panics if the internal RwLock is poisoned.
    pub fn prepare<Q, F>(&self, name: &str, build: F) -> Arc<PreparedStatement>
    where
        Q: QueryFragment<ClickHouse> + 'static,
        F: FnOnce() -> Q,
    {
        let key = CacheKey {
            name: name.to_owned(),
            type_id: TypeId::of::<Q>(),
        };

        // Fast path: check read lock first, update access time if found
        {
            let mut cache = self.cache.write()
                .expect("PreparedCache RwLock poisoned during write");
            if let Some(entry) = cache.get_mut(&key) {
                self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                entry.last_accessed = Instant::now();
                return Arc::clone(&entry.statement);
            }
        }

        // Slow path: build and insert
        self.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let query = build();
        let sql = build_sql(&query);
        let stmt = Arc::new(PreparedStatement {
            sql,
            name: name.to_owned(),
        });

        {
            let mut cache = self.cache.write()
                .expect("PreparedCache RwLock poisoned during write");

            // Check again in case another thread inserted
            if let Some(entry) = cache.get_mut(&key) {
                entry.last_accessed = Instant::now();
                return Arc::clone(&entry.statement);
            }

            // LRU eviction: remove least recently used entries when at capacity
            if cache.len() >= self.max_size {
                // Find entries to evict (oldest half)
                let evict_count = self.max_size / 2;
                let mut entries: Vec<_> = cache.iter()
                    .map(|(k, e)| (k.clone(), e.last_accessed))
                    .collect();

                // Sort by last accessed time (oldest first)
                entries.sort_by_key(|(_, time)| *time);

                // Remove oldest entries
                for (key, _) in entries.into_iter().take(evict_count) {
                    cache.remove(&key);
                }
            }

            let entry = CacheEntry {
                statement: Arc::clone(&stmt),
                last_accessed: Instant::now(),
            };
            cache.insert(key, entry);
        }

        stmt
    }

    /// Prepare a query with a hash-based key.
    ///
    /// This is useful when you don't have a string name but want to
    /// cache based on the query structure.
    pub fn prepare_hashed<Q, F>(&self, build: F) -> Arc<PreparedStatement>
    where
        Q: QueryFragment<ClickHouse> + Hash + 'static,
        F: FnOnce() -> Q,
    {
        // Use type name as the key
        let name = std::any::type_name::<Q>();
        self.prepare(name, build)
    }

    /// Get cache statistics.
    ///
    /// # Panics
    ///
    /// Panics if the internal RwLock is poisoned.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            size: self.cache.read()
                .expect("PreparedCache RwLock poisoned during read")
                .len(),
            max_size: self.max_size,
            hits: self.hits.load(std::sync::atomic::Ordering::Relaxed),
            misses: self.misses.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Clear all cached statements.
    ///
    /// # Panics
    ///
    /// Panics if the internal RwLock is poisoned.
    pub fn clear(&self) {
        self.cache.write()
            .expect("PreparedCache RwLock poisoned during write")
            .clear();
    }

    /// Get a cached statement by name without building.
    ///
    /// Note: This does not update the LRU access time.
    ///
    /// # Panics
    ///
    /// Panics if the internal RwLock is poisoned.
    pub fn get(&self, name: &str) -> Option<Arc<PreparedStatement>> {
        let cache = self.cache.read()
            .expect("PreparedCache RwLock poisoned during read");
        for (key, entry) in cache.iter() {
            if key.name == name {
                return Some(Arc::clone(&entry.statement));
            }
        }
        None
    }
}

impl Default for PreparedCache {
    fn default() -> Self {
        Self::default_size()
    }
}

/// A prepared SQL statement.
#[derive(Debug, Clone)]
pub struct PreparedStatement {
    /// The compiled SQL string.
    pub sql: String,
    /// The name/identifier of this statement.
    pub name: String,
}

// =============================================================================
// SQL Escaping Utilities
// =============================================================================

/// Escape a string value for use in SQL single-quoted strings.
/// Escapes single quotes by doubling them.
#[inline]
fn escape_sql_string(s: &str) -> String {
    if s.contains('\'') {
        s.replace('\'', "''")
    } else {
        s.to_string()
    }
}

impl PreparedStatement {
    /// Create a new prepared statement.
    pub fn new(name: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sql: sql.into(),
        }
    }

    /// Get the SQL string.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Get the statement name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Create a SQL string with parameters substituted.
    ///
    /// # Safety Warning
    ///
    /// This method performs **no SQL escaping**. The parameters are inserted
    /// directly into the SQL string. This is **unsafe** if the parameters
    /// contain user-provided data.
    ///
    /// For safe parameter substitution, use [`with_params_escaped`] instead.
    ///
    /// This replaces `?` placeholders with the provided values.
    #[deprecated(since = "0.2.0", note = "Use with_params_escaped for safe parameter substitution")]
    pub fn with_params(&self, params: &[&dyn std::fmt::Display]) -> String {
        let mut result = String::with_capacity(self.sql.len() + params.len() * 10);
        let mut param_idx = 0;

        for c in self.sql.chars() {
            if c == '?' && param_idx < params.len() {
                result.push_str(&params[param_idx].to_string());
                param_idx += 1;
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Create a SQL string with parameters safely substituted.
    ///
    /// This replaces `?` placeholders with the provided values, properly
    /// escaping string values to prevent SQL injection.
    ///
    /// # Parameters
    ///
    /// Parameters are wrapped based on their type:
    /// - Strings are quoted and escaped (single quotes doubled)
    /// - Numbers are inserted as-is
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let stmt = PreparedStatement::new("find_user", "SELECT * FROM users WHERE name = ?");
    /// let sql = stmt.with_params_escaped(&[SqlParam::String("O'Brien")]);
    /// // Result: SELECT * FROM users WHERE name = 'O''Brien'
    /// ```
    pub fn with_params_escaped(&self, params: &[SqlParam<'_>]) -> String {
        let mut result = String::with_capacity(self.sql.len() + params.len() * 20);
        let mut param_idx = 0;

        for c in self.sql.chars() {
            if c == '?' && param_idx < params.len() {
                match &params[param_idx] {
                    SqlParam::String(s) => {
                        result.push('\'');
                        result.push_str(&escape_sql_string(s));
                        result.push('\'');
                    }
                    SqlParam::Int(n) => {
                        result.push_str(&n.to_string());
                    }
                    SqlParam::UInt(n) => {
                        result.push_str(&n.to_string());
                    }
                    SqlParam::Float(n) => {
                        result.push_str(&n.to_string());
                    }
                    SqlParam::Bool(b) => {
                        result.push_str(if *b { "true" } else { "false" });
                    }
                    SqlParam::Null => {
                        result.push_str("NULL");
                    }
                    SqlParam::Raw(s) => {
                        // Raw is unescaped - user takes responsibility
                        result.push_str(s);
                    }
                }
                param_idx += 1;
            } else {
                result.push(c);
            }
        }

        result
    }
}

/// A typed SQL parameter for safe query building.
#[derive(Debug, Clone)]
pub enum SqlParam<'a> {
    /// A string value (will be quoted and escaped).
    String(&'a str),
    /// A signed integer.
    Int(i64),
    /// An unsigned integer.
    UInt(u64),
    /// A floating point number.
    Float(f64),
    /// A boolean value.
    Bool(bool),
    /// A NULL value.
    Null,
    /// A raw SQL fragment (no escaping - use with caution).
    Raw(&'a str),
}

impl<'a> From<&'a str> for SqlParam<'a> {
    fn from(s: &'a str) -> Self {
        SqlParam::String(s)
    }
}

impl From<i64> for SqlParam<'_> {
    fn from(n: i64) -> Self {
        SqlParam::Int(n)
    }
}

impl From<u64> for SqlParam<'_> {
    fn from(n: u64) -> Self {
        SqlParam::UInt(n)
    }
}

impl From<f64> for SqlParam<'_> {
    fn from(n: f64) -> Self {
        SqlParam::Float(n)
    }
}

impl From<bool> for SqlParam<'_> {
    fn from(b: bool) -> Self {
        SqlParam::Bool(b)
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    /// Current number of cached statements.
    pub size: usize,
    /// Maximum cache size.
    pub max_size: usize,
    /// Number of cache hits.
    pub hits: u64,
    /// Number of cache misses.
    pub misses: u64,
}

impl CacheStats {
    /// Get the hit rate (0.0 to 1.0).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Build SQL from a query fragment.
fn build_sql<Q: QueryFragment<ClickHouse>>(query: &Q) -> String {
    use crate::core::backend::{GenericQueryBuilder, GenericBindCollector, QueryBuilder};
    use crate::core::query_builder::AstPass;

    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass = AstPass::<ClickHouse>::new(&mut builder, &mut collector);
    let _ = query.walk_ast(pass);
    builder.finish()
}

/// A query template with placeholders.
///
/// This allows creating parameterized queries that can be efficiently
/// reused with different parameter values.
///
/// # Example
///
/// ```rust,ignore
/// let tpl = QueryTemplate::new("SELECT * FROM {0} WHERE name = {1}");
/// let sql = tpl.render_with_params(&[
///     TemplateParam::Identifier("users"),
///     TemplateParam::String("O'Brien"),
/// ]);
/// // Result: SELECT * FROM `users` WHERE name = 'O''Brien'
/// ```
#[derive(Debug, Clone)]
pub struct QueryTemplate {
    /// The SQL template with `{0}`, `{1}`, etc. placeholders.
    template: String,
    /// Number of parameters.
    param_count: usize,
}

/// Parameter types for query templates.
#[derive(Debug, Clone)]
pub enum TemplateParam<'a> {
    /// A SQL identifier (table name, column name) - will be backtick-escaped.
    Identifier(&'a str),
    /// A string value - will be single-quote escaped.
    String(&'a str),
    /// An integer value.
    Int(i64),
    /// An unsigned integer value.
    UInt(u64),
    /// A raw SQL fragment - no escaping (use with caution).
    Raw(&'a str),
}

impl QueryTemplate {
    /// Create a new query template.
    ///
    /// Use `{0}`, `{1}`, etc. for parameter placeholders.
    pub fn new(template: impl Into<String>) -> Self {
        let template = template.into();
        let param_count = (0..100)
            .take_while(|i| template.contains(&format!("{{{}}}", i)))
            .count();

        Self { template, param_count }
    }

    /// Get the number of parameters.
    pub fn param_count(&self) -> usize {
        self.param_count
    }

    /// Render the template with the given parameters (no escaping).
    ///
    /// # Safety Warning
    ///
    /// This method performs **no SQL escaping**. Use [`render_with_params`]
    /// for safe parameter substitution.
    #[deprecated(since = "0.2.0", note = "Use render_with_params for safe parameter substitution")]
    pub fn render(&self, params: &[&str]) -> String {
        let mut result = self.template.clone();
        for (i, param) in params.iter().enumerate() {
            result = result.replace(&format!("{{{}}}", i), param);
        }
        result
    }

    /// Render with SQL-escaped string parameters.
    ///
    /// Note: All parameters are treated as strings. Use [`render_with_params`]
    /// for more control over parameter types.
    #[deprecated(since = "0.2.0", note = "Use render_with_params for more control over parameter types")]
    pub fn render_escaped(&self, params: &[&str]) -> String {
        let escaped: Vec<String> = params.iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect();
        let refs: Vec<&str> = escaped.iter().map(|s| s.as_str()).collect();
        #[allow(deprecated)]
        self.render(&refs)
    }

    /// Render the template with typed parameters.
    ///
    /// This is the recommended method for safe query building.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let tpl = QueryTemplate::new("SELECT * FROM {0} WHERE name = {1} AND id = {2}");
    /// let sql = tpl.render_with_params(&[
    ///     TemplateParam::Identifier("users"),
    ///     TemplateParam::String("O'Brien"),
    ///     TemplateParam::Int(42),
    /// ]);
    /// // Result: SELECT * FROM `users` WHERE name = 'O''Brien' AND id = 42
    /// ```
    pub fn render_with_params(&self, params: &[TemplateParam<'_>]) -> String {
        let mut result = self.template.clone();
        for (i, param) in params.iter().enumerate() {
            let replacement = match param {
                TemplateParam::Identifier(s) => {
                    if s.contains('`') {
                        format!("`{}`", s.replace('`', "``"))
                    } else {
                        format!("`{}`", s)
                    }
                }
                TemplateParam::String(s) => {
                    format!("'{}'", escape_sql_string(s))
                }
                TemplateParam::Int(n) => n.to_string(),
                TemplateParam::UInt(n) => n.to_string(),
                TemplateParam::Raw(s) => s.to_string(),
            };
            result = result.replace(&format!("{{{}}}", i), &replacement);
        }
        result
    }
}

/// Global prepared statement cache.
///
/// This provides a convenient way to share a cache across your application.
static GLOBAL_CACHE: std::sync::OnceLock<PreparedCache> = std::sync::OnceLock::new();

/// Get or initialize the global prepared cache.
pub fn global_cache() -> &'static PreparedCache {
    GLOBAL_CACHE.get_or_init(PreparedCache::default_size)
}

/// Initialize the global cache with a custom size.
///
/// This must be called before any use of `global_cache()`.
/// Returns `Err` if the cache was already initialized.
pub fn init_global_cache(max_size: usize) -> Result<(), PreparedCache> {
    GLOBAL_CACHE.set(PreparedCache::new(max_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepared_cache() {
        let cache = PreparedCache::new(10);

        // First call should miss
        let stmt1 = cache.prepare::<String, _>("test", || "SELECT 1".to_string());
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);

        // Second call should hit
        let stmt2 = cache.prepare::<String, _>("test", || "SELECT 2".to_string());
        assert_eq!(cache.stats().hits, 1);

        // Should return same SQL (first one)
        assert_eq!(stmt1.sql, stmt2.sql);
    }

    #[test]
    #[allow(deprecated)]
    fn test_prepared_statement_with_params_deprecated() {
        let stmt = PreparedStatement::new("test", "SELECT * FROM users WHERE id = ? AND name = ?");
        let sql = stmt.with_params(&[&42, &"alice"]);
        assert_eq!(sql, "SELECT * FROM users WHERE id = 42 AND name = alice");
    }

    #[test]
    fn test_prepared_statement_with_params_escaped() {
        let stmt = PreparedStatement::new("test", "SELECT * FROM users WHERE id = ? AND name = ?");
        let sql = stmt.with_params_escaped(&[SqlParam::Int(42), SqlParam::String("alice")]);
        assert_eq!(sql, "SELECT * FROM users WHERE id = 42 AND name = 'alice'");
    }

    #[test]
    fn test_prepared_statement_sql_injection_prevention() {
        let stmt = PreparedStatement::new("test", "SELECT * FROM users WHERE name = ?");
        // Attempt SQL injection
        let sql = stmt.with_params_escaped(&[SqlParam::String("'; DROP TABLE users; --")]);
        // The single quote should be escaped: ' (open) + '' (escaped quote) + rest + ' (close)
        assert_eq!(sql, "SELECT * FROM users WHERE name = '''; DROP TABLE users; --'");
        // The original single quote is now escaped as two single quotes
        assert!(sql.contains("'''"));  // open quote + escaped quote = three quotes
    }

    #[test]
    fn test_sql_param_types() {
        let stmt = PreparedStatement::new("test", "SELECT ? AS int, ? AS uint, ? AS float, ? AS bool, ? AS null");
        let sql = stmt.with_params_escaped(&[
            SqlParam::Int(-42),
            SqlParam::UInt(100),
            SqlParam::Float(3.14),
            SqlParam::Bool(true),
            SqlParam::Null,
        ]);
        assert_eq!(sql, "SELECT -42 AS int, 100 AS uint, 3.14 AS float, true AS bool, NULL AS null");
    }

    #[test]
    #[allow(deprecated)]
    fn test_query_template_deprecated() {
        let tpl = QueryTemplate::new("SELECT * FROM {0} WHERE id = {1}");
        assert_eq!(tpl.param_count(), 2);

        let sql = tpl.render(&["users", "42"]);
        assert_eq!(sql, "SELECT * FROM users WHERE id = 42");
    }

    #[test]
    fn test_query_template_with_params() {
        let tpl = QueryTemplate::new("SELECT * FROM {0} WHERE id = {1}");
        assert_eq!(tpl.param_count(), 2);

        let sql = tpl.render_with_params(&[
            TemplateParam::Identifier("users"),
            TemplateParam::Int(42),
        ]);
        assert_eq!(sql, "SELECT * FROM `users` WHERE id = 42");
    }

    #[test]
    fn test_query_template_sql_injection_prevention() {
        let tpl = QueryTemplate::new("SELECT * FROM {0} WHERE name = {1}");

        // Attempt SQL injection via table name
        let sql = tpl.render_with_params(&[
            TemplateParam::Identifier("users`; DROP TABLE users; --"),
            TemplateParam::String("test"),
        ]);
        // Backticks should be escaped
        assert!(sql.contains("`users``; DROP TABLE users; --`"));

        // Attempt SQL injection via string value
        let sql = tpl.render_with_params(&[
            TemplateParam::Identifier("users"),
            TemplateParam::String("'; DROP TABLE users; --"),
        ]);
        // Single quotes should be escaped
        assert!(sql.contains("'''; DROP TABLE users; --'"));
    }

    #[test]
    fn test_cache_stats() {
        let cache = PreparedCache::new(10);

        cache.prepare::<String, _>("q1", || "SELECT 1".to_string());
        cache.prepare::<String, _>("q1", || "SELECT 1".to_string());
        cache.prepare::<String, _>("q2", || "SELECT 2".to_string());

        let stats = cache.stats();
        assert_eq!(stats.size, 2);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
        assert!((stats.hit_rate() - 0.333).abs() < 0.01);
    }
}
