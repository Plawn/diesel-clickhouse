//! Getting started with diesel-clickhouse.
//!
//! This example demonstrates the unified API that works with both HTTP and Native backends.
//! Use `#[clickhouse_row]` attribute for your structs and the same code works everywhere!
//!
//! Run with:
//!   cargo run --example getting_started
//!
//! Prerequisites: docker-compose up -d

// The `clickhouse_row` attribute macro is from the same crate as the derives,
// so Rust's legacy_derive_helpers lint incorrectly flags it as a derive helper.
// This is a false positive - `clickhouse_row` is a standalone attribute macro.
#![allow(legacy_derive_helpers)]

use diesel_clickhouse::migrations::{EmbeddedMigrations, MigrationHarness};
use diesel_clickhouse::prelude::*;
use diesel_clickhouse::Connection;
use diesel_clickhouse::ConnectionBuilder;
use diesel_clickhouse::{insert_into, update};
use include_dir::include_dir;

// JSON support (ClickHouse 25.11+)
#[cfg(feature = "json")]
use diesel_clickhouse::types::JsonTyped;
#[cfg(feature = "json")]
use serde::{Deserialize, Serialize};

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

// Table with JSON column (ClickHouse 25.11+)
#[cfg(feature = "json")]
diesel_clickhouse::table! {
    events (id, created_at) {
        id -> UInt64,
        event_type -> CHString,
        metadata -> Json,
        created_at -> DateTime,
    }
}

// =============================================================================
// 2. Define your row types - use #[clickhouse_row] for optimized binary deserialization
// =============================================================================

/// For inserting new users - #[clickhouse_row] adds backend serialization (serde + ToNativeBlock),
/// then #[derive(Insertable)] adds SQL generation for INSERT statements.
#[clickhouse_row]
#[derive(Debug, Clone, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table_name = users)]
pub struct NewUser {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
}

/// For inserting new posts
#[clickhouse_row]
#[derive(Debug, Clone, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table_name = posts)]
pub struct NewPost {
    pub id: u64,
    pub user_id: u64,
    pub title: String,
    pub content: String,
}


/// For querying users - #[clickhouse_row(table_name = X)] with Selectable generates
/// optimized binary deserialization AND compile-time type verification against the table schema.
/// Note: For DateTime, we use Utc with the clickhouse serde helper for HTTP compatibility.
#[clickhouse_row(table_name = users)]
#[derive(Debug, Clone, diesel_clickhouse::Queryable, diesel_clickhouse::Selectable)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
    #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// JSON types (ClickHouse 25.11+)
// =============================================================================

/// Define your JSON schema as a Rust struct
#[cfg(feature = "json")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventMetadata {
    pub action: String,
    pub user_agent: String,
    pub tags: Vec<String>,
}

/// For inserting events with typed JSON metadata
#[cfg(feature = "json")]
#[clickhouse_row]
#[derive(Debug, Clone, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table_name = events)]
pub struct NewEvent {
    pub id: u64,
    pub event_type: String,
    pub metadata: JsonTyped<EventMetadata>,
}

/// For querying events - JsonTyped<T> implements Deref for direct field access.
/// The table! macro automatically handles CAST for JSON columns.
#[cfg(feature = "json")]
#[clickhouse_row]
#[derive(Debug, Clone, diesel_clickhouse::Queryable)]
pub struct Event {
    pub id: u64,
    pub event_type: String,
    pub metadata: JsonTyped<EventMetadata>,
}

