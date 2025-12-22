//! Getting started with diesel-clickhouse.
//!
//! This example demonstrates the unified API that works with both HTTP and Native backends.
//! Just use `#[derive(Row)]` for your structs and the same code works everywhere!
//!
//! Run with: cargo run --example getting_started
//! Prerequisites: docker-compose up -d

use diesel_clickhouse::prelude::*;
use diesel_clickhouse::Connection;
use diesel_clickhouse::{update, insert_into};
use diesel_clickhouse::migrations::{EmbeddedMigrations, MigrationHarness, MigrationSource};
use include_dir::include_dir;

// =============================================================================
// Embed migrations from the migrations folder at compile time
// =============================================================================

static MIGRATIONS: EmbeddedMigrations = EmbeddedMigrations::new(
    include_dir!("$CARGO_MANIFEST_DIR/examples/migrations")
);

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
// 2. Define your row types - use #[derive(Row)] for unified HTTP + Native support
// =============================================================================

/// For inserting new users - derives Insertable for SQL generation
/// Row generates serde::Serialize and serde::Deserialize automatically
#[derive(Debug, Clone, Row, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = users)]
pub struct NewUser {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
}


/// For inserting new posts
#[derive(Debug, Clone, Row, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = posts)]
pub struct NewPost {
    pub id: u64,
    pub user_id: u64,
    pub title: String,
    pub content: String,
}

/// For querying users - Row generates all needed traits
#[derive(Debug, Clone, Row)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
    // Note: For DateTime, you might need to use u32 or String depending on format
    pub created_at: u32,  // Unix timestamp
}

// =============================================================================
// 3. Use the API
// =============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123/test_db".to_string());

    let conn = match Connection::establish(&url).await {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Connection failed: {} (run: docker-compose up -d)\n", e);
            return Ok(demo_mode());
        }
    };

    // Run migrations - access underlying HTTP connection for migrations
    println!("Running migrations...");
    if let Some(mut http_conn) = conn.as_http().cloned() {
        let applied = http_conn.run_pending_migrations(&MIGRATIONS).await?;
        println!("Applied {} migrations", applied.len());
    }

    // INSERT users - idiomatic Diesel style with .execute(&conn)
    let new_users = vec![
        NewUser { id: 1, name: "Alice".into(), email: "alice@example.com".into(), age: 30, active: true },
        NewUser { id: 2, name: "Bob".into(), email: "bob@example.com".into(), age: 25, active: true },
        NewUser { id: 3, name: "Charlie".into(), email: "charlie@example.com".into(), age: 35, active: false },
    ];
    insert_into(users::table)
        .values(new_users.as_slice())
        .execute(&conn)
        .await?;
    println!("Inserted {} users", new_users.len());

    // INSERT posts
    let new_posts = vec![
        NewPost { id: 1, user_id: 1, title: "Hello World".into(), content: "My first post".into() },
        NewPost { id: 2, user_id: 1, title: "Rust is great".into(), content: "I love Rust".into() },
        NewPost { id: 3, user_id: 2, title: "ClickHouse tips".into(), content: "Fast analytics".into() },
    ];
    insert_into(posts::table)
        .values(new_posts.as_slice())
        .execute(&conn)
        .await?;
    println!("Inserted {} posts", new_posts.len());

    // SELECT with filters - idiomatic style with .load(&conn)
    let active_users: Vec<User> = users::table
        .filter(users::active.eq(true))
        .and_filter(users::age.gt(25))
        .order_by(users::age.desc())
        .limit(10)
        .load(&conn)
        .await?;
    println!("Active users over 25: {:?}", active_users.iter().map(|u| &u.name).collect::<Vec<_>>());

    // SELECT first - using .first(&conn)
    let alice: User = users::table
        .filter(users::name.eq("Alice"))
        .first(&conn)
        .await?;
    println!("Found: {} ({})", alice.name, alice.email);

    // SELECT optional - using .get_result(&conn)
    let maybe_user: Option<User> = users::table
        .filter(users::name.eq("Unknown"))
        .get_result(&conn)
        .await?;
    println!("Unknown user: {:?}", maybe_user);

    // JOIN - users with their posts (one row per post)
    #[derive(Debug, Clone, Row)]
    struct UserWithPost {
        user_name: String,
        post_title: String,
    }

    let users_with_posts: Vec<UserWithPost> = users::table
        .select((users::name, posts::title))
        .inner_join_on(posts::table, users::id.eq(posts::user_id))
        .filter(users::active.eq(true))
        .load(&conn)
        .await?;
    println!("Users with posts (flat):");
    for uwp in &users_with_posts {
        println!("  {} wrote: {}", uwp.user_name, uwp.post_title);
    }

    // JOIN with groupArray - accumulate posts per user
    #[derive(Debug, Clone, Row)]
    struct UserWithPosts {
        user_id: u64,
        user_name: String,
        post_titles: Vec<String>,  // All posts collected into array
        post_count: u64,
    }

    let users_with_all_posts: Vec<UserWithPosts> = users::table
        .select((
            users::id,
            users::name,
            group_array(posts::title),  // Accumulate titles into array
            count(posts::id),           // Count posts
        ))
        .inner_join_on(posts::table, users::id.eq(posts::user_id))
        .filter(users::active.eq(true))
        .group_by((users::id, users::name))
        .load(&conn)
        .await?;
    println!("\nUsers with all their posts (grouped):");
    for u in &users_with_all_posts {
        println!("  {} ({} posts): {:?}", u.user_name, u.post_count, u.post_titles);
    }

    // UPDATE - idiomatic style
    update(users::table)
        .filter(users::id.eq(1u64))
        .set(users::name.eq("Alice Updated"))
        .execute(&conn)
        .await?;

    // Cleanup
    conn.execute("TRUNCATE TABLE posts").await?;
    conn.execute("TRUNCATE TABLE users").await?;
    println!("Done!");

    Ok(())
}

