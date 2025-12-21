//! Zero-copy parsing for high-performance response handling.
//!
//! This module provides zero-copy parsing utilities that minimize memory
//! allocations when deserializing ClickHouse responses.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::zero_copy::{ZeroCopyRow, ZeroCopyParser, BorrowedValue};
//!
//! // Parse a TSV response without copying
//! let response = b"1\talice\t100\n2\tbob\t200\n";
//! let parser = ZeroCopyParser::tsv(response);
//!
//! for row in parser {
//!     let id: u64 = row.get(0)?.parse()?;
//!     let name: &str = row.get(1)?;  // Zero-copy reference!
//!     let score: i32 = row.get(2)?.parse()?;
//!     println!("{}: {} ({})", id, name, score);
//! }
//! ```

use std::borrow::Cow;
use std::str;

/// A borrowed value from a parsed response.
///
/// This type holds a reference to the original data without copying,
/// enabling zero-copy parsing of responses.
#[derive(Debug, Clone, Copy)]
pub struct BorrowedValue<'a> {
    bytes: &'a [u8],
}

impl<'a> BorrowedValue<'a> {
    /// Create a new borrowed value.
    #[inline]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Get the raw bytes.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Try to get as a UTF-8 string.
    #[inline]
    pub fn as_str(&self) -> Result<&'a str, str::Utf8Error> {
        str::from_utf8(self.bytes)
    }

    /// Get as UTF-8 string, with lossy conversion.
    #[inline]
    pub fn as_str_lossy(&self) -> Cow<'a, str> {
        String::from_utf8_lossy(self.bytes)
    }

    /// Check if the value is NULL (empty or literal "\\N").
    #[inline]
    pub fn is_null(&self) -> bool {
        self.bytes.is_empty() || self.bytes == b"\\N" || self.bytes == b"NULL"
    }

    /// Check if the value is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Get the length in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Parse as an integer.
    #[inline]
    pub fn parse_i64(&self) -> Result<i64, ParseError> {
        let s = self.as_str().map_err(|_| ParseError::InvalidUtf8)?;
        s.parse().map_err(|_| ParseError::InvalidInteger)
    }

    /// Parse as an unsigned integer.
    #[inline]
    pub fn parse_u64(&self) -> Result<u64, ParseError> {
        let s = self.as_str().map_err(|_| ParseError::InvalidUtf8)?;
        s.parse().map_err(|_| ParseError::InvalidInteger)
    }

    /// Parse as a float.
    #[inline]
    pub fn parse_f64(&self) -> Result<f64, ParseError> {
        let s = self.as_str().map_err(|_| ParseError::InvalidUtf8)?;
        s.parse().map_err(|_| ParseError::InvalidFloat)
    }

    /// Parse as a boolean.
    #[inline]
    pub fn parse_bool(&self) -> Result<bool, ParseError> {
        match self.bytes {
            b"1" | b"true" | b"True" | b"TRUE" => Ok(true),
            b"0" | b"false" | b"False" | b"FALSE" => Ok(false),
            _ => Err(ParseError::InvalidBoolean),
        }
    }

    /// Convert to owned String.
    pub fn to_string_owned(&self) -> Result<String, ParseError> {
        self.as_str()
            .map(|s| s.to_owned())
            .map_err(|_| ParseError::InvalidUtf8)
    }

    /// Convert to owned bytes.
    pub fn to_bytes_owned(&self) -> Vec<u8> {
        self.bytes.to_vec()
    }
}

