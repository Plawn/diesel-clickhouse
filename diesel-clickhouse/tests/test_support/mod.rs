//! Test support utilities for integration tests.
//!
//! This module provides helpers for setting up test infrastructure,
//! including testcontainers support for ClickHouse.

#[cfg(feature = "testcontainers")]
pub mod containers;

#[cfg(feature = "testcontainers")]
pub use containers::*;
