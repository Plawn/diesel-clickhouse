//! Zero-copy parsing for ClickHouse query results.
//!
//! This module provides a zero-copy API for processing query results without
//! allocating owned data structures. Instead of deserializing into owned types,
//! you work with borrowed references directly into the response buffer.
//!
//! # When to Use
//!
//! - Processing large result sets where allocation overhead matters
//! - Simple data transformations (aggregation, filtering, writing to files)
//! - When you don't need to store the parsed data beyond the callback
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//!
//! // Process rows without allocating String/Vec for each row
//! conn.load_zero_copy(
//!     "SELECT id, name, score FROM users",
//!     &["id", "name", "score"],
//!     |row| {
//!         let id: u64 = row.get_u64("id")?;
//!         let name: &str = row.get_str("name")?;  // Borrowed from buffer!
//!         let score: f64 = row.get_f64("score")?;
//!
//!         println!("User {}: {} (score: {})", id, name, score);
//!         Ok(())
//!     }
//! ).await?;
//! ```
//!
//! # Format
//!
//! Uses ClickHouse's TabSeparated format internally, which is optimal for
//! zero-copy parsing (no JSON escaping, simple field delimiters).

use smallvec::SmallVec;
use std::collections::HashMap;

use crate::core::result::{Error, QueryResult};

// =============================================================================
// Borrowed Value
// =============================================================================

/// A borrowed reference to a value in the response buffer.
///
/// This type provides zero-copy access to field values. The underlying
/// bytes are borrowed from the response buffer and remain valid only
/// within the callback scope.
#[derive(Debug, Clone, Copy)]
pub struct BorrowedValue<'a> {
    bytes: &'a [u8],
}

impl<'a> BorrowedValue<'a> {
    /// Create a new borrowed value from a byte slice.
    #[inline]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Get the raw bytes.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Try to interpret the value as a UTF-8 string.
    #[inline]
    pub fn as_str(&self) -> Result<&'a str, Error> {
        std::str::from_utf8(self.bytes)
            .map_err(|e| Error::DeserializationError(format!("Invalid UTF-8: {}", e)))
    }

    /// Check if the value represents NULL (ClickHouse uses `\N` in TSV).
    #[inline]
    pub fn is_null(&self) -> bool {
        self.bytes == b"\\N"
    }

    /// Check if the value is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Parse as u64.
    #[inline]
    pub fn parse_u64(&self) -> Result<u64, Error> {
        if self.is_null() {
            return Err(Error::DeserializationError("Cannot parse NULL as u64".to_string()));
        }
        let s = self.as_str()?;
        s.parse()
            .map_err(|e| Error::DeserializationError(format!("Invalid u64 '{}': {}", s, e)))
    }

    /// Parse as i64.
    #[inline]
    pub fn parse_i64(&self) -> Result<i64, Error> {
        if self.is_null() {
            return Err(Error::DeserializationError("Cannot parse NULL as i64".to_string()));
        }
        let s = self.as_str()?;
        s.parse()
            .map_err(|e| Error::DeserializationError(format!("Invalid i64 '{}': {}", s, e)))
    }

    /// Parse as f64.
    #[inline]
    pub fn parse_f64(&self) -> Result<f64, Error> {
        if self.is_null() {
            return Err(Error::DeserializationError("Cannot parse NULL as f64".to_string()));
        }
        let s = self.as_str()?;
        s.parse()
            .map_err(|e| Error::DeserializationError(format!("Invalid f64 '{}': {}", s, e)))
    }

    /// Parse as bool (ClickHouse uses 0/1 in TSV).
    #[inline]
    pub fn parse_bool(&self) -> Result<bool, Error> {
        if self.is_null() {
            return Err(Error::DeserializationError("Cannot parse NULL as bool".to_string()));
        }
        match self.bytes {
            b"1" | b"true" | b"True" | b"TRUE" => Ok(true),
            b"0" | b"false" | b"False" | b"FALSE" => Ok(false),
            _ => {
                let s = self.as_str().unwrap_or("<invalid utf8>");
                Err(Error::DeserializationError(format!("Invalid bool: '{}'", s)))
            }
        }
    }

    /// Parse as optional u64 (returns None for NULL).
    #[inline]
    pub fn parse_optional_u64(&self) -> Result<Option<u64>, Error> {
        if self.is_null() {
            Ok(None)
        } else {
            self.parse_u64().map(Some)
        }
    }

    /// Parse as optional i64 (returns None for NULL).
    #[inline]
    pub fn parse_optional_i64(&self) -> Result<Option<i64>, Error> {
        if self.is_null() {
            Ok(None)
        } else {
            self.parse_i64().map(Some)
        }
    }

    /// Parse as optional f64 (returns None for NULL).
    #[inline]
    pub fn parse_optional_f64(&self) -> Result<Option<f64>, Error> {
        if self.is_null() {
            Ok(None)
        } else {
            self.parse_f64().map(Some)
        }
    }

    /// Parse as optional string (returns None for NULL).
    #[inline]
    pub fn as_optional_str(&self) -> Result<Option<&'a str>, Error> {
        if self.is_null() {
            Ok(None)
        } else {
            self.as_str().map(Some)
        }
    }
}

