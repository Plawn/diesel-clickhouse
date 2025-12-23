//! Apache Arrow integration for zero-copy columnar data processing.
//!
//! This module provides high-performance data loading using Apache Arrow's
//! columnar format. Arrow enables true zero-copy data access and is optimized
//! for analytical workloads.
//!
//! # Benefits over TSV/JSON
//!
//! - **Zero-copy**: Data is accessed directly from the buffer without parsing
//! - **Columnar**: Efficient for analytical queries that access few columns
//! - **Type-safe**: Schema is embedded in the data
//! - **SIMD-friendly**: Arrow's memory layout enables vectorized operations
//! - **Interoperable**: Works with Polars, DataFusion, DuckDB, etc.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//! use arrow::array::{Int64Array, StringArray};
//!
//! // Load data as Arrow RecordBatches
//! let batches = conn.load_arrow("SELECT id, name FROM users").await?;
//!
//! for batch in batches {
//!     // Access columns with zero-copy
//!     let ids = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
//!     let names = batch.column(1).as_any().downcast_ref::<StringArray>().unwrap();
//!
//!     for i in 0..batch.num_rows() {
//!         println!("User {}: {}", ids.value(i), names.value(i));
//!     }
//! }
//! ```
//!
//! # Performance Comparison
//!
//! | Format | Parse Time | Memory | Zero-Copy |
//! |--------|-----------|--------|-----------|
//! | Arrow  | ~0 (binary) | Minimal | Yes |
//! | TSV    | O(n) text parsing | O(n) | Partial |
//! | JSON   | O(n) tokenization | O(n) | No |

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use arrow::array::{
    Array, RecordBatch,
    Int8Array, Int16Array, Int32Array, Int64Array,
    UInt8Array, UInt16Array, UInt32Array, UInt64Array,
    Float32Array, Float64Array, StringArray, BinaryArray,
    BooleanArray,
};
use arrow::ipc::reader::StreamReader;

use crate::core::result::{Error, QueryResult};

/// Re-export arrow types for convenience
pub use arrow::array;
pub use arrow::datatypes::{DataType, Field, Schema};

// =============================================================================
// ArrowRow - Row-by-row access to Arrow data
// =============================================================================

/// A zero-copy view into a single row of an Arrow RecordBatch.
///
/// This type provides a familiar row-oriented API on top of Arrow's columnar
/// data, enabling migration from TSV-based zero-copy while leveraging Arrow's
/// performance benefits.
///
/// # Example
///
/// ```rust,ignore
/// conn.load_zero_copy("SELECT id, name FROM users", |row| {
///     let id = row.get_u64("id")?;
///     let name = row.get_str("name")?;  // Zero-copy borrow
///     println!("{}: {}", id, name);
///     Ok(())
/// }).await?;
/// ```
#[derive(Debug)]
pub struct ArrowRow<'a> {
    batch: &'a RecordBatch,
    row_index: usize,
    column_indices: &'a HashMap<Arc<str>, usize>,
}

impl<'a> ArrowRow<'a> {
    /// Create a new ArrowRow view into a batch at the given row index.
    #[inline]
    pub fn new(
        batch: &'a RecordBatch,
        row_index: usize,
        column_indices: &'a HashMap<Arc<str>, usize>,
    ) -> Self {
        Self {
            batch,
            row_index,
            column_indices,
        }
    }

    /// Get the row index within the batch.
    #[inline]
    pub fn row_index(&self) -> usize {
        self.row_index
    }

    /// Get column index by name.
    #[inline]
    fn column_index(&self, name: &str) -> QueryResult<usize> {
        self.column_indices
            .get(name)
            .copied()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' not found", name)))
    }

    /// Get a string value (zero-copy borrow).
    #[inline]
    pub fn get_str(&self, name: &str) -> QueryResult<&'a str> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<StringArray>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not a string", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get an optional string value (zero-copy borrow).
    #[inline]
    pub fn get_str_opt(&self, name: &str) -> QueryResult<Option<&'a str>> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<StringArray>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not a string", name)))?;