/// Parsing errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Invalid UTF-8 sequence.
    InvalidUtf8,
    /// Invalid integer format.
    InvalidInteger,
    /// Invalid float format.
    InvalidFloat,
    /// Invalid boolean format.
    InvalidBoolean,
    /// Column index out of bounds.
    ColumnOutOfBounds { index: usize, count: usize },
    /// Unexpected end of input.
    UnexpectedEof,
    /// Invalid format.
    InvalidFormat(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::InvalidUtf8 => write!(f, "invalid UTF-8 sequence"),
            ParseError::InvalidInteger => write!(f, "invalid integer format"),
            ParseError::InvalidFloat => write!(f, "invalid float format"),
            ParseError::InvalidBoolean => write!(f, "invalid boolean format"),
            ParseError::ColumnOutOfBounds { index, count } => {
                write!(f, "column index {} out of bounds (count: {})", index, count)
            }
            ParseError::UnexpectedEof => write!(f, "unexpected end of input"),
            ParseError::InvalidFormat(msg) => write!(f, "invalid format: {}", msg),
        }
    }
}

impl std::error::Error for ParseError {}

/// A zero-copy row from a parsed response.
#[derive(Debug)]
pub struct ZeroCopyRow<'a> {
    values: Vec<BorrowedValue<'a>>,
}

impl<'a> ZeroCopyRow<'a> {
    /// Create a new row with the given values.
    pub fn new(values: Vec<BorrowedValue<'a>>) -> Self {
        Self { values }
    }

    /// Get the number of columns.
    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if the row is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Get a column by index.
    #[inline]
    pub fn get(&self, index: usize) -> Result<BorrowedValue<'a>, ParseError> {
        self.values.get(index).copied().ok_or(ParseError::ColumnOutOfBounds {
            index,
            count: self.values.len(),
        })
    }

    /// Get a column as a string slice.
    #[inline]
    pub fn get_str(&self, index: usize) -> Result<&'a str, ParseError> {
        self.get(index)?.as_str().map_err(|_| ParseError::InvalidUtf8)
    }

    /// Get a column as i64.
    #[inline]
    pub fn get_i64(&self, index: usize) -> Result<i64, ParseError> {
        self.get(index)?.parse_i64()
    }

    /// Get a column as u64.
    #[inline]
    pub fn get_u64(&self, index: usize) -> Result<u64, ParseError> {
        self.get(index)?.parse_u64()
    }

    /// Get a column as f64.
    #[inline]
    pub fn get_f64(&self, index: usize) -> Result<f64, ParseError> {
        self.get(index)?.parse_f64()
    }

    /// Get a column as bool.
    #[inline]
    pub fn get_bool(&self, index: usize) -> Result<bool, ParseError> {
        self.get(index)?.parse_bool()
    }

    /// Check if a column is NULL.
    #[inline]
    pub fn is_null(&self, index: usize) -> Result<bool, ParseError> {
        Ok(self.get(index)?.is_null())
    }

    /// Iterate over all values.
    pub fn iter(&self) -> impl Iterator<Item = BorrowedValue<'a>> + '_ {
        self.values.iter().copied()
    }
}

/// Zero-copy parser for TSV (Tab-Separated Values) format.
pub struct TsvParser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> TsvParser<'a> {
    /// Create a new TSV parser.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Parse the next row.
    pub fn next_row(&mut self) -> Option<ZeroCopyRow<'a>> {
        if self.pos >= self.data.len() {
            return None;
        }

        let start = self.pos;
        let mut values = Vec::with_capacity(8);
        let mut col_start = start;

        while self.pos < self.data.len() {
            match self.data[self.pos] {
                b'\t' => {
                    values.push(BorrowedValue::new(&self.data[col_start..self.pos]));
                    self.pos += 1;
                    col_start = self.pos;
                }
                b'\n' => {
                    values.push(BorrowedValue::new(&self.data[col_start..self.pos]));
                    self.pos += 1;
                    return Some(ZeroCopyRow::new(values));
                }
                _ => {
                    self.pos += 1;
                }
            }
        }

        // Handle last row without trailing newline
        if col_start < self.pos {
            values.push(BorrowedValue::new(&self.data[col_start..self.pos]));
        }

        if values.is_empty() {
            None
        } else {
            Some(ZeroCopyRow::new(values))
        }
    }

    /// Get remaining unparsed data.
    pub fn remaining(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    /// Check if parsing is complete.
    pub fn is_done(&self) -> bool {
        self.pos >= self.data.len()
    }
}

