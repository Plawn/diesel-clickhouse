//! # Unified Connection Example
//!
//! Demonstrates the unified `Connection` API that works with both HTTP and Native backends.
//!
//! ## Run with HTTP backend (default):
//! ```bash
//! cargo run --example unified_connection
//! ```
//!
//! ## Run with Native backend:
//! ```bash
//! cargo run --example unified_connection --no-default-features --features native
//! ```
//!
//! Prerequisites: `docker-compose up -d`

use diesel_clickhouse::prelude::*;
use diesel_clickhouse::Connection;

// =============================================================================
// 1. Define Table Schema
// =============================================================================

diesel_clickhouse::table! {
    events (id, timestamp) {
        id -> UInt64,
        user_id -> UInt32,
        event_type -> CHString,
        value -> Float64,
        timestamp -> DateTime,
    }
}

// =============================================================================
// 2. Define Row Types
// =============================================================================

/// For INSERT operations - used by query builder
/// Works on both HTTP and Native backends
#[derive(Debug, Clone)]
#[derive(diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = events)]
pub struct NewEvent {
    pub id: u64,
    pub user_id: u32,
    pub event_type: String,
    pub value: f64,
}

/// For HTTP streaming inserts - requires clickhouse::Row
#[cfg(feature = "http")]
#[derive(Debug, Clone, clickhouse::Row, serde::Serialize)]
pub struct NewEventHttp {
    pub id: u64,
    pub user_id: u32,
    pub event_type: String,
    pub value: f64,
}

/// For FETCH operations on HTTP backend - requires clickhouse::Row
#[cfg(feature = "http")]
#[derive(Debug, Clone, clickhouse::Row, serde::Deserialize)]
pub struct Event {
    pub id: u64,
    pub user_id: u32,
    pub event_type: String,
    pub value: f64,
}

/// For FETCH operations on Native backend - requires serde::Deserialize
#[cfg(all(feature = "native", not(feature = "http")))]
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Event {
    pub id: u64,
    pub user_id: u32,
    pub event_type: String,
    pub value: f64,
}

// =============================================================================
// 3. Main
// =============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // URL determines backend: http:// -> HTTP, tcp:// -> Native
    let url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123/test_db".to_string());

    println!("=== Unified Connection Example ===\n");
    println!("Connecting to: {}", url);

    let conn = match Connection::establish(&url).await {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Connection failed: {}", e);
            eprintln!("Run: docker-compose up -d\n");
            return demo_mode();
        }
    };

    // Identify backend
    println!("Backend: {}", if conn.is_http() { "HTTP" } else { "Native" });
    println!("Database: {}\n", conn.database());

    // =========================================================================
    // UNIFIED OPERATIONS (work on both backends)
    // =========================================================================

    println!("--- Unified Operations ---\n");

    // 1. Execute DDL
    conn.execute(
        "CREATE TABLE IF NOT EXISTS events (
            id UInt64,
            user_id UInt32,
            event_type String,
            value Float64,
            timestamp DateTime DEFAULT now()
        ) ENGINE = MergeTree() ORDER BY (id, timestamp)"
    ).await?;
    println!("[OK] CREATE TABLE");

    // 2. Insert using raw VALUES
    conn.insert_values(
        "events",
        "(1, 100, 'click', 1.5), (2, 100, 'view', 0.5), (3, 200, 'click', 2.0)"
    ).await?;
    println!("[OK] INSERT raw VALUES (3 rows)");

    // 3. Insert using query builder with Insertable derive
    let new_events = vec![
        NewEvent { id: 4, user_id: 100, event_type: "purchase".into(), value: 99.99 },
        NewEvent { id: 5, user_id: 200, event_type: "signup".into(), value: 0.0 },
    ];
    conn.insert(insert_into(events::table).values(new_events.as_slice())).await?;
    println!("[OK] INSERT via query builder (2 rows)");

    // 4. Execute UPDATE via query builder
    let update_stmt = update(events::table)
        .filter(events::id.eq(1u64))
        .set(events::value.eq(10.0));
    conn.execute_query(update_stmt).await?;
    println!("[OK] UPDATE via query builder");

    // 5. Build SQL without executing (debugging)
    let query = events::table
        .filter(events::user_id.eq(100u32))
        .order_by(events::timestamp.desc())
        .limit(10);
    let sql = conn.build_sql(query);
    println!("[OK] Build SQL: {}", sql);

    // =========================================================================
    // FETCH OPERATIONS (backend-specific trait requirements)
    // =========================================================================

    println!("\n--- Fetch Operations ---\n");

    // fetch_all - returns Vec<T>
    let all_events: Vec<Event> = conn.fetch_all(
        events::table.filter(events::user_id.eq(100u32))
    ).await?;
    println!("[OK] fetch_all: {} rows", all_events.len());
    for e in &all_events {
        println!("     {:?}", e);
    }

    // fetch_one - returns T (error if no rows)
    let first: Event = conn.fetch_one(
        events::table.order_by(events::id.asc()).limit(1)
    ).await?;
    println!("[OK] fetch_one: {:?}", first);

    // fetch_optional - returns Option<T>
    let maybe: Option<Event> = conn.fetch_optional(
        events::table.filter(events::user_id.eq(999u32))
    ).await?;
    println!("[OK] fetch_optional: {:?}", maybe);

    // fetch_all_raw - raw SQL
    let raw_results: Vec<Event> = conn.fetch_all_raw(
        "SELECT id, user_id, event_type, value FROM events LIMIT 3"
    ).await?;
    println!("[OK] fetch_all_raw: {} rows", raw_results.len());

    // =========================================================================
    // BACKEND-SPECIFIC ACCESS
    // =========================================================================

    println!("\n--- Backend-Specific Access ---\n");

    #[cfg(feature = "http")]
    if let Some(http_conn) = conn.as_http() {
        // Access clickhouse crate's Client directly for advanced operations
        println!("[HTTP] Direct client access available");

        // Example: streaming inserter (more efficient for large batches)
        let mut inserter = http_conn.client()
            .insert::<NewEventHttp>("events")
            .await?;

        inserter.write(&NewEventHttp {
            id: 100,
            user_id: 999,
            event_type: "streaming".into(),
            value: 42.0,
        }).await?;
        inserter.end().await?;
        println!("[HTTP] Streaming insert completed");
    }

    #[cfg(feature = "native")]
    if let Some(_native_conn) = conn.as_native() {
        println!("[Native] Direct connection access available");
        // Access clickhouse-rs Block API for advanced operations
    }

    // =========================================================================
    // CLEANUP
    // =========================================================================

    conn.execute("TRUNCATE TABLE events").await?;
    println!("\n[OK] Table truncated");
    println!("\nDone!");

    Ok(())
}