// =============================================================================
// Zero-Copy Row
// =============================================================================

/// A row of borrowed values from a query result.
///
/// The row provides access to field values by index or by name.
/// All values are borrowed from the underlying response buffer.
pub struct ZeroCopyRow<'a, 'b> {
    /// Column values (inline storage for small rows)
    values: SmallVec<[BorrowedValue<'a>; 16]>,
    /// Column name to index mapping
    column_indices: &'b HashMap<Box<str>, usize>,
}

impl<'a, 'b> ZeroCopyRow<'a, 'b> {
    /// Create a new row from values and column mapping.
    #[inline]
    pub(crate) fn new(
        values: SmallVec<[BorrowedValue<'a>; 16]>,
        column_indices: &'b HashMap<Box<str>, usize>,
    ) -> Self {
        Self { values, column_indices }
    }

    /// Get a value by column index.
    #[inline]
    pub fn get_by_index(&self, index: usize) -> Result<BorrowedValue<'a>, Error> {
        self.values
            .get(index)
            .copied()
            .ok_or_else(|| Error::DeserializationError(format!("Column index {} out of bounds", index)))
    }

    /// Get a value by column name.
    #[inline]
    pub fn get(&self, name: &str) -> Result<BorrowedValue<'a>, Error> {
        let index = self.column_indices
            .get(name)
            .ok_or_else(|| Error::DeserializationError(format!("Unknown column: {}", name)))?;
        self.get_by_index(*index)
    }

    /// Get a string value by name (zero-copy).
    #[inline]
    pub fn get_str(&self, name: &str) -> Result<&'a str, Error> {
        self.get(name)?.as_str()
    }

    /// Get an optional string value by name (zero-copy).
    #[inline]
    pub fn get_optional_str(&self, name: &str) -> Result<Option<&'a str>, Error> {
        self.get(name)?.as_optional_str()
    }

    /// Get a u64 value by name.
    #[inline]
    pub fn get_u64(&self, name: &str) -> Result<u64, Error> {
        self.get(name)?.parse_u64()
    }

    /// Get an optional u64 value by name.
    #[inline]
    pub fn get_optional_u64(&self, name: &str) -> Result<Option<u64>, Error> {
        self.get(name)?.parse_optional_u64()
    }

    /// Get an i64 value by name.
    #[inline]
    pub fn get_i64(&self, name: &str) -> Result<i64, Error> {
        self.get(name)?.parse_i64()
    }

    /// Get an optional i64 value by name.
    #[inline]
    pub fn get_optional_i64(&self, name: &str) -> Result<Option<i64>, Error> {
        self.get(name)?.parse_optional_i64()
    }

    /// Get an f64 value by name.
    #[inline]
    pub fn get_f64(&self, name: &str) -> Result<f64, Error> {
        self.get(name)?.parse_f64()
    }

    /// Get an optional f64 value by name.
    #[inline]
    pub fn get_optional_f64(&self, name: &str) -> Result<Option<f64>, Error> {
        self.get(name)?.parse_optional_f64()
    }

    /// Get a bool value by name.
    #[inline]
    pub fn get_bool(&self, name: &str) -> Result<bool, Error> {
        self.get(name)?.parse_bool()
    }

    /// Get the raw bytes for a column.
    #[inline]
    pub fn get_bytes(&self, name: &str) -> Result<&'a [u8], Error> {
        Ok(self.get(name)?.as_bytes())
    }

    /// Check if a column value is NULL.
    #[inline]
    pub fn is_null(&self, name: &str) -> Result<bool, Error> {
        Ok(self.get(name)?.is_null())
    }

    /// Get the number of columns in this row.
    #[inline]
    pub fn column_count(&self) -> usize {
        self.values.len()
    }
}

