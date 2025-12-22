//! Window frame bounds and specifications.
//!
//! This module provides frame bound types for window functions:
//! - `UnboundedPreceding`, `UnboundedFollowing`
//! - `CurrentRow`
//! - `Preceding(n)`, `Following(n)`
//! - `RowsFrame`, `RangeFrame`

use crate::backend::Backend;
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

// =============================================================================
// Frame Bounds
// =============================================================================

/// Trait for frame bounds (UNBOUNDED PRECEDING, CURRENT ROW, etc.)
pub trait FrameBound: QueryFragment<crate::backend::HttpBackend> + QueryFragment<crate::backend::NativeBackend> {}

/// UNBOUNDED PRECEDING
#[derive(Debug, Clone, Copy)]
pub struct UnboundedPreceding;

impl FrameBound for UnboundedPreceding {}

impl<DB: Backend> QueryFragment<DB> for UnboundedPreceding {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("UNBOUNDED PRECEDING");
        Ok(())
    }
}

/// UNBOUNDED FOLLOWING
#[derive(Debug, Clone, Copy)]
pub struct UnboundedFollowing;

impl FrameBound for UnboundedFollowing {}

impl<DB: Backend> QueryFragment<DB> for UnboundedFollowing {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("UNBOUNDED FOLLOWING");
        Ok(())
    }
}

/// CURRENT ROW
#[derive(Debug, Clone, Copy)]
pub struct CurrentRow;

impl FrameBound for CurrentRow {}

impl<DB: Backend> QueryFragment<DB> for CurrentRow {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("CURRENT ROW");
        Ok(())
    }
}

/// N PRECEDING
#[derive(Debug, Clone, Copy)]
pub struct Preceding(pub i64);

impl FrameBound for Preceding {}

impl<DB: Backend> QueryFragment<DB> for Preceding {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_bindable(&self.0)?;
        pass.push_sql(" PRECEDING");
        Ok(())
    }
}

/// N FOLLOWING
#[derive(Debug, Clone, Copy)]
pub struct Following(pub i64);

impl FrameBound for Following {}

impl<DB: Backend> QueryFragment<DB> for Following {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_bindable(&self.0)?;
        pass.push_sql(" FOLLOWING");
        Ok(())
    }
}

// =============================================================================
// Frame Types
// =============================================================================

/// ROWS BETWEEN frame specification.
#[derive(Debug, Clone, Copy)]
pub struct RowsFrame<S, E> {
    pub(crate) start: S,
    pub(crate) end: E,
}

impl<S: FrameBound, E: FrameBound, DB: Backend> QueryFragment<DB> for RowsFrame<S, E>
where
    S: QueryFragment<DB>,
    E: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("ROWS BETWEEN ");
        self.start.walk_ast(pass.reborrow())?;
        pass.push_sql(" AND ");
        self.end.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

/// RANGE BETWEEN frame specification.
#[derive(Debug, Clone, Copy)]
pub struct RangeFrame<S, E> {
    pub(crate) start: S,
    pub(crate) end: E,
}

impl<S: FrameBound, E: FrameBound, DB: Backend> QueryFragment<DB> for RangeFrame<S, E>
where
    S: QueryFragment<DB>,
    E: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        pass.push_sql("RANGE BETWEEN ");
        self.start.walk_ast(pass.reborrow())?;
        pass.push_sql(" AND ");
        self.end.walk_ast(pass.reborrow())?;
        Ok(())
    }
}

// =============================================================================
// IsWindowClause trait
// =============================================================================

/// Trait to check if window clauses are empty.
pub trait IsWindowClause {
    /// Returns true if this clause is empty (unit type).
    fn is_empty(&self) -> bool;
}

impl IsWindowClause for () {
    fn is_empty(&self) -> bool {
        true
    }
}

impl<S: FrameBound, E: FrameBound> IsWindowClause for RowsFrame<S, E> {
    fn is_empty(&self) -> bool {
        false
    }
}

impl<S: FrameBound, E: FrameBound> IsWindowClause for RangeFrame<S, E> {
    fn is_empty(&self) -> bool {
        false
    }
}
