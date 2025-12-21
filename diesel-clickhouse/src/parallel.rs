//! Parallel processing for large result sets.
//!
//! This module provides utilities for processing large ClickHouse results
//! in parallel using Rayon's work-stealing thread pool.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::parallel::{ParallelProcessor, ChunkProcessor};
//!
//! // Process rows in parallel
//! let results: Vec<ProcessedRow> = ParallelProcessor::new(rows)
//!     .chunk_size(1000)
//!     .process(|row| {
//!         // CPU-intensive processing
//!         ProcessedRow::from(row)
//!     });
//!
//! // Or use parallel iteration directly
//! use rayon::prelude::*;
//! let sum: i64 = rows.par_iter()
//!     .map(|row| row.value)
//!     .sum();
//! ```

use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Configuration for parallel processing.
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Minimum number of items before using parallel processing.
    /// Below this threshold, sequential processing is used.
    pub threshold: usize,
    /// Chunk size for parallel iteration.
    pub chunk_size: usize,
    /// Maximum number of threads to use (0 = use all available).
    pub max_threads: usize,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            threshold: 1000,      // Don't parallelize small datasets
            chunk_size: 256,      // Good balance for most workloads
            max_threads: 0,       // Use all available threads
        }
    }
}

impl ParallelConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the parallelization threshold.
    pub fn threshold(mut self, threshold: usize) -> Self {
        self.threshold = threshold;
        self
    }

    /// Set the chunk size.
    pub fn chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size;
        self
    }

    /// Set maximum threads.
    pub fn max_threads(mut self, max_threads: usize) -> Self {
        self.max_threads = max_threads;
        self
    }

    /// Check if parallel processing should be used for the given count.
    #[inline]
    pub fn should_parallelize(&self, count: usize) -> bool {
        count >= self.threshold
    }
}

/// A parallel processor for collections.
pub struct ParallelProcessor<T> {
    items: Vec<T>,
    config: ParallelConfig,
}

impl<T: Send + Sync> ParallelProcessor<T> {
    /// Create a new parallel processor.
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            config: ParallelConfig::default(),
        }
    }

    /// Set the chunk size.
    pub fn chunk_size(mut self, chunk_size: usize) -> Self {
        self.config.chunk_size = chunk_size;
        self
    }

    /// Set the parallelization threshold.
    pub fn threshold(mut self, threshold: usize) -> Self {
        self.config.threshold = threshold;
        self
    }

    /// Set custom config.
    pub fn config(mut self, config: ParallelConfig) -> Self {
        self.config = config;
        self
    }

    /// Process items in parallel, returning transformed results.
    pub fn process<F, R>(self, f: F) -> Vec<R>
    where
        F: Fn(&T) -> R + Send + Sync,
        R: Send,
    {
        if self.config.should_parallelize(self.items.len()) {
            self.items.par_iter().map(f).collect()
        } else {
            self.items.iter().map(f).collect()
        }
    }

    /// Process items in parallel with index.
    pub fn process_indexed<F, R>(self, f: F) -> Vec<R>
    where
        F: Fn(usize, &T) -> R + Send + Sync,
        R: Send,
    {
        if self.config.should_parallelize(self.items.len()) {
            self.items.par_iter().enumerate().map(|(i, item)| f(i, item)).collect()
        } else {
            self.items.iter().enumerate().map(|(i, item)| f(i, item)).collect()
        }
    }

    /// Filter items in parallel.
    pub fn filter<F>(self, f: F) -> Vec<T>
    where
        F: Fn(&T) -> bool + Send + Sync,
        T: Clone,
    {
        if self.config.should_parallelize(self.items.len()) {
            self.items.par_iter().filter(|item| f(item)).cloned().collect()
        } else {
            self.items.into_iter().filter(|item| f(item)).collect()
        }
    }

    /// Reduce items in parallel using a combiner function.
    ///
    /// # Arguments
    ///
    /// - `identity`: The identity element for the reduction
    /// - `fold_op`: Function to fold each item into an accumulator
    /// - `combine_op`: Function to combine two accumulators (must be associative)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Sum all values
    /// let sum = processor.reduce(
    ///     0i64,
    ///     |acc, item| acc + item.value,
    ///     |a, b| a + b
    /// );
    /// ```
    pub fn reduce<F, C, R>(self, identity: R, fold_op: F, combine_op: C) -> R
    where
        F: Fn(R, &T) -> R + Send + Sync,
        C: Fn(R, R) -> R + Send + Sync,
        R: Clone + Send + Sync,
    {
        if self.config.should_parallelize(self.items.len()) {
            self.items
                .par_iter()
                .fold(|| identity.clone(), |acc, item| fold_op(acc, item))
                .reduce(|| identity.clone(), |a, b| combine_op(a, b))
        } else {
            self.items.iter().fold(identity, |acc, item| fold_op(acc, item))
        }
    }

    /// Sum numeric values in parallel.
    pub fn sum<F, R>(self, f: F) -> R
    where
        F: Fn(&T) -> R + Send + Sync,
        R: std::iter::Sum + Send + Sync,
    {
        if self.config.should_parallelize(self.items.len()) {
            self.items.par_iter().map(f).sum()
        } else {
            self.items.iter().map(f).sum()
        }
    }

    /// Get the item count.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Consume and return the items.
    pub fn into_inner(self) -> Vec<T> {
        self.items
    }
}

