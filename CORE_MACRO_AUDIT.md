# diesel-clickhouse-core Macro Audit Report

**Date**: 2026-01-04
**Scope**: 28 declarative macros in diesel-clickhouse-core
**Status**: PASS with minor findings

---

## Executive Summary

All 28 macros in diesel-clickhouse-core have been audited. The macros are well-designed, follow consistent patterns, and pose no security risks. Minor improvements are recommended for consistency and documentation.

| Category | Macros | Status | Issues |
|----------|--------|--------|--------|
| SQL Operators | 5 | PASS | 0 |
| SQL Functions | 6 | PASS | 0 |
| Expression Bounds | 6 | PASS | 0 |
| Serialization | 5 | PASS | 0 |
| Deserialization | 4 | PASS | 0 |
| Backend | 2 | PASS | 0 |
| Query Builder | 5 | PASS | 0 |

---

## 1. SQL Operator Macros (operators.rs)

### 1.1 `binary_operator!` (line 17)

**Purpose**: Generate binary comparison operators (=, >, <, etc.)

**Generated Types**: `Eq`, `NotEq`, `Gt`, `GtEq`, `Lt`, `LtEq`, `Like`, `ILike`

**Audit**:
- [x] Proper trait bounds (Expression, SelectableExpression, AppearsOnTable)
- [x] Uses `pass.push_sql()` for SQL literals (safe)
- [x] Delegates to child `walk_ast()` for values (parameterized)
- [x] No string interpolation of values
- [x] Proper `#[derive(Debug, Clone, Copy)]`

**Verdict**: PASS

---

### 1.2 `wrapped_binary_operator!` (line 68)

**Purpose**: Generate boolean operators with parentheses (AND, OR)

**Generated Types**: `And`, `Or`

**Audit**:
- [x] Enforces `Expression<SqlType = Bool>` on both operands
- [x] Wraps in parentheses for correct precedence
- [x] Same safety properties as `binary_operator!`

**Verdict**: PASS

---

### 1.3 `unary_suffix_operator!` (line 121)

**Purpose**: Generate suffix operators (IS NULL, IS NOT NULL)

**Generated Types**: `IsNull`, `IsNotNull`

**Audit**:
- [x] Simple suffix append to expression
- [x] Returns `Bool` type
- [x] No value interpolation

**Verdict**: PASS

---

### 1.4 `unary_prefix_operator!` (line 164)

**Purpose**: Generate prefix operators (NOT)

**Generated Types**: `Not`

**Audit**:
- [x] Enforces `Expression<SqlType = Bool>` on operand
- [x] Wraps in parentheses: `NOT (expr)`
- [x] Correct precedence handling

**Verdict**: PASS

---

### 1.5 `impl_in_query_fragment!` (line 343)

**Purpose**: Generate QueryFragment for IN/NOT IN with collections

**Generated Types**: `In<L, Vec<T>>`, `In<L, &[T]>`, `NotIn<...>`

**Audit**:
- [x] Uses `pass.push_bindable()` for each value (parameterized)
- [x] Proper comma separation
- [x] Handles both `Vec<T>` and `&[T]`
- [x] Requires `T: ToBindableValue` - ensures safe binding

**Verdict**: PASS

---

## 2. SQL Function Macros (functions.rs)

### 2.1 `sql_function_nullary!` (line 18)

**Purpose**: Generate 0-argument SQL functions

**Generated**: `count_star()`, `now()`, `today()`

**Audit**:
- [x] Static SQL string pushed via `push_sql()`
- [x] No user input involved
- [x] Correct return type specified

**Verdict**: PASS

---

### 2.2 `sql_function_unary!` (line 53)

**Purpose**: Generate 1-argument SQL functions with fixed return type

**Generated**: `count()`, `avg()`, `uniq()`, `lower()`, etc.

**Audit**:
- [x] Function name via `concat!($sql, "(")` - compile-time string
- [x] Expression passed through `walk_ast()` (safe)
- [x] Proper trait implementations

**Verdict**: PASS

---

### 2.3 `sql_function_unary_preserve_type!` (line 103)

**Purpose**: Generate 1-argument SQL functions preserving input type

**Generated**: `sum()`, `min()`, `max()`, `any()`

**Audit**:
- [x] Same structure as `sql_function_unary!`
- [x] Uses `E::SqlType` for return type (type-safe)

**Verdict**: PASS

---

