//! Prepared statement cache example for diesel-clickhouse.
//!
//! This example demonstrates how to use the prepared statement cache
//! to reduce query compilation overhead for repeated queries.
//!
//! Run with: cargo run --example prepared_cache

use diesel_clickhouse::prepared::{PreparedCache, PreparedStatement, QueryTemplate, global_cache};

fn main() {
    println!("=== Prepared Statement Cache Example ===\n");

    // -------------------------------------------------------------------------
    // 1. Creating a PreparedCache
    // -------------------------------------------------------------------------
    println!("1. Creating a PreparedCache:");

    let cache = PreparedCache::new(100);  // LRU cache with 100 entries
    println!("   Created cache with max size: 100");

    let stats = cache.stats().expect("stats failed");
    println!("   Initial state:");
    println!("   - Size: {}", stats.size);
    println!("   - Hits: {}", stats.hits);
    println!("   - Misses: {}", stats.misses);
    println!("   - Hit rate: {:.1}%", stats.hit_rate() * 100.0);
    println!();

    // -------------------------------------------------------------------------
    // 2. PreparedStatement Basics
    // -------------------------------------------------------------------------
    println!("2. PreparedStatement basics:");

    let stmt = PreparedStatement::new(
        "get_user_by_id",
        "SELECT * FROM users WHERE id = ?"
    );

    println!("   Name: {}", stmt.name());
    println!("   SQL: {}", stmt.sql());

    // Substitute parameters (deprecated, but shown for completeness)
    #[allow(deprecated)]
    let sql_with_params = stmt.with_params(&[&42]);
    println!("   With id=42: {}", sql_with_params);

    #[allow(deprecated)]
    let sql_with_params = stmt.with_params(&[&123]);
    println!("   With id=123: {}", sql_with_params);
    println!();

    // -------------------------------------------------------------------------
    // 3. Multiple Parameters
    // -------------------------------------------------------------------------
    println!("3. Multiple parameters:");

    let stmt = PreparedStatement::new(
        "search_users",
        "SELECT * FROM users WHERE name = ? AND age > ? LIMIT ?"
    );

    #[allow(deprecated)]
    let sql = stmt.with_params(&[&"'Alice'", &25, &100]);
    println!("   Template: {}", stmt.sql());
    println!("   Rendered: {}", sql);
    println!();

    // -------------------------------------------------------------------------
    // 4. QueryTemplate with Named Placeholders
    // -------------------------------------------------------------------------
    println!("4. QueryTemplate with indexed placeholders:");

    let template = QueryTemplate::new(
        "SELECT * FROM {0} WHERE {1} = {2} ORDER BY {3} LIMIT {4}"
    );

    println!("   Template param count: {}", template.param_count());

    #[allow(deprecated)]
    let sql = template.render(&["users", "status", "'active'", "created_at DESC", "100"]);
    println!("   Rendered: {}", sql);

    // Different table
    #[allow(deprecated)]
    let sql = template.render(&["orders", "user_id", "42", "order_date", "50"]);
    println!("   Rendered: {}", sql);
    println!();

    // -------------------------------------------------------------------------
    // 5. QueryTemplate with Escaping
    // -------------------------------------------------------------------------
    println!("5. QueryTemplate with auto-escaping:");

    let template = QueryTemplate::new(
        "SELECT * FROM users WHERE name = {0}"
    );

    // render_escaped wraps params in quotes and escapes
    #[allow(deprecated)]
    let sql = template.render_escaped(&["O'Brien"]);
    println!("   Input: O'Brien");
    println!("   Escaped: {}", sql);

    #[allow(deprecated)]
    let sql = template.render_escaped(&["normal_name"]);
    println!("   Input: normal_name");
    println!("   Escaped: {}", sql);
    println!();

    // -------------------------------------------------------------------------
    // 6. Cache Simulation
    // -------------------------------------------------------------------------
    println!("6. Cache hit/miss simulation:");

    let cache = PreparedCache::new(10);

    // First call - cache miss
    let _stmt = cache.prepare::<String, _>("query_a", || {
        "SELECT * FROM table_a".to_string()
    }).expect("prepare failed");
    println!("   After query_a: misses={}", cache.stats().expect("stats failed").misses);

    // Second call - cache hit
    let _stmt = cache.prepare::<String, _>("query_a", || {
        "SELECT * FROM table_a".to_string()
    }).expect("prepare failed");
    println!("   After query_a (again): hits={}", cache.stats().expect("stats failed").hits);

    // Different query - cache miss
    let _stmt = cache.prepare::<String, _>("query_b", || {
        "SELECT * FROM table_b".to_string()
    }).expect("prepare failed");
    println!("   After query_b: misses={}", cache.stats().expect("stats failed").misses);

    let stats = cache.stats().expect("stats failed");
    println!("   Final: size={}, hits={}, misses={}, hit_rate={:.1}%",
        stats.size, stats.hits, stats.misses, stats.hit_rate() * 100.0);
    println!();

    // -------------------------------------------------------------------------
    // 7. Global Cache
    // -------------------------------------------------------------------------
    println!("7. Using global cache:");

    let global = global_cache();

    // Use global cache
    let _stmt = global.prepare::<String, _>("global_query_1", || {
        "SELECT 1".to_string()
    }).expect("prepare failed");
    let _stmt = global.prepare::<String, _>("global_query_2", || {
        "SELECT 2".to_string()
    }).expect("prepare failed");

    println!("   Global cache size: {}", global.stats().expect("stats failed").size);
    println!();

    // -------------------------------------------------------------------------
    // 8. Cache Lookup
    // -------------------------------------------------------------------------
    println!("8. Cache lookup by name:");

    let cache = PreparedCache::new(10);
    cache.prepare::<String, _>("my_query", || "SELECT * FROM test".to_string())
        .expect("prepare failed");

    match cache.get("my_query").expect("get failed") {
        Some(stmt) => println!("   Found: {}", stmt.sql()),
        None => println!("   Not found"),
    }

    match cache.get("nonexistent").expect("get failed") {
        Some(stmt) => println!("   Found: {}", stmt.sql()),
        None => println!("   Not found (expected)"),
    }
    println!();

    // -------------------------------------------------------------------------
    // 9. Cache Clear
    // -------------------------------------------------------------------------
    println!("9. Clearing cache:");

    let cache = PreparedCache::new(10);
    cache.prepare::<String, _>("q1", || "SELECT 1".to_string()).expect("prepare failed");
    cache.prepare::<String, _>("q2", || "SELECT 2".to_string()).expect("prepare failed");

    println!("   Before clear: size={}", cache.stats().expect("stats failed").size);
    cache.clear().expect("clear failed");
    println!("   After clear: size={}", cache.stats().expect("stats failed").size);
    println!();

    // -------------------------------------------------------------------------
    // 10. Performance Simulation
    // -------------------------------------------------------------------------
    println!("10. Performance comparison:");

    let cache = PreparedCache::new(100);
    let iterations = 10_000;

    // Simulate uncached (always build)
    let start = std::time::Instant::now();
    for i in 0..iterations {
        let _ = format!(
            "SELECT * FROM users WHERE id = {} AND status = 'active'",
            i % 100
        );
    }
    let uncached_time = start.elapsed();

    // Simulate cached (reuse statements)
    let start = std::time::Instant::now();
    for i in 0..iterations {
        let key = format!("user_query_{}", i % 100);
        let stmt = cache.prepare::<String, _>(&key, || {
            format!(
                "SELECT * FROM users WHERE id = {} AND status = 'active'",
                i % 100
            )
        }).expect("prepare failed");
        let _ = stmt.sql();
    }
    let cached_time = start.elapsed();

    println!("   {} iterations:", iterations);
    println!("   - Uncached: {:?}", uncached_time);
    println!("   - Cached: {:?}", cached_time);
    println!("   - Hit rate: {:.1}%", cache.stats().expect("stats failed").hit_rate() * 100.0);
    println!();

    // -------------------------------------------------------------------------
    // 11. Best Practices
    // -------------------------------------------------------------------------
    println!("11. Best practices:");
    println!();
    println!("   - Use global_cache() for application-wide caching");
    println!("   - Use descriptive names for prepare() calls");
    println!("   - Size cache based on unique query count (100-1000 typical)");
    println!("   - Monitor hit rate in production (should be >90%)");
    println!("   - Call cache.clear() after schema migrations");
    println!("   - Use QueryTemplate for parameterized queries");
    println!("   - Use render_escaped() for user-provided values");
    println!();

    println!("=== End of Prepared Cache Example ===");
}