/// Process chunks of data in parallel.
pub struct ChunkProcessor<T> {
    items: Vec<T>,
    chunk_size: usize,
}

impl<T: Send + Sync> ChunkProcessor<T> {
    /// Create a new chunk processor.
    pub fn new(items: Vec<T>, chunk_size: usize) -> Self {
        Self { items, chunk_size }
    }

    /// Process each chunk in parallel.
    pub fn process_chunks<F, R>(self, f: F) -> Vec<R>
    where
        F: Fn(&[T]) -> R + Send + Sync,
        R: Send,
    {
        self.items
            .par_chunks(self.chunk_size)
            .map(f)
            .collect()
    }

    /// Process chunks and flatten results.
    pub fn flat_map_chunks<F, R>(self, f: F) -> Vec<R>
    where
        F: Fn(&[T]) -> Vec<R> + Send + Sync,
        R: Send,
    {
        self.items
            .par_chunks(self.chunk_size)
            .flat_map(f)
            .collect()
    }
}

/// Parallel JSON deserializer for large result sets.
pub struct ParallelJsonParser {
    config: ParallelConfig,
}

impl ParallelJsonParser {
    /// Create a new parallel JSON parser.
    pub fn new() -> Self {
        Self {
            config: ParallelConfig::default(),
        }
    }

    /// Set configuration.
    pub fn config(mut self, config: ParallelConfig) -> Self {
        self.config = config;
        self
    }

    /// Parse JSON lines in parallel.
    pub fn parse_lines<T>(&self, lines: Vec<&str>) -> Vec<Result<T, String>>
    where
        T: serde::de::DeserializeOwned + Send,
    {
        if self.config.should_parallelize(lines.len()) {
            lines
                .par_iter()
                .map(|line| {
                    serde_json::from_str(line)
                        .map_err(|e| format!("JSON parse error: {}", e))
                })
                .collect()
        } else {
            lines
                .iter()
                .map(|line| {
                    serde_json::from_str(line)
                        .map_err(|e| format!("JSON parse error: {}", e))
                })
                .collect()
        }
    }

    /// Parse JSON lines, collecting only successful results.
    pub fn parse_lines_ok<T>(&self, lines: Vec<&str>) -> Vec<T>
    where
        T: serde::de::DeserializeOwned + Send,
    {
        if self.config.should_parallelize(lines.len()) {
            lines
                .par_iter()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        } else {
            lines
                .iter()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        }
    }
}

