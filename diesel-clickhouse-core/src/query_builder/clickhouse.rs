//! ClickHouse-specific query extensions.

use crate::backend::Backend;
use crate::expression::Expression;
use crate::result::QueryResult;
use super::{QueryFragment, AstPass};

/// FINAL modifier for ReplacingMergeTree queries.
#[derive(Debug, Clone, Copy)]
pub struct Final<T> {
    inner: T,
}

impl<T> Final<T> {
    /// Create a new FINAL wrapper.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Get the inner query.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T, DB> QueryFragment<DB> for Final<T>
where
    T: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.inner.walk_ast(pass.reborrow())?;
        pass.push_sql(" FINAL");
        Ok(())
    }
}

/// PREWHERE clause for optimized filtering.
#[derive(Debug, Clone, Copy)]
pub struct Prewhere<T, P> {
    inner: T,
    predicate: P,
}

impl<T, P> Prewhere<T, P> {
    /// Create a new PREWHERE clause.
    pub fn new(inner: T, predicate: P) -> Self {
        Self { inner, predicate }
    }
}

impl<T, P, DB> QueryFragment<DB> for Prewhere<T, P>
where
    T: QueryFragment<DB>,
    P: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.inner.walk_ast(pass.reborrow())?;
        pass.push_sql(" PREWHERE ");
        self.predicate.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// SAMPLE clause for approximate queries.
#[derive(Debug, Clone, Copy)]
pub struct Sample<T> {
    inner: T,
    ratio: f64,
    offset: Option<f64>,
}

impl<T> Sample<T> {
    /// Create a new SAMPLE clause with a ratio.
    pub fn ratio(inner: T, ratio: f64) -> Self {
        Self {
            inner,
            ratio,
            offset: None,
        }
    }

    /// Create a new SAMPLE clause with ratio and offset.
    pub fn with_offset(inner: T, ratio: f64, offset: f64) -> Self {
        Self {
            inner,
            ratio,
            offset: Some(offset),
        }
    }
}

impl<T, DB> QueryFragment<DB> for Sample<T>
where
    T: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.inner.walk_ast(pass.reborrow())?;
        pass.push_sql(" SAMPLE ");
        let mut buf = ryu::Buffer::new();
        pass.push_sql(buf.format_finite(self.ratio));
        if let Some(offset) = self.offset {
            pass.push_sql(" OFFSET ");
            let mut buf = ryu::Buffer::new();
            pass.push_sql(buf.format_finite(offset));
        }
        Ok(())
    }
}

/// WITH TOTALS modifier for aggregate queries.
#[derive(Debug, Clone, Copy)]
pub struct WithTotals<T> {
    inner: T,
}

impl<T> WithTotals<T> {
    /// Create a new WITH TOTALS wrapper.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T, DB> QueryFragment<DB> for WithTotals<T>
where
    T: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.inner.walk_ast(pass.reborrow())?;
        pass.push_sql(" WITH TOTALS");
        Ok(())
    }
}

/// FORMAT clause for specifying output format.
#[derive(Debug, Clone)]
pub struct Format<T> {
    inner: T,
    format: String,
}

impl<T> Format<T> {
    /// Create a new FORMAT clause.
    pub fn new(inner: T, format: impl Into<String>) -> Self {
        Self {
            inner,
            format: format.into(),
        }
    }
}

impl<T, DB> QueryFragment<DB> for Format<T>
where
    T: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.inner.walk_ast(pass.reborrow())?;
        pass.push_sql(" FORMAT ");
        pass.push_sql(&self.format);
        Ok(())
    }
}

/// SETTINGS clause for query-level settings.
#[derive(Debug, Clone)]
pub struct Settings<T> {
    inner: T,
    settings: Vec<(String, String)>,
}

impl<T> Settings<T> {
    /// Create a new SETTINGS clause.
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            // Pre-allocate for typical use case (2-4 settings)
            settings: Vec::with_capacity(4),
        }
    }

    /// Add a setting.
    pub fn set(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.settings.push((key.into(), value.into()));
        self
    }
}

impl<T, DB> QueryFragment<DB> for Settings<T>
where
    T: QueryFragment<DB>,
    DB: Backend,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.inner.walk_ast(pass.reborrow())?;
        if !self.settings.is_empty() {
            pass.push_sql(" SETTINGS ");
            for (i, (key, value)) in self.settings.iter().enumerate() {
                if i > 0 {
                    pass.push_sql(", ");
                }
                pass.push_sql(key);
                pass.push_sql(" = ");
                pass.push_sql(value);
            }
        }
        Ok(())
    }
}

/// Extension trait for ClickHouse-specific query modifications.
pub trait ClickHouseQueryExt: Sized {
    /// Add FINAL modifier (forces merge for *MergeTree tables).
    fn final_(self) -> Final<Self> {
        Final::new(self)
    }

    /// Add PREWHERE clause (optimized pre-filtering).
    fn prewhere<P>(self, predicate: P) -> Prewhere<Self, P>
    where
        P: Expression,
    {
        Prewhere::new(self, predicate)
    }

    /// Add SAMPLE clause.
    fn sample(self, ratio: f64) -> Sample<Self> {
        Sample::ratio(self, ratio)
    }

    /// Add SAMPLE with OFFSET.
    fn sample_with_offset(self, ratio: f64, offset: f64) -> Sample<Self> {
        Sample::with_offset(self, ratio, offset)
    }

    /// Add WITH TOTALS modifier.
    fn with_totals(self) -> WithTotals<Self> {
        WithTotals::new(self)
    }

    /// Specify output FORMAT.
    fn format(self, format: impl Into<String>) -> Format<Self> {
        Format::new(self, format)
    }

    /// Add SETTINGS clause.
    fn settings(self) -> Settings<Self> {
        Settings::new(self)
    }
}

// Implement for all types
impl<T> ClickHouseQueryExt for T {}
