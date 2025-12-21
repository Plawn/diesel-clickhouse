//! SQL escaping utilities for ClickHouse.
//!
//! This module provides centralized functions for escaping SQL values and identifiers
//! to prevent SQL injection attacks.

use std::borrow::Cow;

/// Escape a string value for use in SQL single-quoted strings.
///
/// Escapes single quotes by doubling them (`'` -> `''`).
///
/// # Example
///
/// ```
/// use diesel_clickhouse_core::escape::escape_sql_string;
///
/// assert_eq!(escape_sql_string("hello"), "hello");
/// assert_eq!(escape_sql_string("O'Brien"), "O''Brien");
/// assert_eq!(escape_sql_string("It's a 'test'"), "It''s a ''test''");
/// ```
#[inline]
pub fn escape_sql_string(s: &str) -> Cow<'_, str> {
    if s.contains('\'') {
        Cow::Owned(s.replace('\'', "''"))
    } else {
        Cow::Borrowed(s)
    }
}

/// Escape a string value, always returning an owned String.
///
/// Use this when you need an owned String regardless of whether escaping occurred.
#[inline]
pub fn escape_sql_string_owned(s: &str) -> String {
    if s.contains('\'') {
        s.replace('\'', "''")
    } else {
        s.to_string()
    }
}

/// Escape an identifier for use in SQL (table names, column names).
///
/// Wraps the identifier in backticks and escapes any backticks within by doubling them.
///
/// # Example
///
/// ```
/// use diesel_clickhouse_core::escape::escape_identifier;
///
/// assert_eq!(escape_identifier("users"), "`users`");
/// assert_eq!(escape_identifier("my`table"), "`my``table`");
/// ```
#[inline]
pub fn escape_identifier(s: &str) -> String {
    if s.contains('`') {
        format!("`{}`", s.replace('`', "``"))
    } else {
        format!("`{}`", s)
    }
}

/// Check if a string needs SQL escaping.
///
/// Returns true if the string contains characters that need escaping.
#[inline]
pub fn needs_string_escaping(s: &str) -> bool {
    s.contains('\'')
}

/// Check if an identifier needs escaping.
///
/// Returns true if the identifier contains backticks.
#[inline]
pub fn needs_identifier_escaping(s: &str) -> bool {
    s.contains('`')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_sql_string_no_quotes() {
        assert_eq!(escape_sql_string("hello world"), "hello world");
        assert!(matches!(escape_sql_string("hello"), Cow::Borrowed(_)));
    }

    #[test]
    fn test_escape_sql_string_with_quotes() {
        assert_eq!(escape_sql_string("O'Brien"), "O''Brien");
        assert_eq!(escape_sql_string("It's"), "It''s");
        assert_eq!(escape_sql_string("'quoted'"), "''quoted''");
        assert!(matches!(escape_sql_string("O'Brien"), Cow::Owned(_)));
    }

    #[test]
    fn test_escape_sql_string_multiple_quotes() {
        assert_eq!(escape_sql_string("'''"), "''''''");
        assert_eq!(escape_sql_string("a'b'c"), "a''b''c");
    }

    #[test]
    fn test_escape_identifier_simple() {
        assert_eq!(escape_identifier("users"), "`users`");
        assert_eq!(escape_identifier("my_table"), "`my_table`");
    }

    #[test]
    fn test_escape_identifier_with_backticks() {
        assert_eq!(escape_identifier("my`table"), "`my``table`");
        assert_eq!(escape_identifier("`weird`"), "```weird```");
    }

    #[test]
    fn test_sql_injection_prevention() {
        // Test that SQL injection attempts are properly escaped
        let malicious = "'; DROP TABLE users; --";
        let escaped = escape_sql_string(malicious);
        assert_eq!(escaped, "''; DROP TABLE users; --");
        // The escaped string, when wrapped in quotes, would be:
        // '''' DROP TABLE users; --'
        // which is just a string literal, not executable SQL

        let malicious_id = "users`; DROP TABLE users; --";
        let escaped_id = escape_identifier(malicious_id);
        assert_eq!(escaped_id, "`users``; DROP TABLE users; --`");
    }

    #[test]
    fn test_needs_escaping() {
        assert!(!needs_string_escaping("hello"));
        assert!(needs_string_escaping("it's"));

        assert!(!needs_identifier_escaping("users"));
        assert!(needs_identifier_escaping("my`table"));
    }
}