impl Default for ParallelJsonParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for parallel processing.
#[derive(Debug, Default)]
pub struct ParallelStats {
    /// Total items processed.
    pub total_items: AtomicUsize,
    /// Items processed successfully.
    pub success_count: AtomicUsize,
    /// Items that failed processing.
    pub error_count: AtomicUsize,
}

impl ParallelStats {
    /// Create new stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful item.
    #[inline]
    pub fn record_success(&self) {
        self.total_items.fetch_add(1, Ordering::Relaxed);
        self.success_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed item.
    #[inline]
    pub fn record_error(&self) {
        self.total_items.fetch_add(1, Ordering::Relaxed);
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total items.
    pub fn total(&self) -> usize {
        self.total_items.load(Ordering::Relaxed)
    }

    /// Get success count.
    pub fn successes(&self) -> usize {
        self.success_count.load(Ordering::Relaxed)
    }

    /// Get error count.
    pub fn errors(&self) -> usize {
        self.error_count.load(Ordering::Relaxed)
    }

    /// Get success rate.
    pub fn success_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            1.0
        } else {
            self.successes() as f64 / total as f64
        }
    }

    /// Reset all counters.
    pub fn reset(&self) {
        self.total_items.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
    }
}

/// Extension trait for parallel iteration on Vec.
pub trait ParallelExt<T> {
    /// Create a parallel processor for this collection.
    fn parallel(self) -> ParallelProcessor<T>;

    /// Create a chunk processor for this collection.
    fn chunks_parallel(self, chunk_size: usize) -> ChunkProcessor<T>;
}

impl<T: Send + Sync> ParallelExt<T> for Vec<T> {
    fn parallel(self) -> ParallelProcessor<T> {
        ParallelProcessor::new(self)
    }

    fn chunks_parallel(self, chunk_size: usize) -> ChunkProcessor<T> {
        ChunkProcessor::new(self, chunk_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_processor_map() {
        let items: Vec<i32> = (0..10000).collect();
        let result: Vec<i32> = ParallelProcessor::new(items)
            .threshold(100)
            .process(|&x| x * 2);

        assert_eq!(result.len(), 10000);
        assert_eq!(result[0], 0);
        assert_eq!(result[5000], 10000);
    }

    #[test]
    fn test_parallel_processor_sum() {
        let items: Vec<i64> = (1..=1000).collect();
        let sum: i64 = ParallelProcessor::new(items)
            .threshold(100)
            .sum(|&x| x);

        assert_eq!(sum, 500500); // Sum of 1 to 1000
    }

    #[test]
    fn test_parallel_processor_filter() {
        let items: Vec<i32> = (0..10000).collect();
        let result = ParallelProcessor::new(items)
            .threshold(100)
            .filter(|&x| x % 2 == 0);

        assert_eq!(result.len(), 5000);
        assert!(result.iter().all(|&x| x % 2 == 0));
    }

    #[test]
    fn test_chunk_processor() {
        let items: Vec<i32> = (0..1000).collect();
        let chunk_sums: Vec<i32> = ChunkProcessor::new(items, 100)
            .process_chunks(|chunk| chunk.iter().sum());

        assert_eq!(chunk_sums.len(), 10);
    }

    #[test]
    fn test_parallel_stats() {
        let stats = ParallelStats::new();

        stats.record_success();
        stats.record_success();
        stats.record_error();

        assert_eq!(stats.total(), 3);
        assert_eq!(stats.successes(), 2);
        assert_eq!(stats.errors(), 1);
        assert!((stats.success_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_parallel_config() {
        let config = ParallelConfig::new()
            .threshold(500)
            .chunk_size(128);

        assert!(!config.should_parallelize(100));
        assert!(config.should_parallelize(1000));
    }

    #[test]
    fn test_parallel_ext() {
        let items: Vec<i32> = (0..100).collect();
        let result: Vec<i32> = items.parallel()
            .threshold(10)
            .process(|&x| x + 1);

        assert_eq!(result.len(), 100);
        assert_eq!(result[0], 1);
    }
}