        if array.is_null(self.row_index) {
            Ok(None)
        } else {
            Ok(Some(array.value(self.row_index)))
        }
    }

    /// Get binary data (zero-copy borrow).
    #[inline]
    pub fn get_bytes(&self, name: &str) -> QueryResult<&'a [u8]> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<BinaryArray>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not binary", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get a boolean value.
    #[inline]
    pub fn get_bool(&self, name: &str) -> QueryResult<bool> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<BooleanArray>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not boolean", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get an i8 value.
    #[inline]
    pub fn get_i8(&self, name: &str) -> QueryResult<i8> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<Int8Array>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not Int8", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get an i16 value.
    #[inline]
    pub fn get_i16(&self, name: &str) -> QueryResult<i16> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<Int16Array>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not Int16", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get an i32 value.
    #[inline]
    pub fn get_i32(&self, name: &str) -> QueryResult<i32> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<Int32Array>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not Int32", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get an i64 value.
    #[inline]
    pub fn get_i64(&self, name: &str) -> QueryResult<i64> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<Int64Array>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not Int64", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get a u8 value.
    #[inline]
    pub fn get_u8(&self, name: &str) -> QueryResult<u8> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<UInt8Array>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not UInt8", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get a u16 value.
    #[inline]
    pub fn get_u16(&self, name: &str) -> QueryResult<u16> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<UInt16Array>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not UInt16", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get a u32 value.
    #[inline]
    pub fn get_u32(&self, name: &str) -> QueryResult<u32> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<UInt32Array>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not UInt32", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get a u64 value.
    #[inline]
    pub fn get_u64(&self, name: &str) -> QueryResult<u64> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<UInt64Array>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not UInt64", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get an f32 value.
    #[inline]
    pub fn get_f32(&self, name: &str) -> QueryResult<f32> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<Float32Array>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not Float32", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Get an f64 value.
    #[inline]
    pub fn get_f64(&self, name: &str) -> QueryResult<f64> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<Float64Array>()
            .ok_or_else(|| Error::DeserializationError(format!("Column '{}' is not Float64", name)))?;

        if array.is_null(self.row_index) {
            return Err(Error::DeserializationError(format!("Column '{}' is null", name)));
        }
        Ok(array.value(self.row_index))
    }

    /// Check if a column is null.
    #[inline]
    pub fn is_null(&self, name: &str) -> QueryResult<bool> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        Ok(col.is_null(self.row_index))
    }

    /// Get the number of columns.
    #[inline]
    pub fn num_columns(&self) -> usize {
        self.batch.num_columns()
    }
}

/// Helper to build column name index for ArrowRow.
pub fn build_column_index(schema: &Schema) -> HashMap<Arc<str>, usize> {
    schema
        .fields()
        .iter()
        .enumerate()
        .map(|(i, f)| (Arc::from(f.name().as_str()), i))
        .collect()
}

/// Iterate over rows in a RecordBatch, calling a callback for each row.
pub fn for_each_row<F>(
    batch: &RecordBatch,
    column_indices: &HashMap<Arc<str>, usize>,
    mut callback: F,
) -> QueryResult<usize>
where
    F: FnMut(ArrowRow<'_>) -> QueryResult<()>,
{
    let num_rows = batch.num_rows();
    for row_idx in 0..num_rows {
        let row = ArrowRow::new(batch, row_idx, column_indices);
        callback(row)?;
    }
    Ok(num_rows)
}

/// A collection of Arrow RecordBatches from a query result.
///
/// This type wraps the Arrow data and provides convenient access methods.
#[derive(Debug)]
pub struct ArrowResult {
    /// The schema of the result set
    schema: Arc<Schema>,
    /// The record batches containing the data
    batches: Vec<RecordBatch>,
    /// Total number of rows across all batches
    total_rows: usize,
}

impl ArrowResult {
    /// Create a new ArrowResult from schema and batches
    pub fn new(schema: Arc<Schema>, batches: Vec<RecordBatch>) -> Self {
        let total_rows = batches.iter().map(|b| b.num_rows()).sum();
        Self {
            schema,
            batches,
            total_rows,
        }
    }

    /// Get the schema of the result
    #[inline]
    pub fn schema(&self) -> &Arc<Schema> {
        &self.schema
    }

    /// Get the record batches
    #[inline]
    pub fn batches(&self) -> &[RecordBatch] {
        &self.batches
    }

    /// Consume and return the batches
    #[inline]
    pub fn into_batches(self) -> Vec<RecordBatch> {
        self.batches
    }

    /// Get the total number of rows
    #[inline]
    pub fn num_rows(&self) -> usize {
        self.total_rows
    }

    /// Get the number of columns
    #[inline]
    pub fn num_columns(&self) -> usize {
        self.schema.fields().len()
    }

    /// Get the number of batches
    #[inline]
    pub fn num_batches(&self) -> usize {
        self.batches.len()
    }

    /// Check if the result is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.total_rows == 0
    }

    /// Get a column by index from all batches concatenated
    ///
    /// Returns None if the index is out of bounds.
    pub fn column(&self, index: usize) -> Option<Vec<&dyn arrow::array::Array>> {
        if index >= self.num_columns() {
            return None;
        }
        Some(
            self.batches
                .iter()
                .map(|b| b.column(index).as_ref())
                .collect(),
        )
    }

    /// Get a column by name from all batches
    ///
    /// Returns None if the column doesn't exist.
    pub fn column_by_name(&self, name: &str) -> Option<Vec<&dyn arrow::array::Array>> {
        let index = self.schema.index_of(name).ok()?;
        self.column(index)
    }

    /// Iterate over batches
    pub fn iter(&self) -> impl Iterator<Item = &RecordBatch> {
        self.batches.iter()
    }
}

