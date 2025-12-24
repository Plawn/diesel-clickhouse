//! Unified row traits for backend-agnostic query execution.
//!
//! These traits abstract over the different row type requirements of HTTP and Native
//! backends, eliminating the need for repetitive `#[cfg]` attributes on query methods.
//!
//! # How it works
//!
//! Instead of writing 3 versions of each method with different bounds, you can use
//! these unified traits that have conditional supertraits based on enabled features.

// =============================================================================
// UnifiedRow - for load() methods
// =============================================================================

/// Trait for row types that can be loaded from the database.
///
/// This trait unifies the row type requirements across HTTP and Native backends.
/// It has different supertraits depending on which features are enabled:
///
/// - **HTTP only**: Requires `clickhouse::Row + RowOwned + RowRead + Send`
/// - **Native only**: Requires `FromNativeBlock + Send`
/// - **Both backends**: Requires all of the above
///
/// Types marked with `#[row]` or `#[derive(Row)]` automatically implement this trait.
#[cfg(all(feature = "http", not(feature = "native")))]
pub trait UnifiedRow: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send {}

#[cfg(all(feature = "http", not(feature = "native")))]
impl<T> UnifiedRow for T where T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send
{}

#[cfg(all(feature = "native", not(feature = "http")))]
pub trait UnifiedRow: crate::native::FromNativeBlock + Send {}

#[cfg(all(feature = "native", not(feature = "http")))]
impl<T> UnifiedRow for T where T: crate::native::FromNativeBlock + Send {}

#[cfg(all(feature = "http", feature = "native"))]
pub trait UnifiedRow:
    clickhouse::Row
    + clickhouse::RowOwned
    + clickhouse::RowRead
    + crate::native::FromNativeBlock
    + Send
{
}

#[cfg(all(feature = "http", feature = "native"))]
impl<T> UnifiedRow for T where
    T: clickhouse::Row
        + clickhouse::RowOwned
        + clickhouse::RowRead
        + crate::native::FromNativeBlock
        + Send
{
}

// =============================================================================
// StreamableRow - for stream() methods (requires 'static)
// =============================================================================

/// Trait for row types that can be streamed from the database.
///
/// Similar to `UnifiedRow`, but with the additional `'static` bound required
/// for streaming operations (needed for spawning background tasks).
#[cfg(all(feature = "http", not(feature = "native")))]
pub trait StreamableRow:
    clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send + 'static
{
}

#[cfg(all(feature = "http", not(feature = "native")))]
impl<T> StreamableRow for T where
    T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send + 'static
{
}

#[cfg(all(feature = "native", not(feature = "http")))]
pub trait StreamableRow: crate::native::FromAnyBlock + Send + 'static {}

#[cfg(all(feature = "native", not(feature = "http")))]
impl<T> StreamableRow for T where T: crate::native::FromAnyBlock + Send + 'static {}

#[cfg(all(feature = "http", feature = "native"))]
pub trait StreamableRow:
    clickhouse::Row
    + clickhouse::RowOwned
    + clickhouse::RowRead
    + crate::native::FromAnyBlock
    + Send
    + 'static
{
}

#[cfg(all(feature = "http", feature = "native"))]
impl<T> StreamableRow for T where
    T: clickhouse::Row
        + clickhouse::RowOwned
        + clickhouse::RowRead
        + crate::native::FromAnyBlock
        + Send
        + 'static
{
}

// =============================================================================
// CallbackStreamableRow - for stream_for_each() without 'static bound
// =============================================================================

/// Trait for row types used with callback-based streaming.
///
/// Unlike `StreamableRow`, this trait does not require `'static` because
/// callback-based streaming doesn't spawn background tasks.
#[cfg(all(feature = "http", not(feature = "native")))]
pub trait CallbackStreamableRow:
    clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send
{
}

#[cfg(all(feature = "http", not(feature = "native")))]
impl<T> CallbackStreamableRow for T where
    T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send
{
}

#[cfg(all(feature = "native", not(feature = "http")))]
pub trait CallbackStreamableRow: crate::native::FromAnyBlock + Send {}

#[cfg(all(feature = "native", not(feature = "http")))]
impl<T> CallbackStreamableRow for T where T: crate::native::FromAnyBlock + Send {}

#[cfg(all(feature = "http", feature = "native"))]
pub trait CallbackStreamableRow:
    clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromAnyBlock + Send
{
}

#[cfg(all(feature = "http", feature = "native"))]
impl<T> CallbackStreamableRow for T where
    T: clickhouse::Row
        + clickhouse::RowOwned
        + clickhouse::RowRead
        + crate::native::FromAnyBlock
        + Send
{
}

