//! Getting started with diesel-clickhouse.
//!
//! This example demonstrates the unified API that works with both HTTP and Native backends.
//! Just use `#[row]` attribute for your structs and the same code works everywhere!
//!
//! Run with:
//!   # HTTP backend (default)
//!   cargo run --example getting_started --features http
//!
//!   # Native backend
//!   cargo run --example getting_started --features native
//!
//!   # With Arrow zero-copy (HTTP)
//!   cargo run --example getting_started --features "http,arrow"
//!
//!   # With true zero-copy streaming (Native Arrow)
//!   cargo run --example getting_started --features "native-arrow"
//!
//!   # With URL (auto-detects backend from scheme)
//!   CLICKHOUSE_URL=http://default:default@localhost:8123/test_db cargo run --example getting_started --features http
//!   CLICKHOUSE_URL=tcp://default:default@localhost:9000/test_db cargo run --example getting_started --features native
//!
//! Prerequisites: docker-compose up -d

use diesel_clickhouse::migrations::{EmbeddedMigrations, MigrationHarness};
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
/// Note: For DateTime, we use Utc with the clickhouse serde helper for HTTP compatibility.
#[row]
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
    #[cfg_attr(feature = "http", serde(with = "clickhouse::serde::chrono::datetime"))]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// 3. Use the API
// =============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connect using URL from environment, or default based on enabled features

    // Default connection based on enabled features
    #[cfg(all(feature = "native", not(feature = "http")))]
    {
        println!("Connecting via Native backend (default)");
        let mut conn = Connection::native()
            .host("localhost")
            .user("default")
            .password("default")
            .database("test_db")
            .port(9000)
            .build()
            .await?;
    }
    #[cfg(feature = "http")]
    {
        println!("Connecting via HTTP backend (default)");
        let mut conn = Connection::http()
            .host("localhost")
            .user("default")
            .password("default")
            .database("test_db")
            .port(8123)
            .build()
            .await?;
    }

    // Clean up any existing data from previous runs
    conn.execute("TRUNCATE TABLE IF EXISTS posts").await?;
    conn.execute("TRUNCATE TABLE IF EXISTS users").await?;

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
    // Use .alias() to give explicit names to aggregate columns.
    // This ensures column names are consistent across both HTTP and Native backends.
    #[row]
    #[derive(Debug, Clone)]
    struct UserWithPosts {
        id: u64,
        name: String,
        post_titles: Vec<String>,
        post_count: u64,
    }

    let users_with_all_posts: Vec<UserWithPosts> = users::table
        .select((
            users::id,
            users::name,
            group_array(posts::title).alias("post_titles"),
            count(posts::id).alias("post_count"),
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

    // =========================================================================
    // Zero-Copy Processing (Arrow-based)
    // =========================================================================
    //
    // For large datasets, zero-copy parsing avoids allocating strings/vectors
    // for each row. Instead, you get borrowed references directly into Arrow buffers.

    #[cfg(feature = "arrow")]
    {
        println!("\n--- Zero-Copy with Arrow (HTTP backend) ---");

        // Re-insert some data for the demo
        insert_into(users::table)
            .values(
                vec![
                    NewUser {
                        id: 10,
                        name: "ZeroCopy1".into(),
                        email: "zc1@test.com".into(),
                        age: 20,
                        active: true,
                    },
                    NewUser {
                        id: 11,
                        name: "ZeroCopy2".into(),
                        email: "zc2@test.com".into(),
                        age: 30,
                        active: true,
                    },
                    NewUser {
                        id: 12,
                        name: "ZeroCopy3".into(),
                        email: "zc3@test.com".into(),
                        age: 40,
                        active: false,
                    },
                ]
                .as_slice(),
            )
            .execute(&conn)
            .await?;

        // Process rows with zero-copy - no String allocations per row!
        let count = conn
            .load_zero_copy(
                "SELECT id, name, email, age FROM users WHERE id >= 10",
                |row| {
                    // These are borrowed references into the Arrow buffer - zero allocations!
                    let id = row.get_u64("id")?;
                    let name = row.get_str("name")?; // &str, not String
                    let email = row.get_str("email")?; // &str, not String
                    let age = row.get_u8("age")?;

                    println!(
                        "  [zero-copy] User {}: {} ({}) - age {}",
                        id, name, email, age
                    );
                    Ok(())
                },
            )
            .await?;
        println!("Processed {} rows with zero-copy", count);

        // You can also use the columnar Arrow API directly for analytics
        let result = conn
            .load_arrow("SELECT id, name, age FROM users WHERE id >= 10")
            .await?;
        println!(
            "Arrow result: {} rows, {} columns",
            result.num_rows(),
            result.num_columns()
        );
    }

    #[cfg(feature = "native-arrow")]
    {
        use diesel_clickhouse::native_arrow::NativeArrowConnection;
        use futures::StreamExt;

        println!("\n--- True Zero-Copy Streaming (Native Arrow) ---");

        // Connect via native protocol with Arrow support
        let native_conn = NativeArrowConnection::establish("localhost:9000", "test_db").await?;

        // Re-insert some data for the demo
        native_conn.execute("INSERT INTO users (id, name, email, age, active) VALUES (20, 'Stream1', 's1@test.com', 25, 1), (21, 'Stream2', 's2@test.com', 35, 1)").await?;

        // Stream RecordBatches as they arrive - true zero-copy streaming!
        println!("Streaming RecordBatches:");
        let mut stream = native_conn
            .stream_arrow("SELECT id, name, age FROM users WHERE id >= 20")
            .await?;
        while let Some(batch_result) = stream.next().await {
            let batch = batch_result?;
            println!("  Received batch with {} rows", batch.num_rows());
        }

        // Or use the row-by-row API with zero-copy access
        let count = native_conn
            .load_zero_copy("SELECT id, name, email FROM users WHERE id >= 20", |row| {
                let id = row.get_u64("id")?;
                let name = row.get_str("name")?; // Zero-copy borrow!
                println!("  [native zero-copy] User {}: {}", id, name);
                Ok(())
            })
            .await?;
        println!("Processed {} rows via native zero-copy streaming", count);
    }

    // Cleanup
    conn.execute("TRUNCATE TABLE posts").await?;
    conn.execute("TRUNCATE TABLE users").await?;
    println!("Done!");

    Ok(())
}
