# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Memory

Before starting a complex task, read files in `.claude/memory/`. After each milestone, update `.claude/memory/NOTES.md` with:
- What was accomplished
- Decisions made and why
- Next steps
- Problems encountered

## Build Commands

```bash
cargo build --all                                                  # Full workspace
cargo build -p diesel-clickhouse --features http                   # HTTP backend only
cargo build -p diesel-clickhouse --features native                 # Native backend only
cargo test --all                                                   # Unit tests
cargo test -p diesel-clickhouse --features testcontainers          # Integration tests (auto Docker)
cargo test -p diesel-clickhouse --features testcontainers test_name  # Single integration test
docker-compose up -d && cargo test --all --features integration    # Integration tests (manual Docker)
cargo clippy --all --all-features                                  # Lint
```

### Testcontainers (Recommended for Integration Tests)

The `testcontainers` feature automatically manages a ClickHouse Docker container:

```bash
# Run all integration tests with automatic Docker management
cargo test -p diesel-clickhouse --features testcontainers

# Run specific test
cargo test -p diesel-clickhouse --features testcontainers test_http_basic_query
```

**Requirements:** Docker must be running. No manual `docker-compose up` needed.

## Architecture

```
diesel-clickhouse-types    → SQL types (UInt64, DateTime, Array, Map)
        ↓
diesel-clickhouse-core     → Core traits, SQL generation, shared logic
        ↓
diesel-clickhouse-derive   → Proc macros (#[row], #[typed_row], table!)
        ↓
diesel-clickhouse          → HTTP + Native backends, unified Connection
        ↓
diesel-clickhouse-migrations / diesel-clickhouse-cli
```

**Two backends** via feature flags: `http` (port 8123) and `native` (port 9000).
Unified API in `unified.rs` wraps both.

### Connection URL Schemes

| Scheme | Backend | Default Port | Example |
|--------|---------|--------------|---------|
| `http://` | HTTP | 8123 | `http://localhost:8123/mydb` |
| `https://` | HTTP | 8443 | `https://ch.example.com/mydb` |
| `tcp://` | Native | 9000 | `tcp://localhost:9000/mydb` |

### Key Traits

| Trait | Location | Purpose |
|-------|----------|---------|
| `ClickHouseConnection` | `core::connection` | Unified async connection trait |
| `UnifiedRow` | `diesel-clickhouse::unified_row` | Backend-agnostic row deserialization |
| `Expression` | `core::expression` | Base trait for all SQL expressions |
| `QueryFragment` | `core::query_builder` | SQL generation trait |
| `Table` / `Column` | `core::query_source` | Schema representation |

## Key Files

- `diesel-clickhouse/src/unified.rs` — Unified `Connection` enum
- `diesel-clickhouse/src/http/mod.rs` — HTTP backend
- `diesel-clickhouse/src/native/mod.rs` — Native backend
- `diesel-clickhouse-core/src/query_builder/` — SQL generation
- `diesel-clickhouse-derive/src/lib.rs` — Proc macros

## Feature Flags

Key feature flags for `diesel-clickhouse`:

| Feature | Description | Default |
|---------|-------------|---------|
| `http` | HTTP backend (port 8123) | Yes |
| `native` | Native TCP protocol (port 9000) | Yes |
| `arrow` | Zero-copy Apache Arrow integration | Yes |
| `chrono` | DateTime via chrono crate | Yes |
| `pool` | Connection pooling | No |
| `migrations` | Migration system | No |
| `testcontainers` | Auto Docker for tests | No |

## Mandatory Rules

### 1. Read Before Writing
Search for existing patterns before implementing anything:
```bash
grep -r "similar_pattern" --include="*.rs"
```

### 2. Where Code Lives

| What | Where | Never in |
|------|-------|----------|
| SQL generation | `diesel-clickhouse-core` | backends |
| Type conversions | `diesel-clickhouse-types` | backends |
| Shared enums/structs | `diesel-clickhouse-core` | multiple places |
| Transport logic only | `http/` or `native/` | core |

**One definition rule:** If a type (enum, struct) is used by both backends, it lives in `core` exactly once. Backends re-export, never redefine.

### 3. Backend API Consistency

- New method in one backend → implement in both (or document why not)
- Same method signature in both backends
- Expose in unified `Connection` API
- Document intentional divergence in doc comments

### 4. Proc-Macro Deduplication

Similar macros must share code generation:
```rust
// Extract shared logic into helpers
fn generate_serde_struct(...) -> TokenStream2 { ... }
fn generate_native_block_impls(...) -> TokenStream2 { ... }

// Macros compose helpers — no duplicate quote! blocks
#[proc_macro_attribute]
pub fn row(...) -> TokenStream {
    quote! { #(generate_serde_struct(...))* #(generate_native_block_impls(...))* }
}
```

### 5. Code Style

- `#![deny(clippy::unwrap_used, clippy::expect_used)]` in lib code
- No `#[allow(dead_code)]` without issue reference
- Use `From`/`Into` traits, not custom conversion methods
- Pre-allocate collections: `Vec::with_capacity(n)`
- Borrow (`&T`) over clone when possible

## Final Checklist

Before submitting code:

**Research**
- [ ] Searched for similar existing code
- [ ] Checked core/types for reusable utilities

**Factorization**
- [ ] Shared logic in `diesel-clickhouse-core`, not duplicated in backends
- [ ] Types/enums defined once, in lowest common crate
- [ ] Proc-macro helpers extracted (no duplicate `quote!` blocks)

**API Consistency**
- [ ] Method exists in both backends (or divergence documented)
- [ ] Method exposed in unified `Connection`
- [ ] Signatures match between backends

**Quality**
- [ ] `cargo clippy --all --all-features` passes
- [ ] `cargo test --all` passes
- [ ] No `.unwrap()` / `.expect()` in lib code
- [ ] No unnecessary `.clone()`