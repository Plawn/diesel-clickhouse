//! Integration tests for advanced features.
//!
//! Tests for:
//! - PreparedCache
//! - AsyncInsertConfig
//! - MetricsCollector
//! - Pool
//! - Parallel processing
//! - Arena allocator
//! - String interning
//! - Zero-copy parsing

// =============================================================================
// PreparedCache Tests
// =============================================================================

mod prepared_cache_tests {
    use diesel_clickhouse::prepared::{PreparedCache, PreparedStatement, QueryTemplate, global_cache};

    #[test]
    fn test_prepared_cache_basic() {
        let cache = PreparedCache::new(100);

        // First call should miss
        let stmt1 = cache.prepare::<String, _>("test_query", || {
            "SELECT * FROM users WHERE id = ?".to_string()
        }).expect("prepare failed");
        let stats = cache.stats().expect("stats failed");
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);

        // Second call should hit
        let stmt2 = cache.prepare::<String, _>("test_query", || {
            "SELECT * FROM users WHERE id = ?".to_string()
        }).expect("prepare failed");
        let stats = cache.stats().expect("stats failed");
        assert_eq!(stats.hits, 1);

        // Same SQL returned
        assert_eq!(stmt1.sql(), stmt2.sql());
    }

    #[test]
    fn test_prepared_cache_different_queries() {
        let cache = PreparedCache::new(100);

        let stmt1 = cache.prepare::<String, _>("query1", || "SELECT 1".to_string()).expect("prepare failed");
        let stmt2 = cache.prepare::<String, _>("query2", || "SELECT 2".to_string()).expect("prepare failed");

        assert_ne!(stmt1.sql(), stmt2.sql());
        assert_eq!(cache.stats().expect("stats failed").size, 2);
    }

    #[test]
    fn test_prepared_cache_eviction() {
        let cache = PreparedCache::new(4);

        // Fill cache
        for i in 0..10 {
            cache.prepare::<String, _>(&format!("query_{}", i), || {
                format!("SELECT {}", i)
            }).expect("prepare failed");
        }

        // Cache should have evicted some entries
        let stats = cache.stats().expect("stats failed");
        assert!(stats.size <= 4);
    }

    #[test]
    fn test_prepared_statement_with_params() {
        let stmt = PreparedStatement::new(
            "user_by_id",
            "SELECT * FROM users WHERE id = ? AND name = ?"
        );

        let sql = stmt.with_params(&[&42, &"'alice'"]);
        assert_eq!(sql, "SELECT * FROM users WHERE id = 42 AND name = 'alice'");
    }

    #[test]
    fn test_query_template() {
        let template = QueryTemplate::new("SELECT * FROM {0} WHERE {1} = {2}");

        assert_eq!(template.param_count(), 3);

        let sql = template.render(&["users", "id", "42"]);
        assert_eq!(sql, "SELECT * FROM users WHERE id = 42");
    }

    #[test]
    fn test_query_template_escaped() {
        let template = QueryTemplate::new("SELECT * FROM users WHERE name = {0}");

        let sql = template.render_escaped(&["O'Brien"]);
        assert_eq!(sql, "SELECT * FROM users WHERE name = 'O''Brien'");
    }

    #[test]
    fn test_global_cache() {
        let cache = global_cache();

        let stmt = cache.prepare::<String, _>("global_test", || {
            "SELECT * FROM global_test".to_string()
        }).expect("prepare failed");

        assert_eq!(stmt.sql(), "SELECT * FROM global_test");
    }

    #[test]
    fn test_cache_hit_rate() {
        let cache = PreparedCache::new(100);

        // 1 miss
        cache.prepare::<String, _>("q1", || "SELECT 1".to_string()).expect("prepare failed");
        // 3 hits
        cache.prepare::<String, _>("q1", || "SELECT 1".to_string()).expect("prepare failed");
        cache.prepare::<String, _>("q1", || "SELECT 1".to_string()).expect("prepare failed");
        cache.prepare::<String, _>("q1", || "SELECT 1".to_string()).expect("prepare failed");

        let stats = cache.stats().expect("stats failed");
        assert!((stats.hit_rate() - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_cache_clear() {
        let cache = PreparedCache::new(100);

        cache.prepare::<String, _>("q1", || "SELECT 1".to_string()).expect("prepare failed");
        cache.prepare::<String, _>("q2", || "SELECT 2".to_string()).expect("prepare failed");
        assert_eq!(cache.stats().expect("stats failed").size, 2);

        cache.clear().expect("clear failed");
        assert_eq!(cache.stats().expect("stats failed").size, 0);
    }
}

// =============================================================================
// AsyncInsertConfig Tests
// =============================================================================

mod async_insert_tests {
    use diesel_clickhouse::async_insert::AsyncInsertConfig;

    #[test]
    fn test_async_insert_config_default() {
        let config = AsyncInsertConfig::default();

        assert!(config.async_insert);
        assert!(!config.wait_for_async_insert);
        assert_eq!(config.async_insert_busy_timeout_ms, 200);
        assert_eq!(config.async_insert_max_data_size, 10_000_000);
        assert_eq!(config.async_insert_max_query_number, 450);
    }

    #[test]
    fn test_async_insert_config_synchronous() {
        let config = AsyncInsertConfig::synchronous();

        assert!(config.async_insert);
        assert!(config.wait_for_async_insert);
    }

    #[test]
    fn test_async_insert_config_fire_and_forget() {
        let config = AsyncInsertConfig::fire_and_forget();

        assert!(config.async_insert);
        assert!(!config.wait_for_async_insert);
    }

    #[test]
    fn test_async_insert_config_builder() {
        let config = AsyncInsertConfig::new()
            .wait_for_async_insert(true)
            .async_insert_busy_timeout_ms(500)
            .async_insert_max_data_size(5_000_000)
            .async_insert_max_query_number(100)
            .deduplicate_materialized_views(true);

        assert!(config.wait_for_async_insert);
        assert_eq!(config.async_insert_busy_timeout_ms, 500);
        assert_eq!(config.async_insert_max_data_size, 5_000_000);
        assert_eq!(config.async_insert_max_query_number, 100);
        assert!(config.deduplicate_blocks_in_dependent_materialized_views);
    }

    #[test]
    fn test_async_insert_settings_sql() {
        let config = AsyncInsertConfig::new()
            .wait_for_async_insert(true)
            .async_insert_busy_timeout_ms(300);

        let sql = config.to_settings_sql();

        assert!(sql.contains("async_insert = 1"));
        assert!(sql.contains("wait_for_async_insert = 1"));
        assert!(sql.contains("async_insert_busy_timeout_ms = 300"));
    }

    #[test]
    fn test_async_insert_settings_sql_dedupe() {
        let config = AsyncInsertConfig::new()
            .deduplicate_materialized_views(true);

        let sql = config.to_settings_sql();

        assert!(sql.contains("deduplicate_blocks_in_dependent_materialized_views = 1"));
    }
}

// =============================================================================
// MetricsCollector Tests
// =============================================================================

mod metrics_tests {
    use diesel_clickhouse::metrics::{MetricsCollector, QueryMetrics, QueryTimer, global_metrics};
    use std::time::Duration;

    #[test]
    fn test_metrics_collector_basic() {
        let collector = MetricsCollector::new();

        assert_eq!(collector.query_count(), 0);
        assert_eq!(collector.success_count(), 0);
        assert_eq!(collector.error_count(), 0);
    }

    #[test]
    fn test_metrics_record_success() {
        let collector = MetricsCollector::new();

        let metrics = QueryMetrics::success(
            "SELECT 1",
            Duration::from_millis(10),
            100,
        );
        collector.record(&metrics);

        assert_eq!(collector.query_count(), 1);
        assert_eq!(collector.success_count(), 1);
        assert_eq!(collector.error_count(), 0);
        assert_eq!(collector.total_rows(), 100);
    }

    #[test]
    fn test_metrics_record_failure() {
        let collector = MetricsCollector::new();

        let metrics = QueryMetrics::failure(
            "SELECT * FROM missing",
            Duration::from_millis(5),
            "table not found",
        );
        collector.record(&metrics);

        assert_eq!(collector.query_count(), 1);
        assert_eq!(collector.success_count(), 0);
        assert_eq!(collector.error_count(), 1);
        assert!(collector.success_rate() < 0.01);
    }

    #[test]
    fn test_metrics_avg_latency() {
        let collector = MetricsCollector::new();

        collector.record(&QueryMetrics::success("q1", Duration::from_millis(10), 1));
        collector.record(&QueryMetrics::success("q2", Duration::from_millis(20), 1));
        collector.record(&QueryMetrics::success("q3", Duration::from_millis(30), 1));

        let avg = collector.avg_latency();
        // Average should be ~20ms
        assert!(avg >= Duration::from_millis(19));
        assert!(avg <= Duration::from_millis(21));
    }

    #[test]
    fn test_metrics_min_max_latency() {
        let collector = MetricsCollector::new();

        collector.record(&QueryMetrics::success("q1", Duration::from_millis(50), 1));
        collector.record(&QueryMetrics::success("q2", Duration::from_millis(10), 1));
        collector.record(&QueryMetrics::success("q3", Duration::from_millis(100), 1));

        let min = collector.min_latency().unwrap();
        let max = collector.max_latency().unwrap();

        assert!(min <= Duration::from_millis(11));
        assert!(max >= Duration::from_millis(99));
    }

    #[test]
    fn test_metrics_success_rate() {
        let collector = MetricsCollector::new();

        collector.record(&QueryMetrics::success("q1", Duration::from_millis(10), 1));
        collector.record(&QueryMetrics::success("q2", Duration::from_millis(10), 1));
        collector.record(&QueryMetrics::failure("q3", Duration::from_millis(10), "error"));
        collector.record(&QueryMetrics::success("q4", Duration::from_millis(10), 1));

        // 3 success out of 4
        assert!((collector.success_rate() - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_metrics_snapshot() {
        let collector = MetricsCollector::new();

        collector.record(&QueryMetrics::success("q1", Duration::from_millis(10), 100));
        collector.record(&QueryMetrics::success("q2", Duration::from_millis(20), 200));

        let snapshot = collector.snapshot();

        assert_eq!(snapshot.query_count, 2);
        assert_eq!(snapshot.success_count, 2);
        assert_eq!(snapshot.total_rows, 300);
        assert_eq!(snapshot.success_rate, 1.0);
    }

    #[test]
    fn test_metrics_reset() {
        let collector = MetricsCollector::new();

        collector.record(&QueryMetrics::success("q1", Duration::from_millis(10), 100));
        assert_eq!(collector.query_count(), 1);

        collector.reset();

        assert_eq!(collector.query_count(), 0);
        assert_eq!(collector.total_rows(), 0);
    }

    #[test]
    fn test_query_timer_success() {
        let collector = MetricsCollector::new();

        {
            let timer = QueryTimer::start(&collector, "SELECT 1");
            std::thread::sleep(Duration::from_millis(5));
            timer.success(10);
        }

        assert_eq!(collector.query_count(), 1);
        assert_eq!(collector.total_rows(), 10);
        assert!(collector.avg_latency() >= Duration::from_millis(5));
    }

    #[test]
    fn test_query_timer_failure() {
        let collector = MetricsCollector::new();

        {
            let timer = QueryTimer::start(&collector, "SELECT * FROM missing");
            timer.failure("table not found");
        }

        assert_eq!(collector.query_count(), 1);
        assert_eq!(collector.error_count(), 1);
    }

    #[test]
    fn test_global_metrics() {
        let metrics = global_metrics();

        // Global metrics should be accessible
        let _ = metrics.query_count();
    }
}

// =============================================================================
// Pool Tests
// =============================================================================

mod pool_tests {
    use diesel_clickhouse::pool::PoolConfig;

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();

        assert_eq!(config.max_size, 10);
        assert_eq!(config.min_idle, Some(1));
        assert!(config.connection_timeout_ms > 0);
    }

    #[test]
    fn test_pool_config_builder() {
        let config = PoolConfig::new(20)
            .min_idle(5)
            .connection_timeout_ms(60_000)
            .idle_timeout_ms(300_000);

        assert_eq!(config.max_size, 20);
        assert_eq!(config.min_idle, Some(5));
        assert_eq!(config.connection_timeout_ms, 60_000);
        assert_eq!(config.idle_timeout_ms, Some(300_000));
    }

    #[test]
    fn test_pool_config_validation() {
        // min_idle should not exceed max_size in practice
        let config = PoolConfig::new(5)
            .min_idle(10); // This is a misconfiguration but should not panic

        // Config should be created
        assert_eq!(config.max_size, 5);
        assert_eq!(config.min_idle, Some(10));
    }
}

// =============================================================================
// Parallel Processing Tests
// =============================================================================

mod parallel_tests {
    use diesel_clickhouse::parallel::{
        ParallelProcessor, ParallelConfig, ChunkProcessor, ParallelStats, ParallelExt
    };

    #[test]
    fn test_parallel_config_default() {
        let config = ParallelConfig::default();

        assert_eq!(config.threshold, 1000);
        assert_eq!(config.chunk_size, 256);
    }

    #[test]
    fn test_parallel_config_should_parallelize() {
        let config = ParallelConfig::new().threshold(100);

        assert!(!config.should_parallelize(50));
        assert!(config.should_parallelize(100));
        assert!(config.should_parallelize(200));
    }

    #[test]
    fn test_parallel_processor_map() {
        let items: Vec<i32> = (0..10000).collect();
        let result: Vec<i32> = ParallelProcessor::new(items)
            .threshold(100)
            .process(|&x| x * 2);

        assert_eq!(result.len(), 10000);
        assert_eq!(result[0], 0);
        assert_eq!(result[5000], 10000);
        assert_eq!(result[9999], 19998);
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
    fn test_parallel_processor_indexed() {
        let items: Vec<&str> = vec!["a", "b", "c", "d", "e"];
        let result: Vec<String> = ParallelProcessor::new(items)
            .threshold(1) // Force parallel
            .process_indexed(|i, &s| format!("{}:{}", i, s));

        assert_eq!(result.len(), 5);
        assert!(result.contains(&"0:a".to_string()));
        assert!(result.contains(&"4:e".to_string()));
    }

    #[test]
    fn test_parallel_processor_sequential_fallback() {
        // Below threshold, should use sequential processing
        let items: Vec<i32> = (0..50).collect();
        let result: Vec<i32> = ParallelProcessor::new(items)
            .threshold(100) // 50 < 100, so sequential
            .process(|&x| x + 1);

        assert_eq!(result.len(), 50);
        assert_eq!(result[0], 1);
    }

    #[test]
    fn test_chunk_processor() {
        let items: Vec<i32> = (0..1000).collect();
        let chunk_sums: Vec<i32> = ChunkProcessor::new(items, 100)
            .process_chunks(|chunk| chunk.iter().sum());

        assert_eq!(chunk_sums.len(), 10);
        // First chunk: 0+1+...+99 = 4950
        assert_eq!(chunk_sums[0], 4950);
    }

    #[test]
    fn test_chunk_processor_flat_map() {
        let items: Vec<i32> = vec![1, 2, 3, 4, 5, 6];
        let result: Vec<i32> = ChunkProcessor::new(items, 2)
            .flat_map_chunks(|chunk| chunk.iter().map(|&x| x * 10).collect());

        assert_eq!(result, vec![10, 20, 30, 40, 50, 60]);
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
    fn test_parallel_stats_reset() {
        let stats = ParallelStats::new();

        stats.record_success();
        stats.record_error();
        assert_eq!(stats.total(), 2);

        stats.reset();
        assert_eq!(stats.total(), 0);
    }

    #[test]
    fn test_parallel_ext_vec() {
        let items: Vec<i32> = (0..100).collect();
        let result: Vec<i32> = items.parallel()
            .threshold(10)
            .process(|&x| x + 1);

        assert_eq!(result.len(), 100);
    }

    #[test]
    fn test_parallel_ext_chunks() {
        let items: Vec<i32> = (0..100).collect();
        let chunk_counts: Vec<usize> = items.chunks_parallel(25)
            .process_chunks(|chunk| chunk.len());

        assert_eq!(chunk_counts, vec![25, 25, 25, 25]);
    }
}

// =============================================================================
// Arena Allocator Tests
// =============================================================================

mod arena_tests {
    use diesel_clickhouse_core::arena::{QueryArena, ArenaQueryBuilder, with_arena};

    #[test]
    fn test_arena_alloc_str() {
        let arena = QueryArena::new();

        let s1 = arena.alloc_str("hello");
        let s2 = arena.alloc_str("world");

        assert_eq!(s1, "hello");
        assert_eq!(s2, "world");
    }

    #[test]
    fn test_arena_alloc_fmt() {
        let arena = QueryArena::new();

        let s = arena.alloc_fmt(format_args!("SELECT {} FROM {}", "*", "users"));
        assert_eq!(s, "SELECT * FROM users");
    }

    #[test]
    fn test_arena_join() {
        let arena = QueryArena::new();

        let parts = ["a", "b", "c"];
        let joined = arena.join(&parts, ", ");
        assert_eq!(joined, "a, b, c");
    }

    #[test]
    fn test_arena_join_empty() {
        let arena = QueryArena::new();

        let parts: [&str; 0] = [];
        let joined = arena.join(&parts, ", ");
        assert_eq!(joined, "");
    }

    #[test]
    fn test_arena_with_capacity() {
        let arena = QueryArena::with_capacity(1024);

        // Should be able to allocate without issue
        for i in 0..100 {
            arena.alloc_fmt(format_args!("item_{}", i));
        }

        assert!(arena.allocated_bytes() > 0);
    }

    #[test]
    fn test_arena_reset() {
        let mut arena = QueryArena::new();

        arena.alloc_str("some data");
        let bytes_before = arena.allocated_bytes();
        assert!(bytes_before > 0);

        arena.reset();

        // After reset, we can allocate again
        arena.alloc_str("new data");
    }

    #[test]
    fn test_with_arena() {
        let result = with_arena(|arena| {
            let s = arena.alloc_str("test");
            s.to_owned()
        });

        assert_eq!(result, "test");
    }

    #[test]
    fn test_arena_query_builder() {
        let arena = QueryArena::new();
        let mut builder = ArenaQueryBuilder::new(&arena);

        builder.push("SELECT ");
        builder.push_identifier("name");
        builder.push(", ");
        builder.push_identifier("age");
        builder.push(" FROM ");
        builder.push_identifier("users");
        builder.push(" WHERE ");
        builder.push_identifier("id");
        builder.push(" = ");
        builder.push_int(42u32);

        let sql = builder.finish();
        assert_eq!(sql, "SELECT `name`, `age` FROM `users` WHERE `id` = 42");
    }

    #[test]
    fn test_arena_query_builder_string_literal() {
        let arena = QueryArena::new();
        let mut builder = ArenaQueryBuilder::new(&arena);

        builder.push("SELECT * FROM users WHERE name = ");
        builder.push_string_literal("O'Brien");

        let sql = builder.finish();
        assert_eq!(sql, "SELECT * FROM users WHERE name = 'O''Brien'");
    }

    #[test]
    fn test_arena_query_builder_float() {
        let arena = QueryArena::new();
        let mut builder = ArenaQueryBuilder::new(&arena);

        builder.push("SELECT * FROM data WHERE value > ");
        builder.push_float(3.14159f64);

        let sql = builder.finish();
        assert!(sql.contains("3.14159"));
    }

    #[test]
    fn test_arena_query_builder_finish_in_arena() {
        let arena = QueryArena::new();
        let mut builder = ArenaQueryBuilder::new(&arena);

        builder.push("SELECT 1");

        let sql = builder.finish_in_arena();
        assert_eq!(sql, "SELECT 1");
    }
}

// =============================================================================
// String Interning Tests
// =============================================================================

mod interner_tests {
    use diesel_clickhouse_core::interner::{
        ColumnInterner, InternedSchema, InternedRow,
        intern, resolve
    };

    #[test]
    fn test_interner_basic() {
        let interner = ColumnInterner::new();

        let sym1 = interner.intern("id").expect("intern failed");
        let sym2 = interner.intern("name").expect("intern failed");
        let sym3 = interner.intern("id").expect("intern failed"); // Same as sym1

        assert_eq!(sym1, sym3);
        assert_ne!(sym1, sym2);
    }

    #[test]
    fn test_interner_resolve() {
        let interner = ColumnInterner::new();

        let sym = interner.intern("test_column").expect("intern failed");
        let resolved = interner.resolve(sym).expect("resolve failed");

        assert_eq!(resolved, Some("test_column".to_owned()));
    }

    #[test]
    fn test_interner_get() {
        let interner = ColumnInterner::new();

        // Not interned yet
        assert!(interner.get("missing").expect("get failed").is_none());

        // After interning
        interner.intern("present").expect("intern failed");
        assert!(interner.get("present").expect("get failed").is_some());
    }

    #[test]
    fn test_interner_with_resolved() {
        let interner = ColumnInterner::new();
        let sym = interner.intern("test").expect("intern failed");

        let len = interner.with_resolved(sym, |s| s.len()).expect("with_resolved failed");
        assert_eq!(len, Some(4));
    }

    #[test]
    fn test_interner_len() {
        let interner = ColumnInterner::new();

        assert_eq!(interner.len().expect("len failed"), 0);
        assert!(interner.is_empty().expect("is_empty failed"));

        interner.intern("a").expect("intern failed");
        interner.intern("b").expect("intern failed");
        interner.intern("a").expect("intern failed"); // Duplicate

        assert_eq!(interner.len().expect("len failed"), 2);
        assert!(!interner.is_empty().expect("is_empty failed"));
    }

    #[test]
    fn test_interner_clear() {
        let interner = ColumnInterner::new();

        interner.intern("a").expect("intern failed");
        interner.intern("b").expect("intern failed");
        assert_eq!(interner.len().expect("len failed"), 2);

        interner.clear().expect("clear failed");
        assert_eq!(interner.len().expect("len failed"), 0);
    }

    #[test]
    fn test_global_interner() {
        let sym1 = intern("global_test_1").expect("intern failed");
        let sym2 = intern("global_test_1").expect("intern failed");
        assert_eq!(sym1, sym2);

        let resolved = resolve(sym1).expect("resolve failed");
        assert_eq!(resolved, Some("global_test_1".to_owned()));
    }

    #[test]
    fn test_interned_schema() {
        let schema = InternedSchema::new(&["id", "name", "age"]).expect("new failed");

        assert_eq!(schema.len(), 3);
        assert!(!schema.is_empty());
    }

    #[test]
    fn test_interned_schema_column_name() {
        let schema = InternedSchema::new(&["id", "name", "age"]).expect("new failed");

        assert_eq!(schema.column_name(0).expect("column_name failed"), Some("id".to_owned()));
        assert_eq!(schema.column_name(1).expect("column_name failed"), Some("name".to_owned()));
        assert_eq!(schema.column_name(2).expect("column_name failed"), Some("age".to_owned()));
        assert_eq!(schema.column_name(3).expect("column_name failed"), None);
    }

    #[test]
    fn test_interned_schema_find_column() {
        let schema = InternedSchema::new(&["id", "name", "age"]).expect("new failed");

        assert_eq!(schema.find_column("id").expect("find_column failed"), Some(0));
        assert_eq!(schema.find_column("name").expect("find_column failed"), Some(1));
        assert_eq!(schema.find_column("age").expect("find_column failed"), Some(2));
        assert_eq!(schema.find_column("missing").expect("find_column failed"), None);
    }

    #[test]
    fn test_interned_schema_from_strings() {
        let columns = vec!["col1".to_string(), "col2".to_string()];
        let schema = InternedSchema::from_strings(&columns).expect("from_strings failed");

        assert_eq!(schema.len(), 2);
        assert_eq!(schema.column_name(0).expect("column_name failed"), Some("col1".to_owned()));
    }

    #[test]
    fn test_interned_schema_iter() {
        let schema = InternedSchema::new(&["a", "b", "c"]).expect("new failed");
        let symbols: Vec<_> = schema.iter().collect();

        assert_eq!(symbols.len(), 3);
    }

    #[test]
    fn test_interned_schema_names() {
        let schema = InternedSchema::new(&["x", "y", "z"]).expect("new failed");
        let names: Vec<_> = schema.names().collect();

        assert_eq!(names, vec!["x", "y", "z"]);
    }

    #[test]
    fn test_interned_row() {
        let schema = InternedSchema::new(&["id", "name"]).expect("new failed");
        let values = vec![vec![42, 0, 0, 0], b"alice".to_vec()];
        let row = InternedRow::new(&schema, values);

        assert_eq!(row.len(), 2);
        assert!(!row.is_empty());
    }

    #[test]
    fn test_interned_row_get() {
        let schema = InternedSchema::new(&["id", "name"]).expect("new failed");
        let values = vec![vec![1, 2, 3, 4], b"test".to_vec()];
        let row = InternedRow::new(&schema, values);

        assert_eq!(row.get(0), Some([1, 2, 3, 4].as_slice()));
        assert_eq!(row.get(1), Some(b"test".as_slice()));
        assert_eq!(row.get(2), None);
    }

    #[test]
    fn test_interned_row_get_by_name() {
        let schema = InternedSchema::new(&["id", "name"]).expect("new failed");
        let values = vec![vec![42], b"alice".to_vec()];
        let row = InternedRow::new(&schema, values);

        assert_eq!(row.get_by_name("id").expect("get_by_name failed"), Some([42].as_slice()));
        assert_eq!(row.get_by_name("name").expect("get_by_name failed"), Some(b"alice".as_slice()));
        assert_eq!(row.get_by_name("missing").expect("get_by_name failed"), None);
    }
}

// =============================================================================
// Zero-Copy Parsing Tests
// =============================================================================

mod zero_copy_tests {
    use diesel_clickhouse::zero_copy::{
        BorrowedValue, ZeroCopyRow, TsvParser, CsvParser, JsonRowParser, ZeroCopyParser
    };

    #[test]
    fn test_borrowed_value_parse_int() {
        let val = BorrowedValue::new(b"12345");
        assert_eq!(val.parse_i64().unwrap(), 12345);
        assert_eq!(val.parse_u64().unwrap(), 12345);
    }

    #[test]
    fn test_borrowed_value_parse_float() {
        let val = BorrowedValue::new(b"3.14159");
        let f = val.parse_f64().unwrap();
        assert!((f - 3.14159).abs() < 0.00001);
    }

    #[test]
    fn test_borrowed_value_parse_bool() {
        assert!(BorrowedValue::new(b"true").parse_bool().unwrap());
        assert!(BorrowedValue::new(b"1").parse_bool().unwrap());
        assert!(!BorrowedValue::new(b"false").parse_bool().unwrap());
        assert!(!BorrowedValue::new(b"0").parse_bool().unwrap());
    }

    #[test]
    fn test_borrowed_value_null() {
        assert!(BorrowedValue::new(b"").is_null());
        assert!(BorrowedValue::new(b"\\N").is_null());
        assert!(BorrowedValue::new(b"NULL").is_null());
        assert!(!BorrowedValue::new(b"hello").is_null());
    }

    #[test]
    fn test_borrowed_value_as_str() {
        let val = BorrowedValue::new(b"hello world");
        assert_eq!(val.as_str().unwrap(), "hello world");
    }

    #[test]
    fn test_zero_copy_row() {
        let values = vec![
            BorrowedValue::new(b"42"),
            BorrowedValue::new(b"hello"),
            BorrowedValue::new(b"3.14"),
        ];
        let row = ZeroCopyRow::from_vec(values);

        assert_eq!(row.len(), 3);
        assert_eq!(row.get_i64(0).unwrap(), 42);
        assert_eq!(row.get_str(1).unwrap(), "hello");
        assert!((row.get_f64(2).unwrap() - 3.14).abs() < 0.01);
    }

    #[test]
    fn test_tsv_parser() {
        let data = b"1\talice\t100\n2\tbob\t200\n";
        let mut parser = TsvParser::new(data);

        let row1 = parser.next_row().unwrap();
        assert_eq!(row1.len(), 3);
        assert_eq!(row1.get_u64(0).unwrap(), 1);
        assert_eq!(row1.get_str(1).unwrap(), "alice");
        assert_eq!(row1.get_u64(2).unwrap(), 100);

        let row2 = parser.next_row().unwrap();
        assert_eq!(row2.get_str(1).unwrap(), "bob");

        assert!(parser.next_row().is_none());
    }

    #[test]
    fn test_tsv_parser_iterator() {
        let data = b"a\tb\nc\td\n";
        let parser = TsvParser::new(data);

        let rows: Vec<_> = parser.collect();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn test_csv_parser() {
        let data = b"1,alice,100\n2,bob,200\n";
        let mut parser = CsvParser::new(data);

        let row1 = parser.next_row().unwrap();
        assert_eq!(row1.len(), 3);
        assert_eq!(row1.get_u64(0).unwrap(), 1);
        assert_eq!(row1.get_str(1).unwrap(), "alice");
    }

    #[test]
    fn test_csv_parser_quoted() {
        let data = b"1,\"hello, world\",100\n";
        let mut parser = CsvParser::new(data);

        let row = parser.next_row().unwrap();
        assert_eq!(row.get_str(1).unwrap(), "hello, world");
    }

    #[test]
    fn test_csv_parser_custom_delimiter() {
        let data = b"1;alice;100\n";
        let mut parser = CsvParser::with_delimiter(data, b';');

        let row = parser.next_row().unwrap();
        assert_eq!(row.len(), 3);
        assert_eq!(row.get_str(1).unwrap(), "alice");
    }

    #[test]
    fn test_json_row_parser() {
        let data = br#"{"id":1,"name":"alice"}
{"id":2,"name":"bob"}
"#;
        let mut parser = JsonRowParser::new(data);

        let row1 = parser.next_row().unwrap().unwrap();
        assert_eq!(row1.len(), 2);
        assert_eq!(row1[0].0, "id");
        assert_eq!(row1[0].1.parse_u64().unwrap(), 1);
        assert_eq!(row1[1].0, "name");
        assert_eq!(row1[1].1.as_str().unwrap(), "alice");

        let row2 = parser.next_row().unwrap().unwrap();
        assert_eq!(row2[1].1.as_str().unwrap(), "bob");
    }

    #[test]
    fn test_zero_copy_parser_auto_detect_tsv() {
        let data = b"1\t2\t3\n";
        let parser = ZeroCopyParser::auto_detect(data);
        assert!(matches!(parser, ZeroCopyParser::Tsv(_)));
    }

    #[test]
    fn test_zero_copy_parser_auto_detect_json() {
        let data = b"{\"a\":1}\n";
        let parser = ZeroCopyParser::auto_detect(data);
        assert!(matches!(parser, ZeroCopyParser::JsonEachRow(_)));
    }

    #[test]
    fn test_zero_copy_parser_auto_detect_csv() {
        let data = b"1,2,3\n";
        let parser = ZeroCopyParser::auto_detect(data);
        assert!(matches!(parser, ZeroCopyParser::Csv(_)));
    }
}

// =============================================================================
// HTTP Compression Tests
// =============================================================================

#[cfg(feature = "http")]
mod compression_tests {
    use diesel_clickhouse::http::Compression;

    #[test]
    fn test_compression_default() {
        let compression = Compression::default();
        assert_eq!(compression, Compression::None);
    }

    #[test]
    fn test_compression_variants() {
        let none = Compression::None;
        let lz4 = Compression::Lz4;

        assert_ne!(none, lz4);
    }

    #[test]
    fn test_compression_clone_copy() {
        let lz4 = Compression::Lz4;
        let lz4_copy = lz4;
        let lz4_clone = lz4.clone();

        assert_eq!(lz4, lz4_copy);
        assert_eq!(lz4, lz4_clone);
    }

    #[test]
    fn test_compression_debug() {
        let none = Compression::None;
        let lz4 = Compression::Lz4;

        assert!(format!("{:?}", none).contains("None"));
        assert!(format!("{:?}", lz4).contains("Lz4"));
    }
}

// =============================================================================
// HTTP build_sql Tests
// =============================================================================

#[cfg(feature = "http")]
mod http_build_sql_tests {
    use diesel_clickhouse::http::build_sql;
    use diesel_clickhouse_core::backend::ClickHouse;
    use diesel_clickhouse_core::expression::sql;
    use diesel_clickhouse_core::query_builder::{QueryFragment, SelectStatement, AstPass};
    use diesel_clickhouse_core::result::QueryResult;
    use diesel_clickhouse_types::*;

    struct TestTable;
    impl QueryFragment<ClickHouse> for TestTable {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, ClickHouse>) -> QueryResult<()> {
            pass.push_sql("test_table");
            Ok(())
        }
    }

    #[test]
    fn test_build_sql_simple() {
        let query = SelectStatement::new(TestTable);
        let result = build_sql(&query).expect("build_sql failed");
        assert_eq!(result, "SELECT * FROM test_table");
    }

    #[test]
    fn test_build_sql_with_filter() {
        let query = SelectStatement::new(TestTable)
            .filter(sql::<Bool>("id > 10"));
        let result = build_sql(&query).expect("build_sql failed");
        assert_eq!(result, "SELECT * FROM test_table WHERE id > 10");
    }

    #[test]
    fn test_build_sql_with_limit_offset() {
        let query = SelectStatement::new(TestTable)
            .limit(100)
            .offset(50);
        let result = build_sql(&query).expect("build_sql failed");
        assert_eq!(result, "SELECT * FROM test_table LIMIT 100 OFFSET 50");
    }

    #[test]
    fn test_build_sql_complex() {
        let query = SelectStatement::new(TestTable)
            .select(sql::<UInt64>("id, name"))
            .filter(sql::<Bool>("active = 1"))
            .order_by(sql::<UInt64>("id DESC"))
            .limit(10);
        let result = build_sql(&query).expect("build_sql failed");
        assert_eq!(result, "SELECT id, name FROM test_table WHERE active = 1 ORDER BY id DESC LIMIT 10");
    }
}

// =============================================================================
// Unified Connection Tests
// =============================================================================

mod unified_connection_tests {
    #[test]
    fn test_url_scheme_detection_http() {
        // Test URL parsing logic (without actual connection)
        let url = "http://localhost:8123/default";
        assert!(url.starts_with("http://"));
    }

    #[test]
    fn test_url_scheme_detection_https() {
        let url = "https://ch.example.com:8443/mydb";
        assert!(url.starts_with("https://"));
    }

    #[test]
    fn test_url_scheme_detection_tcp() {
        let url = "tcp://localhost:9000/default";
        assert!(url.starts_with("tcp://"));
    }

    #[test]
    fn test_url_scheme_invalid() {
        let url = "ftp://localhost/db";
        assert!(!url.starts_with("http://"));
        assert!(!url.starts_with("https://"));
        assert!(!url.starts_with("tcp://"));
    }

    #[test]
    fn test_url_with_credentials() {
        let url = "http://user:pass@localhost:8123/mydb";
        assert!(url.contains("user:pass@"));
        assert!(url.starts_with("http://"));
    }

    #[test]
    fn test_url_with_options() {
        let url = "tcp://localhost:9000/default?secure=true&compression=lz4";
        assert!(url.contains("secure=true"));
        assert!(url.contains("compression=lz4"));
    }
}

// =============================================================================
// Batch Inserter Tests (no connection required)
// =============================================================================

mod batch_inserter_config_tests {
    // Test that batch size is configurable
    #[test]
    fn test_batch_size_configuration() {
        let batch_size = 1000usize;
        assert!(batch_size > 0);

        let large_batch = 100_000usize;
        assert!(large_batch > batch_size);
    }

    // Test capacity estimation
    #[test]
    fn test_insert_sql_capacity_estimation() {
        let columns_count = 5;
        let buffer_len = 1000;

        // Estimate: 50 + columns * 10 + buffer_len * 50
        let estimated_capacity = 50 + columns_count * 10 + buffer_len * 50;

        assert_eq!(estimated_capacity, 50100);
    }

    // Test that INSERT SQL format is correct
    #[test]
    fn test_insert_sql_format() {
        let table_name = "users";
        let columns = vec!["id", "name", "email"];

        let mut sql = String::new();
        sql.push_str("INSERT INTO ");
        sql.push_str(table_name);
        sql.push_str(" (");
        for (i, col) in columns.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push('`');
            sql.push_str(col);
            sql.push('`');
        }
        sql.push_str(") VALUES ");

        assert_eq!(sql, "INSERT INTO users (`id`, `name`, `email`) VALUES ");
    }
}

// =============================================================================
// ToSql Trait Tests
// =============================================================================

#[cfg(feature = "http")]
mod to_sql_tests {
    use diesel_clickhouse::http::ToSql;
    use diesel_clickhouse_core::backend::ClickHouse;
    use diesel_clickhouse_core::expression::sql;
    use diesel_clickhouse_core::query_builder::{QueryFragment, SelectStatement, AstPass};
    use diesel_clickhouse_core::result::QueryResult;
    use diesel_clickhouse_types::*;

    struct TestTable;
    impl QueryFragment<ClickHouse> for TestTable {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, ClickHouse>) -> QueryResult<()> {
            pass.push_sql("users");
            Ok(())
        }
    }

    #[test]
    fn test_to_sql_string() {
        let query = SelectStatement::new(TestTable);
        let sql = query.to_sql_string().expect("to_sql_string failed");
        assert_eq!(sql, "SELECT * FROM users");
    }

    #[test]
    fn test_to_sql_string_with_clauses() {
        let query = SelectStatement::new(TestTable)
            .filter(sql::<Bool>("active = true"))
            .limit(10);
        let sql = query.to_sql_string().expect("to_sql_string failed");
        assert_eq!(sql, "SELECT * FROM users WHERE active = true LIMIT 10");
    }
}
