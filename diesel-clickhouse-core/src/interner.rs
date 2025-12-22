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
//! // Resolve back to string
//! assert_eq!(interner.resolve(id_sym), Some("id"));
//! ```

use std::sync::RwLock;
use string_interner::{DefaultSymbol, StringInterner, DefaultBackend};

/// Error type for interner operations.
#[derive(Debug, Clone)]
pub struct InternerError(String);

impl std::fmt::Display for InternerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for InternerError {}

/// Result type for interner operations.
pub type InternerResult<T> = Result<T, InternerError>;

/// A symbol representing an interned string.
///
/// Symbols are lightweight (just a u32) and can be compared
/// in O(1) time. They remain valid as long as the interner exists.
pub type Symbol = DefaultSymbol;

/// Thread-safe string interner for column names.
///
/// This interner stores unique strings once and returns symbols
/// that can be used for fast comparison and lookup.
pub struct ColumnInterner {
    inner: RwLock<StringInterner<DefaultBackend>>,
}

impl ColumnInterner {
    /// Create a new column interner.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(StringInterner::new()),
        }
    }

    /// Create an interner with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: RwLock::new(StringInterner::with_capacity(capacity)),
        }
    }

    /// Intern a string, returning its symbol.
    ///
    /// If the string was already interned, returns the existing symbol.
    /// Otherwise, stores the string and returns a new symbol.
    ///
    /// Returns an error if the internal RwLock is poisoned.
    #[inline]
    pub fn intern(&self, s: &str) -> InternerResult<Symbol> {
        // Fast path: check if already interned (read lock)
        {
            let interner = self.inner.read()
                .map_err(|e| InternerError(format!("ColumnInterner RwLock poisoned: {}", e)))?;
            if let Some(sym) = interner.get(s) {
                return Ok(sym);
            }
        }

        // Slow path: intern the string (write lock)
        let mut interner = self.inner.write()
            .map_err(|e| InternerError(format!("ColumnInterner RwLock poisoned: {}", e)))?;
        Ok(interner.get_or_intern(s))
    }

    /// Get the symbol for a string if it was already interned.
    ///
    /// Returns `Ok(None)` if the string has not been interned.
    /// Returns an error if the internal RwLock is poisoned.
    #[inline]
    pub fn get(&self, s: &str) -> InternerResult<Option<Symbol>> {
        let interner = self.inner.read()
            .map_err(|e| InternerError(format!("ColumnInterner RwLock poisoned: {}", e)))?;
        Ok(interner.get(s))
    }

    /// Resolve a symbol back to its string.
    ///
    /// Returns `Ok(None)` if the symbol is invalid.
    /// Returns an error if the internal RwLock is poisoned.
    #[inline]
    pub fn resolve(&self, sym: Symbol) -> InternerResult<Option<String>> {
        let interner = self.inner.read()
            .map_err(|e| InternerError(format!("ColumnInterner RwLock poisoned: {}", e)))?;
        Ok(interner.resolve(sym).map(|s| s.to_owned()))
    }

    /// Resolve a symbol, returning a reference via a closure.
    ///
    /// This avoids allocating a new string.
    ///
    /// Returns an error if the internal RwLock is poisoned.
    #[inline]
    pub fn with_resolved<F, R>(&self, sym: Symbol, f: F) -> InternerResult<Option<R>>
    where
        F: FnOnce(&str) -> R,
    {
        let interner = self.inner.read()
            .map_err(|e| InternerError(format!("ColumnInterner RwLock poisoned: {}", e)))?;
        Ok(interner.resolve(sym).map(f))
    }

    /// Get the number of interned strings.
    ///
    /// Returns an error if the internal RwLock is poisoned.
    pub fn len(&self) -> InternerResult<usize> {
        let interner = self.inner.read()
            .map_err(|e| InternerError(format!("ColumnInterner RwLock poisoned: {}", e)))?;
        Ok(interner.len())
    }

    /// Check if the interner is empty.
    ///
    /// Returns an error if the internal RwLock is poisoned.
    pub fn is_empty(&self) -> InternerResult<bool> {
        Ok(self.len()? == 0)
    }

    /// Clear all interned strings.
    ///
    /// Warning: This invalidates all existing symbols!
    ///
    /// Returns an error if the internal RwLock is poisoned.
    pub fn clear(&self) -> InternerResult<()> {
        let mut interner = self.inner.write()
            .map_err(|e| InternerError(format!("ColumnInterner RwLock poisoned: {}", e)))?;
        *interner = StringInterner::<DefaultBackend>::new();
        Ok(())
    }
}

impl Default for ColumnInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ColumnInterner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.len().unwrap_or(0);
        f.debug_struct("ColumnInterner")
            .field("len", &len)
            .finish()
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
pub fn intern(s: &str) -> InternerResult<Symbol> {
    global_interner().intern(s)
}

/// Resolve a symbol from the global interner.
///
/// Convenience function equivalent to `global_interner().resolve(sym)`.
#[inline]
pub fn resolve(sym: Symbol) -> InternerResult<Option<String>> {
    global_interner().resolve(sym)
}

/// A column schema with interned column names.
///
/// This is useful for query results where the same column names
/// are used across many rows.
#[derive(Debug, Clone)]
pub struct InternedSchema {
    /// Interned column name symbols.
    columns: Vec<Symbol>,
}

