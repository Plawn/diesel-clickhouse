# CLAUDE.md

## Build Commands

```bash
cargo build --all                              # Full workspace
cargo build -p diesel-clickhouse --features http   # HTTP backend only
cargo build -p diesel-clickhouse --features native # Native backend only
cargo test --all                               # Unit tests
docker-compose up -d && cargo test --all --features integration  # Integration tests
cargo clippy --all --all-features              # Lint
```

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

## Key Files

- `diesel-clickhouse/src/unified.rs` — Unified `Connection` enum
- `diesel-clickhouse/src/http/mod.rs` — HTTP backend
- `diesel-clickhouse/src/native/mod.rs` — Native backend
- `diesel-clickhouse-core/src/query_builder/` — SQL generation
- `diesel-clickhouse-derive/src/lib.rs` — Proc macros

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

## Performance Rules

**Avoid:**
- `.clone()` when `&T` works
- `Vec::new()` in loops → `Vec::with_capacity(n)`
- `for i in 0..len { v[i] }` → `for item in &v`
- O(n) lookup in loops → `HashSet`/`HashMap`
- Intermediate `.collect()` between iterator chains
- `String` params → `&str` params
- Sequential async in loops → `join_all` / `buffer_unordered`
- `if !map.contains_key() { map.insert() }` → `map.entry().or_insert_with()`

**Use:**
- `Cow<str>` when sometimes borrowed, sometimes owned
- `SmallVec` for small, fixed-size collections
- `#[inline]` on small hot-path functions

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