### 2.4 `sql_function_unary_to_array!` (line 152)

**Purpose**: Generate functions returning Array<E::SqlType>

**Generated**: `group_array()`, `group_uniq_array()`

**Audit**:
- [x] Returns `Array<E::SqlType>` (correct ClickHouse semantics)
- [x] Same safe pattern as other function macros

**Verdict**: PASS

---

### 2.5 `sql_function_binary!` (line 201)

**Purpose**: Generate 2-argument SQL functions with fixed return type

**Generated**: `has()`, `concat()`

**Audit**:
- [x] Both arguments passed through `walk_ast()`
- [x] Proper comma separation
- [x] Field names configurable

**Verdict**: PASS

---

### 2.6 `sql_function_binary_preserve_first!` (line 259)

**Purpose**: Generate 2-argument functions preserving first argument's type

**Generated**: `coalesce()`, `array_concat()`

**Audit**:
- [x] Enforces type compatibility: `B: Expression<SqlType = A::SqlType>`
- [x] Same safe SQL generation pattern

**Verdict**: PASS

---

## 3. Expression Bound Macros (expression/mod.rs)

### 3.1 `impl_bound_numeric!` (line 151)

**Purpose**: Implement QueryFragment for Bound<numeric_type>

**Types**: i8, i16, i32, i64, u8, u16, u32, u64, f32, f64

**Audit**:
- [x] Uses `pass.push_bindable()` - safe parameter binding
- [x] No string formatting of values

**Verdict**: PASS

---

### 3.2 `impl_bound_numeric_ref!` (line 166)

**Purpose**: Same as above but for reference types

**Audit**:
- [x] Identical safe pattern with `&'a $t`

**Verdict**: PASS

---

### 3.3 `impl_bound_option!` (line 197)

**Purpose**: Implement QueryFragment for Option<numeric>

**Audit**:
- [x] `Some(v)` → `push_bindable(v)` (safe)
- [x] `None` → `push_sql("NULL")` (literal, safe)

**Verdict**: PASS

---

### 3.4 `impl_expression_tuple!` (line 275)

**Purpose**: Implement Expression for tuples

**Sizes**: 1-8 element tuples

**Audit**:
- [x] Derives SqlType tuple from element SqlTypes
- [x] Proper SelectableExpression/AppearsOnTable propagation

**Verdict**: PASS

---

### 3.5 `impl_as_expression!` (line 355)

**Purpose**: Implement AsExpression for Rust primitives

**Audit**:
- [x] Creates `Bound<T, SqlType>` wrapper
- [x] Handles both owned and borrowed types

**Verdict**: PASS

---

### 3.6 `impl_as_expression_option!` (line 399)

**Purpose**: Implement AsExpression for Option<T>

**Audit**:
- [x] Maps to `Nullable<SqlType>`
- [x] Consistent with nullable column handling

**Verdict**: PASS

---

## 4. Serialization Macros (serialize.rs)

### 4.1 `impl_write_sql_value_bindable!` (line 64)

**Purpose**: Implement WriteSqlValue for primitives

**Audit**:
- [x] Delegates to `push_bindable()` - parameterized
- [x] No direct string formatting

**Verdict**: PASS

---

### 4.2 `impl_to_row_primitive!` (line 211)

**Purpose**: Implement ToRow for primitives

**Audit**:
- [x] Uses `ToSql` trait for type-safe serialization
- [x] Column name is static `"value"`

**Verdict**: PASS

---

### 4.3 `impl_to_row_tuple!` (line 244)

**Purpose**: Implement ToRow for tuples

**Sizes**: 2-8 element tuples

**Audit**:
- [x] Iterates through tuple elements correctly
- [x] Column names are compile-time strings

**Verdict**: PASS

---

### 4.4 `impl_to_sql_literal_int!` (line 344)

**Purpose**: Convert integers to SQL literals

**Audit**:
- [x] Uses `itoa::Buffer` - fast, correct formatting
- [x] Returns `Cow::Owned` - no injection possible

**Verdict**: PASS

---

### 4.5 `impl_to_sql_literal_float!` (line 361)

**Purpose**: Convert floats to SQL literals

**Audit**:
- [x] Uses `ryu::Buffer` - fast, correct formatting
- [x] Same safety properties as integer version

**Verdict**: PASS

---

## 5. Deserialization Macros (deserialize.rs)

### 5.1 `impl_from_row_primitive!` (line 37)

