//! Getting started with diesel-clickhouse.
//!
//! Run with: cargo run --example getting_started
//! Prerequisites: docker-compose up -d

use clickhouse::Row;
use serde::{Deserialize, Serialize};

use diesel_clickhouse::http::ClickHouseConnection;
use diesel_clickhouse::prelude::*;
use diesel_clickhouse::{update, insert_into};

// =============================================================================
// 1. Define your table schemas
// =============================================================================

diesel_clickhouse::table! {
    users (id, created_at) {
        id -> UInt64,
        name -> CHString,
        email -> CHString,
        age -> UInt8,
        active -> Bool,
        created_at -> DateTime,
    }
}

diesel_clickhouse::table! {
    posts (id, created_at) {
        id -> UInt64,
        user_id -> UInt64,
        title -> CHString,
        content -> CHString,
        created_at -> DateTime,
    }
}

// =============================================================================
// 2. Define your row types
// =============================================================================

/// For inserting new users - derives Insertable for SQL generation
#[derive(Debug, Clone, Row, Serialize, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = users)]
pub struct NewUser {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
}

/// For inserting new posts
#[derive(Debug, Clone, Row, Serialize, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = posts)]
pub struct NewPost {
    pub id: u64,
    pub user_id: u64,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Row, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
    #[serde(with = "clickhouse::serde::time::datetime")]
    pub created_at: time::OffsetDateTime,
}

// =============================================================================
// 3. Use the API
// =============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123/test_db".to_string());

    let conn = match ClickHouseConnection::new(&url).await {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Connection failed: {} (run: docker-compose up -d)\n", e);
            return Ok(demo_mode());
        }
    };

    // Create table
    conn.execute_raw(
        "CREATE TABLE IF NOT EXISTS users (
            id UInt64, name String, email String, age UInt8,
            active Bool DEFAULT true, created_at DateTime DEFAULT now()
        ) ENGINE = MergeTree() ORDER BY (id, created_at)",
    )
    .await?;

    // INSERT - fluent API with table
    let new_users = vec![
        NewUser { id: 1, name: "Alice".into(), email: "alice@example.com".into(), age: 30, active: true },
        NewUser { id: 2, name: "Bob".into(), email: "bob@example.com".into(), age: 25, active: true },
        NewUser { id: 3, name: "Charlie".into(), email: "charlie@example.com".into(), age: 35, active: false },
    ];
    let mut inserter = users::table.inserter::<NewUser>(&conn).await?;
    for row in &new_users {
        inserter.write(row).await?;
    }
    inserter.end().await?;

    // SELECT with filters
    let active_users: Vec<User> = conn.query(
        users::table
            .filter(users::active.eq(true))
            .and_filter(users::age.gt(25))
            .order_by(users::age.desc())
            .limit(10)
    ).fetch_all().await?;
    println!("Active users over 25: {:?}", active_users.iter().map(|u| &u.name).collect::<Vec<_>>());

    // SELECT first
    let alice: User = conn.query(
        users::table.filter(users::name.eq("Alice"))
    ).fetch_one().await?;
    println!("Found: {} ({})", alice.name, alice.email);

    // SELECT optional
    let maybe_user: Option<User> = conn.query(
        users::table.filter(users::name.eq("Unknown"))
    ).fetch_optional().await?;
    println!("Unknown user: {:?}", maybe_user);

    // UPDATE
    update(users::table)
        .filter(users::id.eq(1u64))
        .set(users::name.eq("Alice Updated"))
        .execute(&conn)
        .await?;

    // Cleanup
    conn.execute_raw("TRUNCATE TABLE users").await?;
    println!("Done!");

    Ok(())
}

fn demo_mode() {
    println!("=== Demo Mode (SQL Generation) ===\n");

    // SELECT
    let select = users::table
        .filter(users::active.eq(true))
        .and_filter(users::age.gt(25))
        .order_by(users::age.desc())
        .limit(10);
    println!("SELECT:\n  {}\n", select.to_sql_string());

    // INSERT - single row (NEW!)
    let new_user = NewUser {
        id: 1,
        name: "Alice".into(),
        email: "alice@example.com".into(),
        age: 30,
        active: true,
    };
    let insert_one = insert_into(users::table).values(&new_user);
    println!("INSERT (single):\n  {}\n", insert_one.to_sql_string());

    // INSERT - multiple rows (NEW!)
    let new_users = vec![
        NewUser { id: 2, name: "Bob".into(), email: "bob@example.com".into(), age: 25, active: true },
        NewUser { id: 3, name: "Charlie".into(), email: "charlie@example.com".into(), age: 35, active: false },
    ];
    let insert_batch = insert_into(users::table).values(new_users.as_slice());
    println!("INSERT (batch):\n  {}\n", insert_batch.to_sql_string());

    // UPDATE
    let upd = update(users::table)
        .filter(users::id.eq(1u64))
        .set(users::name.eq("New Name"));
    println!("UPDATE:\n  {}\n", upd.to_sql_string());

    // JOIN - inner join with custom ON clause (NEW!)
    // Use .select() to start a SelectStatement, then add join
    let join_query = users::table
        .select(users::star)
        .inner_join_on(posts::table, users::id.eq(posts::user_id))
        .filter(users::active.eq(true))
        .limit(10);
    println!("INNER JOIN:\n  {}\n", join_query.to_sql_string());

    // JOIN - left join (NEW!)
    let left_join = users::table
        .select(users::star)
        .left_join_on(posts::table, users::id.eq(posts::user_id))
        .filter(users::age.gt(18));
    println!("LEFT JOIN:\n  {}\n", left_join.to_sql_string());

    // ClickHouse-specific
    println!("FINAL:\n  {}\n", users::table.final_().to_sql_string());
    println!("SAMPLE:\n  {}", users::table.sample(0.1).to_sql_string());
}
