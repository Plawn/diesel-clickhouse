# Backends

diesel-clickhouse supports two backends for connecting to ClickHouse via a unified API:

| Backend | Protocol | Ports | Performance | Use Case |
|---------|----------|-------|-------------|----------|
| **HTTP** | HTTP/HTTPS | 8123 / 8443 | Good | Default, easy to deploy |
| **Native** | Binary TCP | 9000 / 9440 | Better | High performance |

## Unified API (Recommended)

The unified `Connection` API works with both backends:

```rust
use diesel_clickhouse::prelude::*;
use diesel_clickhouse::Connection;

// HTTP backend
let http_conn = Connection::http()
    .host("localhost")
    .port(8123)
    .user("default")
    .password("default")
    .database("mydb")
    .build()
    .await?;

// Native backend
let native_conn = Connection::native()
    .host("localhost")
    .port(9000)
    .user("default")
    .password("default")
    .database("mydb")
    .build()
    .await?;

// Or via URL
let conn = Connection::establish("http://localhost:8123/default").await?;
let conn = Connection::establish("tcp://localhost:9000/default").await?;

// Same API for both!
conn.execute("CREATE TABLE test (id UInt64) ENGINE = Memory").await?;
```

## HTTP Backend

The HTTP backend uses ClickHouse's REST interface. It's the default choice because it works through proxies, load balancers, and firewalls.

### Enabling

```toml
[dependencies]
diesel-clickhouse = { version = "0.1", features = ["http"] }

# With TLS
diesel-clickhouse = { version = "0.1", features = ["http", "rustls-tls"] }
# or
diesel-clickhouse = { version = "0.1", features = ["http", "native-tls"] }
```

### Connection

```rust
use diesel_clickhouse::Connection;

// Builder pattern (recommended)
let conn = Connection::http()
    .host("localhost")
    .port(8123)
    .user("default")
    .password("default")
    .database("mydb")
    .build()
    .await?;

// With TLS (port 8443)
let conn = Connection::establish("https://localhost:8443/default").await?;
```

### Queries and Inserts

```rust
use diesel_clickhouse::prelude::*;

#[derive(Debug, Clone, ClickHouseRow, Queryable)]
struct User {
    id: u64,
    name: String,
}

#[derive(Debug, Clone, ClickHouseRow, Insertable)]
#[diesel_clickhouse(table_name = users)]
struct NewUser {
    id: u64,
    name: String,
}

// Query with the query builder
let users: Vec<User> = users::table
    .filter(users::active.eq(true))
    .load(&conn)
    .await?;

// Idiomatic Diesel-style insert
insert_into(users::table)
    .values(&[
        NewUser { id: 1, name: "Alice".into() },
        NewUser { id: 2, name: "Bob".into() },
    ])
    .insert(&conn)
    .await?;
```

---

## Native Backend

The Native backend uses ClickHouse's binary TCP protocol. It offers better performance because it avoids HTTP overhead and uses an optimized binary format.

### Enabling

```toml
[dependencies]
diesel-clickhouse = { version = "0.1", features = ["native"] }

# With TLS
diesel-clickhouse = { version = "0.1", features = ["native", "native-tls-native"] }
```

### Connection

```rust
use diesel_clickhouse::Connection;

// Builder pattern (recommended)
let conn = Connection::native()
    .host("localhost")
    .port(9000)
    .user("default")
    .password("default")
    .database("mydb")
    .build()
    .await?;

// Via URL
let conn = Connection::establish("tcp://localhost:9000/default").await?;

// With options in URL
let conn = Connection::establish(
    "tcp://admin:secret@clickhouse.example.com:9000/analytics?compression=lz4"
).await?;
```

### Queries and Inserts

```rust
use diesel_clickhouse::prelude::*;

#[derive(Debug, Clone, ClickHouseRow, Queryable)]
struct User {
    id: u64,
    name: String,
}

// Same API as HTTP!
let users: Vec<User> = users::table
    .filter(users::active.eq(true))
    .load(&conn)
    .await?;

insert_into(users::table)
    .values(&new_users)
    .insert(&conn)
    .await?;
```

---

## Detailed Comparison

### Performance

| Operation | HTTP | Native | Difference |
|-----------|------|--------|------------|
| Connection latency | ~5ms | ~2ms | Native 2x faster |
| Per-request overhead | ~1ms | ~0.1ms | Native 10x less overhead |
| INSERT throughput | Good | Excellent | Native ~30% faster |
| SELECT throughput | Good | Excellent | Native ~20% faster |

*Note: Numbers vary based on network and data size.*

### Features

| Feature | HTTP | Native |
|---------|------|--------|
| Diesel query builder | ✅ | ✅ |
| Unified Connection API | ✅ | ✅ |
| TLS | ✅ | ✅ |
| Compression | ✅ (via HTTP) | ✅ (LZ4) |
| Connection pooling | ✅ (via Pool) | ✅ (via Pool) |
| Streaming results | ✅ | ✅ |
| Zero-copy Arrow | ✅ | ✅ |
| Progress tracking | ❌ | ✅ |
| Works behind proxy | ✅ | ❌ |
| Works with CDN | ✅ | ❌ |

### When to Use HTTP

- Deployment behind a reverse proxy (nginx, haproxy)
- Access via CDN or load balancer
- Cloud environment with network restrictions
- Simple local development
- Integration with HTTP monitoring tools

### When to Use Native

- High performance required
- Direct server-to-server communication
- Large data volumes
- Need for progress tracking

---

## Streaming and Arrow

Both backends support streaming and Arrow:

```rust
// Streaming - works with both HTTP and Native
let mut stream = conn
    .stream::<User, _>(users::table.filter(users::active.eq(true)))
    .await?;

while let Some(user) = stream.next().await? {
    println!("User: {}", user.name);
}

// Zero-copy Arrow (HTTP only for now)
let count = conn.load_zero_copy(
    "SELECT id, name FROM users",
    |row| {
        let name = row.get_str("name")?;  // &str, zero-copy
        Ok(())
    }
).await?;
```

---

## Connection Pooling

Both backends support pooling via `Pool`:

```rust
use diesel_clickhouse::{Connection, pool::Pool};

// HTTP pool
let pool = Pool::builder(
    Connection::http()
        .host("localhost")
        .port(8123)
        .database("mydb")
)
.max_size(20)
.min_idle(5)
.build()
.await?;

// Native pool
let pool = Pool::builder(
    Connection::native()
        .host("localhost")
        .port(9000)
        .database("mydb")
)
.max_size(20)
.build()
.await?;

let conn = pool.get().await?;
```

---

## TLS Configuration

### HTTP with TLS

```rust
use diesel_clickhouse::Connection;

// rustls (recommended, pure Rust)
// Cargo.toml: features = ["http", "rustls-tls"]
let conn = Connection::establish("https://localhost:8443/default").await?;

// native-tls (uses OpenSSL)
// Cargo.toml: features = ["http", "native-tls"]
let conn = Connection::establish("https://localhost:8443/default").await?;
```

### Native with TLS

```rust
use diesel_clickhouse::Connection;

// Cargo.toml: features = ["native", "native-tls-native"]
let conn = Connection::establish(
    "tcp://localhost:9440/default?secure=true"
).await?;
```

---

## ClickHouse Ports

| Service | Default Port | Description |
|---------|--------------|-------------|
| HTTP | 8123 | HTTP interface without TLS |
| HTTPS | 8443 | HTTP interface with TLS |
| Native | 9000 | Native protocol without TLS |
| Native TLS | 9440 | Native protocol with TLS |

Check your ClickHouse configuration (`config.xml`) for exact port numbers.
