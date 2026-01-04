//! String interning for column names and frequently used strings.
//!
//! String interning stores unique strings once and returns lightweight
//! symbols that can be compared in O(1) time. This is especially useful
//! for column names that are repeated across many rows.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse_core::interner::{ColumnInterner, global_interner};
//!
//! // Get the global interner
//! let interner = global_interner();
//!
//! // Intern column names (deduplicates automatically)
//! let id_sym = interner.intern("id");
//! let name_sym = interner.intern("name");
//! let id_sym2 = interner.intern("id");
//!
//! // Same string = same symbol
//! assert_eq!(id_sym, id_sym2);
//!
//! // Resolve back to string (zero-allocation)
//! let is_id = interner.with_resolved(id_sym, |s| s == "id");
//! assert_eq!(is_id, Some(true));
//! ```
//!
//! # Performance Notes
//!
//! - Uses `lasso::ThreadedRodeo` with AHash for fast, thread-safe interning
//! - O(1) interning and resolution
//! - Use `with_resolved()` for zero-allocation string access
//! - For `InternedSchema`, column lookup is O(1) via internal HashMap (AHash)

use ahash::{HashMap, HashMapExt};
use lasso::{Capacity, Spur, ThreadedRodeo};
use smallvec::SmallVec;

/// A symbol representing an interned string.
///
/// Symbols are lightweight (just a u32) and can be compared
/// in O(1) time. They remain valid as long as the interner exists.
pub type Symbol = Spur;

/// Thread-safe string interner for column names.
///
/// This interner stores unique strings once and returns symbols
/// that can be used for fast comparison and lookup.
///
/// Uses `lasso::ThreadedRodeo` which provides:
/// - O(1) interning and resolution
/// - Built-in thread safety (no external locking needed)
/// - AHash for fast hashing
/// - Lock-free reads after initial interning
#[derive(Debug)]
pub struct ColumnInterner {
    inner: ThreadedRodeo<Spur>,
}

impl ColumnInterner {
    /// Create a new column interner.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: ThreadedRodeo::new(),
        }
    }

    /// Create an interner with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: ThreadedRodeo::with_capacity(Capacity::for_strings(capacity)),
        }
    }

    /// Intern a string, returning its symbol.
    ///
    /// If the string was already interned, returns the existing symbol.
    /// Otherwise, stores the string and returns a new symbol.
    #[inline]
    #[must_use]
    pub fn intern(&self, s: &str) -> Symbol {
        self.inner.get_or_intern(s)
    }

    /// Get the symbol for a string if it was already interned.
    ///
    /// Returns `None` if the string has not been interned.
    #[inline]
    #[must_use]
    pub fn get(&self, s: &str) -> Option<Symbol> {
        self.inner.get(s)
    }

    /// Resolve a symbol, returning a reference via a closure.
    ///
    /// This is the zero-allocation way to access interned strings.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let sym = interner.intern("column_name");
    /// let len = interner.with_resolved(sym, |s| s.len());
    /// assert_eq!(len, Some(11));
    /// ```
    #[inline]
    #[must_use]
    pub fn with_resolved<F, R>(&self, sym: Symbol, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        self.inner.try_resolve(&sym).map(f)
    }

    /// Resolve a symbol directly to a string reference.
    ///
    /// Returns `None` if the symbol is not valid.
    ///
    /// # Note
    ///
    /// The returned reference has the same lifetime as `self`. For the global
    /// interner, this is effectively `'static`.
    #[inline]
    #[must_use]
    pub fn resolve(&self, sym: Symbol) -> Option<&str> {
        self.inner.try_resolve(&sym)
    }

    /// Get the number of interned strings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if the interner is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for ColumnInterner {
    fn default() -> Self {
        Self::new()
    }
}

/// Global column interner instance.
static GLOBAL_INTERNER: std::sync::OnceLock<ColumnInterner> = std::sync::OnceLock::new();

/// Get the global column interner.
///
/// This interner is shared across all threads and is useful for
/// interning column names that are used throughout an application.
pub fn global_interner() -> &'static ColumnInterner {
    GLOBAL_INTERNER.get_or_init(|| ColumnInterner::with_capacity(256))
}

/// Intern a string in the global interner.
///
/// Convenience function equivalent to `global_interner().intern(s)`.
#[inline]
#[must_use]
pub fn intern(s: &str) -> Symbol {
    global_interner().intern(s)
}

/// Maximum columns stored inline (no heap allocation).
/// Most database tables have fewer than 16 columns.
const INLINE_COLUMNS: usize = 16;