// =============================================================================
// TSV Parser
// =============================================================================

/// Parser for ClickHouse's TabSeparated format.
///
/// This parser operates on borrowed byte slices without allocating
/// for each field value.
///
/// Uses `Box<str>` internally for column names (more compact than String).
pub struct TsvParser<'a> {
    data: &'a [u8],
    column_indices: HashMap<Box<str>, usize>,
}

impl<'a> TsvParser<'a> {
    /// Create a new parser with the given column names.
    ///
    /// Column names must match the order of columns in the query result.
    pub fn new(data: &'a [u8], columns: &[&str]) -> Self {
        let column_indices: HashMap<Box<str>, usize> = columns
            .iter()
            .enumerate()
            .map(|(i, name)| (Box::from(*name), i))
            .collect();

        Self { data, column_indices }
    }

    /// Parse a single line into a row.
    ///
    /// Returns None if the line is empty.
    #[inline]
    fn parse_line<'b>(&'b self, line: &'a [u8]) -> Option<ZeroCopyRow<'a, 'b>> {
        if line.is_empty() {
            return None;
        }

        let mut values = SmallVec::with_capacity(self.column_indices.len());
        let mut start = 0;

        for (i, &byte) in line.iter().enumerate() {
            if byte == b'\t' {
                values.push(BorrowedValue::new(&line[start..i]));
                start = i + 1;
            }
        }

        // Last field (no trailing tab)
        values.push(BorrowedValue::new(&line[start..]));

        Some(ZeroCopyRow::new(values, &self.column_indices))
    }

    /// Iterate over rows, calling the callback for each one.
    ///
    /// Returns the number of rows processed.
    pub fn for_each<F>(&self, mut callback: F) -> QueryResult<usize>
    where
        F: for<'b> FnMut(ZeroCopyRow<'a, 'b>) -> QueryResult<()>,
    {
        let mut count = 0;
        let mut start = 0;

        for (i, &byte) in self.data.iter().enumerate() {
            if byte == b'\n' {
                let line = &self.data[start..i];
                // Handle \r\n line endings
                let line = line.strip_suffix(b"\r").unwrap_or(line);

                if let Some(row) = self.parse_line(line) {
                    callback(row)?;
                    count += 1;
                }
                start = i + 1;
            }
        }

        // Handle last line (no trailing newline)
        if start < self.data.len() {
            let line = &self.data[start..];
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if let Some(row) = self.parse_line(line) {
                callback(row)?;
                count += 1;
            }
        }

        Ok(count)
    }
}

// =============================================================================
// Streaming TSV Parser
// =============================================================================

/// A streaming TSV parser that processes data chunk by chunk.
///
/// This parser maintains internal state to handle rows that span chunk boundaries.
///
/// Uses `Box<str>` internally for column names (more compact than String).
pub struct StreamingTsvParser {
    column_indices: HashMap<Box<str>, usize>,
    buffer: Vec<u8>,
}

impl StreamingTsvParser {
    /// Create a new streaming parser with the given column names.
    pub fn new(columns: &[&str]) -> Self {
        let column_indices: HashMap<Box<str>, usize> = columns
            .iter()
            .enumerate()
            .map(|(i, name)| (Box::from(*name), i))
            .collect();

        Self {
            column_indices,
            buffer: Vec::with_capacity(4096),
        }
    }

