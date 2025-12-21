//! Async insert example for diesel-clickhouse.
//!
//! This example demonstrates high-throughput inserts using ClickHouse's
//! async_insert mode, which buffers data server-side for optimal performance.
//!
//! Run with: cargo run --example async_inserts

use diesel_clickhouse::async_insert::AsyncInsertConfig;

fn main() {
    println!("=== Async Insert Example ===\n");

    // -------------------------------------------------------------------------
    // 1. Default Configuration
    // -------------------------------------------------------------------------
    println!("1. Default AsyncInsertConfig:");

    let config = AsyncInsertConfig::default();
    println!("   async_insert: {}", config.async_insert);
    println!("   wait_for_async_insert: {}", config.wait_for_async_insert);
    println!("   async_insert_busy_timeout_ms: {}", config.async_insert_busy_timeout_ms);
    println!("   async_insert_max_data_size: {} bytes", config.async_insert_max_data_size);
    println!("   async_insert_max_query_number: {}\n", config.async_insert_max_query_number);

    // -------------------------------------------------------------------------
    // 2. Fire-and-Forget Mode (Highest Throughput)
    // -------------------------------------------------------------------------
    println!("2. Fire-and-Forget mode (highest throughput):");

    let fire_and_forget = AsyncInsertConfig::fire_and_forget();
    println!("   async_insert: {}", fire_and_forget.async_insert);
    println!("   wait_for_async_insert: {}", fire_and_forget.wait_for_async_insert);
    println!("   Use case: Logs, metrics, analytics where some data loss is acceptable\n");

    // -------------------------------------------------------------------------
    // 3. Synchronous Mode (Highest Durability)
    // -------------------------------------------------------------------------
    println!("3. Synchronous mode (highest durability):");

    let synchronous = AsyncInsertConfig::synchronous();
    println!("   async_insert: {}", synchronous.async_insert);
    println!("   wait_for_async_insert: {}", synchronous.wait_for_async_insert);
    println!("   Use case: Financial transactions, critical data\n");

    // -------------------------------------------------------------------------
    // 4. Custom Configuration with Builder Pattern
    // -------------------------------------------------------------------------
    println!("4. Custom configuration:");

    let custom = AsyncInsertConfig::default()
        .wait_for_async_insert(true)
        .async_insert_busy_timeout_ms(5000)       // Flush after 5 seconds
        .async_insert_max_data_size(10_000_000)   // Or when 10MB accumulated
        .async_insert_max_query_number(1000)      // Or after 1000 queries
        .deduplicate_materialized_views(true);    // Enable dedup for ReplicatedMergeTree

    println!("   async_insert_busy_timeout_ms: {}", custom.async_insert_busy_timeout_ms);
    println!("   async_insert_max_data_size: {} bytes ({} MB)",
        custom.async_insert_max_data_size,
        custom.async_insert_max_data_size / 1_000_000);
    println!("   async_insert_max_query_number: {}\n", custom.async_insert_max_query_number);

    // -------------------------------------------------------------------------
    // 5. Generated SQL Settings
    // -------------------------------------------------------------------------
    println!("5. Generated SQL SETTINGS clause:");

    let settings_sql = custom.to_settings_sql();
    println!("   {}\n", settings_sql);

    // Show what a fire-and-forget config generates
    let ff_settings = AsyncInsertConfig::fire_and_forget().to_settings_sql();
    println!("   Fire-and-forget: {}\n", ff_settings);

    // -------------------------------------------------------------------------
    // 6. Configuration Presets Comparison
    // -------------------------------------------------------------------------
    println!("6. Configuration presets comparison:");
    println!();
    println!("   | Mode             | wait | timeout_ms | Latency | Durability | Throughput |");
    println!("   |------------------|------|------------|---------|------------|------------|");

    let configs = [
        ("fire_and_forget", AsyncInsertConfig::fire_and_forget()),
        ("synchronous", AsyncInsertConfig::synchronous()),
        ("default", AsyncInsertConfig::default()),
    ];

    for (name, cfg) in &configs {
        let latency = if cfg.wait_for_async_insert { "Higher" } else { "Lowest" };
        let durability = if cfg.wait_for_async_insert { "High" } else { "None" };
        let throughput = if cfg.wait_for_async_insert { "Moderate" } else { "Highest" };

        println!("   | {:<16} | {:<4} | {:<10} | {:<7} | {:<10} | {:<10} |",
            name,
            cfg.wait_for_async_insert,
            cfg.async_insert_busy_timeout_ms,
            latency,
            durability,
            throughput);
    }
    println!();

    // -------------------------------------------------------------------------
    // 7. Tuning Guidelines
    // -------------------------------------------------------------------------
    println!("7. Tuning guidelines:");
    println!();
    println!("   busy_timeout_ms:");
    println!("   - Lower (100-500ms): More frequent flushes, lower latency");
    println!("   - Higher (1000-10000ms): Larger batches, better throughput");
    println!();
    println!("   max_data_size:");
    println!("   - Small tables/rows: 1-10 MB is fine");
    println!("   - Large rows/blobs: Consider 50-100 MB");
    println!();
    println!("   max_query_number:");
    println!("   - High QPS: Increase to 1000-10000");
    println!("   - Low QPS: Default 450 is usually fine");
    println!();

    // -------------------------------------------------------------------------
    // 8. Example Insert SQL
    // -------------------------------------------------------------------------
    println!("8. Example INSERT with async settings:");

    let table = "events";
    let config = AsyncInsertConfig::fire_and_forget()
        .async_insert_busy_timeout_ms(1000);

    // Build an example INSERT statement
    let sql = format!(
        "INSERT INTO {} (id, user_id, event_type, value) {} VALUES (1, 42, 'click', 1.5)",
        table,
        config.to_settings_sql()
    );
    println!("   {}\n", sql);

    // -------------------------------------------------------------------------
    // 9. Monitoring Async Inserts
    // -------------------------------------------------------------------------
    println!("9. Monitoring async inserts (ClickHouse queries):");
    println!();
    println!("   -- Check pending async inserts");
    println!("   SELECT * FROM system.asynchronous_insert_queue");
    println!();
    println!("   -- Force flush all pending inserts");
    println!("   SYSTEM FLUSH ASYNC INSERT QUEUE");
    println!();
    println!("   -- Monitor async insert metrics");
    println!("   SELECT");
    println!("       event,");
    println!("       value");
    println!("   FROM system.events");
    println!("   WHERE event LIKE '%AsyncInsert%'");
    println!();

    // -------------------------------------------------------------------------
    // 10. Best Practices Summary
    // -------------------------------------------------------------------------
    println!("10. Best practices:");
    println!();
    println!("   - Use fire_and_forget() for logs/metrics (accept some data loss)");
    println!("   - Use synchronous() for critical/financial data");
    println!("   - Set busy_timeout_ms based on latency requirements:");
    println!("     - Low latency needed: 100-500ms");
    println!("     - High throughput needed: 1000-10000ms");
    println!("   - Set max_data_size based on available memory (1-100 MB)");
    println!("   - Enable deduplicate_materialized_views for ReplicatedMergeTree");
    println!("   - Monitor system.asynchronous_insert_queue in production");
    println!();

    println!("=== End of Async Insert Example ===");
}
