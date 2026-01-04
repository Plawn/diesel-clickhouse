//! Apache Arrow integration for zero-copy columnar data processing.
//!
//! This module provides high-performance data loading using Apache Arrow's
//! columnar format. Arrow enables true zero-copy data access and is optimized
//! for analytical workloads.
//!
//! # Zero-Copy Architecture
//!
//! The streaming implementation uses `StreamDecoder` with `bytes::Bytes` to achieve
//! true zero-copy from network to Arrow buffers:
//!
//! ```text
//! Network (HTTP chunks)
//!     ↓ bytes::Bytes (reference-counted, no copy)
//! arrow_buffer::Buffer (zero-copy From<Bytes>)
//!     ↓ StreamDecoder (incremental parsing)
//! RecordBatch (zero-copy column access)
//!     ↓
//! &str / &[u8] / primitive values
//! ```
//!
//! # Benefits over TSV/JSON
//!
//! - **Zero-copy**: Data is accessed directly from the buffer without parsing
//! - **Columnar**: Efficient for analytical queries that access few columns
//! - **Type-safe**: Schema is embedded in the data
//! - **SIMD-friendly**: Arrow's memory layout enables vectorized operations
//! - **Interoperable**: Works with Polars, DataFusion, DuckDB, etc.
//! - **Streaming**: Process batches as they arrive, no buffering required
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//! use arrow::array::{Int64Array, StringArray};
//!
//! // Stream data as Arrow RecordBatches (zero-copy)
//! let mut stream = conn.stream_arrow("SELECT id, name FROM users");
//!
//! while let Some(batch) = stream.next().await? {
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
//! | Format | Parse Time | Memory | Zero-Copy | Streaming |
//! |--------|-----------|--------|-----------|-----------|
//! | Arrow (StreamDecoder) | ~0 | O(batch) | Yes | Yes |
//! | Arrow (buffered) | ~0 | O(total) | Partial | No |
//! | TSV    | O(n) parsing | O(n) | Partial | Yes |
//! | JSON   | O(n) tokenization | O(n) | No | Yes |

use std::borrow::Cow;
use std::io::Cursor;
use std::sync::Arc;

use arrow::array::{
    Array, BinaryArray, BooleanArray, Float32Array, Float64Array, Int16Array, Int32Array,
    Int64Array, Int8Array, RecordBatch, StringArray, UInt16Array, UInt32Array, UInt64Array,
    UInt8Array,
};
use arrow::ipc::reader::{StreamDecoder, StreamReader};

use crate::core::interner::InternedSchema;
use crate::core::result::{Error, QueryResult};

// =============================================================================
// Zero-Copy Streaming Arrow Decoder
// =============================================================================

/// A zero-copy Arrow stream decoder that processes `bytes::Bytes` chunks.
///
/// This decoder uses Arrow's `StreamDecoder` to parse IPC stream data
/// incrementally without copying the underlying bytes. Each `Bytes` chunk
/// from the network is converted to an `arrow_buffer::Buffer` via zero-copy
/// `From` implementation.
///
/// # Zero-Copy Guarantee
///
/// - `bytes::Bytes` → `arrow_buffer::Buffer`: Zero-copy (reference counting)
/// - `StreamDecoder::decode()`: Zero-copy for aligned data, auto-aligns if needed
/// - `RecordBatch` column access: Zero-copy (slices into buffer)
///
/// # Example
///
/// ```rust,ignore
/// let mut decoder = ZeroCopyArrowDecoder::new();
///
/// // Feed chunks as they arrive from the network
/// for chunk in http_response_chunks {
///     for batch in decoder.decode_chunk(chunk)? {
///         process_batch(batch);
///     }
/// }
///
/// decoder.finish()?;
/// ```
pub struct ZeroCopyArrowDecoder {
    decoder: StreamDecoder,
    /// Accumulated buffer for partial messages spanning multiple chunks
    pending: ArrowBuffer,
}

impl ZeroCopyArrowDecoder {
    /// Create a new zero-copy Arrow decoder.
    pub fn new() -> Self {
        Self {
            decoder: StreamDecoder::new(),
            pending: ArrowBuffer::from(BytesChunk::new()),
        }
    }

