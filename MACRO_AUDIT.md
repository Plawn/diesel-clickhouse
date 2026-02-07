# Macro Audit - diesel-clickhouse

This document provides a comprehensive inventory of all macros in the diesel-clickhouse library for auditing purposes.

**Last Updated**: 2026-01-04

---

## Summary

| Crate | Declarative Macros | Proc Macros | Total |
|-------|-------------------|-------------|-------|
| diesel-clickhouse-derive | 0 | 7 | 7 |
| diesel-clickhouse-core | 28 | 0 | 28 |
| diesel-clickhouse-types | 2 | 0 | 2 |
| diesel-clickhouse-migrations | 1 | 0 | 1 |
| diesel-clickhouse | 4 | 0 | 4 |
| **Total** | **35** | **7** | **42** |

---

## 1. Procedural Macros (diesel-clickhouse-derive)

### 1.1 User-Facing Proc Macros

| Macro | Type | Location | Description |
|-------|------|----------|-------------|
| `table!` | `proc_macro` | lib.rs:84 | Defines ClickHouse table schema with columns and types |
| `#[derive(ClickHouseRow)]` | `proc_macro_derive` | lib.rs | Optimized binary row serialization for both HTTP and Native backends |
| `#[derive(Queryable)]` | `proc_macro_derive` | lib.rs | Row deserialization from query results |
| `#[derive(Insertable)]` | `proc_macro_derive` | lib.rs | Row serialization for INSERT statements |
| `#[derive(Selectable)]` | `proc_macro_derive` | lib.rs | Explicit column selection with type safety |

### 1.2 Proc Macro Attributes

| Attribute | Used By | Purpose |
|-----------|---------|---------|
| `#[diesel_clickhouse(table_name = ...)]` | Queryable, Insertable, Selectable | Specifies the table module |
| `#[diesel_clickhouse(column_name = "...")]` | ClickHouseRow, Queryable, Insertable | Renames field to different DB column |
| `#[serde(with = "...")]` | ClickHouseRow | Custom serde serialization for fields (e.g., chrono DateTime) |

---

## 2. Declarative Macros - diesel-clickhouse-core

### 2.1 SQL Operator Macros (expression/operators.rs)

| Macro | Line | Purpose | Generated Types |
|-------|------|---------|-----------------|
| `binary_operator!` | 17 | Binary ops returning Bool (=, >, <, etc.) | Eq, Gt, Lt, Ge, Le, Ne |
| `wrapped_binary_operator!` | 68 | Bool ops wrapped in parens (AND, OR) | And, Or |
| `unary_suffix_operator!` | 121 | Suffix ops (IS NULL, IS NOT NULL) | IsNull, IsNotNull |
| `unary_prefix_operator!` | 164 | Prefix ops (NOT) | Not |
| `impl_in_query_fragment!` | 343 | IN clause with iterator | Internal helper |

### 2.2 SQL Function Macros (expression/functions.rs)

| Macro | Line | Purpose | Example Output |
|-------|------|---------|----------------|
| `sql_function_nullary!` | 18 | 0-arg functions | `now()`, `today()`, `count(*)` |
| `sql_function_unary!` | 53 | 1-arg with fixed return type | `count(x)` -> UInt64 |
| `sql_function_unary_preserve_type!` | 103 | 1-arg preserving input type | `min(x)`, `max(x)`, `sum(x)` |
| `sql_function_unary_to_array!` | 152 | 1-arg returning Array | `groupArray(x)` |
| `sql_function_binary!` | 201 | 2-arg with fixed return type | `has(arr, elem)` -> Bool |
| `sql_function_binary_preserve_first!` | 259 | 2-arg preserving first type | `coalesce(x, default)` |

### 2.3 Expression Bound Macros (expression/mod.rs)

| Macro | Line | Purpose |
|-------|------|---------|
| `impl_bound_numeric!` | 151 | Bound trait for numeric types |
| `impl_bound_numeric_ref!` | 166 | Bound trait for numeric refs |
| `impl_bound_option!` | 197 | Bound trait for Option types |
| `impl_expression_tuple!` | 275 | Expression impl for tuples |
| `impl_as_expression!` | 355 | AsExpression for primitives |
| `impl_as_expression_option!` | 399 | AsExpression for Option |

### 2.4 Serialization Macros (serialize.rs)

| Macro | Line | Purpose |
|-------|------|---------|
| `impl_write_sql_value_bindable!` | 64 | WriteSqlValue for primitives |
| `impl_to_row_primitive!` | 211 | ToRow for primitives |
| `impl_to_row_tuple!` | 244 | ToRow for tuples (2-8 elements) |
| `impl_to_sql_literal_int!` | 344 | SQL literal for integers (itoa) |
| `impl_to_sql_literal_float!` | 361 | SQL literal for floats (ryu) |

### 2.5 Deserialization Macros (deserialize.rs)

| Macro | Line | Purpose |
|-------|------|---------|
| `impl_from_row_primitive!` | 37 | FromRow for primitives |
| `impl_from_row_tuple!` | 114 | FromRow for tuples |
| `impl_queryable_primitive!` | 225 | Queryable for primitives |
| `impl_queryable_tuple!` | 255 | Queryable for tuples |