impl InternedSchema {
    /// Create a new interned schema from column names.
    ///
    /// Returns an error if any interning operation fails.
    pub fn new(column_names: &[&str]) -> InternerResult<Self> {
        let interner = global_interner();
        let mut columns = Vec::with_capacity(column_names.len());
        for &name in column_names {
            columns.push(interner.intern(name)?);
        }
        Ok(Self { columns })
    }

    /// Create from owned strings.
    ///
    /// Returns an error if any interning operation fails.
    pub fn from_strings(column_names: &[String]) -> InternerResult<Self> {
        let interner = global_interner();
        let mut columns = Vec::with_capacity(column_names.len());
        for name in column_names {
            columns.push(interner.intern(name)?);
        }
        Ok(Self { columns })
    }

    /// Get the number of columns.
    #[inline]
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Check if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Get a column symbol by index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<Symbol> {
        self.columns.get(index).copied()
    }

    /// Get the column name by index.
    ///
    /// Returns an error if the interner lock is poisoned.
    pub fn column_name(&self, index: usize) -> InternerResult<Option<String>> {
        match self.columns.get(index) {
            Some(&sym) => resolve(sym),
            None => Ok(None),
        }
    }

    /// Find the index of a column by name.
    ///
    /// Returns an error if the interner lock is poisoned.
    pub fn find_column(&self, name: &str) -> InternerResult<Option<usize>> {
        let interner = global_interner();
        match interner.get(name)? {
            Some(target_sym) => Ok(self.columns.iter().position(|&sym| sym == target_sym)),
            None => Ok(None),
        }
    }

    /// Iterate over column symbols.
    pub fn iter(&self) -> impl Iterator<Item = Symbol> + '_ {
        self.columns.iter().copied()
    }

    /// Iterate over column names (allocates strings).
    ///
    /// Silently skips columns that fail to resolve.
    pub fn names(&self) -> impl Iterator<Item = String> + '_ {
        self.columns.iter().filter_map(|&sym| {
            resolve(sym).ok().flatten()
        })
    }
}

/// An interned row that uses symbols for column lookups.
#[derive(Debug)]
pub struct InternedRow<'a> {
    schema: &'a InternedSchema,
    values: Vec<Vec<u8>>,
}

impl<'a> InternedRow<'a> {
    /// Create a new interned row.
    pub fn new(schema: &'a InternedSchema, values: Vec<Vec<u8>>) -> Self {
        Self { schema, values }
    }

    /// Get value by column index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&[u8]> {
        self.values.get(index).map(|v| v.as_slice())
    }

    /// Get value by column name.
    ///
    /// Returns an error if the interner lock is poisoned.
    pub fn get_by_name(&self, name: &str) -> InternerResult<Option<&[u8]>> {
        match self.schema.find_column(name)? {
            Some(index) => Ok(self.get(index)),
            None => Ok(None),
        }
    }

    /// Get the number of columns.
    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_and_resolve() {
        let interner = ColumnInterner::new();

        let sym1 = interner.intern("id").expect("intern failed");
        let sym2 = interner.intern("name").expect("intern failed");
        let sym3 = interner.intern("id").expect("intern failed");

        // Same string = same symbol
        assert_eq!(sym1, sym3);
        assert_ne!(sym1, sym2);

        // Resolve back to string
        assert_eq!(interner.resolve(sym1).expect("resolve failed"), Some("id".to_owned()));
        assert_eq!(interner.resolve(sym2).expect("resolve failed"), Some("name".to_owned()));
    }

    #[test]
    fn test_with_resolved() {
        let interner = ColumnInterner::new();
        let sym = interner.intern("test_column").expect("intern failed");

        let len = interner.with_resolved(sym, |s| s.len()).expect("with_resolved failed");
        assert_eq!(len, Some(11));
    }

    #[test]
    fn test_interned_schema() {
        let schema = InternedSchema::new(&["id", "name", "age"]).expect("schema creation failed");

        assert_eq!(schema.len(), 3);
        assert_eq!(schema.column_name(0).expect("column_name failed"), Some("id".to_owned()));
        assert_eq!(schema.column_name(1).expect("column_name failed"), Some("name".to_owned()));
        assert_eq!(schema.find_column("age").expect("find_column failed"), Some(2));
        assert_eq!(schema.find_column("missing").expect("find_column failed"), None);
    }

    #[test]
    fn test_interned_row() {
        let schema = InternedSchema::new(&["id", "name"]).expect("schema creation failed");
        let values = vec![vec![1, 0, 0, 0], b"alice".to_vec()];
        let row = InternedRow::new(&schema, values);

        assert_eq!(row.get(0), Some([1, 0, 0, 0].as_slice()));
        assert_eq!(row.get_by_name("name").expect("get_by_name failed"), Some(b"alice".as_slice()));
        assert_eq!(row.get_by_name("missing").expect("get_by_name failed"), None);
    }

    #[test]
    fn test_global_interner() {
        let sym1 = intern("global_test").expect("intern failed");
        let sym2 = intern("global_test").expect("intern failed");
        assert_eq!(sym1, sym2);
        assert_eq!(resolve(sym1).expect("resolve failed"), Some("global_test".to_owned()));
    }
}
