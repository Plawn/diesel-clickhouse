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

### CRITICAL: Read Before Writing

**NEVER write code without first understanding the existing codebase.** Before implementing anything:
1. Search for similar existing implementations using grep/glob
2. Identify patterns already used in the codebase
3. Reuse existing utilities, traits, and helpers
4. Follow the established code style exactly

### Unified Interface First

Each new feature must be available in the unified `Connection` interface (in `unified.rs`) if technically possible. Don't add backend-specific features without exposing them through the unified API.

### Multi-Backend Factorization (MANDATORY)

This project supports HTTP and Native backends. **Code must be factorized properly:**

```rust
// ❌ FORBIDDEN: Duplicated logic in each backend
// http/mod.rs
impl HttpConnection {
    pub async fn execute_query(&self, sql: &str) -> Result<()> {
        let sql = self.prepare_sql(sql);  // Duplicated
        let sql = self.add_settings(sql); // Duplicated
        self.http_client.post(sql).await
    }
}
// native/mod.rs
impl NativeConnection {
    pub async fn execute_query(&self, sql: &str) -> Result<()> {
        let sql = self.prepare_sql(sql);  // Same logic duplicated!
        let sql = self.add_settings(sql); // Same logic duplicated!
        self.native_client.execute(sql).await
    }
}

// ✅ MANDATORY: Shared logic in core, backend-specific only where needed
// core: shared preparation logic
fn prepare_query(sql: &str, settings: &Settings) -> String { ... }

// backends: only transport-specific code
impl HttpConnection {
    pub async fn execute_query(&self, sql: &str) -> Result<()> {
        let sql = prepare_query(sql, &self.settings);
        self.http_client.post(sql).await  // HTTP-specific only
    }
}
```

**Factorization checklist:**
- [ ] SQL generation: MUST be in `diesel-clickhouse-core` (backend-agnostic)
- [ ] Type conversions: MUST be in `diesel-clickhouse-types`
- [ ] Query building: MUST use shared `QueryFragment` trait
- [ ] Only transport layer differs between backends
- [ ] New trait? Ask: can both backends implement it identically? If yes → core crate

### Code Deduplication (MANDATORY)

**Never duplicate code. Extract and reuse.**

```rust
// ❌ FORBIDDEN: Copy-pasted logic
fn process_users(users: Vec<User>) -> Vec<ProcessedUser> {
    users.into_iter()
        .filter(|u| u.active)
        .map(|u| ProcessedUser { name: u.name.to_uppercase(), ... })
        .collect()
}
fn process_admins(admins: Vec<Admin>) -> Vec<ProcessedAdmin> {
    admins.into_iter()
        .filter(|a| a.active)  // Same filter!
        .map(|a| ProcessedAdmin { name: a.name.to_uppercase(), ... })  // Same transform!
        .collect()
}

// ✅ MANDATORY: Extract common behavior via traits
trait Processable {
    fn is_active(&self) -> bool;
    fn name(&self) -> &str;
}
fn process<T: Processable, R>(items: Vec<T>, map_fn: impl Fn(T) -> R) -> Vec<R> {
    items.into_iter().filter(|i| i.is_active()).map(map_fn).collect()
}
```

**Before writing new code, ALWAYS:**
1. `grep -r "similar_pattern"` to find existing implementations
2. Check if a trait already exists for the behavior
3. Check `diesel-clickhouse-core` for reusable utilities
4. If 3+ lines are similar somewhere → extract to shared function/trait

### Idiomatic Rust (MANDATORY)

**Write Rust the Rust way. Follow community conventions.**

```rust
// ❌ FORBIDDEN: Non-idiomatic patterns
fn get_value(opt: Option<i32>) -> i32 {
    if opt.is_some() {
        opt.unwrap()  // Anti-pattern
    } else {
        0
    }
}

fn find_user(users: &[User], id: u64) -> Option<&User> {
    for user in users {
        if user.id == id {
            return Some(user);
        }
    }
    None
}

// ✅ MANDATORY: Idiomatic patterns
fn get_value(opt: Option<i32>) -> i32 {
    opt.unwrap_or(0)
}

fn find_user(users: &[User], id: u64) -> Option<&User> {
    users.iter().find(|u| u.id == id)
}
```

**Idiomatic patterns to use:**
- `Option`: use `map`, `and_then`, `unwrap_or`, `ok_or`, `?` operator
- `Result`: use `map_err`, `context` (anyhow), `?` operator, never `.unwrap()` in lib code
- `Iterator`: use combinators (`map`, `filter`, `fold`, `find`, `any`, `all`)
- `match`: prefer over `if let` chains when multiple variants
- `impl Into<T>` / `AsRef<T>`: for flexible APIs
- Builder pattern: for complex struct construction
- `From`/`Into`: for type conversions, not custom methods
- `Default`: implement for structs with sensible defaults
- `#[must_use]`: on functions returning values that shouldn't be ignored