// =============================================================================
// 3. Use the API
// =============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // =========================================================================
    // Connect via HTTP backend
    // =========================================================================
    println!("=== HTTP Backend Demo ===\n");
    println!("Connecting via HTTP backend...");
    let mut http_conn = Connection::http()
        .host("localhost")
        .user("default")
        .password("default")
        .database("test_db")
        .port(8123)
        .build()
        .await?;

    // Clean up any existing data from previous runs
    http_conn.execute("TRUNCATE TABLE IF EXISTS posts").await?;
    http_conn.execute("TRUNCATE TABLE IF EXISTS users").await?;

    // Run migrations
    println!("Running migrations...");
    let applied = http_conn.run_pending_migrations(&MIGRATIONS).await?;
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
        .insert(&http_conn)
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
        .insert(&http_conn)
        .await?;
    println!("Inserted {} posts", new_posts.len());

    // SELECT with filters
    let active_users: Vec<User> = users::table
        .filter(users::active.eq(true))
        .and_filter(users::age.gt(18))
        .order_by(users::age.desc())
        .limit(10)
        .load(&http_conn)
        .await?;
    println!(
        "Active users over 25: {:?}",
        active_users.iter().map(|u| &u.name).collect::<Vec<_>>()
    );

    // SELECT first
    let alice: User = users::table
        .filter(users::name.eq("Alice"))
        .first(&http_conn)
        .await?;
    println!("Found: {} ({})", alice.name, alice.email);

    // SELECT optional
    let maybe_user: Option<User> = users::table
        .filter(users::name.eq("Unknown"))
        .get_result(&http_conn)
        .await?;
    println!("Unknown user: {:?}", maybe_user);

    // JOIN - users with their posts (one row per post)
    // SQL types are automatically deduced from Rust field types via HasSqlType trait:
    //   String -> CHString, u64 -> UInt64, Vec<String> -> Array<CHString>, etc.
    #[clickhouse_row]
    #[derive(Debug, Clone, diesel_clickhouse::Queryable)]
    struct UserWithPost {
        name: String,
        title: String,
    }


    let users_with_posts: Vec<UserWithPost> = users::table
        .select((users::name, posts::title))
        .inner_join_on(posts::table, users::id.eq(posts::user_id))
        .filter(users::active.eq(true))
        .load(&http_conn)
        .await?;
    println!("Users with posts (flat):");
    for uwp in &users_with_posts {
        println!("  {} wrote: {}", uwp.name, uwp.title);
    }

    // JOIN with groupArray - accumulate posts per user
    // Use .alias() to give explicit names to aggregate columns.
    // This ensures column names are consistent across both HTTP and Native backends.
    //
    // For custom selects, use #[diesel_clickhouse(column_name = "...")] to map struct fields to query column names.
    // This generates serde(rename) for HTTP and uses the correct column name for Native backend.
    // IMPORTANT: The column_name must match the .alias() in your query!
    #[clickhouse_row]
    #[derive(Debug, Clone, diesel_clickhouse::Queryable)]
    struct UserWithPosts {
        id: u64,
        name: String,
        #[diesel_clickhouse(column_name = "post_titles")]
        post_titles: Vec<String>,
        #[diesel_clickhouse(column_name = "post_count")]
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
        .load(&http_conn)
        .await?;
    println!("\nUsers with all their posts (grouped):");
    for u in &users_with_all_posts {
        println!("  {} ({} posts): {:?}", u.name, u.post_count, u.post_titles);
    }

    // UPDATE
    update(users::table)
        .filter(users::id.eq(1u64))
        .set(users::name.eq("Alice Updated"))
        .execute(&http_conn)
        .await?;

    // =========================================================================
    // Zero-Copy Processing (Arrow-based) - HTTP
    // =========================================================================
    //
    // For large datasets, zero-copy parsing avoids allocating strings/vectors
    // for each row. Instead, you get borrowed references directly into Arrow buffers.

    println!("\n--- Zero-Copy with Arrow (HTTP) ---");

    // Re-insert some data for the demo
    let zero_copy_users = vec![
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
    ];
    insert_into(users::table)
        .values(zero_copy_users.as_slice())
        .insert(&http_conn)
        .await?;

    // Process rows with zero-copy - no String allocations per row!
    let _count = http_conn
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
    println!("Processed {} rows with zero-copy", _count);

    // You can also use the columnar Arrow API directly for analytics
    let result = http_conn
        .load_arrow("SELECT id, name, age FROM users WHERE id >= 10")
        .await?;
    println!(
        "Arrow result: {} rows, {} columns",
        result.num_rows(),
        result.num_columns()
    );

    // =========================================================================
    // Native Backend Demo
    // =========================================================================
    println!("\n\n=== Native Backend Demo ===\n");
    println!("Connecting via Native backend...");
    let native_conn = Connection::native()
        .host("localhost")
        .user("default")
        .password("default")
        .database("test_db")
        .port(9000)
        .build()
        .await?;

    // Query using native backend
    let users: Vec<User> = users::table
        .filter(users::active.eq(true))
        .limit(5)
        .load(&native_conn)
        .await?;
    println!("Loaded {} users via Native backend", users.len());
    insert_into(users::table)
        .values(zero_copy_users.as_slice())
        .insert(&native_conn)
        .await?;
    // =========================================================================
    // Streaming Demo - Memory-efficient processing for large datasets
    // =========================================================================
    println!("\n\n=== Streaming Demo ===\n");

    // Using .stream() - returns an async iterator
    // This is useful when you need more control over iteration (break, continue, etc.)
    println!("--- HTTP Backend: stream() ---");
    let mut stream = http_conn
        .stream::<User, _>(users::table.filter(users::active.eq(true)))
        .await?;
    while let Some(user) = stream.next().await? {
        println!("  [HTTP stream()] User: {} (age )", user.name);
    }

    println!("\n--- Native Backend: stream() ---");
    let mut stream = native_conn
        .stream::<User, _>(users::table.filter(users::active.eq(true)))
        .await?;
    while let Some(user) = stream.next().await? {
        println!("  [Native stream()] User: {} (age {})", user.name, user.age);
    }

    // Using .stream_for_each() - callback-based streaming
    // HTTP Streaming - true row-by-row streaming, O(1) memory
    println!("\n--- HTTP Backend: stream_for_each ---");
    let mut http_count = 0u64;
    http_conn
        .stream_for_each(users::table.filter(users::active.eq(true)), |user: User| {
            println!("  [HTTP stream] User: {} (age {})", user.name, user.age);
            http_count += 1;
            Ok(())
        })
        .await?;
    println!("Streamed {} users via HTTP\n", http_count);

    // Native Streaming - true block-by-block streaming, O(block_size) memory
    // Uses clickhouse-rs stream_blocks() for true network streaming
    println!("--- Native Backend: stream_for_each ---");
    let mut native_count = 0u64;
    native_conn
        .stream_for_each(users::table.filter(users::active.eq(true)), |user: User| {
            println!("  [Native stream] User: {} (age {})", user.name, user.age);
            native_count += 1;
            Ok(())
        })
        .await?;
    println!("Streamed {} users via Native\n", native_count);
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
        .load(&native_conn)
        .await?;
    // Async callback version - useful for I/O operations per row
    println!("--- Async Streaming (HTTP) ---");
    http_conn
        .stream_for_each_async(
            users::table.filter(users::id.gt(10u64)),
            |user: User| async move {
                // Simulate async processing (e.g., HTTP call, database write)
                println!("  [async] Processing user: {}", user.name);
                Ok(())
            },
        )
        .await?;

    // =========================================================================
    // JSON Column Support (ClickHouse 25.11+)
    // =========================================================================
    #[cfg(feature = "json")]
    {
        println!("\n\n=== JSON Column Demo (ClickHouse 25.11+) ===\n");

        // Drop and recreate the events table with a JSON column
        // (ensures clean schema for the demo)
        native_conn.execute("DROP TABLE IF EXISTS events").await?;
        native_conn
            .execute(
                "CREATE TABLE events (
                    id UInt64,
                    event_type String,
                    metadata JSON,
                    created_at DateTime DEFAULT now()
                ) ENGINE = MergeTree()
                ORDER BY (id, created_at)",
            )
            .await?;

        // Insert events with typed JSON metadata using the API
        let new_events = vec![
            NewEvent {
                id: 1,
                event_type: "page_view".into(),
                metadata: JsonTyped::new(EventMetadata {
                    action: "view".into(),
                    user_agent: "Mozilla/5.0".into(),
                    tags: vec!["homepage".into(), "organic".into()],
                }),
            },
            NewEvent {
                id: 2,
                event_type: "button_click".into(),
                metadata: JsonTyped::new(EventMetadata {
                    action: "click".into(),
                    user_agent: "Chrome/120".into(),
                    tags: vec!["cta".into(), "signup".into()],
                }),
            },
            NewEvent {
                id: 3,
                event_type: "form_submit".into(),
                metadata: JsonTyped::new(EventMetadata {
                    action: "submit".into(),
                    user_agent: "Safari/17".into(),
                    tags: vec!["contact".into(), "lead".into()],
                }),
            },
        ];

        // Insert JSON data using the standard insert pattern
        // The InsertDsl automatically uses SQL-based insert for JSON columns
        insert_into(events::table)
            .values(new_events.as_slice())
            .insert(&native_conn)
            .await?;
        insert_into(events::table)
            .values(new_events.as_slice())
            .insert(&http_conn)
            .await?;
        println!("Inserted {} events with JSON metadata", new_events.len());

        // Query events with typed JSON - JsonTyped<T> implements Deref for direct field access!
        // The table! macro automatically adds CAST(metadata AS String) for JSON columns
        let loaded_events: Vec<Event> = events::table
            .select((events::id, events::event_type, events::metadata))
            .load(&native_conn)
            .await?;

        println!("\nLoaded events with typed JSON:");
        for event in &loaded_events {
            // Direct field access via Deref - no need for .0 or .into_inner()!
            println!(
                "  Event {}: {} - action='{}', user_agent='{}', tags={:?}",
                event.id,
                event.event_type,
                event.metadata.action,      // Direct access!
                event.metadata.user_agent,  // Direct access!
                event.metadata.tags         // Direct access!
            );
        }

        // Filter by event type
        let click_events: Vec<Event> = events::table
            .select((events::id, events::event_type, events::metadata))
            .filter(events::event_type.eq("button_click"))
            .load(&native_conn)
            .await?;
        println!(
            "\nButton click events: {} (tags: {:?})",
            click_events.len(),
            click_events.first().map(|e| &e.metadata.tags)
        );

        // You can also use serde_json::Value for dynamic JSON
        // (useful when the JSON structure varies)
        #[clickhouse_row]
        #[derive(Debug, Clone, diesel_clickhouse::Queryable)]
        struct EventDynamic {
            id: u64,
            event_type: String,
            metadata: serde_json::Value,
        }

        let dynamic_events: Vec<EventDynamic> = events::table
            .select((events::id, events::event_type, events::metadata))
            .load(&native_conn)
            .await?;
        println!("\nDynamic JSON access:");
        for event in &dynamic_events {
            if let Some(action) = event.metadata.get("action") {
                println!("  Event {}: action = {}", event.id, action);
            }
        }

        // Cleanup events table
        native_conn.execute("DROP TABLE IF EXISTS events").await?;
        println!("\nJSON demo complete!");
    }

    #[cfg(not(feature = "json"))]
    {
        println!("\n(JSON demo skipped - enable with: --features json)");
    }

    // Cleanup
    http_conn.execute("TRUNCATE TABLE posts").await?;
    http_conn.execute("TRUNCATE TABLE users").await?;
    println!("\nDone!");

    Ok(())
}