    /// Process a chunk of data, calling the callback for each complete row.
    ///
    /// Returns the number of rows processed from this chunk.
    pub fn process_chunk<F>(&mut self, chunk: &[u8], mut callback: F) -> QueryResult<usize>
    where
        F: for<'a, 'b> FnMut(ZeroCopyRow<'a, 'b>) -> QueryResult<()>,
    {
        self.buffer.extend_from_slice(chunk);

        let mut count = 0;
        let mut start = 0;

        // Process complete lines
        for (i, &byte) in self.buffer.iter().enumerate() {
            if byte == b'\n' {
                let line = &self.buffer[start..i];
                let line = line.strip_suffix(b"\r").unwrap_or(line);

                if !line.is_empty() {
                    let row = self.parse_line(line);
                    callback(row)?;
                    count += 1;
                }
                start = i + 1;
            }
        }

        // Remove processed data from buffer
        if start > 0 {
            self.buffer.drain(..start);
        }

        Ok(count)
    }

    /// Finish processing, handling any remaining data in the buffer.
    pub fn finish<F>(&mut self, mut callback: F) -> QueryResult<usize>
    where
        F: for<'a, 'b> FnMut(ZeroCopyRow<'a, 'b>) -> QueryResult<()>,
    {
        if self.buffer.is_empty() {
            return Ok(0);
        }

        let line = self.buffer.strip_suffix(b"\r").unwrap_or(&self.buffer);
        let line = line.strip_suffix(b"\n").unwrap_or(line);

        if line.is_empty() {
            return Ok(0);
        }

        let row = self.parse_line(line);
        callback(row)?;
        self.buffer.clear();
        Ok(1)
    }

    #[inline]
    fn parse_line<'a, 'b>(&'b self, line: &'a [u8]) -> ZeroCopyRow<'a, 'b> {
        let mut values = SmallVec::with_capacity(self.column_indices.len());
        let mut start = 0;

        for (i, &byte) in line.iter().enumerate() {
            if byte == b'\t' {
                values.push(BorrowedValue::new(&line[start..i]));
                start = i + 1;
            }
        }

        // Last field
        values.push(BorrowedValue::new(&line[start..]));

        ZeroCopyRow::new(values, &self.column_indices)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borrowed_value_parsing() {
        let val = BorrowedValue::new(b"42");
        assert_eq!(val.parse_u64().unwrap(), 42);
        assert_eq!(val.parse_i64().unwrap(), 42);

        let val = BorrowedValue::new(b"-123");
        assert_eq!(val.parse_i64().unwrap(), -123);

        let val = BorrowedValue::new(b"3.14");
        assert!((val.parse_f64().unwrap() - 3.14).abs() < 0.001);

        let val = BorrowedValue::new(b"1");
        assert!(val.parse_bool().unwrap());

        let val = BorrowedValue::new(b"0");
        assert!(!val.parse_bool().unwrap());
    }

    #[test]
    fn test_null_handling() {
        let val = BorrowedValue::new(b"\\N");
        assert!(val.is_null());
        assert!(val.parse_optional_u64().unwrap().is_none());
        assert!(val.as_optional_str().unwrap().is_none());
    }

    #[test]
    fn test_tsv_parser() {
        let data = b"1\talice\t100\n2\tbob\t200\n";
        let parser = TsvParser::new(data, &["id", "name", "score"]);

        let mut rows = Vec::new();
        parser
            .for_each(|row| {
                rows.push((
                    row.get_u64("id").unwrap(),
                    row.get_str("name").unwrap().to_string(),
                    row.get_u64("score").unwrap(),
                ));
                Ok(())
            })
            .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], (1, "alice".to_string(), 100));
        assert_eq!(rows[1], (2, "bob".to_string(), 200));
    }

    #[test]
    fn test_streaming_parser() {
        let columns = ["id", "name"];
        let mut parser = StreamingTsvParser::new(&columns);

        // First chunk ends mid-row
        let chunk1 = b"1\talice\n2\tbo";
        let mut count1 = 0;
        parser
            .process_chunk(chunk1, |_row| {
                count1 += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(count1, 1); // Only first row complete

        // Second chunk completes the row
        let chunk2 = b"b\n";
        let mut count2 = 0;
        parser
            .process_chunk(chunk2, |row| {
                assert_eq!(row.get_str("name").unwrap(), "bob");
                count2 += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(count2, 1);
    }
}