fn demo_mode() {
    println!("=== Demo Mode (SQL Generation) ===\n");

    // Show embedded migrations
    println!("=== Embedded Migrations ===");
    for m in MIGRATIONS.migrations().unwrap() {
        println!("  {} - {}", m.version, m.name);
    }
    println!();

    // SELECT - idiomatic chain
    let select = users::table
        .filter(users::active.eq(true))
        .and_filter(users::age.gt(25))
        .order_by(users::age.desc())
        .limit(10);
    println!("SELECT:\n  {}\n", select.to_sql_string().unwrap_or_else(|e| format!("Error: {}", e)));

    // INSERT - single row
    let new_user = NewUser {
        id: 1,
        name: "Alice".into(),
        email: "alice@example.com".into(),
        age: 30,
        active: true,
    };
    let insert_one = insert_into(users::table).values(&new_user);
    println!("INSERT (single):\n  {}\n", insert_one.to_sql_string().unwrap_or_else(|e| format!("Error: {}", e)));

    // INSERT - multiple rows
    let new_users = vec![
        NewUser { id: 2, name: "Bob".into(), email: "bob@example.com".into(), age: 25, active: true },
        NewUser { id: 3, name: "Charlie".into(), email: "charlie@example.com".into(), age: 35, active: false },
    ];
    let insert_batch = insert_into(users::table).values(new_users.as_slice());
    println!("INSERT (batch):\n  {}\n", insert_batch.to_sql_string().unwrap_or_else(|e| format!("Error: {}", e)));

    // UPDATE
    let upd = update(users::table)
        .filter(users::id.eq(1u64))
        .set(users::name.eq("New Name"));
    println!("UPDATE:\n  {}\n", upd.to_sql_string().unwrap_or_else(|e| format!("Error: {}", e)));

    // JOIN
    let join_query = users::table
        .select(users::star)
        .inner_join_on(posts::table, users::id.eq(posts::user_id))
        .filter(users::active.eq(true))
        .limit(10);
    println!("INNER JOIN:\n  {}\n", join_query.to_sql_string().unwrap_or_else(|e| format!("Error: {}", e)));

    // ClickHouse-specific
    println!("FINAL:\n  {}\n", users::table.final_().to_sql_string().unwrap_or_else(|e| format!("Error: {}", e)));
    println!("SAMPLE:\n  {}", users::table.sample(0.1).to_sql_string().unwrap_or_else(|e| format!("Error: {}", e)));
}