/// A column schema with interned column names.
///
/// This is useful for query results where the same column names
/// are used across many rows.
///
/// # Allocation Optimization
///
/// - Columns are stored in a `SmallVec<[Symbol; 16]>`, avoiding heap allocation
///   for schemas with ≤16 columns (covers most use cases).
/// - Lookup HashMap uses AHash for fast O(1) column name lookups.
#[derive(Debug, Clone)]
pub struct InternedSchema {
    /// Interned column name symbols (inline for ≤16 columns).
    columns: SmallVec<[Symbol; INLINE_COLUMNS]>,
    /// Symbol -> index lookup for O(1) find_column.
    lookup: HashMap<Symbol, usize>,
}

impl InternedSchema {
    /// Create a new interned schema from column names.
    ///
    /// Accepts any iterator of string-like types (`&str`, `String`, `&String`, etc.).
    ///
    /// # Allocation Behavior
    ///
    /// - ≤16 columns: No heap allocation for column storage (inline SmallVec)
    /// - >16 columns: Single heap allocation for columns
    /// - HashMap always allocates (for O(1) lookup)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // From string slices (no heap alloc for columns)
    /// let schema = InternedSchema::new(&["id", "name", "age"]);
    ///
    /// // From owned strings
    /// let names = vec!["id".to_string(), "name".to_string()];
    /// let schema = InternedSchema::new(&names);
    ///
    /// // From iterator
    /// let schema = InternedSchema::new(["id", "name"].into_iter());
    /// ```
    #[must_use]
    pub fn new<I, S>(column_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let interner = global_interner();
        let iter = column_names.into_iter();
        let (lower, upper) = iter.size_hint();
        let cap = upper.unwrap_or(lower);

        let mut columns = SmallVec::with_capacity(cap);
        let mut lookup = HashMap::with_capacity(cap);

        for (idx, name) in iter.enumerate() {
            let sym = interner.intern(name.as_ref());
            columns.push(sym);
            lookup.insert(sym, idx);
        }

        Self { columns, lookup }
    }

    /// Get the number of columns.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Check if empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Get a column symbol by index.
    #[inline]
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Symbol> {
        self.columns.get(index).copied()
    }

    /// Access a column name by index via closure (zero-allocation).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let schema = InternedSchema::new(&["id", "name"]);
    /// let name_len = schema.with_column_name(0, |s| s.len());
    /// assert_eq!(name_len, Some(2)); // "id".len()
    /// ```
    #[inline]
    #[must_use]
    pub fn with_column_name<F, R>(&self, index: usize, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        self.columns
            .get(index)
            .and_then(|&sym| global_interner().with_resolved(sym, f))
    }

    /// Find the index of a column by name (O(1) lookup).
    #[inline]
    #[must_use]
    pub fn find_column(&self, name: &str) -> Option<usize> {
        global_interner()
            .get(name)
            .and_then(|sym| self.lookup.get(&sym).copied())
    }

    /// Get the column name by index.
    ///
    /// Returns a reference to the interned string, which lives as long as the
    /// global interner (effectively `'static`).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let schema = InternedSchema::new(&["id", "name"]);
    /// assert_eq!(schema.column_name(0), Some("id"));
    /// assert_eq!(schema.column_name(1), Some("name"));
    /// assert_eq!(schema.column_name(2), None);
    /// ```
    #[inline]
    #[must_use]
    pub fn column_name(&self, index: usize) -> Option<&'static str> {
        self.columns
            .get(index)
            .and_then(|&sym| global_interner().resolve(sym))
    }

    /// Iterate over column symbols.
    pub fn iter(&self) -> impl Iterator<Item = Symbol> + '_ {
        self.columns.iter().copied()
    }

    /// Iterate over columns, calling a closure for each name (zero-allocation).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let schema = InternedSchema::new(&["id", "name", "age"]);
    /// let mut names = Vec::new();
    /// schema.for_each_name(|name| names.push(name.to_uppercase()));
    /// assert_eq!(names, vec!["ID", "NAME", "AGE"]);
    /// ```
    pub fn for_each_name<F>(&self, mut f: F)
    where
        F: FnMut(&str),
    {
        let interner = global_interner();
        for &sym in &self.columns {
            let _ = interner.with_resolved(sym, &mut f);
        }
    }
}

/// Maximum column offsets stored inline (N+1 for N columns).
const INLINE_OFFSETS: usize = INLINE_COLUMNS + 1;

