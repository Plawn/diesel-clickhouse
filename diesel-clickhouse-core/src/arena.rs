//! Arena allocator for efficient query building.
//!
//! This module provides arena-based allocation for complex query building,
//! reducing allocation overhead when constructing large or nested queries.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse_core::arena::QueryArena;
//!
//! let arena = QueryArena::new();
//!
//! // Allocate strings in the arena (no individual heap allocations)
//! let table_name = arena.alloc_str("users");
//! let column1 = arena.alloc_str("id");
//! let column2 = arena.alloc_str("name");
//!
//! // Build query parts using arena-allocated strings
//! let query = arena.alloc_fmt(format_args!(
//!     "SELECT {}, {} FROM {}",
//!     column1, column2, table_name
//! ));
//!
//! // All memory is freed when arena is dropped
//! ```

use bumpalo::Bump;
use std::cell::RefCell;
use std::fmt;

/// Thread-local arena for query building.
///
/// Provides fast, bump-allocated memory for temporary strings
/// during query construction. Memory is released in bulk when
/// the arena is reset or dropped.
pub struct QueryArena {
    bump: Bump,
}

impl QueryArena {
    /// Create a new query arena.
    pub fn new() -> Self {
        Self {
            bump: Bump::new(),
        }
    }

    /// Create an arena with pre-allocated capacity.
    ///
    /// Use this when you know approximately how much memory you'll need.
    pub fn with_capacity(bytes: usize) -> Self {
        Self {
            bump: Bump::with_capacity(bytes),
        }
    }

    /// Allocate a string slice in the arena.
    ///
    /// Returns a reference that lives as long as the arena.
    #[inline]
    pub fn alloc_str(&self, s: &str) -> &str {
        self.bump.alloc_str(s)
    }

    /// Allocate a formatted string in the arena.
    #[inline]
    pub fn alloc_fmt(&self, args: fmt::Arguments<'_>) -> &str {
        use bumpalo::collections::String as BumpString;
        use std::fmt::Write;

        let mut s = BumpString::new_in(&self.bump);
        let _ = s.write_fmt(args);
        s.into_bump_str()
    }

    /// Allocate and join strings with a separator.
    pub fn join(&self, parts: &[&str], sep: &str) -> &str {
        if parts.is_empty() {
            return self.alloc_str("");
        }

        // Calculate total length
        let total_len: usize = parts.iter().map(|s| s.len()).sum();
        let sep_len = sep.len() * (parts.len().saturating_sub(1));

        use bumpalo::collections::String as BumpString;
        let mut result = BumpString::with_capacity_in(total_len + sep_len, &self.bump);

        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                result.push_str(sep);
            }
            result.push_str(part);
        }

        result.into_bump_str()
    }

    /// Allocate a Vec in the arena.
    #[inline]
    pub fn alloc_vec<T>(&self) -> bumpalo::collections::Vec<'_, T> {
        bumpalo::collections::Vec::new_in(&self.bump)
    }

    /// Allocate a Vec with capacity in the arena.
    #[inline]
    pub fn alloc_vec_with_capacity<T>(&self, capacity: usize) -> bumpalo::collections::Vec<'_, T> {
        bumpalo::collections::Vec::with_capacity_in(capacity, &self.bump)
    }

    /// Allocate a String in the arena.
    #[inline]
    pub fn alloc_string(&self) -> bumpalo::collections::String<'_> {
        bumpalo::collections::String::new_in(&self.bump)
    }

    /// Allocate a String with capacity in the arena.
    #[inline]
    pub fn alloc_string_with_capacity(&self, capacity: usize) -> bumpalo::collections::String<'_> {
        bumpalo::collections::String::with_capacity_in(capacity, &self.bump)
    }

    /// Get the total bytes allocated by this arena.
    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }

    /// Reset the arena, deallocating all memory.
    ///
    /// This is faster than dropping and recreating the arena
    /// because it reuses the underlying memory.
    pub fn reset(&mut self) {
        self.bump.reset();
    }
}

impl Default for QueryArena {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for QueryArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueryArena")
            .field("allocated_bytes", &self.allocated_bytes())
            .finish()
    }
}

// Thread-local query arena for zero-allocation query building.
// This provides a convenient way to use arena allocation without
// passing the arena explicitly through your code.
thread_local! {
    static THREAD_ARENA: RefCell<QueryArena> = RefCell::new(QueryArena::with_capacity(4096));
}

/// Execute a function with access to the thread-local arena.
///
/// The arena is automatically reset after each use to prevent
/// unbounded memory growth.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse_core::arena::with_arena;
///
/// let sql = with_arena(|arena| {
///     let parts = [
///         arena.alloc_str("SELECT * FROM "),
///         arena.alloc_str("users"),
///         arena.alloc_str(" WHERE id = 1"),
///     ];
///     arena.join(&parts, "").to_owned()
/// });
/// ```
pub fn with_arena<F, R>(f: F) -> R
where
    F: FnOnce(&QueryArena) -> R,
{
    THREAD_ARENA.with(|arena| {
        let result = f(&arena.borrow());
        // Reset arena after use to prevent memory growth
        arena.borrow_mut().reset();
        result
    })
}