// =============================================================================
// Demo Mode (no database connection)
// =============================================================================

fn demo_mode() -> anyhow::Result<()> {
    println!("=== Demo Mode (SQL Generation Only) ===\n");

    // Show what SQL would be generated

    // INSERT single
    let event = NewEvent { id: 1, user_id: 100, event_type: "click".into(), value: 1.5 };
    let insert = insert_into(events::table).values(&event);
    println!("INSERT (single):\n  {}\n", insert.to_sql_string());

    // INSERT batch
    let events = vec![
        NewEvent { id: 2, user_id: 100, event_type: "view".into(), value: 0.5 },
        NewEvent { id: 3, user_id: 200, event_type: "click".into(), value: 2.0 },
    ];
    let insert_batch = insert_into(events::table).values(events.as_slice());
    println!("INSERT (batch):\n  {}\n", insert_batch.to_sql_string());

    // SELECT with filter
    let select = events::table
        .filter(events::user_id.eq(100u32))
        .and_filter(events::event_type.eq("click"))
        .order_by(events::timestamp.desc())
        .limit(10);
    println!("SELECT:\n  {}\n", select.to_sql_string());

    // UPDATE
    let upd = update(events::table)
        .filter(events::id.eq(1u64))
        .set(events::value.eq(10.0));
    println!("UPDATE:\n  {}\n", upd.to_sql_string());

    // DELETE
    let del = delete(events::table).filter(events::id.eq(1u64));
    println!("DELETE:\n  {}\n", del.to_sql_string());

    // Aggregation with GROUP BY
    let agg = events::table
        .select((events::user_id, count(events::id), sum(events::value)))
        .group_by(events::user_id)
        .order_by(count(events::id).desc());
    println!("AGGREGATION:\n  {}\n", agg.to_sql_string());

    // ClickHouse-specific: FINAL
    println!("FINAL:\n  {}\n", events::table.final_().to_sql_string());

    // ClickHouse-specific: SAMPLE
    println!("SAMPLE 10%:\n  {}", events::table.sample(0.1).to_sql_string());

    Ok(())
}
