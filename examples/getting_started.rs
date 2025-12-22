//! Getting started with diesel-clickhouse.
//!
//! This example demonstrates the unified API that works with both HTTP and Native backends.
//! Just use `#[row]` attribute for your structs and the same code works everywhere!
//!
//! Run with: cargo run --example getting_started
//! Prerequisites: docker-compose up -d

use diesel_clickhouse::migrations::{EmbeddedMigrations, MigrationHarness, MigrationSource};
use diesel_clickhouse::prelude::*;
use diesel_clickhouse::Connection;
use diesel_clickhouse::{insert_into, update};
use include_dir::include_dir;

// =============================================================================
// Embed migrations from the migrations folder at compile time
// =============================================================================

static MIGRATIONS: EmbeddedMigrations =
    EmbeddedMigrations::new(include_dir!("$CARGO_MANIFEST_DIR/examples/migrations"));

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
// 2. Define your row types - use #[row] for optimized binary deserialization
// =============================================================================

/// For inserting new users - derives Insertable for SQL generation
#[row]
#[derive(Debug, Clone, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = users)]
pub struct NewUser {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
}

/// For inserting new posts
#[row]
#[derive(Debug, Clone, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = posts)]
pub struct NewPost {
    pub id: u64,
    pub user_id: u64,
    pub title: String,
    pub content: String,
}

/// For querying users - #[row] generates optimized binary deserialization
#[row]
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
    // Note: For DateTime, you might need to use u32 or String depending on format
    pub created_at: u32, // Unix timestamp
}

// =============================================================================
// 3. Use the API
// =============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // let url = std::env::var("CLICKHOUSE_URL")
    //     .unwrap_or_else(|_| "http://default:default@localhost:8123/test_db".to_string());

    let mut conn = Connection::http()
        .host("localhost")
        .user("default")
        .password("default")
        .database("test_db")
        .port(8123)
        .build()
        .await?;

    // Run migrations
    println!("Running migrations...");
    let applied = conn.run_pending_migrations(&MIGRATIONS).await?;
    println!("Applied {} migrations", applied.len());

    // INSERT users - idiomatic Diesel style with .execute(&conn)
    let new_users = vec![
        NewUser {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.com".into(),
            age: 30,
            active: true,
        },
        NewUser {
            id: 2,
            name: "Bob".into(),
            email: "bob@example.com".into(),
            age: 25,
            active: true,
        },
        NewUser {
            id: 3,
            name: "Charlie".into(),
            email: "charlie@example.com".into(),
            age: 35,
            active: false,
        },
    ];
    insert_into(users::table)
        .values(new_users.as_slice())
        .execute(&conn)
        .await?;
    println!("Inserted {} users", new_users.len());

    // INSERT posts
    let new_posts = vec![
        NewPost {
            id: 1,
            user_id: 1,
            title: "Hello World".into(),
            content: "My first post".into(),
        },
        NewPost {
            id: 2,
            user_id: 1,
            title: "Rust is great".into(),
            content: "I love Rust".into(),
        },
        NewPost {
            id: 3,
            user_id: 2,
            title: "ClickHouse tips".into(),
            content: "Fast analytics".into(),
        },
    ];
    insert_into(posts::table)
        .values(new_posts.as_slice())
        .execute(&conn)
        .await?;
    println!("Inserted {} posts", new_posts.len());

    // SELECT with filters - idiomatic style with .load(&conn)
    let active_users: Vec<User> = users::table
        .filter(users::active.eq(true))
        .and_filter(users::age.gt(18))
        .order_by(users::age.desc())
        .limit(10)
        .load(&conn)
        .await?;
    println!(
        "Active users over 25: {:?}",
        active_users.iter().map(|u| &u.name).collect::<Vec<_>>()
    );

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
    #[row]
    #[derive(Debug, Clone)]
    struct UserWithPost {
        name: String,
        title: String,
    }

    let users_with_posts: Vec<UserWithPost> = users::table
        .select((users::name, posts::title))
        .inner_join_on(posts::table, users::id.eq(posts::user_id))
        .filter(users::active.eq(true))
        .load(&conn)
        .await?;
    println!("Users with posts (flat):");
    for uwp in &users_with_posts {
        println!("  {} wrote: {}", uwp.name, uwp.title);
    }

    // JOIN with groupArray - accumulate posts per user
    #[row]
    #[derive(Debug, Clone)]
    struct UserWithPosts {
        id: u64,
        name: String,
        #[serde(rename = "groupArray(title)")]
        post_titles: Vec<String>, // Accumulate titles into array
        #[serde(rename = "count(id)")]
        post_count: u64,
    }

    let users_with_all_posts: Vec<UserWithPosts> = users::table
        .select((
            users::id,
            users::name,
            group_array(posts::title), // Accumulate titles into array
            count(posts::id),          // Count posts
        ))
        .inner_join_on(posts::table, users::id.eq(posts::user_id))
        .filter(users::active.eq(true))
        .group_by((users::id, users::name))
        .load(&conn)
        .await?;
    println!("\nUsers with all their posts (grouped):");
    for u in &users_with_all_posts {
        println!("  {} ({} posts): {:?}", u.name, u.post_count, u.post_titles);
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