### 2.6 Backend Macros (backend.rs)

| Macro | Line | Purpose |
|-------|------|---------|
| `impl_to_bindable!` | 203 | ToBindable for owned types |
| `impl_into_bindable!` | 245 | IntoBindable conversion |

### 2.7 Query Builder Macros

| Macro | Line | File | Purpose |
|-------|------|------|---------|
| `impl_query_fragment_tuple!` | 105 | mod.rs | QueryFragment for tuples |
| `impl_cte_list_tuple!` | 131 | with.rs | CTE list for tuples |
| `impl_with_queries_builder!` | 311 | with.rs | WITH clause builder |
| `impl_as_changeset_tuple!` | 131 | update.rs | AsChangeset for tuples |
| `impl_assignments_tuple!` | 191 | update.rs | Assignments for UPDATE |

---

## 3. Declarative Macros - diesel-clickhouse-types

| Macro | Line | File | Purpose |
|-------|------|------|---------|
| `impl_sql_type_tuple!` | 149 | lib.rs | SqlType for tuples (2-16 elements) |
| `impl_integer_conversion!` | 231 | integers.rs | Integer type conversions |

---

## 4. Declarative Macros - diesel-clickhouse-migrations

| Macro | Line | Purpose | Visibility |
|-------|------|---------|------------|
| `embed_migrations!` | 78 | Embed migrations at compile time | `#[macro_export]` |

---

## 5. Declarative Macros - diesel-clickhouse

### 5.1 Connection Macros (unified.rs)

| Macro | Line | Purpose | Visibility |
|-------|------|---------|------------|
| `with_connection!` | 215 | Dispatch to HTTP/Native backend | Internal |
| `http_only!` | 228 | HTTP-only methods (Arrow) | Internal |

### 5.2 Data Conversion Macros

| Macro | Line | File | Purpose |
|-------|------|------|---------|
| `impl_arrow_value_primitive!` | 272 | arrow.rs | ArrowValue for primitives |
| `impl_block_value!` | 62 | native/block.rs | BlockValue for primitives |
| `impl_into_block_column_primitive!` | 125 | native/column.rs | BlockColumn conversions |

### 5.3 Test Support Macros (tests only)

| Macro | Line | File | Purpose |
|-------|------|------|---------|
| `test_table!` | 440 | tests/test_support/containers.rs | Create test table |
| `drop_test_table!` | 466 | tests/test_support/containers.rs | Drop test table |

---

## 6. Macro Categories by Purpose

### 6.1 Type Implementation (18 macros)
Reduce boilerplate for implementing traits on multiple types.
- `impl_*` pattern macros in core/types

### 6.2 SQL Generation (12 macros)
Generate SQL operators and functions with type safety.
- `binary_operator!`, `sql_function_*!`

### 6.3 Schema Definition (1 macro)
Define table schemas.
- `table!`

### 6.4 Row Serialization/Deserialization (6 macros)
Handle row data conversion.
- `#[derive(ClickHouseRow)]`, `#[derive(Queryable)]`, `#[derive(Insertable)]`, `#[derive(Selectable)]`

### 6.5 Internal Utilities (5 macros)
Helper macros for internal use.
- `with_connection!`, `http_only!`, test macros

---

## 7. Security Considerations

### 7.1 Proc Macro Safety
- All proc macros run at **compile time only**
- `#![allow(clippy::unwrap_used)]` is acceptable in proc-macro crate (panics = compile errors)
- No runtime code execution from proc macros

### 7.2 SQL Injection Prevention
- `table!` macro escapes identifiers
- `binary_operator!` uses parameterized queries
- No string interpolation of user values

### 7.3 Macro Hygiene
- All macros use proper hygiene (no identifier leakage)
- Internal macros are not exported
- `#[macro_export]` only on `embed_migrations!`

---

## 8. Audit Checklist

- [ ] All proc macros validate input thoroughly
- [ ] No macro generates unsafe code
- [ ] SQL-generating macros use proper escaping
- [ ] Test macros are `#[cfg(test)]` only
- [ ] No macros expose internal implementation details
- [ ] Macro-generated code follows same standards as handwritten code

---

## 9. File Index

```
diesel-clickhouse-derive/
  src/lib.rs           - 7 proc macros

diesel-clickhouse-core/
  src/expression/
    operators.rs       - 5 macros (SQL operators)
    functions.rs       - 6 macros (SQL functions)
    mod.rs             - 6 macros (expression traits)
  src/deserialize.rs   - 4 macros
  src/serialize.rs     - 5 macros
  src/backend.rs       - 2 macros
  src/query_builder/
    mod.rs             - 1 macro
    with.rs            - 2 macros
    update.rs          - 2 macros

diesel-clickhouse-types/
  src/lib.rs           - 1 macro
  src/integers.rs      - 1 macro

diesel-clickhouse-migrations/
  src/lib.rs           - 1 macro

diesel-clickhouse/
  src/unified.rs       - 2 macros
  src/arrow.rs         - 1 macro
  src/native/
    block.rs           - 1 macro
    column.rs          - 1 macro
  tests/test_support/
    containers.rs      - 2 macros (test only)
```