```rust
// ❌ FORBIDDEN: Custom conversion method
impl Foo {
    fn to_bar(&self) -> Bar { ... }
}

// ✅ MANDATORY: Use From trait
impl From<&Foo> for Bar {
    fn from(foo: &Foo) -> Self { ... }
}
// Usage: Bar::from(&foo) or foo.into()
```

### Error Handling

```rust
// ❌ FORBIDDEN in library code
.unwrap()
.expect("msg")
panic!()

// ✅ MANDATORY: Propagate errors
.ok_or_else(|| Error::NotFound)?
.map_err(Error::from)?
```

### Naming Conventions

- Types: `PascalCase`
- Functions/methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Modules: `snake_case`
- Trait methods that return `Self`: `new`, `with_*`, `into_*`
- Trait methods that borrow: `as_*`, `to_*` (allocating), methods without prefix (non-allocating)
- Boolean methods: `is_*`, `has_*`, `can_*`

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

# Appendice: Code Rust Performant

## Principe Fondamental

**Écris du code performant dès le premier jet.** Rust te donne les outils pour être rapide — utilise-les. Ne livre jamais de code "qui compile" mais qui est lent.

## Règles Obligatoires

### Ownership et Allocations

1. **Évite les allocations inutiles.** Chaque `String`, `Vec`, `Box` a un coût.
2. **Préfère les références** (`&str`, `&[T]`) aux types owned quand tu n'as pas besoin de propriété.
3. **Réutilise les buffers** au lieu de réallouer.

```rust
// ❌ INTERDIT : allocation à chaque itération
for item in items {
    let s = item.to_string(); // Nouvelle String à chaque fois
    process(&s);
}

// ✅ OBLIGATOIRE : réutiliser le buffer
let mut buf = String::new();
for item in items {
    buf.clear();
    write!(&mut buf, "{}", item).unwrap();
    process(&buf);
}
```

```rust
// ❌ INTERDIT : clone inutile
fn process(data: Vec<u8>) { /* ... */ }
process(my_vec.clone()); // Clone juste pour passer en paramètre

// ✅ OBLIGATOIRE : emprunter
fn process(data: &[u8]) { /* ... */ }
process(&my_vec);
```

### Collections et Lookups

```rust
// ❌ INTERDIT : lookup O(n) dans une boucle = O(n²)
for item in &items {
    if other_items.iter().any(|o| o.id == item.id) {
        // ...
    }
}

// ✅ OBLIGATOIRE : HashSet/HashMap pour lookups O(1)
let other_ids: HashSet<_> = other_items.iter().map(|o| o.id).collect();
for item in &items {
    if other_ids.contains(&item.id) {
        // ...
    }
}
```

```rust
// ❌ INTERDIT : collect() intermédiaire inutile
let result: Vec<_> = items.iter()
    .filter(|x| x.active)
    .collect::<Vec<_>>()  // Allocation inutile
    .iter()
    .map(|x| x.value)
    .collect();

// ✅ OBLIGATOIRE : chaîner les itérateurs
let result: Vec<_> = items.iter()
    .filter(|x| x.active)
    .map(|x| x.value)
    .collect();
```

### Pré-allocation

```rust
// ❌ INTERDIT : réallocations multiples
let mut results = Vec::new();
for item in items {
    results.push(transform(item)); // Réalloc potentielle à chaque push
}

// ✅ OBLIGATOIRE : pré-allouer
let mut results = Vec::with_capacity(items.len());
for item in items {
    results.push(transform(item));
}

// ✅ ENCORE MIEUX : collect avec size hint
let results: Vec<_> = items.into_iter().map(transform).collect();
```

### Strings

```rust
// ❌ INTERDIT : concaténation avec + ou format! en boucle
let mut result = String::new();
for item in items {
    result = result + &item.to_string(); // Réalloc à chaque +
    // ou: result = format!("{}{}", result, item); // Pire
}

// ✅ OBLIGATOIRE : push_str ou write!
let mut result = String::with_capacity(estimated_size);
for item in items {
    write!(&mut result, "{}", item).unwrap();
}

// ✅ OU : join pour les cas simples
let result: String = items.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("");
```

```rust
// ❌ INTERDIT : String quand &str suffit
fn greet(name: String) -> String {
    format!("Hello, {}", name)
}

// ✅ OBLIGATOIRE : accepter &str, retourner String seulement si nécessaire
fn greet(name: &str) -> String {
    format!("Hello, {}", name)
}
```

### Clonage

```rust
// ❌ INTERDIT : .clone() par facilité
let data = expensive_data.clone();
some_function(data);

// ✅ OBLIGATOIRE : se demander si on a vraiment besoin de clone
// - Peut-on emprunter ?
// - Peut-on utiliser Rc/Arc si partagé ?
// - Peut-on restructurer pour éviter ?
some_function(&expensive_data); // Emprunter si possible
```

### Async et I/O