    /// Get the decoded schema, if available.
    ///
    /// The schema becomes available after the first chunk containing
    /// the schema message has been decoded.
    pub fn schema(&self) -> Option<Arc<arrow::datatypes::Schema>> {
        self.decoder.schema()
    }

    /// Decode a chunk of bytes, yielding any complete RecordBatches.
    ///
    /// This method is zero-copy: the `Bytes` chunk is converted to an
    /// Arrow `Buffer` without copying, and batches are parsed directly
    /// from that buffer.
    ///
    /// # Returns
    ///
    /// A vector of decoded `RecordBatch`es. May be empty if the chunk
    /// doesn't contain any complete batches (partial data is buffered
    /// internally for the next call).
    pub fn decode_chunk(&mut self, chunk: BytesChunk) -> QueryResult<Vec<RecordBatch>> {
        let mut batches = Vec::new();

        // Combine pending data with new chunk (zero-copy if pending is empty)
        let mut buffer = if self.pending.is_empty() {
            ArrowBuffer::from(chunk)
        } else {
            // We have pending data - need to combine
            // This is a copy, but only happens when messages span chunks
            let mut combined = Vec::with_capacity(self.pending.len() + chunk.len());
            combined.extend_from_slice(&self.pending);
            combined.extend_from_slice(&chunk);
            ArrowBuffer::from(combined)
        };

        // Decode all complete messages from this buffer
        while !buffer.is_empty() {
            match self.decoder.decode(&mut buffer) {
                Ok(Some(batch)) => {
                    batches.push(batch);
                }
                Ok(None) => {
                    // Need more data - save remaining as pending
                    break;
                }
                Err(e) => {
                    return Err(Error::DeserializationError(Cow::Owned(format!(
                        "Arrow decode error: {}",
                        e
                    ))));
                }
            }
        }

        // Save any remaining data for next chunk
        self.pending = buffer;

        Ok(batches)
    }

    /// Finish decoding and validate the stream is complete.
    ///
    /// Call this after all chunks have been processed to ensure
    /// no partial data remains.
    pub fn finish(mut self) -> QueryResult<()> {
        // Process any remaining pending data
        if !self.pending.is_empty() {
            let mut buffer =
                std::mem::replace(&mut self.pending, ArrowBuffer::from(BytesChunk::new()));
            while !buffer.is_empty() {
                match self.decoder.decode(&mut buffer) {
                    Ok(Some(_)) => {
                        // Batch decoded from remaining data
                    }
                    Ok(None) => {
                        // Still incomplete - this is an error
                        return Err(Error::DeserializationError(Cow::Borrowed(
                            "Arrow stream ended with incomplete data",
                        )));
                    }
                    Err(e) => {
                        return Err(Error::DeserializationError(Cow::Owned(format!(
                            "Arrow decode error in finish: {}",
                            e
                        ))));
                    }
                }
            }
        }

        self.decoder.finish().map_err(|e| {
            Error::DeserializationError(Cow::Owned(format!("Arrow stream incomplete: {}", e)))
        })?;

        Ok(())
    }

    /// Consume the decoder and return any pending data.
    ///
    /// This is useful for error recovery or debugging.
    pub fn into_pending(self) -> ArrowBuffer {
        self.pending
    }
}

impl Default for ZeroCopyArrowDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Re-export arrow types for convenience.
pub use arrow::array;
pub use arrow::buffer::Buffer;
pub use arrow::datatypes::{DataType, Field, Schema};

/// Re-export `bytes::Bytes` for zero-copy streaming.
///
/// This is the type used by `ZeroCopyArrowDecoder::decode_chunk()`.
pub use bytes::Bytes;

// Use the public exports within this module
use Buffer as ArrowBuffer;
use Bytes as BytesChunk;

// =============================================================================
// ArrowValue Trait - Generic value extraction from Arrow arrays
// =============================================================================

/// Trait for types that can be extracted from Arrow arrays.
///
/// This trait enables generic value extraction from Arrow RecordBatches,
/// reducing code duplication in `ArrowRow` methods.
///
/// # Example
///
/// ```rust,ignore
/// // Get any supported type generically
/// let id: u64 = row.get("id")?;
/// let name: &str = row.get("name")?;
/// ```
pub trait ArrowValue<'a>: Sized {
    /// The Arrow array type that holds this value type.
    type ArrayType: 'static;

    /// Extract a value from the array at the given index.
    fn from_array(array: &'a Self::ArrayType, idx: usize) -> Self;

    /// The type name for error messages.
    fn type_name() -> &'static str;
}