/// An interned row with flattened storage for column values.
///
/// # Allocation Optimization
///
/// Instead of `Vec<Vec<u8>>` (N+1 allocations), this uses:
/// - Single `Vec<u8>` buffer containing all column data
/// - `SmallVec<[u32; 17]>` for offsets (inline for ≤16 columns)
///
/// This reduces allocations from N+1 to 1-2 per row.
#[derive(Debug, Clone)]
pub struct InternedRow<'a> {
    schema: &'a InternedSchema,
    /// Concatenated column data.
    data: Vec<u8>,
    /// Byte offsets for each column (length = num_columns + 1).
    /// Column i spans `data[offsets[i]..offsets[i+1]]`.
    offsets: SmallVec<[u32; INLINE_OFFSETS]>,
}

impl<'a> InternedRow<'a> {
    /// Create a new interned row from separate column values.
    ///
    /// This flattens the values into a single buffer for better cache locality.
    #[must_use]
    pub fn new(schema: &'a InternedSchema, values: Vec<Vec<u8>>) -> Self {
        let total_len: usize = values.iter().map(|v| v.len()).sum();
        let mut data = Vec::with_capacity(total_len);
        let mut offsets = SmallVec::with_capacity(values.len() + 1);

        for value in values {
            offsets.push(data.len() as u32);
            data.extend_from_slice(&value);
        }
        offsets.push(data.len() as u32);

        Self { schema, data, offsets }
    }

    /// Create from pre-flattened data (zero-copy for data).
    ///
    /// `offsets` should have length `num_columns + 1`, where
    /// column `i` spans `data[offsets[i]..offsets[i+1]]`.
    #[must_use]
    pub fn from_flattened(schema: &'a InternedSchema, data: Vec<u8>, offsets: SmallVec<[u32; INLINE_OFFSETS]>) -> Self {
        Self { schema, data, offsets }
    }

    /// Get value by column index.
    #[inline]
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&[u8]> {
        if index + 1 < self.offsets.len() {
            let start = self.offsets[index] as usize;
            let end = self.offsets[index + 1] as usize;
            Some(&self.data[start..end])
        } else {
            None
        }
    }

    /// Get value by column name.
    #[must_use]
    pub fn get_by_name(&self, name: &str) -> Option<&[u8]> {
        self.schema.find_column(name).and_then(|index| self.get(index))
    }

    /// Get the number of columns.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// Check if empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.offsets.len() <= 1
    }

    /// Get total data size in bytes.
    #[inline]
    #[must_use]
    pub fn data_size(&self) -> usize {
        self.data.len()
    }
}

/// Zero-copy view over borrowed row data.
///
/// # Allocation Optimization
///
/// This is a completely zero-allocation row view:
/// - Borrows the schema (no copy)
/// - Borrows the data buffer (no copy)
/// - Borrows the offsets (no copy)
///
/// Use this for high-throughput scenarios where you're iterating
/// over many rows without needing to own the data.
#[derive(Debug, Clone, Copy)]
pub struct InternedRowView<'a> {
    schema: &'a InternedSchema,
    /// Borrowed column data.
    data: &'a [u8],
    /// Borrowed offsets.
    offsets: &'a [u32],
}

impl<'a> InternedRowView<'a> {
    /// Create a zero-copy view over row data.
    ///
    /// `offsets` should have length `num_columns + 1`.
    #[inline]
    #[must_use]
    pub fn new(schema: &'a InternedSchema, data: &'a [u8], offsets: &'a [u32]) -> Self {
        Self { schema, data, offsets }
    }

    /// Create a view from an owned InternedRow.
    #[inline]
    #[must_use]
    pub fn from_row(row: &'a InternedRow<'a>) -> Self {
        Self {
            schema: row.schema,
            data: &row.data,
            offsets: &row.offsets,
        }
    }