impl<'a> Iterator for TsvParser<'a> {
    type Item = ZeroCopyRow<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_row()
    }
}

/// Zero-copy parser for CSV format.
pub struct CsvParser<'a> {
    data: &'a [u8],
    pos: usize,
    delimiter: u8,
}

impl<'a> CsvParser<'a> {
    /// Create a new CSV parser with comma delimiter.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            delimiter: b',',
        }
    }

    /// Create with a custom delimiter.
    pub fn with_delimiter(data: &'a [u8], delimiter: u8) -> Self {
        Self {
            data,
            pos: 0,
            delimiter,
        }
    }

    /// Parse the next row, handling quoted values.
    pub fn next_row(&mut self) -> Option<ZeroCopyRow<'a>> {
        if self.pos >= self.data.len() {
            return None;
        }

        let mut values = Vec::with_capacity(8);

        loop {
            if self.pos >= self.data.len() {
                break;
            }

            // Check for quoted value
            if self.data[self.pos] == b'"' {
                self.pos += 1;
                let start = self.pos;

                // Find closing quote (handle escaped quotes "")
                while self.pos < self.data.len() {
                    if self.data[self.pos] == b'"' {
                        if self.pos + 1 < self.data.len() && self.data[self.pos + 1] == b'"' {
                            // Escaped quote, skip both
                            self.pos += 2;
                        } else {
                            // End of quoted value
                            break;
                        }
                    } else {
                        self.pos += 1;
                    }
                }

                values.push(BorrowedValue::new(&self.data[start..self.pos]));

                // Skip closing quote
                if self.pos < self.data.len() && self.data[self.pos] == b'"' {
                    self.pos += 1;
                }

                // Skip delimiter or newline
                if self.pos < self.data.len() {
                    if self.data[self.pos] == self.delimiter {
                        self.pos += 1;
                    } else if self.data[self.pos] == b'\n' {
                        self.pos += 1;
                        return Some(ZeroCopyRow::new(values));
                    } else if self.data[self.pos] == b'\r' {
                        self.pos += 1;
                        if self.pos < self.data.len() && self.data[self.pos] == b'\n' {
                            self.pos += 1;
                        }
                        return Some(ZeroCopyRow::new(values));
                    }
                }
            } else {
                // Unquoted value
                let start = self.pos;

                while self.pos < self.data.len() {
                    let c = self.data[self.pos];
                    if c == self.delimiter {
                        values.push(BorrowedValue::new(&self.data[start..self.pos]));
                        self.pos += 1;
                        break;
                    } else if c == b'\n' {
                        values.push(BorrowedValue::new(&self.data[start..self.pos]));
                        self.pos += 1;
                        return Some(ZeroCopyRow::new(values));
                    } else if c == b'\r' {
                        values.push(BorrowedValue::new(&self.data[start..self.pos]));
                        self.pos += 1;
                        if self.pos < self.data.len() && self.data[self.pos] == b'\n' {
                            self.pos += 1;
                        }
                        return Some(ZeroCopyRow::new(values));
                    } else {
                        self.pos += 1;
                    }
                }

                // End of data
                if self.pos >= self.data.len() && start < self.pos {
                    values.push(BorrowedValue::new(&self.data[start..self.pos]));
                }
            }
        }

        if values.is_empty() {
            None
        } else {
            Some(ZeroCopyRow::new(values))
        }
    }
}

impl<'a> Iterator for CsvParser<'a> {
    type Item = ZeroCopyRow<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_row()
    }
}

