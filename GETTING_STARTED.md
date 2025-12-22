# Getting Started with diesel-clickhouse

A quick guide to get you up and running with diesel-clickhouse in 5 minutes.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
diesel-clickhouse = { version = "0.1", features = ["http", "migrations"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## 1. Define Your Schema

Use the `table!` macro to define your ClickHouse table schema:

```rust
use diesel_clickhouse::prelude::*;

diesel_clickhouse::table! {
    users (id, created_at) {  // (primary key columns)
        id -> UInt64,
        name -> CHString,
        email -> CHString,
        age -> UInt8,
        active -> Bool,
        created_at -> DateTime,
    }
}
```

## 2. Define Row Types

Use `#[derive(Row)]` for query results and `#[derive(Insertable)]` for inserts:

```rust
/// For inserting new users
#[derive(Debug, Clone, Row, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = users)]
pub struct NewUser {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
}

/// For querying users
#[derive(Debug, Clone, Row)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub active: bool,
    pub created_at: u32,  // Unix timestamp
}
```

## 3. Connect to ClickHouse

```rust
use diesel_clickhouse::Connection;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // HTTP connection (recommended)
    let conn = Connection::establish("http://localhost:8123/default").await?;

    // Or with credentials
    // let conn = Connection::establish("http://user:pass@localhost:8123/mydb").await?;

    Ok(())
}
```

## 4. Create the Table

```rust
conn.execute(r#"
    CREATE TABLE IF NOT EXISTS users (
        id UInt64,
        name String,
        email String,
        age UInt8,
        active Bool,
        created_at DateTime DEFAULT now()
    ) ENGINE = MergeTree()
    ORDER BY (id, created_at)
"#).await?;
```

## 5. Insert Data

### Single row

```rust
use diesel_clickhouse::insert_into;

let user = NewUser {
    id: 1,
    name: "Alice".into(),
    email: "alice@example.com".into(),
    age: 30,
    active: true,
};

insert_into(users::table)
    .values(&user)
    .execute(&conn)
    .await?;
```

### Multiple rows

```rust
let new_users = vec![
    NewUser { id: 2, name: "Bob".into(), email: "bob@example.com".into(), age: 25, active: true },
    NewUser { id: 3, name: "Charlie".into(), email: "charlie@example.com".into(), age: 35, active: false },
];

insert_into(users::table)
    .values(new_users.as_slice())
    .execute(&conn)
    .await?;
```

## 6. Query Data

### Select all

```rust
let all_users: Vec<User> = users::table
    .load(&conn)
    .await?;
```

### With filters

```rust
let active_users: Vec<User> = users::table
    .filter(users::active.eq(true).and(users::age.gt(25)))
    .order_by(users::name.asc())
    .limit(10)
    .load(&conn)
    .await?;

// Alternative: chain with and_filter()
let active_users: Vec<User> = users::table
    .filter(users::active.eq(true))
    .and_filter(users::age.gt(25))
    .order_by(users::name.asc())
    .limit(10)
    .load(&conn)
    .await?;
```

### Get first row

```rust
let user: User = users::table
    .filter(users::id.eq(1))
    .first(&conn)
    .await?;
```

### Optional (might not exist)

```rust
let maybe_user: Option<User> = users::table
    .filter(users::id.eq(999))
    .get_result(&conn)
    .await?;
```

## 7. Update Data

```rust
use diesel_clickhouse::update;

update(users::table)
    .filter(users::id.eq(1))
    .set(users::name.eq("Alice Updated"))
    .execute(&conn)
    .await?;
```

## Complete Example

```rust
use diesel_clickhouse::prelude::*;
use diesel_clickhouse::{Connection, insert_into};

diesel_clickhouse::table! {
    users (id, created_at) {
        id -> UInt64,
        name -> CHString,
        age -> UInt8,
        active -> Bool,
        created_at -> DateTime,
    }
}

#[derive(Debug, Row, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = users)]
struct NewUser {
    id: u64,
    name: String,
    age: u8,
    active: bool,
}

#[derive(Debug, Row)]
struct User {
    id: u64,
    name: String,
    age: u8,
    active: bool,
    created_at: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conn = Connection::establish("http://localhost:8123/default").await?;

    // Insert
    let user = NewUser { id: 1, name: "Alice".into(), age: 30, active: true };
    insert_into(users::table)
        .values(&user)
        .execute(&conn)
        .await?;

    // Query
    let active_users: Vec<User> = users::table
        .filter(users::active.eq(true).and(users::age.gt(18)))
        .load(&conn)
        .await?;

    println!("Found {} active users", active_users.len());
    Ok(())
}
```

## ClickHouse-Specific Features

### FINAL (deduplicate ReplacingMergeTree)

```rust
let users: Vec<User> = users::table
    .final_()
    .load(&conn)
    .await?;
```

### SAMPLE (random sampling)

```rust
let sample: Vec<User> = users::table
    .sample(0.1)  // 10% of data
    .load(&conn)
    .await?;
```

### PREWHERE (optimized filtering)

```rust
let users: Vec<User> = users::table
    .prewhere(users::active.eq(true))
    .load(&conn)
    .await?;
```

## Next Steps

- Check out `examples/getting_started.rs` for a complete working example
- See `examples/migrations.rs` for database migrations
- Read about [connection pooling](examples/connection_pooling.rs) for production use
