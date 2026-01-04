//! SQL escaping utilities for ClickHouse.
//!
//! This module provides centralized functions for escaping SQL values and identifiers
//! to prevent SQL injection attacks.
//!
//! # Performance
//!
//! All functions use SIMD-accelerated scanning via `memchr` for the fast path
//! (when no escaping is needed), which is the common case.

use std::borrow::Cow;

/// Check if a string needs SQL escaping (contains `'` or `\`).
///
/// Uses SIMD-accelerated `memchr` for fast scanning on modern CPUs.
#[inline]
pub fn needs_sql_string_escape(s: &str) -> bool {
    memchr::memchr2(b'\'', b'\\', s.as_bytes()).is_some()
}

/// Escape a string value for use in SQL single-quoted strings.
///
/// Escapes single quotes by doubling them (`'` -> `''`) and
/// backslashes by doubling them (`\` -> `\\`).
///
/// Uses SIMD-accelerated `memchr` for the fast path (no escaping needed).
///
/// # Example
///
/// ```
/// use diesel_clickhouse_core::escape::escape_sql_string;
///
/// assert_eq!(escape_sql_string("hello"), "hello");
/// assert_eq!(escape_sql_string("O'Brien"), "O''Brien");
/// assert_eq!(escape_sql_string("It's a 'test'"), "It''s a ''test''");
/// assert_eq!(escape_sql_string("path\\to\\file"), "path\\\\to\\\\file");
/// ```
#[inline]
pub fn escape_sql_string(s: &str) -> Cow<'_, str> {
    if needs_sql_string_escape(s) {
        let mut result = String::with_capacity(s.len() + 4); // +4 for typical escaping
        for ch in s.chars() {
            match ch {
                '\'' => result.push_str("''"),
                '\\' => result.push_str("\\\\"),
                _ => result.push(ch),
            }
        }
        Cow::Owned(result)
    } else {
        Cow::Borrowed(s)
    }
}

/// Escape a string value, always returning an owned String.
///
/// Use this when you need an owned String regardless of whether escaping occurred.
#[inline]
pub fn escape_sql_string_owned(s: &str) -> String {
    if needs_sql_string_escape(s) {
        let mut result = String::with_capacity(s.len() + 4);
        for ch in s.chars() {
            match ch {
                '\'' => result.push_str("''"),
                '\\' => result.push_str("\\\\"),
                _ => result.push(ch),
            }
        }
        result
    } else {
        s.to_string()
    }
}

/// Write a SQL-escaped string literal to a buffer (including surrounding quotes).
///
/// Escapes single quotes by doubling them (`'` → `''`) and
/// backslashes by doubling them (`\` → `\\`).
///
/// Uses SIMD-accelerated detection via `memchr` for the fast path.
///
/// # Example
///
/// ```
/// use diesel_clickhouse_core::escape::write_escaped_sql_string;
///
/// let mut buf = String::new();
/// write_escaped_sql_string(&mut buf, "hello");
/// assert_eq!(buf, "'hello'");
///
/// buf.clear();
/// write_escaped_sql_string(&mut buf, "O'Brien");
/// assert_eq!(buf, "'O''Brien'");
/// ```
#[inline]
pub fn write_escaped_sql_string(buf: &mut String, value: &str) {
    buf.push('\'');
    if needs_sql_string_escape(value) {
        // Slow path: escape special characters
        for ch in value.chars() {
            match ch {
                '\'' => buf.push_str("''"),
                '\\' => buf.push_str("\\\\"),
                _ => buf.push(ch),
            }
        }
    } else {
        // Fast path: no escaping needed
        buf.push_str(value);
    }
    buf.push('\'');
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
    // Pre-allocate: 2 backticks + string length (+ extra for escaping if needed)
    let backtick_count = memchr::memchr_iter(b'`', s.as_bytes()).count();
    let mut result = String::with_capacity(s.len() + 2 + backtick_count);
    write_escaped_identifier(&mut result, s);
    result
}

/// Write an escaped identifier directly to a buffer.
///
/// This is more efficient than `escape_identifier` when building larger strings,
/// as it avoids intermediate allocations.
///
/// # Example
///
/// ```
/// use diesel_clickhouse_core::escape::write_escaped_identifier;
///
/// let mut sql = String::from("SELECT * FROM ");
/// write_escaped_identifier(&mut sql, "users");
/// assert_eq!(sql, "SELECT * FROM `users`");
/// ```
#[inline]
pub fn write_escaped_identifier(buf: &mut String, s: &str) {
    buf.push('`');
    if memchr::memchr(b'`', s.as_bytes()).is_some() {
        // Slow path: escape backticks
        for c in s.chars() {
            if c == '`' {
                buf.push_str("``");
            } else {
                buf.push(c);
            }
        }
    } else {
        // Fast path: no escaping needed
        buf.push_str(s);
    }
    buf.push('`');
}

/// Check if a string needs SQL escaping.
///
/// Returns true if the string contains characters that need escaping (`'` or `\`).
///
/// Uses SIMD-accelerated `memchr` for fast scanning.
#[inline]
#[deprecated(since = "0.2.0", note = "Use needs_sql_string_escape instead")]
pub fn needs_string_escaping(s: &str) -> bool {
    needs_sql_string_escape(s)
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
    fn test_escape_sql_string_no_special_chars() {
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
    fn test_escape_sql_string_with_backslashes() {
        assert_eq!(escape_sql_string("path\\to\\file"), "path\\\\to\\\\file");
        assert_eq!(escape_sql_string("\\"), "\\\\");
        assert!(matches!(escape_sql_string("a\\b"), Cow::Owned(_)));
    }

    #[test]
    fn test_escape_sql_string_mixed() {
        assert_eq!(escape_sql_string("it's a \\path"), "it''s a \\\\path");
        assert_eq!(escape_sql_string("'\\"), "''\\\\");
    }

    #[test]
    fn test_escape_sql_string_multiple_quotes() {
        assert_eq!(escape_sql_string("'''"), "''''''");
        assert_eq!(escape_sql_string("a'b'c"), "a''b''c");
    }

    #[test]
    fn test_write_escaped_sql_string() {
        let mut buf = String::new();
        write_escaped_sql_string(&mut buf, "hello");
        assert_eq!(buf, "'hello'");

        buf.clear();
        write_escaped_sql_string(&mut buf, "O'Brien");
        assert_eq!(buf, "'O''Brien'");

        buf.clear();
        write_escaped_sql_string(&mut buf, "path\\file");
        assert_eq!(buf, "'path\\\\file'");
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
    fn test_needs_sql_string_escape() {
        assert!(!needs_sql_string_escape("hello"));
        assert!(needs_sql_string_escape("it's"));
        assert!(needs_sql_string_escape("path\\file"));
        assert!(needs_sql_string_escape("it's a \\path"));
    }

    #[test]
    fn test_needs_identifier_escaping() {
        assert!(!needs_identifier_escaping("users"));
        assert!(needs_identifier_escaping("my`table"));
    }
}