/// Zero-copy JSON value parser for ClickHouse JSONEachRow format.
///
/// This parser extracts values from JSON without full deserialization,
/// returning borrowed slices from the original data.
pub struct JsonRowParser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> JsonRowParser<'a> {
    /// Create a new JSON row parser.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Skip whitespace.
    #[inline]
    fn skip_whitespace(&mut self) {
        while self.pos < self.data.len() {
            match self.data[self.pos] {
                b' ' | b'\t' | b'\r' | b'\n' => self.pos += 1,
                _ => break,
            }
        }
    }

    /// Parse a JSON string, returning the unescaped content.
    fn parse_string(&mut self) -> Result<&'a [u8], ParseError> {
        if self.pos >= self.data.len() || self.data[self.pos] != b'"' {
            return Err(ParseError::InvalidFormat("expected string".to_owned()));
        }
        self.pos += 1;
        let start = self.pos;

        while self.pos < self.data.len() {
            match self.data[self.pos] {
                b'"' => {
                    let result = &self.data[start..self.pos];
                    self.pos += 1;
                    return Ok(result);
                }
                b'\\' => {
                    // Skip escape sequence
                    self.pos += 2;
                }
                _ => {
                    self.pos += 1;
                }
            }
        }

        Err(ParseError::UnexpectedEof)
    }

    /// Parse a JSON value (returns raw bytes).
    fn parse_value(&mut self) -> Result<&'a [u8], ParseError> {
        self.skip_whitespace();

        if self.pos >= self.data.len() {
            return Err(ParseError::UnexpectedEof);
        }

        let start = self.pos;

        match self.data[self.pos] {
            b'"' => self.parse_string(),
            b'[' | b'{' => {
                // Skip nested structure
                let open = self.data[self.pos];
                let close = if open == b'[' { b']' } else { b'}' };
                let mut depth = 1;
                self.pos += 1;

                while self.pos < self.data.len() && depth > 0 {
                    match self.data[self.pos] {
                        c if c == open => depth += 1,
                        c if c == close => depth -= 1,
                        b'"' => {
                            // Skip string
                            self.pos += 1;
                            while self.pos < self.data.len() {
                                match self.data[self.pos] {
                                    b'"' => break,
                                    b'\\' => self.pos += 1,
                                    _ => {}
                                }
                                self.pos += 1;
                            }
                        }
                        _ => {}
                    }
                    self.pos += 1;
                }

                Ok(&self.data[start..self.pos])
            }
            _ => {
                // Number, boolean, or null
                while self.pos < self.data.len() {
                    match self.data[self.pos] {
                        b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n' => break,
                        _ => self.pos += 1,
                    }
                }
                Ok(&self.data[start..self.pos])
            }
        }
    }

    /// Parse the next JSON row.
    pub fn next_row(&mut self) -> Option<Result<Vec<(&'a str, BorrowedValue<'a>)>, ParseError>> {
        self.skip_whitespace();

        if self.pos >= self.data.len() {
            return None;
        }

        if self.data[self.pos] != b'{' {
            return Some(Err(ParseError::InvalidFormat("expected '{'".to_owned())));
        }
        self.pos += 1;

        let mut fields = Vec::with_capacity(8);

        loop {
            self.skip_whitespace();

            if self.pos >= self.data.len() {
                return Some(Err(ParseError::UnexpectedEof));
            }

            if self.data[self.pos] == b'}' {
                self.pos += 1;
                self.skip_whitespace();
                // Skip newline between rows
                if self.pos < self.data.len() && self.data[self.pos] == b'\n' {
                    self.pos += 1;
                }
                return Some(Ok(fields));
            }

            if self.data[self.pos] == b',' {
                self.pos += 1;
                self.skip_whitespace();
            }

            // Parse key
            let key = match self.parse_string() {
                Ok(k) => match str::from_utf8(k) {
                    Ok(s) => s,
                    Err(_) => return Some(Err(ParseError::InvalidUtf8)),
                },
                Err(e) => return Some(Err(e)),
            };

            self.skip_whitespace();

            // Expect colon
            if self.pos >= self.data.len() || self.data[self.pos] != b':' {
                return Some(Err(ParseError::InvalidFormat("expected ':'".to_owned())));
            }
            self.pos += 1;

            // Parse value
            let value = match self.parse_value() {
                Ok(v) => BorrowedValue::new(v),
                Err(e) => return Some(Err(e)),
            };

            fields.push((key, value));
        }
    }
}