/// Macro to implement ArrowValue for primitive numeric types.
macro_rules! impl_arrow_value_primitive {
    ($rust_ty:ty, $array_ty:ty, $name:literal) => {
        impl<'a> ArrowValue<'a> for $rust_ty {
            type ArrayType = $array_ty;

            #[inline]
            fn from_array(array: &'a Self::ArrayType, idx: usize) -> Self {
                array.value(idx)
            }

            #[inline]
            fn type_name() -> &'static str {
                $name
            }
        }
    };
}

// Implement ArrowValue for all primitive types
impl_arrow_value_primitive!(i8, Int8Array, "Int8");
impl_arrow_value_primitive!(i16, Int16Array, "Int16");
impl_arrow_value_primitive!(i32, Int32Array, "Int32");
impl_arrow_value_primitive!(i64, Int64Array, "Int64");
impl_arrow_value_primitive!(u8, UInt8Array, "UInt8");
impl_arrow_value_primitive!(u16, UInt16Array, "UInt16");
impl_arrow_value_primitive!(u32, UInt32Array, "UInt32");
impl_arrow_value_primitive!(u64, UInt64Array, "UInt64");
impl_arrow_value_primitive!(f32, Float32Array, "Float32");
impl_arrow_value_primitive!(f64, Float64Array, "Float64");
impl_arrow_value_primitive!(bool, BooleanArray, "boolean");

// Implement ArrowValue for string (zero-copy borrow)
impl<'a> ArrowValue<'a> for &'a str {
    type ArrayType = StringArray;

    #[inline]
    fn from_array(array: &'a Self::ArrayType, idx: usize) -> Self {
        array.value(idx)
    }

    #[inline]
    fn type_name() -> &'static str {
        "string"
    }
}

// Implement ArrowValue for binary (zero-copy borrow)
impl<'a> ArrowValue<'a> for &'a [u8] {
    type ArrayType = BinaryArray;

    #[inline]
    fn from_array(array: &'a Self::ArrayType, idx: usize) -> Self {
        array.value(idx)
    }

    #[inline]
    fn type_name() -> &'static str {
        "binary"
    }
}

// =============================================================================
// ArrowRow - Row-by-row access to Arrow data
// =============================================================================

/// Column index type using interned strings for O(1) lookup.
///
/// Uses `InternedSchema` which provides:
/// - O(1) column name lookup via AHash-based HashMap
/// - Interned strings (Symbol) for fast comparison
/// - SmallVec storage for ≤16 columns (no heap allocation)
/// - Global string deduplication across queries
pub type ColumnIndex = InternedSchema;

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
///     // Generic get method
///     let id: u64 = row.get("id")?;
///     let name: &str = row.get("name")?;
///
///     // Or use convenience methods
///     let id = row.get_u64("id")?;
///     let name = row.get_str("name")?;
///
///     println!("{}: {}", id, name);
///     Ok(())
/// }).await?;
/// ```
#[derive(Debug)]
pub struct ArrowRow<'a> {
    batch: &'a RecordBatch,
    row_index: usize,
    column_indices: &'a ColumnIndex,
}

impl<'a> ArrowRow<'a> {
    /// Create a new ArrowRow view into a batch at the given row index.
    #[inline]
    pub fn new(
        batch: &'a RecordBatch,
        row_index: usize,
        column_indices: &'a ColumnIndex,
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
    ///
    /// Uses O(1) lookup via InternedSchema's AHash-based HashMap.
    #[inline]
    fn column_index(&self, name: &str) -> QueryResult<usize> {
        self.column_indices
            .find_column(name)
            .ok_or_else(|| Error::DeserializationError(Cow::Owned(format!("Column '{}' not found", name))))
    }

    /// Get a typed value from a column by name.
    ///
    /// This is the generic method that powers all type-specific getters.
    /// Use this when you want to specify the type explicitly or in generic code.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let id: u64 = row.get("id")?;
    /// let name: &str = row.get("name")?;
    /// let score: f64 = row.get("score")?;
    /// ```
    #[inline]
    pub fn get<T: ArrowValue<'a>>(&self, name: &str) -> QueryResult<T> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<T::ArrayType>()
            .ok_or_else(|| Error::DeserializationError(
                Cow::Owned(format!("Column '{}' is not {}", name, T::type_name()))))?;

        if col.is_null(self.row_index) {
            return Err(Error::DeserializationError(
                Cow::Owned(format!("Column '{}' is null", name))));
        }
        Ok(T::from_array(array, self.row_index))
    }