**Purpose**: Implement FromRow for primitives

**Audit**:
- [x] Uses `FromClickHouse` trait
- [x] Proper error handling with `QueryResult`

**Verdict**: PASS

---

### 5.2 `impl_from_row_tuple!` (line 114)

**Purpose**: Implement FromRow for tuples

**Sizes**: 1-8 element tuples

**Audit**:
- [x] Uses `IndexedColumnRow` wrapper for column access
- [x] Recursive deserialization of elements

**Verdict**: PASS

---

### 5.3 `impl_queryable_primitive!` (line 225)

**Purpose**: Implement Queryable for primitives

**Audit**:
- [x] Simple passthrough - `Row = Self`
- [x] No transformation needed

**Verdict**: PASS

---

### 5.4 `impl_queryable_tuple!` (line 255)

**Purpose**: Implement Queryable for tuples

**Sizes**: 1-8 element tuples

**Audit**:
- [x] Recursively builds tuple from row elements
- [x] Proper error propagation

**Verdict**: PASS

---

## 6. Backend Macros (backend.rs)

### 6.1 `impl_to_bindable!` (line 203)

**Purpose**: Implement ToBindableValue for primitives

**Audit**:
- [x] Direct enum variant creation
- [x] `#[inline]` for performance
- [x] No allocation for primitives

**Verdict**: PASS

---

### 6.2 `impl_into_bindable!` (line 245)

**Purpose**: Implement IntoBindableValue (consuming version)

**Audit**:
- [x] Same pattern as `impl_to_bindable!`
- [x] Allows move semantics for String

**Verdict**: PASS

---

## 7. Query Builder Macros

### 7.1 `impl_query_fragment_tuple!` (query_builder/mod.rs:105)

**Purpose**: Implement QueryFragment for tuples

**Sizes**: 1-8 element tuples

**Audit**:
- [x] Comma-separated output
- [x] Delegates to each element's `walk_ast()`

**Verdict**: PASS

---

### 7.2 `impl_cte_list_tuple!` (with.rs:131)

**Purpose**: Implement CteList for CTE tuples

**Sizes**: 2-12 CTEs

**Audit**:
- [x] Comma-separated CTE definitions
- [x] Recursive pattern for larger tuples

**Verdict**: PASS

---

### 7.3 `impl_with_queries_builder!` (with.rs:311)

**Purpose**: Builder pattern for WITH clause

**Sizes**: 1-12 CTEs

**Audit**:
- [x] Type-safe builder chain
- [x] Returns proper `WithClause` type

**Verdict**: PASS

---

### 7.4 `impl_as_changeset_tuple!` (update.rs:131)

**Purpose**: Implement AsChangeset for tuples

**Sizes**: 1-8 elements

**Audit**:
- [x] Propagates Target type constraint
- [x] Self-returning changeset

**Verdict**: PASS

---

### 7.5 `impl_assignments_tuple!` (update.rs:191)

**Purpose**: Implement QueryFragment for UPDATE assignments

**Sizes**: 1-8 assignments

**Audit**:
- [x] Comma-separated assignments
- [x] Delegates to each `Assign` type

**Verdict**: PASS

---

## Security Analysis

### SQL Injection Prevention

All macros follow safe patterns:

1. **Static SQL Keywords**: Used via `push_sql("literal")` - compile-time strings only
2. **User Values**: Always passed through `push_bindable()` which creates parameterized bindings
3. **Identifiers**: Escaped via `push_identifier()` (not in these macros, but in column/table refs)
4. **No String Interpolation**: No `format!()` or string concatenation with user values

### Type Safety

1. **Compile-Time Type Checking**: All macros enforce proper trait bounds
2. **SqlType Propagation**: Return types correctly derived from input types
3. **Nullable Handling**: Option types properly map to Nullable SQL types

---

## Recommendations

### Minor Improvements (P3)

1. **Documentation**: Add module-level doc examples for operator macros
2. **Consistency**: Consider renaming `impl_in_query_fragment!` to match naming pattern (`impl_query_fragment_in!`)

### No Action Required

All macros are production-ready with no security or correctness issues.

---

## Conclusion

The diesel-clickhouse-core macro system is:

- **Secure**: No SQL injection vectors
- **Type-Safe**: Compile-time type checking throughout
- **Consistent**: Follows uniform patterns across all macro categories
- **Maintainable**: Clear separation of concerns

**Audit Status**: APPROVED
