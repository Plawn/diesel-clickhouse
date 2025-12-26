//! Connection pooling example for diesel-clickhouse.
//!
//! This example demonstrates how to configure connection pools for efficient
//! connection management in high-concurrency applications.
//!
//! Run with:
//!   cargo run --example connection_pooling --features http
//!   cargo run --example connection_pooling --features native
//!
//! Prerequisites: docker-compose up -d

use diesel_clickhouse::pool::{Pool, PoolConfig};
use diesel_clickhouse::Connection;
use diesel_clickhouse::ConnectionBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Connection Pooling Example ===\n");

    // -------------------------------------------------------------------------
    // 0. Create pool using builder API (NEW!)
    // -------------------------------------------------------------------------
    println!("0. Creating pool with builder API:");

    #[cfg(feature = "http")]
    let pool = {
        println!("   Using HTTP backend");
        Pool::builder(
            Connection::http()
                .host("localhost")
                .port(8123)
                .user("default")
                .password("default")
                .database("test_db"),
        )
        .max_size(10)
        .min_idle(2)
        .connection_timeout_ms(30_000)
        .build()
        .await?
    };

    #[cfg(all(feature = "native", not(feature = "http")))]
    let pool = {
        println!("   Using Native backend");
        Pool::builder(
            Connection::native()
                .host("localhost")
                .port(9000)
                .user("default")
                .password("default")
                .database("test_db"),
        )
        .max_size(10)
        .min_idle(2)
        .connection_timeout_ms(30_000)
        .build()
        .await?
    };

    println!("   Pool created: max_size={}, idle={}",
        pool.config().max_size,
        pool.idle_count());

    // Use the pool
    {
        let conn = pool.get().await?;
        conn.execute("SELECT 1").await?;
        println!("   Query executed successfully\n");
    }

    // Alternative: create pool from URL
    println!("   Alternative - create from URL:");
    println!("   Pool::new(\"http://localhost:8123/test_db\", PoolConfig::default()).await?;\n");

    // -------------------------------------------------------------------------
    // 1. Default Pool Configuration
    // -------------------------------------------------------------------------
    println!("1. Default PoolConfig:");

    let default_config = PoolConfig::default();
    println!("   max_size: {}", default_config.max_size);
    println!("   min_idle: {:?}", default_config.min_idle);
    println!("   connection_timeout_ms: {} ms", default_config.connection_timeout_ms);
    println!("   idle_timeout_ms: {:?} ms", default_config.idle_timeout_ms);
    println!("   max_lifetime_ms: {:?} ms", default_config.max_lifetime_ms);
    println!();

    // -------------------------------------------------------------------------
    // 2. Custom Pool for Web Server
    // -------------------------------------------------------------------------
    println!("2. Custom config for web server:");

    let web_config = PoolConfig::new(20)          // Max 20 connections
        .min_idle(5)                               // Keep 5 warm connections
        .connection_timeout_ms(30_000)             // 30 second timeout
        .idle_timeout_ms(600_000)                  // Close idle after 10 min
        .max_lifetime_ms(1_800_000);               // Recycle after 30 min

    println!("   max_size: {}", web_config.max_size);
    println!("   min_idle: {:?}", web_config.min_idle);
    println!("   connection_timeout_ms: {} ms ({} sec)",
        web_config.connection_timeout_ms,
        web_config.connection_timeout_ms / 1000);
    println!("   idle_timeout_ms: {:?} ms (~{} min)",
        web_config.idle_timeout_ms,
        web_config.idle_timeout_ms.unwrap_or(0) / 60_000);
    println!();

    // -------------------------------------------------------------------------
    // 3. Configurations for Different Workloads
    // -------------------------------------------------------------------------
    println!("3. Configurations for different workloads:");

    let cpus = num_cpus();

    // Background job processing
    let job_config = PoolConfig::new(cpus)
        .min_idle(1)
        .connection_timeout_ms(60_000);
    println!("   Background jobs:");
    println!("     max_size: {} (1x CPUs)", job_config.max_size);
    println!("     min_idle: {:?}", job_config.min_idle);

    // Analytics/reporting (long queries)
    let analytics_config = PoolConfig::new(2 * cpus)
        .min_idle(0)  // No need to keep warm
        .connection_timeout_ms(300_000);  // 5 min timeout for long queries
    println!("   Analytics:");
    println!("     max_size: {} (2x CPUs)", analytics_config.max_size);
    println!("     min_idle: {:?}", analytics_config.min_idle);

    // Real-time API (fast fail)
    let api_config = PoolConfig::new(4 * cpus)
        .min_idle(cpus)
        .connection_timeout_ms(5_000);  // Fast fail
    println!("   Real-time API:");
    println!("     max_size: {} (4x CPUs)", api_config.max_size);
    println!("     min_idle: {:?}", api_config.min_idle);
    println!();

    // -------------------------------------------------------------------------
    // 4. Pool Sizing Guidelines
    // -------------------------------------------------------------------------
    println!("4. Pool sizing guidelines:");
    println!();
    println!("   | Workload         | max_size       | min_idle    | Timeout    |");
    println!("   |------------------|----------------|-------------|------------|");
    println!("   | Web server       | 2-4x CPU cores | CPU cores   | 30 sec     |");
    println!("   | Background jobs  | 1x CPU cores   | 1-2         | 60 sec     |");
    println!("   | Analytics        | 1-2x CPU cores | 0           | 5 min      |");
    println!("   | Mixed workload   | 3x CPU cores   | CPU cores/2 | 30 sec     |");
    println!();

    println!("   Your system: {} CPU cores", cpus);
    println!("   Recommended for web server:");
    println!("     max_size: {} (3x CPUs)", cpus * 3);
    println!("     min_idle: {}", cpus);
    println!();

    // -------------------------------------------------------------------------
    // 5. Timeout Configuration
    // -------------------------------------------------------------------------
    println!("5. Timeout configuration:");
    println!();
    println!("   connection_timeout_ms:");
    println!("   - Time to wait for a connection from the pool");
    println!("   - Too low: Requests fail under load");
    println!("   - Too high: Requests hang when pool is exhausted");
    println!("   - Recommendation: 5-30 seconds for APIs, 60+ for batch jobs");
    println!();
    println!("   idle_timeout_ms:");
    println!("   - How long idle connections stay in pool");
    println!("   - Too low: Frequent reconnections (overhead)");
    println!("   - Too high: Holding unused resources");
    println!("   - Recommendation: 5-10 minutes");
    println!();
    println!("   max_lifetime_ms:");
    println!("   - Maximum age of a connection before recycling");
    println!("   - Prevents using stale connections");
    println!("   - Recommendation: 30 minutes to 1 hour");
    println!();

    // -------------------------------------------------------------------------
    // 6. Memory Estimation
    // -------------------------------------------------------------------------
    println!("6. Memory estimation:");

    let conn_memory_kb = 50; // Approximate memory per connection
    let max_sizes = [10, 20, 50, 100];

    println!();
    println!("   | max_size | Estimated Memory |");
    println!("   |----------|------------------|");
    for size in &max_sizes {
        println!("   | {:<8} | ~{} MB           |", size, size * conn_memory_kb / 1024);
    }
    println!();

    // -------------------------------------------------------------------------
    // 7. Monitoring Metrics
    // -------------------------------------------------------------------------
    println!("7. Key monitoring metrics:");
    println!();
    println!("   - Pool utilization: active / max_size");
    println!("   - Wait time: Time spent waiting for a connection");
    println!("   - Connection errors: Failed connection attempts");
    println!("   - Idle count: Connections not in use");
    println!();
    println!("   Alert thresholds:");
    println!("   - Pool utilization > 80%: Consider increasing max_size");
    println!("   - Avg wait time > 100ms: Pool may be undersized");
    println!("   - Connection errors > 1%: Check network/server health");
    println!();

    // -------------------------------------------------------------------------
    // 8. Connection Lifecycle
    // -------------------------------------------------------------------------
    println!("8. Connection lifecycle:");
    println!();
    println!("   1. Request arrives -> pool.get()");
    println!("   2. If idle connection available -> return immediately");
    println!("   3. If pool < max_size -> create new connection");
    println!("   4. If pool = max_size -> wait up to connection_timeout_ms");
    println!("   5. Request completes -> connection returned to pool");
    println!("   6. If connection > max_lifetime_ms -> close and create new");
    println!("   7. If idle > idle_timeout_ms -> close to free resources");
    println!();

    // -------------------------------------------------------------------------
    // 9. Best Practices
    // -------------------------------------------------------------------------
    println!("9. Best practices:");
    println!();
    println!("   - Size pool based on expected concurrency, not traffic volume");
    println!("   - Set min_idle to handle baseline load without cold starts");
    println!("   - Use shorter timeouts for user-facing requests");
    println!("   - Monitor pool metrics and adjust based on actual usage");
    println!("   - Consider separate pools for different query types");
    println!("   - Set max_lifetime to recycle connections periodically");
    println!();

    // -------------------------------------------------------------------------
    // 10. Configuration Examples Summary
    // -------------------------------------------------------------------------
    println!("10. Configuration examples summary:");
    println!();

    let configs = [
        ("Low traffic API", PoolConfig::new(5).min_idle(2).connection_timeout_ms(30_000)),
        ("High traffic API", PoolConfig::new(50).min_idle(10).connection_timeout_ms(10_000)),
        ("Analytics dashboard", PoolConfig::new(10).min_idle(0).connection_timeout_ms(120_000)),
        ("Background workers", PoolConfig::new(cpus).min_idle(1).connection_timeout_ms(60_000)),
    ];

    for (name, cfg) in &configs {
        println!("   {}:", name);
        println!("     max: {}, min_idle: {:?}, timeout: {}s",
            cfg.max_size,
            cfg.min_idle,
            cfg.connection_timeout_ms / 1000);
    }
    println!();

    println!("=== End of Connection Pooling Example ===");

    Ok(())
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}