    /// Get an optional typed value from a column by name.
    ///
    /// Returns `None` if the value is null, `Some(value)` otherwise.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let email: Option<&str> = row.get_opt("email")?;
    /// ```
    #[inline]
    pub fn get_opt<T: ArrowValue<'a>>(&self, name: &str) -> QueryResult<Option<T>> {
        let index = self.column_index(name)?;
        let col = self.batch.column(index);
        let array = col.as_any().downcast_ref::<T::ArrayType>()
            .ok_or_else(|| Error::DeserializationError(
                Cow::Owned(format!("Column '{}' is not {}", name, T::type_name()))))?;

        if col.is_null(self.row_index) {
            Ok(None)
        } else {
            Ok(Some(T::from_array(array, self.row_index)))
        }
    }

    // =========================================================================
    // Convenience methods (delegating to generic get/get_opt)
    // =========================================================================

    /// Get a string value (zero-copy borrow).
    #[inline]
    pub fn get_str(&self, name: &str) -> QueryResult<&'a str> {
        self.get(name)
    }

    /// Get an optional string value (zero-copy borrow).
    #[inline]
    pub fn get_str_opt(&self, name: &str) -> QueryResult<Option<&'a str>> {
        self.get_opt(name)
    }

    /// Alias for `get_str_opt` for backwards compatibility.
    #[inline]
    pub fn get_optional_str(&self, name: &str) -> QueryResult<Option<&'a str>> {
        self.get_str_opt(name)
    }

    /// Get binary data (zero-copy borrow).
    #[inline]
    pub fn get_bytes(&self, name: &str) -> QueryResult<&'a [u8]> {
        self.get(name)
    }

    /// Get a boolean value.
    #[inline]
    pub fn get_bool(&self, name: &str) -> QueryResult<bool> {
        self.get(name)
    }

    /// Get an i8 value.
    #[inline]
    pub fn get_i8(&self, name: &str) -> QueryResult<i8> {
        self.get(name)
    }

    /// Get an i16 value.
    #[inline]
    pub fn get_i16(&self, name: &str) -> QueryResult<i16> {
        self.get(name)
    }

    /// Get an i32 value.
    #[inline]
    pub fn get_i32(&self, name: &str) -> QueryResult<i32> {
        self.get(name)
    }

    /// Get an i64 value.
    #[inline]
    pub fn get_i64(&self, name: &str) -> QueryResult<i64> {
        self.get(name)
    }

    /// Get a u8 value.
    #[inline]
    pub fn get_u8(&self, name: &str) -> QueryResult<u8> {
        self.get(name)
    }

    /// Get a u16 value.
    #[inline]
    pub fn get_u16(&self, name: &str) -> QueryResult<u16> {
        self.get(name)
    }

    /// Get a u32 value.
    #[inline]
    pub fn get_u32(&self, name: &str) -> QueryResult<u32> {
        self.get(name)
    }

    /// Get a u64 value.
    #[inline]
    pub fn get_u64(&self, name: &str) -> QueryResult<u64> {
        self.get(name)
    }

    /// Get an f32 value.
    #[inline]
    pub fn get_f32(&self, name: &str) -> QueryResult<f32> {
        self.get(name)
    }

    /// Get an f64 value.
    #[inline]
    pub fn get_f64(&self, name: &str) -> QueryResult<f64> {
        self.get(name)
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
///
/// Uses `InternedSchema` which provides:
/// - O(1) column lookup via AHash-based HashMap
/// - Interned strings for fast comparison
/// - Global deduplication of column names
pub fn build_column_index(schema: &Schema) -> ColumnIndex {
    InternedSchema::new(schema.fields().iter().map(|f| f.name().as_str()))
}

/// Iterate over rows in a RecordBatch, calling a callback for each row.
pub fn for_each_row<F>(
    batch: &RecordBatch,
    column_indices: &ColumnIndex,
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

/// Parse Arrow IPC stream data into RecordBatches.
///
/// This function reads the ArrowStream format returned by ClickHouse
/// and returns a vector of RecordBatches.
///
/// # Note
///
/// For streaming scenarios, prefer using [`ZeroCopyArrowDecoder`] which
/// processes chunks incrementally without buffering the entire stream.
pub fn parse_arrow_stream(data: &[u8]) -> QueryResult<ArrowResult> {
    if data.is_empty() {
        return Err(Error::DeserializationError(
            Cow::Borrowed("Empty Arrow stream data"),
        ));
    }

    let cursor = Cursor::new(data);
    let reader = StreamReader::try_new(cursor, None).map_err(|e| {
        Error::DeserializationError(Cow::Owned(format!("Failed to create Arrow reader: {}", e)))
    })?;

    let schema = reader.schema();
    let mut batches = Vec::with_capacity(4);

    for batch_result in reader {
        let batch = batch_result.map_err(|e| {
            Error::DeserializationError(Cow::Owned(format!("Failed to read Arrow batch: {}", e)))
        })?;
        batches.push(batch);
    }

    Ok(ArrowResult::new(schema, batches))
}

/// Parse Arrow IPC stream data with streaming callback.
///
/// This function processes batches as they are parsed, without collecting
/// them all in memory first.
///
/// # Note
///
/// For true zero-copy streaming from network, prefer using
/// [`ZeroCopyArrowDecoder`] with `bytes::Bytes` chunks.
pub fn parse_arrow_stream_callback<F>(data: &[u8], mut callback: F) -> QueryResult<usize>
where
    F: FnMut(RecordBatch) -> QueryResult<()>,
{
    if data.is_empty() {
        return Ok(0);
    }

    let cursor = Cursor::new(data);
    let reader = StreamReader::try_new(cursor, None).map_err(|e| {
        Error::DeserializationError(Cow::Owned(format!("Failed to create Arrow reader: {}", e)))
    })?;

    let mut count = 0;
    for batch_result in reader {
        let batch = batch_result.map_err(|e| {
            Error::DeserializationError(Cow::Owned(format!("Failed to read Arrow batch: {}", e)))
        })?;
        let rows = batch.num_rows();
        callback(batch)?;
        count += rows;
    }

    Ok(count)
}

/// Parse Arrow IPC stream from `bytes::Bytes` chunks using zero-copy decoding.
///
/// This is the recommended way to parse Arrow data from streaming sources
/// like HTTP responses. It uses [`ZeroCopyArrowDecoder`] internally.
///
/// # Example
///
/// ```rust,ignore
/// let chunks: Vec<Bytes> = fetch_chunks_from_network().await;
/// let result = parse_arrow_zero_copy(chunks.into_iter())?;
/// ```
pub fn parse_arrow_zero_copy<I>(chunks: I) -> QueryResult<ArrowResult>
where
    I: IntoIterator<Item = BytesChunk>,
{
    let mut decoder = ZeroCopyArrowDecoder::new();
    let mut all_batches = Vec::new();

    for chunk in chunks {
        let batches = decoder.decode_chunk(chunk)?;
        all_batches.extend(batches);
    }

    decoder.finish()?;

    // Get schema from first batch, or return empty result
    if all_batches.is_empty() {
        return Err(Error::DeserializationError(Cow::Borrowed(
            "Arrow stream contained no batches",
        )));
    }

    let schema = all_batches[0].schema();
    Ok(ArrowResult::new(schema, all_batches))
}

/// Parse Arrow IPC stream from `bytes::Bytes` chunks with a callback.
///
/// Zero-copy streaming version that calls the callback for each batch
/// as it's decoded.
///
/// # Example
///
/// ```rust,ignore
/// let mut row_count = 0;
/// parse_arrow_zero_copy_callback(chunks, |batch| {
///     row_count += batch.num_rows();
///     process_batch(batch);
///     Ok(())
/// })?;
/// ```
pub fn parse_arrow_zero_copy_callback<I, F>(chunks: I, mut callback: F) -> QueryResult<usize>
where
    I: IntoIterator<Item = BytesChunk>,
    F: FnMut(RecordBatch) -> QueryResult<()>,
{
    let mut decoder = ZeroCopyArrowDecoder::new();
    let mut count = 0;

    for chunk in chunks {
        let batches = decoder.decode_chunk(chunk)?;
        for batch in batches {
            count += batch.num_rows();
            callback(batch)?;
        }
    }

    decoder.finish()?;
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

    // =========================================================================
    // ZeroCopyArrowDecoder tests
    // =========================================================================

    #[test]
    fn test_zero_copy_decoder_single_chunk() {
        let data = create_arrow_stream();
        let chunk = Bytes::from(data);

        let mut decoder = ZeroCopyArrowDecoder::new();
        let batches = decoder.decode_chunk(chunk).unwrap();
        decoder.finish().unwrap();

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].num_rows(), 3);
        assert_eq!(batches[0].num_columns(), 2);
    }

    #[test]
    fn test_zero_copy_decoder_multiple_chunks() {
        let data = create_arrow_stream();

        // Split data into multiple chunks to simulate streaming
        let chunk_size = data.len() / 3;
        let chunks: Vec<Bytes> = data
            .chunks(chunk_size.max(1))
            .map(|c| Bytes::copy_from_slice(c))
            .collect();

        let mut decoder = ZeroCopyArrowDecoder::new();
        let mut all_batches = Vec::new();

        for chunk in chunks {
            let batches = decoder.decode_chunk(chunk).unwrap();
            all_batches.extend(batches);
        }
        decoder.finish().unwrap();

        // Should have decoded the same batch
        assert_eq!(all_batches.len(), 1);
        assert_eq!(all_batches[0].num_rows(), 3);
    }

    #[test]
    fn test_zero_copy_decoder_schema_available() {
        let data = create_arrow_stream();
        let chunk = Bytes::from(data);

        let mut decoder = ZeroCopyArrowDecoder::new();

        // Schema not available before decoding
        assert!(decoder.schema().is_none());

        let _ = decoder.decode_chunk(chunk).unwrap();

        // Schema should be available after decoding
        assert!(decoder.schema().is_some());
        let schema = decoder.schema().unwrap();
        assert_eq!(schema.fields().len(), 2);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(1).name(), "name");
    }

    #[test]
    fn test_parse_arrow_zero_copy() {
        let data = create_arrow_stream();
        let chunks = vec![Bytes::from(data)];

        let result = parse_arrow_zero_copy(chunks).unwrap();

        assert_eq!(result.num_rows(), 3);
        assert_eq!(result.num_columns(), 2);
        assert_eq!(result.num_batches(), 1);
    }

    #[test]
    fn test_parse_arrow_zero_copy_callback() {
        let data = create_arrow_stream();
        let chunks = vec![Bytes::from(data)];

        let mut batch_count = 0;
        let row_count = parse_arrow_zero_copy_callback(chunks, |batch| {
            batch_count += 1;
            assert_eq!(batch.num_rows(), 3);
            Ok(())
        })
        .unwrap();

        assert_eq!(row_count, 3);
        assert_eq!(batch_count, 1);
    }

    #[test]
    fn test_zero_copy_decoder_multiple_batches() {
        // Create stream with multiple batches
        let batch = create_test_batch();
        let mut buffer = Vec::new();
        {
            let mut writer = StreamWriter::try_new(&mut buffer, &batch.schema()).unwrap();
            writer.write(&batch).unwrap();
            writer.write(&batch).unwrap(); // Write same batch twice
            writer.finish().unwrap();
        }

        let chunk = Bytes::from(buffer);
        let mut decoder = ZeroCopyArrowDecoder::new();
        let batches = decoder.decode_chunk(chunk).unwrap();
        decoder.finish().unwrap();

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].num_rows(), 3);
        assert_eq!(batches[1].num_rows(), 3);
    }

    #[test]
    fn test_zero_copy_decoder_empty_chunks() {
        let data = create_arrow_stream();

        // Intersperse empty chunks
        let chunks = vec![
            Bytes::new(),
            Bytes::from(data),
            Bytes::new(),
        ];

        let mut decoder = ZeroCopyArrowDecoder::new();
        let mut all_batches = Vec::new();

        for chunk in chunks {
            let batches = decoder.decode_chunk(chunk).unwrap();
            all_batches.extend(batches);
        }
        decoder.finish().unwrap();

        assert_eq!(all_batches.len(), 1);
        assert_eq!(all_batches[0].num_rows(), 3);
    }
}
