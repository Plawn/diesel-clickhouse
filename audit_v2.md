# Audit v2 - diesel-clickhouse

Date: 2025-02-14
Scope: code review + examples/docs review in the workspace
Goal: validate correctness, performance alignment with a high-performance, type-safe ClickHouse ORM, and developer experience (DX)

## Summary

The core architecture is solid (typed query AST, backend-specific bind handling, optimized binary insert paths, streaming support). However, there are correctness and performance regressions in the unified API and native backend, and the docs/examples do not match the current macro and API surface. The highest-risk items are the native `LIMIT 1` SQL mutation (can be invalid SQL) and the unified/DSL `first()`/`load_one()` paths that load full result sets.

## Findings (prioritized)

1) High: Native `load_one`/`load_optional` append `LIMIT 1` unsafely (invalid SQL or semantic change)
2) High: `first()`/`get_result()`/`Connection::load_one` load all rows before selecting one
3) Medium: Native JSON session setting applied once per connection, not per pooled handle
4) Medium: HTTP compression toggling cannot reliably disable compression
5) Medium: Parameter binding lacks chrono/time/uuid support, reducing type safety and forcing SQL literals
6) Low: Docs/examples reference non-existent macros/APIs and misname attributes, causing copy/paste failures

## Detailed findings and fixes

### 1) Native `load_one`/`load_optional` append `LIMIT 1` unsafely

Evidence
- `diesel-clickhouse/src/native/mod.rs:432` `load_one`
- `diesel-clickhouse/src/native/mod.rs:457` `load_optional`
- `diesel-clickhouse/src/native/mod.rs:733` `append_limit_1`

Problem
- The code unconditionally appends `" LIMIT 1"` to the generated SQL string.
- This can generate invalid SQL for queries that already include `LIMIT`, or when `FORMAT`/`SETTINGS` appear at the end of the query. Example failures:
  - `SELECT ... LIMIT 10` -> `... LIMIT 10 LIMIT 1` (invalid)
  - `SELECT ... SETTINGS max_threads=8` -> `... SETTINGS max_threads=8 LIMIT 1` (invalid in ClickHouse, since LIMIT must precede SETTINGS)
  - `SELECT ... FORMAT JSONEachRow` -> `... FORMAT JSONEachRow LIMIT 1` (invalid; LIMIT must precede FORMAT)

Impact
- Correctness bug; the generated SQL can fail or change semantics.

Fix
- Do not mutate SQL strings after build; instead add a proper query builder wrapper that injects `LIMIT 1` in the correct place in the AST. Options:
  1) Add a `Limit` modifier at the AST level (or reuse existing `limit(1)` in the builder) and call it from `load_one`/`load_optional`.
  2) For native backend, implement `load_one` by streaming the first row (`stream_raw`), then cancel/stop further reads. This avoids SQL rewriting but still needs to ensure the driver and server stop cleanly.
  3) If string rewriting is retained, parse for trailing `FORMAT`/`SETTINGS` and insert `LIMIT 1` before those clauses. This is fragile and not recommended long-term.

Recommended fix
- Use AST-level `limit(1)` and a dedicated helper that preserves existing `LIMIT` and order of `SETTINGS`/`FORMAT`.

Test additions
- Add unit tests that build queries with `.limit()`, `.format()`, `.settings()` and ensure `load_one` SQL is valid and ordered correctly.

---

### 2) `first()`/`get_result()`/`Connection::load_one` load all rows

Evidence
- `diesel-clickhouse/src/run_query_dsl.rs:151` `first()`
- `diesel-clickhouse/src/run_query_dsl.rs:158` `get_result()`
- `diesel-clickhouse/src/unified.rs:440` `load_one()`

Problem
- These methods call `load()` and then take the first result. This loads the entire result set into memory even when only one row is needed.

Impact
- Performance regression (O(n) memory and time) and potential OOM for large datasets.
- Contradicts the performance claims of the library.

Fix
- For HTTP backend, use `fetch_one()`/`fetch_optional()` (already available in `ClickHouseConnection::load_one`/`load_optional`).
- For native backend, use a corrected `load_one` that applies `LIMIT 1` via the AST, or stream and stop after the first row.
- Update `RunQueryDsl::first()` and `RunQueryDsl::get_result()` to call backend-specific methods (or add a unified `Connection::load_one` that is efficient on both backends).

Recommended fix
- Introduce a fast-path in `RunQueryDsl` that delegates to `Connection::load_one`/`load_optional` without loading all rows.

Test additions
- Benchmark or test ensures only one block is loaded for `first()` on native, and only one row is fetched for HTTP.

---

### 3) Native JSON settings applied once per `NativeConnection`, not per pooled handle

Evidence
- `diesel-clickhouse/src/native/mod.rs:199` `get_handle()` uses `AtomicBool` in the `NativeConnection` object
- `diesel-clickhouse/src/native/builder.rs:235` sets JSON mode once during initial connection creation

Problem
- The JSON settings are only applied once per `NativeConnection` instance. If the pool grows later (new connections created), those new sessions can miss the JSON settings.

