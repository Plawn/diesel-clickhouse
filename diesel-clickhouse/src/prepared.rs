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

use crate::core::backend::ClickHouse;
use crate::core::query_builder::QueryFragment;

/// A cache for prepared SQL statements.
///
/// The cache stores compiled SQL strings keyed by a unique identifier,
/// avoiding repeated query building for frequently used queries.
#[derive(Debug)]
pub struct PreparedCache {
    cache: RwLock<HashMap<CacheKey, Arc<PreparedStatement>>>,
    max_size: usize,
    hits: std::sync::atomic::AtomicU64,
    misses: std::sync::atomic::AtomicU64,
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
    pub fn prepare<Q, F>(&self, name: &str, build: F) -> Arc<PreparedStatement>
    where
        Q: QueryFragment<ClickHouse> + 'static,
        F: FnOnce() -> Q,
    {
        let key = CacheKey {
            name: name.to_owned(),
            type_id: TypeId::of::<Q>(),
        };

        // Fast path: check read lock first
        {
            let cache = self.cache.read().unwrap();
            if let Some(stmt) = cache.get(&key) {
                self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Arc::clone(stmt);
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
            let mut cache = self.cache.write().unwrap();

            // Check again in case another thread inserted
            if let Some(existing) = cache.get(&key) {
                return Arc::clone(existing);
            }

            // Evict if necessary (simple strategy: clear half)
            if cache.len() >= self.max_size {
                let to_remove: Vec<_> = cache.keys()
                    .take(self.max_size / 2)
                    .cloned()
                    .collect();
                for k in to_remove {
                    cache.remove(&k);
                }
            }

            cache.insert(key, Arc::clone(&stmt));
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
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            size: self.cache.read().unwrap().len(),
            max_size: self.max_size,
            hits: self.hits.load(std::sync::atomic::Ordering::Relaxed),
            misses: self.misses.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Clear all cached statements.
    pub fn clear(&self) {
        self.cache.write().unwrap().clear();
    }

    /// Get a cached statement by name without building.
    pub fn get(&self, name: &str) -> Option<Arc<PreparedStatement>> {
        let cache = self.cache.read().unwrap();
        for (key, stmt) in cache.iter() {
            if key.name == name {
                return Some(Arc::clone(stmt));
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
    /// This replaces `?` placeholders with the provided values.
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
#[derive(Debug, Clone)]
pub struct QueryTemplate {
    /// The SQL template with `{0}`, `{1}`, etc. placeholders.
    template: String,
    /// Number of parameters.
    param_count: usize,
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

    /// Render the template with the given parameters.
    pub fn render(&self, params: &[&str]) -> String {
        let mut result = self.template.clone();
        for (i, param) in params.iter().enumerate() {
            result = result.replace(&format!("{{{}}}", i), param);
        }
        result
    }

    /// Render with SQL-escaped string parameters.
    pub fn render_escaped(&self, params: &[&str]) -> String {
        let escaped: Vec<String> = params.iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect();
        let refs: Vec<&str> = escaped.iter().map(|s| s.as_str()).collect();
        self.render(&refs)
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
    fn test_prepared_statement_with_params() {
        let stmt = PreparedStatement::new("test", "SELECT * FROM users WHERE id = ? AND name = ?");
        let sql = stmt.with_params(&[&42, &"'alice'"]);
        assert_eq!(sql, "SELECT * FROM users WHERE id = 42 AND name = 'alice'");
    }

    #[test]
    fn test_query_template() {
        let tpl = QueryTemplate::new("SELECT * FROM {0} WHERE id = {1}");
        assert_eq!(tpl.param_count(), 2);

        let sql = tpl.render(&["users", "42"]);
        assert_eq!(sql, "SELECT * FROM users WHERE id = 42");
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
