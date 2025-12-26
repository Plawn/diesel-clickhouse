# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Build entire workspace
cargo build --all

# Build with specific backend
cargo build -p diesel-clickhouse --features http
cargo build -p diesel-clickhouse --features native

# Build CLI
cargo build -p diesel-clickhouse-cli --release

# Run tests
cargo test --all

# Run tests with integration tests (requires ClickHouse running)
docker-compose up -d
cargo test --all --features integration

# Run clippy
cargo clippy --all --all-features

# Check specific crate
cargo check -p diesel-clickhouse-core
```

## Architecture

This is a **workspace** with 6 crates forming a Diesel-inspired async ORM for ClickHouse:

```
diesel-clickhouse-types    → SQL type system (UInt64, DateTime, Array, Map, etc.)
        ↓
diesel-clickhouse-core     → Core traits: Backend, Expression, QueryDsl, ClickHouseConnection
        ↓
diesel-clickhouse-derive   → Proc macros: #[derive(Queryable, Insertable)], table! macro
        ↓
diesel-clickhouse          → Main crate: HTTP/Native backends, Connection, RunQueryDsl
        ↓
diesel-clickhouse-migrations → Migration system
        ↓
diesel-clickhouse-cli      → CLI tool for migrations
```

### Backend Abstraction

Two protocols supported via feature flags:
- `http` (default): Uses `clickhouse` crate, port 8123
- `native`: Uses `clickhouse-rs` crate, port 9000

The `Connection` enum in `unified.rs` provides a unified API over both backends.

### Query Building Flow

1. `table!` macro generates a module with `Table`, `Column` types
2. Queries built via `QueryDsl` trait methods (`.filter()`, `.select()`, `.limit()`)
3. `QueryFragment` trait converts query to SQL via `AstPass`
4. `RunQueryDsl` trait provides `.load()`, `.first()`, `.execute()` methods that execute on connection

### Key Traits

- `Backend`: Protocol abstraction (HttpBackend, NativeBackend)
- `Expression` / `SelectableExpression`: SQL expression building
- `QueryFragment`: SQL generation via `AstPass::push_sql()`
- `ClickHouseConnection`: Connection trait for both backends
- `FromRow` / `ToRow`: Deserialization/serialization of rows

## Development Rules

- **Unified interface first**: Each new feature must be available in the unified `Connection` interface (in `unified.rs`) if technically possible. Don't add backend-specific features without exposing them through the unified API.

## Code Style

Library crates deny `unwrap`/`expect` in non-test code:
```rust
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
```

Use `Result` types and proper error handling throughout.

## Feature Flags

The main `diesel-clickhouse` crate has these key features:
- `http` / `native`: Backend selection (both enabled by default)
- `arrow`: Zero-copy columnar data with Apache Arrow (default)
- `chrono` / `time`: DateTime handling
- `uuid`: UUID support
- `pool`: Connection pooling
- `migrations`: Migration system
- `native-tls` / `rustls-tls`: TLS for HTTP backend
- `native-tls-native`: TLS for Native backend
- `tracing`: Tracing integration
- `json`: JSON type support (JsonTyped<T>)

## Key Files

- `diesel-clickhouse/src/unified.rs`: Unified `Connection` API
- `diesel-clickhouse/src/run_query_dsl.rs`: `RunQueryDsl`, `InsertDsl`, `ExecuteDsl` traits
- `diesel-clickhouse/src/http/mod.rs`: HTTP backend implementation
- `diesel-clickhouse/src/native/mod.rs`: Native backend implementation
- `diesel-clickhouse-core/src/query_builder/`: Query building infrastructure
- `diesel-clickhouse-derive/src/lib.rs`: Procedural macros (`#[row]`, `table!`, derives)

## Common Tasks

```bash
# Format code
cargo fmt --all

# Run specific test
cargo test -p diesel-clickhouse test_name

# Check for compile errors quickly
cargo check --all --all-features

# Generate documentation
cargo doc --all --no-deps --open
```