/// Arena-backed query builder for complex queries.
///
/// This builder uses arena allocation to minimize heap allocations
/// when constructing complex queries with many parts.
pub struct ArenaQueryBuilder<'a> {
    arena: &'a QueryArena,
    parts: bumpalo::collections::Vec<'a, &'a str>,
}

impl<'a> ArenaQueryBuilder<'a> {
    /// Create a new arena-backed query builder.
    pub fn new(arena: &'a QueryArena) -> Self {
        Self {
            arena,
            parts: arena.alloc_vec_with_capacity(16),
        }
    }

    /// Push a string part to the query.
    #[inline]
    pub fn push(&mut self, s: &str) {
        self.parts.push(self.arena.alloc_str(s));
    }

    /// Push a formatted string to the query.
    #[inline]
    pub fn push_fmt(&mut self, args: fmt::Arguments<'_>) {
        self.parts.push(self.arena.alloc_fmt(args));
    }

    /// Push an identifier (quoted with backticks).
    pub fn push_identifier(&mut self, id: &str) {
        if id.contains('`') {
            // Escape backticks
            let escaped = self.arena.alloc_fmt(format_args!(
                "`{}`",
                id.replace('`', "``")
            ));
            self.parts.push(escaped);
        } else {
            self.parts.push(self.arena.alloc_fmt(format_args!("`{}`", id)));
        }
    }

    /// Push a string literal (quoted with single quotes).
    pub fn push_string_literal(&mut self, s: &str) {
        if s.contains('\'') {
            let escaped = self.arena.alloc_fmt(format_args!(
                "'{}'",
                s.replace('\'', "''")
            ));
            self.parts.push(escaped);
        } else {
            self.parts.push(self.arena.alloc_fmt(format_args!("'{}'", s)));
        }
    }

    /// Push an integer literal.
    #[inline]
    pub fn push_int<T: itoa::Integer>(&mut self, n: T) {
        let mut buf = itoa::Buffer::new();
        self.parts.push(self.arena.alloc_str(buf.format(n)));
    }

    /// Push a float literal.
    #[inline]
    pub fn push_float<T: ryu::Float>(&mut self, n: T) {
        let mut buf = ryu::Buffer::new();
        self.parts.push(self.arena.alloc_str(buf.format_finite(n)));
    }

    /// Get the number of parts.
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Build the final query string.
    ///
    /// This allocates the final string on the heap.
    pub fn finish(&self) -> String {
        // Calculate total length
        let total_len: usize = self.parts.iter().map(|s| s.len()).sum();
        let mut result = String::with_capacity(total_len);
        for part in self.parts.iter() {
            result.push_str(part);
        }
        result
    }

    /// Build the final query string in the arena.
    ///
    /// Returns a reference that lives as long as the arena.
    pub fn finish_in_arena(&self) -> &'a str {
        self.arena.join(&self.parts, "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_alloc_str() {
        let arena = QueryArena::new();
        let s1 = arena.alloc_str("hello");
        let s2 = arena.alloc_str("world");
        assert_eq!(s1, "hello");
        assert_eq!(s2, "world");
    }

    #[test]
    fn test_arena_alloc_fmt() {
        let arena = QueryArena::new();
        let s = arena.alloc_fmt(format_args!("SELECT {} FROM {}", "*", "users"));
        assert_eq!(s, "SELECT * FROM users");
    }

    #[test]
    fn test_arena_join() {
        let arena = QueryArena::new();
        let parts = ["a", "b", "c"];
        let joined = arena.join(&parts, ", ");
        assert_eq!(joined, "a, b, c");
    }

    #[test]
    fn test_arena_reset() {
        let mut arena = QueryArena::new();
        let _ = arena.alloc_str("some data");
        let bytes_before = arena.allocated_bytes();
        assert!(bytes_before > 0);

        arena.reset();
        // After reset, arena can be reused
        let _ = arena.alloc_str("new data");
    }

    #[test]
    fn test_with_arena() {
        let result = with_arena(|arena| {
            let s = arena.alloc_str("test");
            s.to_owned()
        });
        assert_eq!(result, "test");
    }

    #[test]
    fn test_arena_query_builder() {
        let arena = QueryArena::new();
        let mut builder = ArenaQueryBuilder::new(&arena);

        builder.push("SELECT ");
        builder.push_identifier("name");
        builder.push(", ");
        builder.push_identifier("age");
        builder.push(" FROM ");
        builder.push_identifier("users");
        builder.push(" WHERE ");
        builder.push_identifier("id");
        builder.push(" = ");
        builder.push_int(42u32);

        let sql = builder.finish();
        assert_eq!(sql, "SELECT `name`, `age` FROM `users` WHERE `id` = 42");
    }

    #[test]
    fn test_arena_query_builder_string_literal() {
        let arena = QueryArena::new();
        let mut builder = ArenaQueryBuilder::new(&arena);

        builder.push("SELECT * FROM users WHERE name = ");
        builder.push_string_literal("O'Brien");

        let sql = builder.finish();
        assert_eq!(sql, "SELECT * FROM users WHERE name = 'O''Brien'");
    }
}
