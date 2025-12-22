//! Table trait and related types.

use diesel_clickhouse_types::SqlType;
use crate::expression::{Expression, SelectableExpression};

/// A source of data in a query (typically a table).
#[allow(clippy::wrong_self_convention)] // from_clause returns clause, not constructs from it
pub trait QuerySource: Clone + Copy {
    /// The type representing the FROM clause.
    type FromClause;

    /// The default selection (all columns).
    type DefaultSelection: Expression;

    /// Get the FROM clause representation.
    fn from_clause(&self) -> Self::FromClause;

    /// Get the default selection.
    fn default_selection(&self) -> Self::DefaultSelection;
}

/// Represents a database table.
pub trait Table: QuerySource + Sized {
    /// The primary key column(s) - corresponds to ORDER BY in ClickHouse.
    type PrimaryKey: Expression;

    /// All columns in this table.
    type AllColumns: SelectableExpression<Self>;

    /// The SQL type of all columns combined.
    type AllColumnsSqlType: SqlType;

    /// Returns the table name.
    fn table_name() -> &'static str;

    /// Returns all columns as a tuple.
    fn all_columns() -> Self::AllColumns;

    /// Returns the primary key columns.
    fn primary_key() -> Self::PrimaryKey;
}

/// Trait for specifying the table engine.
pub trait TableEngine: Clone + Copy + 'static {
    /// Returns the engine name.
    fn engine_name() -> &'static str;
}

// =============================================================================
// Table Engine Markers
// =============================================================================

/// MergeTree engine - the default and most versatile engine.
#[derive(Debug, Clone, Copy, Default)]
pub struct MergeTree;

impl TableEngine for MergeTree {
    fn engine_name() -> &'static str {
        "MergeTree"
    }
}

/// ReplacingMergeTree engine - deduplicates rows with same primary key.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReplacingMergeTree<V = ()> {
    _version: std::marker::PhantomData<V>,
}

impl<V: 'static + Clone + Copy> TableEngine for ReplacingMergeTree<V> {
    fn engine_name() -> &'static str {
        "ReplacingMergeTree"
    }
}

/// SummingMergeTree engine - pre-aggregates numeric columns.
#[derive(Debug, Clone, Copy, Default)]
pub struct SummingMergeTree<C = ()> {
    _columns: std::marker::PhantomData<C>,
}

impl<C: 'static + Clone + Copy> TableEngine for SummingMergeTree<C> {
    fn engine_name() -> &'static str {
        "SummingMergeTree"
    }
}

/// AggregatingMergeTree engine - stores pre-computed aggregate states.
#[derive(Debug, Clone, Copy, Default)]
pub struct AggregatingMergeTree;

impl TableEngine for AggregatingMergeTree {
    fn engine_name() -> &'static str {
        "AggregatingMergeTree"
    }
}

/// CollapsingMergeTree engine - uses sign column for incremental aggregation.
#[derive(Debug, Clone, Copy, Default)]
pub struct CollapsingMergeTree<S> {
    _sign: std::marker::PhantomData<S>,
}

impl<S: 'static + Clone + Copy> TableEngine for CollapsingMergeTree<S> {
    fn engine_name() -> &'static str {
        "CollapsingMergeTree"
    }
}

/// VersionedCollapsingMergeTree engine.
#[derive(Debug, Clone, Copy, Default)]
pub struct VersionedCollapsingMergeTree<S, V> {
    _sign: std::marker::PhantomData<S>,
    _version: std::marker::PhantomData<V>,
}

impl<S: 'static + Clone + Copy, V: 'static + Clone + Copy> TableEngine for VersionedCollapsingMergeTree<S, V> {
    fn engine_name() -> &'static str {
        "VersionedCollapsingMergeTree"
    }
}

/// Memory engine - stores data in RAM.
#[derive(Debug, Clone, Copy, Default)]
pub struct Memory;

impl TableEngine for Memory {
    fn engine_name() -> &'static str {
        "Memory"
    }
}

/// Log engine - simple append-only storage.
#[derive(Debug, Clone, Copy, Default)]
pub struct Log;

impl TableEngine for Log {
    fn engine_name() -> &'static str {
        "Log"
    }
}

/// Null engine - discards all data (useful for testing).
#[derive(Debug, Clone, Copy, Default)]
pub struct Null;

impl TableEngine for Null {
    fn engine_name() -> &'static str {
        "Null"
    }
}

// =============================================================================
// HasTable trait
// =============================================================================

/// Associates a type with its table.
pub trait HasTable {
    /// The table this type belongs to.
    type Table: Table;

    /// Get the table.
    fn table() -> Self::Table;
}

// =============================================================================
// IntoTable trait
// =============================================================================

/// Converts something into a table reference.
pub trait IntoTable {
    /// The table type.
    type Table: Table;

    /// Convert to table.
    fn into_table(self) -> Self::Table;
}

impl<T: Table> IntoTable for T {
    type Table = T;

    fn into_table(self) -> Self::Table {
        self
    }
}