    /// Get value by column index.
    #[inline]
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&'a [u8]> {
        if index + 1 < self.offsets.len() {
            let start = self.offsets[index] as usize;
            let end = self.offsets[index + 1] as usize;
            Some(&self.data[start..end])
        } else {
            None
        }
    }

    /// Get value by column name.
    #[must_use]
    pub fn get_by_name(&self, name: &str) -> Option<&'a [u8]> {
        self.schema.find_column(name).and_then(|index| self.get(index))
    }

    /// Get the number of columns.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// Check if empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.offsets.len() <= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_and_resolve() {
        let interner = ColumnInterner::new();

        let sym1 = interner.intern("id");
        let sym2 = interner.intern("name");
        let sym3 = interner.intern("id");

        // Same string = same symbol
        assert_eq!(sym1, sym3);
        assert_ne!(sym1, sym2);

        // Resolve back to string via with_resolved
        assert_eq!(interner.with_resolved(sym1, |s| s == "id"), Some(true));
        assert_eq!(interner.with_resolved(sym2, |s| s == "name"), Some(true));
    }

    #[test]
    fn test_with_resolved() {
        let interner = ColumnInterner::new();
        let sym = interner.intern("test_column");

        let len = interner.with_resolved(sym, |s| s.len());
        assert_eq!(len, Some(11));
    }

    #[test]
    fn test_interned_schema() {
        let schema = InternedSchema::new(["id", "name", "age"]);

        assert_eq!(schema.len(), 3);
        assert_eq!(schema.with_column_name(0, |s| s == "id"), Some(true));
        assert_eq!(schema.with_column_name(1, |s| s == "name"), Some(true));
        assert_eq!(schema.find_column("age"), Some(2));
        assert_eq!(schema.find_column("missing"), None);
    }

    #[test]
    fn test_interned_schema_from_strings() {
        // Test that new() works with owned strings
        let names = vec!["col1".to_string(), "col2".to_string()];
        let schema = InternedSchema::new(&names);

        assert_eq!(schema.len(), 2);
        assert_eq!(schema.with_column_name(0, |s| s == "col1"), Some(true));
    }

    #[test]
    fn test_interned_schema_for_each_name() {
        let schema = InternedSchema::new(["id", "name", "age"]);

        let mut collected = Vec::new();
        schema.for_each_name(|name| collected.push(name.to_string()));

        assert_eq!(collected, vec!["id", "name", "age"]);
    }

    #[test]
    fn test_interned_row() {
        let schema = InternedSchema::new(["id", "name"]);
        let values = vec![vec![1, 0, 0, 0], b"alice".to_vec()];
        let row = InternedRow::new(&schema, values);

        assert_eq!(row.len(), 2);
        assert_eq!(row.get(0), Some([1, 0, 0, 0].as_slice()));
        assert_eq!(row.get_by_name("name"), Some(b"alice".as_slice()));
        assert_eq!(row.get_by_name("missing"), None);
        assert_eq!(row.data_size(), 9); // 4 bytes + 5 bytes
    }

    #[test]
    fn test_interned_row_flattened() {
        let schema = InternedSchema::new(["a", "b", "c"]);

        // Pre-flattened data
        let data = vec![1, 2, 3, 4, 5, 6];
        let offsets: SmallVec<[u32; INLINE_OFFSETS]> = smallvec::smallvec![0, 2, 4, 6];

        let row = InternedRow::from_flattened(&schema, data, offsets);

        assert_eq!(row.len(), 3);
        assert_eq!(row.get(0), Some([1, 2].as_slice()));
        assert_eq!(row.get(1), Some([3, 4].as_slice()));
        assert_eq!(row.get(2), Some([5, 6].as_slice()));
        assert_eq!(row.get(3), None);
    }

    #[test]
    fn test_interned_row_view() {
        let schema = InternedSchema::new(["id", "name"]);
        let values = vec![vec![1, 0, 0, 0], b"alice".to_vec()];
        let row = InternedRow::new(&schema, values);

        // Create view from row
        let view = InternedRowView::from_row(&row);

        assert_eq!(view.len(), 2);
        assert_eq!(view.get(0), Some([1, 0, 0, 0].as_slice()));
        assert_eq!(view.get_by_name("name"), Some(b"alice".as_slice()));
        assert_eq!(view.get_by_name("missing"), None);
    }

    #[test]
    fn test_interned_row_view_direct() {
        let schema = InternedSchema::new(["x", "y"]);
        let data: &[u8] = &[10, 20, 30, 40, 50];
        let offsets: &[u32] = &[0, 2, 5];

        let view = InternedRowView::new(&schema, data, offsets);

        assert_eq!(view.len(), 2);
        assert_eq!(view.get(0), Some([10, 20].as_slice()));
        assert_eq!(view.get(1), Some([30, 40, 50].as_slice()));
    }

    #[test]
    fn test_interned_row_empty() {
        let schema = InternedSchema::new::<[&str; 0], &str>([]);
        let row = InternedRow::new(&schema, vec![]);

        assert!(row.is_empty());
        assert_eq!(row.len(), 0);
        assert_eq!(row.get(0), None);
    }

    #[test]
    fn test_global_interner() {
        let sym1 = intern("global_test");
        let sym2 = intern("global_test");
        assert_eq!(sym1, sym2);
        // Use with_resolved for zero-allocation check
        let matches = global_interner().with_resolved(sym1, |s| s == "global_test");
        assert_eq!(matches, Some(true));
    }
}