impl IntoIterator for ArrowResult {
    type Item = RecordBatch;
    type IntoIter = std::vec::IntoIter<RecordBatch>;

    fn into_iter(self) -> Self::IntoIter {
        self.batches.into_iter()
    }
}

/// Parse Arrow IPC stream data into RecordBatches
///
/// This function reads the ArrowStream format returned by ClickHouse
/// and returns a vector of RecordBatches.
pub fn parse_arrow_stream(data: &[u8]) -> QueryResult<ArrowResult> {
    if data.is_empty() {
        return Err(Error::DeserializationError(
            "Empty Arrow stream data".to_string(),
        ));
    }

    let cursor = Cursor::new(data);
    let reader = StreamReader::try_new(cursor, None)
        .map_err(|e| Error::DeserializationError(format!("Failed to create Arrow reader: {}", e)))?;

    let schema = reader.schema();
    let mut batches = Vec::new();

    for batch_result in reader {
        let batch = batch_result
            .map_err(|e| Error::DeserializationError(format!("Failed to read Arrow batch: {}", e)))?;
        batches.push(batch);
    }

    Ok(ArrowResult::new(schema, batches))
}

/// Parse Arrow IPC stream data with streaming callback
///
/// This function processes batches as they are parsed, without collecting
/// them all in memory first.
pub fn parse_arrow_stream_callback<F>(data: &[u8], mut callback: F) -> QueryResult<usize>
where
    F: FnMut(RecordBatch) -> QueryResult<()>,
{
    if data.is_empty() {
        return Ok(0);
    }

    let cursor = Cursor::new(data);
    let reader = StreamReader::try_new(cursor, None)
        .map_err(|e| Error::DeserializationError(format!("Failed to create Arrow reader: {}", e)))?;

    let mut count = 0;
    for batch_result in reader {
        let batch = batch_result
            .map_err(|e| Error::DeserializationError(format!("Failed to read Arrow batch: {}", e)))?;
        let rows = batch.num_rows();
        callback(batch)?;
        count += rows;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Int32Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;

    fn create_test_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("name", DataType::Utf8, false),
        ]));

        let id_array = Int32Array::from(vec![1, 2, 3]);
        let name_array = StringArray::from(vec!["alice", "bob", "charlie"]);

        RecordBatch::try_new(schema, vec![Arc::new(id_array), Arc::new(name_array)]).unwrap()
    }

    fn create_arrow_stream() -> Vec<u8> {
        let batch = create_test_batch();
        let mut buffer = Vec::new();
        {
            let mut writer = StreamWriter::try_new(&mut buffer, &batch.schema()).unwrap();
            writer.write(&batch).unwrap();
            writer.finish().unwrap();
        }
        buffer
    }

    #[test]
    fn test_parse_arrow_stream() {
        let data = create_arrow_stream();
        let result = parse_arrow_stream(&data).unwrap();

        assert_eq!(result.num_rows(), 3);
        assert_eq!(result.num_columns(), 2);
        assert_eq!(result.num_batches(), 1);
    }

    #[test]
    fn test_arrow_result_column_access() {
        let data = create_arrow_stream();
        let result = parse_arrow_stream(&data).unwrap();

        // Access by index
        let columns = result.column(0).unwrap();
        assert_eq!(columns.len(), 1);

        // Access by name
        let name_columns = result.column_by_name("name").unwrap();
        assert_eq!(name_columns.len(), 1);

        // Non-existent column
        assert!(result.column_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_parse_arrow_stream_callback() {
        let data = create_arrow_stream();
        let mut batches = Vec::new();

        let count = parse_arrow_stream_callback(&data, |batch| {
            batches.push(batch);
            Ok(())
        })
        .unwrap();

        assert_eq!(count, 3);
        assert_eq!(batches.len(), 1);
    }

    #[test]
    fn test_empty_stream() {
        let result = parse_arrow_stream(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_arrow_result_iter() {
        let data = create_arrow_stream();
        let result = parse_arrow_stream(&data).unwrap();

        let batch_count: usize = result.iter().count();
        assert_eq!(batch_count, 1);
    }
}