/// High-level zero-copy parser that auto-detects format.
pub enum ZeroCopyParser<'a> {
    /// TSV format.
    Tsv(TsvParser<'a>),
    /// CSV format.
    Csv(CsvParser<'a>),
    /// JSONEachRow format.
    JsonEachRow(JsonRowParser<'a>),
}

impl<'a> ZeroCopyParser<'a> {
    /// Create a TSV parser.
    pub fn tsv(data: &'a [u8]) -> Self {
        ZeroCopyParser::Tsv(TsvParser::new(data))
    }

    /// Create a CSV parser.
    pub fn csv(data: &'a [u8]) -> Self {
        ZeroCopyParser::Csv(CsvParser::new(data))
    }

    /// Create a JSONEachRow parser.
    pub fn json_each_row(data: &'a [u8]) -> Self {
        ZeroCopyParser::JsonEachRow(JsonRowParser::new(data))
    }

    /// Auto-detect format from data.
    pub fn auto_detect(data: &'a [u8]) -> Self {
        // Simple heuristic: check first non-whitespace character
        let trimmed = data.iter().skip_while(|&&c| c == b' ' || c == b'\t' || c == b'\r' || c == b'\n');

        match trimmed.clone().next() {
            Some(b'{') => Self::json_each_row(data),
            Some(b'"') if data.contains(&b',') => Self::csv(data),
            _ if data.contains(&b'\t') => Self::tsv(data),
            _ => Self::csv(data),
        }
    }
}

/// Utility for zero-copy slice operations.
pub mod slice_utils {
    /// Split a slice by a delimiter without allocation.
    #[inline]
    pub fn split_at_byte(data: &[u8], byte: u8) -> impl Iterator<Item = &[u8]> {
        data.split(move |&b| b == byte)
    }

    /// Find the first occurrence of a byte.
    #[inline]
    pub fn find_byte(data: &[u8], byte: u8) -> Option<usize> {
        memchr::memchr(byte, data)
    }

    /// Find the first occurrence of any byte in a set.
    #[inline]
    pub fn find_any_byte(data: &[u8], bytes: &[u8]) -> Option<usize> {
        match bytes.len() {
            0 => None,
            1 => memchr::memchr(bytes[0], data),
            2 => memchr::memchr2(bytes[0], bytes[1], data),
            3 => memchr::memchr3(bytes[0], bytes[1], bytes[2], data),
            _ => data.iter().position(|b| bytes.contains(b)),
        }
    }

    /// Trim leading and trailing whitespace from a byte slice.
    pub fn trim_whitespace(data: &[u8]) -> &[u8] {
        let start = data.iter().position(|&b| !b.is_ascii_whitespace()).unwrap_or(data.len());
        let end = data.iter().rposition(|&b| !b.is_ascii_whitespace()).map(|i| i + 1).unwrap_or(0);
        if start < end {
            &data[start..end]
        } else {
            &[]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borrowed_value_parse() {
        let val = BorrowedValue::new(b"12345");
        assert_eq!(val.parse_i64().unwrap(), 12345);
        assert_eq!(val.parse_u64().unwrap(), 12345);

        let val = BorrowedValue::new(b"3.14159");
        assert!((val.parse_f64().unwrap() - 3.14159).abs() < 0.00001);

        let val = BorrowedValue::new(b"true");
        assert!(val.parse_bool().unwrap());

        let val = BorrowedValue::new(b"0");
        assert!(!val.parse_bool().unwrap());
    }

    #[test]
    fn test_borrowed_value_null() {
        assert!(BorrowedValue::new(b"").is_null());
        assert!(BorrowedValue::new(b"\\N").is_null());
        assert!(BorrowedValue::new(b"NULL").is_null());
        assert!(!BorrowedValue::new(b"hello").is_null());
    }

    #[test]
    fn test_tsv_parser() {
        let data = b"1\talice\t100\n2\tbob\t200\n";
        let mut parser = TsvParser::new(data);

        let row1 = parser.next_row().unwrap();
        assert_eq!(row1.len(), 3);
        assert_eq!(row1.get_u64(0).unwrap(), 1);
        assert_eq!(row1.get_str(1).unwrap(), "alice");
        assert_eq!(row1.get_u64(2).unwrap(), 100);

        let row2 = parser.next_row().unwrap();
        assert_eq!(row2.get_str(1).unwrap(), "bob");

        assert!(parser.next_row().is_none());
    }

    #[test]
    fn test_csv_parser() {
        let data = b"1,alice,100\n2,bob,200\n";
        let mut parser = CsvParser::new(data);

        let row1 = parser.next_row().unwrap();
        assert_eq!(row1.len(), 3);
        assert_eq!(row1.get_u64(0).unwrap(), 1);
        assert_eq!(row1.get_str(1).unwrap(), "alice");

        let row2 = parser.next_row().unwrap();
        assert_eq!(row2.get_str(1).unwrap(), "bob");
    }

    #[test]
    fn test_csv_quoted() {
        let data = b"1,\"hello, world\",100\n";
        let mut parser = CsvParser::new(data);

        let row = parser.next_row().unwrap();
        assert_eq!(row.get_str(1).unwrap(), "hello, world");
    }

    #[test]
    fn test_json_row_parser() {
        let data = br#"{"id":1,"name":"alice"}
{"id":2,"name":"bob"}
"#;
        let mut parser = JsonRowParser::new(data);

        let row1 = parser.next_row().unwrap().unwrap();
        assert_eq!(row1.len(), 2);
        assert_eq!(row1[0].0, "id");
        assert_eq!(row1[0].1.parse_u64().unwrap(), 1);
        assert_eq!(row1[1].0, "name");
        assert_eq!(row1[1].1.as_str().unwrap(), "alice");

        let row2 = parser.next_row().unwrap().unwrap();
        assert_eq!(row2[1].1.as_str().unwrap(), "bob");
    }

    #[test]
    fn test_zero_copy_row() {
        let values = vec![
            BorrowedValue::new(b"42"),
            BorrowedValue::new(b"hello"),
            BorrowedValue::new(b"3.14"),
        ];
        let row = ZeroCopyRow::new(values);

        assert_eq!(row.len(), 3);
        assert_eq!(row.get_i64(0).unwrap(), 42);
        assert_eq!(row.get_str(1).unwrap(), "hello");
        assert!((row.get_f64(2).unwrap() - 3.14).abs() < 0.01);

        assert!(row.get(10).is_err());
    }

    #[test]
    fn test_slice_utils() {
        use slice_utils::*;

        let data = b"a\tb\tc";
        let parts: Vec<_> = split_at_byte(data, b'\t').collect();
        assert_eq!(parts, vec![b"a".as_slice(), b"b", b"c"]);

        assert_eq!(find_byte(b"hello", b'l'), Some(2));
        assert_eq!(find_byte(b"hello", b'x'), None);

        let trimmed = trim_whitespace(b"  hello  ");
        assert_eq!(trimmed, b"hello");
    }

    #[test]
    fn test_auto_detect() {
        let tsv = b"1\t2\t3\n";
        assert!(matches!(ZeroCopyParser::auto_detect(tsv), ZeroCopyParser::Tsv(_)));

        let json = b"{\"a\":1}\n";
        assert!(matches!(ZeroCopyParser::auto_detect(json), ZeroCopyParser::JsonEachRow(_)));

        let csv = b"1,2,3\n";
        assert!(matches!(ZeroCopyParser::auto_detect(csv), ZeroCopyParser::Csv(_)));
    }
}