```rust
// ❌ INTERDIT : séquentiel
for url in urls {
    let response = client.get(url).await?;
    results.push(response);
}

// ✅ OBLIGATOIRE : parallèle avec futures
use futures::future::join_all;
let futures: Vec<_> = urls.iter().map(|url| client.get(url)).collect();
let results = join_all(futures).await;

// ✅ OU : stream avec buffer pour contrôler la concurrence
use futures::stream::{self, StreamExt};
let results: Vec<_> = stream::iter(urls)
    .map(|url| client.get(url))
    .buffer_unordered(10) // Max 10 requêtes simultanées
    .collect()
    .await;
```

### Itérateurs vs Boucles indexées

```rust
// ❌ INTERDIT : accès indexé avec bounds check
for i in 0..items.len() {
    process(&items[i]); // Bounds check à chaque accès
}

// ✅ OBLIGATOIRE : itérateur (pas de bounds check)
for item in &items {
    process(item);
}

// ✅ SI index nécessaire : enumerate
for (i, item) in items.iter().enumerate() {
    process(i, item);
}
```

### Box et Indirection

```rust
// ❌ INTERDIT : Box inutile pour petits types
struct Config {
    timeout: Box<u64>,  // Indirection inutile pour 8 bytes
}

// ✅ OBLIGATOIRE : inline pour petits types
struct Config {
    timeout: u64,
}

// ✅ Box justifié : gros types, récursion, trait objects
struct Node {
    children: Vec<Box<Node>>,  // Récursif, OK
}
```

### Compilation

```rust
// Dans Cargo.toml pour la release :
[profile.release]
lto = true          # Link-time optimization
codegen-units = 1   # Meilleure optimisation, compilation plus lente
panic = "abort"     # Binaire plus petit si panic = crash OK
```

## Patterns spécifiques

### Entry API pour HashMap

```rust
// ❌ INTERDIT : double lookup
if !map.contains_key(&key) {
    map.insert(key, compute_value());
}

// ✅ OBLIGATOIRE : entry API
map.entry(key).or_insert_with(|| compute_value());
```

### Cow pour flexibilité sans coût

```rust
use std::borrow::Cow;

// ✅ Emprunté si possible, owned si nécessaire
fn process(input: &str) -> Cow<str> {
    if needs_modification(input) {
        Cow::Owned(modify(input))
    } else {
        Cow::Borrowed(input)
    }
}
```

### SmallVec pour petites collections

```rust
use smallvec::SmallVec;

// ✅ Stack-allocated jusqu'à N éléments, heap après
let mut items: SmallVec<[u32; 8]> = SmallVec::new();
```

## Checklist avant de livrer

- [ ] Pas de `.clone()` injustifié
- [ ] Références (`&T`) préférées aux types owned quand possible
- [ ] Collections pré-allouées avec `with_capacity`
- [ ] Pas de lookup linéaire dans des boucles (utiliser HashMap/HashSet)
- [ ] Pas de `collect()` intermédiaire inutile
- [ ] I/O async parallélisées (join_all, buffer_unordered)
- [ ] Itérateurs préférés aux boucles indexées
- [ ] `#[inline]` sur les petites fonctions hot-path si pertinent
- [ ] Release build avec LTO activé

## En cas de doute

Profile avec `cargo flamegraph` ou `perf`. Benchmark avec `criterion`. Ne devine pas — mesure.

```bash
cargo build --release
cargo flamegraph --bin my_app
```

---

# Final Checklist (BEFORE SUBMITTING ANY CODE)

Run through this checklist for every piece of code:

## 1. Did I Research First?
- [ ] Searched for similar existing code in the codebase
- [ ] Identified patterns already used
- [ ] Checked core/types crates for reusable utilities

## 2. Is the Code Properly Factorized?
- [ ] No duplicated logic between HTTP and Native backends
- [ ] Shared logic lives in `diesel-clickhouse-core`
- [ ] Backend implementations only contain transport-specific code
- [ ] No copy-pasted code blocks (3+ similar lines → extract)

## 3. Is It Idiomatic Rust?
- [ ] Using iterator combinators instead of manual loops
- [ ] Using `?` operator for error propagation
- [ ] Using `From`/`Into` traits for conversions
- [ ] Using `Option`/`Result` methods (`map`, `and_then`, `unwrap_or`)
- [ ] No `.unwrap()` or `.expect()` in library code
- [ ] Following naming conventions (`is_*`, `as_*`, `into_*`)

## 4. Is It Performant?
- [ ] No unnecessary `.clone()`
- [ ] Using references (`&T`) where ownership not needed
- [ ] Collections pre-allocated with `with_capacity`
- [ ] No O(n) lookups inside loops (use HashMap/HashSet)
- [ ] Iterators chained without intermediate `collect()`
- [ ] Async I/O parallelized where appropriate

## 5. Does It Compile Clean?
- [ ] `cargo clippy --all --all-features` passes
- [ ] `cargo test --all` passes
- [ ] No new warnings introduced

---

*Write it right the first time. Read the codebase. Follow the patterns.*