Impact
- Potential deserialization failures or incorrect JSON handling depending on server version and session settings.

Fix
- Apply JSON settings per handle when the `json` feature is enabled. Options:
  1) Execute `SET output_format_native_write_json_as_string = 1` on each newly acquired handle and cache per-handle state (if the driver exposes a handle id).
  2) Use pool/session settings in the DSN or driver if supported, so that every connection inherits the setting.
  3) Apply settings per query (append `SETTINGS output_format_native_write_json_as_string = 1`) when JSON columns are detected. This is less efficient but safe.

Recommended fix
- Prefer DSN/session-level settings so the pool guarantees all handles are configured consistently.

Test additions
- Integration test with a pool that expands beyond the initial size, then reads JSON columns from both early and late connections.

---

### 4) HTTP compression toggling cannot reliably disable compression

Evidence
- `diesel-clickhouse/src/http/mod.rs:100` `with_compression()`

Problem
- `with_compression()` updates the stored `compression` enum, but does not reset the underlying clickhouse `Client` when switching to `None` or `Zstd`. If compression was enabled and then disabled, the client may still compress requests.

Impact
- Incorrect behavior and confusing API results (reported compression differs from actual client behavior).

Fix
- Make `ClickHouseConnection` immutable for compression and require it to be set in the builder (recommended), or rebuild the client when compression changes (requires storing all builder options).
- If toggling is retained, add a `rebuild_client` method that re-applies URL, database, user, password, and options without compression.

Recommended fix
- Deprecate `with_compression` on a live connection and push compression to `HttpClientBuilder` only.

Test additions
- Unit test that toggles compression on/off and verifies `Client` options (if observable) or uses integration-level check on request headers.

---

### 5) Parameter binding lacks chrono/time/uuid support

Evidence
- `diesel-clickhouse-core/src/backend.rs:181` `ToBindableValue` only implemented for primitives/strings

Problem
- Bound parameters are limited to primitives and strings; there are no bindings for chrono/time/uuid types. This forces users to hand-format values as SQL literals, reducing type safety and increasing risk.

Impact
- DX friction and loss of type safety; also blocks SQL fallback inserts for JSON rows with temporal/uuid fields.

Fix
- Extend `BindableValue` with variants for chrono/time/uuid types (behind feature gates) and implement `Serialize` for them to support HTTP bindings.
- For native backend (interpolated SQL), implement `write_sql_literal` for these types (format as `toDateTime64(...)`, etc.), or serialize to string and cast appropriately.

Recommended fix
- Add feature-gated implementations of `ToBindableValue` for chrono/time/uuid and map them to new `BindableValue` variants.

Test additions
- Unit tests for bound WHERE clauses using chrono/time/uuid types on both backends.

---

### 6) Docs/examples reference non-existent macros and APIs

Evidence
- `README.md:51` uses `#[row]` and `#[diesel_clickhouse(table = ...)]`
- `diesel-clickhouse/src/run_query_dsl.rs:11` uses `#[typed_row]` in docs
- `PERFORMANCE.md:82` references `prepared::*` which is not in the crate
- `PERFORMANCE.md:117` references `BatchInserter` which is not implemented
- `PERFORMANCE.md:175` references `ClickHouseConnection::builder()` (does not exist)
- `PERFORMANCE.md:197` references `diesel_clickhouse::arena` / `interner` (not publicly re-exported)

Problem
- Copy/paste from docs will not compile. This is a major DX issue.

Impact
- High onboarding friction; reduced trust in documentation; avoidable support burden.

Fix
- Align docs/examples with the current API:
  - Use `#[clickhouse_row]` and `#[diesel_clickhouse(table_name = ...)]`.
  - Replace `#[typed_row]` with the real macro (or implement `typed_row` if intended).
  - Remove or implement `prepared`, `BatchInserter` and builder APIs.
  - If `arena`/`interner` are intended public APIs, re-export them from the top-level crate and document the correct paths.

Recommended fix
- Update README + PERFORMANCE.md + docs to compile as-is using the current APIs. Add `cargo test --doc` or `cargo test --examples` in CI.

Test additions
- Add a doctest compile check (or CI that runs `cargo test --doc` and `cargo test --examples`).

## Additional performance/DX opportunities (non-blocking)

- Provide `Connection::load_one_fast` or equivalent to avoid confusion and to make the fast path explicit.
- Add a `LIMIT 1` helper that handles `SETTINGS`/`FORMAT` in the query builder.
- Consider re-exporting useful performance tooling (arena allocator, interner) at the top level if they are intended for end users.

## Suggested fix order

1) Fix native `LIMIT 1` handling and unify fast-path `first()`/`load_one()` to avoid full load.
2) Fix native JSON session settings for pooled connections.
3) Fix compression toggling or move compression configuration into builders only.
4) Add chrono/time/uuid bindings.
5) Update docs/examples; add CI checks for docs/examples.

## Testing gaps

- No integration tests for `first()`/`load_one()` SQL ordering with `SETTINGS`/`FORMAT`.
- No tests covering pooled native connections that expand beyond the initial size.
- Doc/examples are not compiled in CI (assumed; confirm in CI config).

