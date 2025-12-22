//! Test script using clickhouse-rs native protocol directly.
//!
//! Run with: cargo run --example native_test
//! Prerequisites: docker-compose up -d (ClickHouse on port 9000)

use clickhouse_rs::{types::Block, Pool};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Test clickhouse-rs Native Protocol ===\n");

    // Connect to ClickHouse via native protocol (port 9000)
    let url = std::env::var("CLICKHOUSE_NATIVE_URL")
        .unwrap_or_else(|_| "tcp://default:default@localhost:9000/test_db".to_string());

    println!("Connecting to: {}", url);
    let pool = Pool::new(url);
    let mut client = pool.get_handle().await?;
    println!("Connected!\n");

    // 1. Create test table
    println!("1. Creating test table...");
    client
        .execute(
            r"
            CREATE TABLE IF NOT EXISTS native_test (
                id UInt64,
                name String,
                value Float64,
                created_at DateTime DEFAULT now()
            ) ENGINE = MergeTree()
            ORDER BY id
            ",
        )
        .await?;
    println!("   Table created.\n");

    // 2. Insert data using Block
    println!("2. Inserting data...");
    let block = Block::new()
        .column("id", vec![1u64, 2, 3, 4, 5])
        .column(
            "name",
            vec!["Alice", "Bob", "Charlie", "Diana", "Eve"],
        )
        .column("value", vec![10.5f64, 20.3, 30.1, 40.7, 50.9]);

    client.insert("native_test", block).await?;
    println!("   Inserted 5 rows.\n");

    // 3. Query data
    println!("3. Querying data...");
    let block = client
        .query("SELECT id, name, value FROM native_test ORDER BY id")
        .fetch_all()
        .await?;

    println!("   Results ({} rows):", block.row_count());
    for row in block.rows() {
        let id: u64 = row.get("id")?;
        let name: &str = row.get("name")?;
        let value: f64 = row.get("value")?;
        println!("   - id={}, name={}, value={}", id, name, value);
    }
    println!();

    // 4. Query with filter
    println!("4. Query with filter (value > 25)...");
    let block = client
        .query("SELECT id, name, value FROM native_test WHERE value > 25 ORDER BY value DESC")
        .fetch_all()
        .await?;

    println!("   Results ({} rows):", block.row_count());
    for row in block.rows() {
        let id: u64 = row.get("id")?;
        let name: &str = row.get("name")?;
        let value: f64 = row.get("value")?;
        println!("   - id={}, name={}, value={}", id, name, value);
    }
    println!();

    // 5. Aggregation query
    println!("5. Aggregation query...");
    let block = client
        .query("SELECT count() as cnt, sum(value) as total, avg(value) as average FROM native_test")
        .fetch_all()
        .await?;

    for row in block.rows() {
        let cnt: u64 = row.get("cnt")?;
        let total: f64 = row.get("total")?;
        let avg: f64 = row.get("average")?;
        println!("   count={}, sum={}, avg={:.2}", cnt, total, avg);
    }
    println!();

    // 6. Cleanup
    println!("6. Cleaning up...");
    client.execute("DROP TABLE IF EXISTS native_test").await?;
    println!("   Table dropped.\n");

    println!("=== All tests passed! ===");
    Ok(())
}
