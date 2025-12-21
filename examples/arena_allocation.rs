//! Arena allocation example for diesel-clickhouse.
//!
//! This example demonstrates how to use arena allocation for efficient
//! query building with minimal heap allocations.
//!
//! Run with: cargo run --example arena_allocation

use diesel_clickhouse::{QueryArena, ArenaQueryBuilder, with_arena};

fn main() {
    println!("=== Arena Allocation Example ===\n");

    // -------------------------------------------------------------------------
    // 1. Basic Arena Usage - Allocating Strings
    // -------------------------------------------------------------------------
    println!("1. Basic arena usage:");

    let arena = QueryArena::new();

    // Allocate strings in the arena (bump allocation - very fast)
    let s1 = arena.alloc_str("SELECT ");
    let s2 = arena.alloc_str("* FROM ");
    let s3 = arena.alloc_str("users");

    println!("   Allocated: '{}', '{}', '{}'", s1, s2, s3);
    println!("   Total bytes allocated: {}\n", arena.allocated_bytes());

    // -------------------------------------------------------------------------
    // 2. Arena with Pre-allocated Capacity
    // -------------------------------------------------------------------------
    println!("2. Arena with pre-allocated capacity:");

    let arena = QueryArena::with_capacity(4096);
    let _ = arena.alloc_str("Some initial string");
    println!("   Created arena with 4KB capacity");
    println!("   Allocated bytes: {}\n", arena.allocated_bytes());

    // -------------------------------------------------------------------------
    // 3. Formatted Strings in Arena
    // -------------------------------------------------------------------------
    println!("3. Formatted strings:");

    let arena = QueryArena::new();
    let user_id = 42u64;
    let limit = 100usize;

    // Use alloc_fmt for formatted strings without heap allocation
    let formatted = arena.alloc_fmt(format_args!(
        "SELECT * FROM users WHERE id = {} LIMIT {}",
        user_id, limit
    ));

    println!("   Formatted: {}", formatted);
    println!("   Allocated: {} bytes\n", arena.allocated_bytes());

    // -------------------------------------------------------------------------
    // 4. Joining Strings Efficiently
    // -------------------------------------------------------------------------
    println!("4. Joining strings:");

    let arena = QueryArena::new();

    // Join column names
    let columns = ["id", "name", "email", "created_at"];
    let joined = arena.join(&columns, ", ");
    println!("   Columns: {}", joined);

    // Build a query from parts
    let parts = [
        arena.alloc_str("SELECT "),
        joined,
        arena.alloc_str(" FROM users"),
    ];
    let query = arena.join(&parts, "");
    println!("   Query: {}", query);
    println!("   Allocated: {} bytes\n", arena.allocated_bytes());

    // -------------------------------------------------------------------------
    // 5. Arena Reset and Reuse
    // -------------------------------------------------------------------------
    println!("5. Arena reset (memory reuse):");

    let mut arena = QueryArena::new();

    let _ = arena.alloc_str("First allocation with some data");
    println!("   After first alloc: {} bytes", arena.allocated_bytes());

    arena.reset(); // Memory is freed but underlying capacity is retained
    println!("   After reset: {} bytes", arena.allocated_bytes());

    let _ = arena.alloc_str("Second allocation");
    println!("   After second alloc: {} bytes\n", arena.allocated_bytes());

    // -------------------------------------------------------------------------
    // 6. ArenaQueryBuilder - Building SQL Queries
    // -------------------------------------------------------------------------
    println!("6. ArenaQueryBuilder for SQL construction:");

    let arena = QueryArena::new();
    let mut builder = ArenaQueryBuilder::new(&arena);

    builder.push("SELECT ");
    builder.push_identifier("name");  // Adds backticks: `name`
    builder.push(", ");
    builder.push_identifier("age");
    builder.push(" FROM ");
    builder.push_identifier("users");
    builder.push(" WHERE ");
    builder.push_identifier("id");
    builder.push(" = ");
    builder.push_int(42u32);  // Fast integer formatting with itoa

    let sql = builder.finish();
    println!("   SQL: {}", sql);
    println!("   Parts count: {}\n", builder.len());

    // -------------------------------------------------------------------------
    // 7. String Literals with Escaping
    // -------------------------------------------------------------------------
    println!("7. String literals with escaping:");

    let arena = QueryArena::new();
    let mut builder = ArenaQueryBuilder::new(&arena);

    builder.push("SELECT * FROM users WHERE name = ");
    builder.push_string_literal("O'Brien");  // Escapes single quotes

    let sql = builder.finish();
    println!("   SQL: {}", sql);

    // Another example with nested quotes
    let mut builder2 = ArenaQueryBuilder::new(&arena);
    builder2.push("SELECT * FROM users WHERE bio = ");
    builder2.push_string_literal("She said 'hello' to me");
    println!("   SQL: {}\n", builder2.finish());

    // -------------------------------------------------------------------------
    // 8. Float Formatting with ryu
    // -------------------------------------------------------------------------
    println!("8. Numeric formatting:");

    let arena = QueryArena::new();
    let mut builder = ArenaQueryBuilder::new(&arena);

    builder.push("SELECT * FROM events WHERE ");
    builder.push_identifier("value");
    builder.push(" > ");
    builder.push_float(3.14159f64);  // Fast float formatting with ryu
    builder.push(" AND ");
    builder.push_identifier("count");
    builder.push(" >= ");
    builder.push_int(1000i64);

    let sql = builder.finish();
    println!("   SQL: {}\n", sql);

    // -------------------------------------------------------------------------
    // 9. Thread-Local Arena with with_arena()
    // -------------------------------------------------------------------------
    println!("9. Thread-local arena (with_arena):");

    // with_arena provides a thread-local arena that's automatically reset after use
    let sql = with_arena(|arena| {
        let mut builder = ArenaQueryBuilder::new(arena);
        builder.push("SELECT count(*) FROM ");
        builder.push_identifier("events");
        builder.push(" WHERE ");
        builder.push_identifier("timestamp");
        builder.push(" > now() - INTERVAL 1 DAY");
        builder.finish()  // Returns owned String
    });

    println!("   Built: {}\n", sql);

    // -------------------------------------------------------------------------
    // 10. Building Complex Queries
    // -------------------------------------------------------------------------
    println!("10. Complex query example:");

    let sql = build_search_query(
        "events",
        &["id", "user_id", "event_type", "timestamp"],
        &[("user_id", "42"), ("event_type", "click")],
        Some("timestamp DESC"),
        100,
    );
    println!("   Query:\n   {}\n", sql);

    // -------------------------------------------------------------------------
    // 11. Performance Comparison Simulation
    // -------------------------------------------------------------------------
    println!("11. Performance comparison:");

    let iterations = 10_000;

    // Method 1: String concatenation (many allocations)
    let start = std::time::Instant::now();
    for i in 0..iterations {
        let _ = format!(
            "SELECT `id`, `name` FROM `users` WHERE `id` = {} LIMIT 100",
            i
        );
    }
    let string_time = start.elapsed();

    // Method 2: Arena allocation (minimal allocations)
    let start = std::time::Instant::now();
    let mut arena = QueryArena::with_capacity(256);
    for i in 0..iterations {
        {
            let mut builder = ArenaQueryBuilder::new(&arena);
            builder.push("SELECT ");
            builder.push_identifier("id");
            builder.push(", ");
            builder.push_identifier("name");
            builder.push(" FROM ");
            builder.push_identifier("users");
            builder.push(" WHERE ");
            builder.push_identifier("id");
            builder.push(" = ");
            builder.push_int(i as u32);
            builder.push(" LIMIT 100");
            let _ = builder.finish();
        }
        arena.reset();
    }
    let arena_time = start.elapsed();

    println!("   String format ({}x): {:?}", iterations, string_time);
    println!("   Arena builder ({}x): {:?}", iterations, arena_time);
    println!("   Speedup: {:.2}x\n",
        string_time.as_nanos() as f64 / arena_time.as_nanos() as f64);

    // -------------------------------------------------------------------------
    // 12. Arena Collections
    // -------------------------------------------------------------------------
    println!("12. Arena collections:");

    let arena = QueryArena::new();

    // Allocate a Vec in the arena
    let mut columns = arena.alloc_vec::<&str>();
    columns.push("id");
    columns.push("name");
    columns.push("email");
    println!("   Vec in arena: {:?}", columns.as_slice());

    // Allocate a String in the arena
    let mut query = arena.alloc_string_with_capacity(64);
    query.push_str("SELECT ");
    query.push_str(&columns.join(", "));
    query.push_str(" FROM users");
    println!("   String in arena: {}", query);
    println!("   Total allocated: {} bytes\n", arena.allocated_bytes());

    println!("=== End of Arena Allocation Example ===");
}

/// Build a search query using arena allocation.
fn build_search_query(
    table: &str,
    columns: &[&str],
    filters: &[(&str, &str)],
    order_by: Option<&str>,
    limit: usize,
) -> String {
    with_arena(|arena| {
        let mut builder = ArenaQueryBuilder::new(arena);

        // SELECT clause
        builder.push("SELECT ");
        for (i, col) in columns.iter().enumerate() {
            if i > 0 {
                builder.push(", ");
            }
            builder.push_identifier(col);
        }

        // FROM clause
        builder.push(" FROM ");
        builder.push_identifier(table);

        // WHERE clause
        if !filters.is_empty() {
            builder.push(" WHERE ");
            for (i, (col, val)) in filters.iter().enumerate() {
                if i > 0 {
                    builder.push(" AND ");
                }
                builder.push_identifier(col);
                builder.push(" = ");
                builder.push_string_literal(val);
            }
        }

        // ORDER BY clause
        if let Some(order) = order_by {
            builder.push(" ORDER BY ");
            builder.push(order);
        }

        // LIMIT clause
        builder.push(" LIMIT ");
        builder.push_int(limit as u64);

        builder.finish()
    })
}
