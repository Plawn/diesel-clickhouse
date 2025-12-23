//! Integration tests for advanced features.
//!
//! Tests for:
//! - Arrow zero-copy parsing
//! - Pool
//! - Arena allocator

// =============================================================================
// Arrow Zero-Copy Row Tests
// =============================================================================

#[cfg(feature = "arrow")]
mod arrow_row_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use arrow::array::{Int64Array, Float64Array, StringArray, BooleanArray, RecordBatch};
    use arrow::datatypes::{DataType, Field, Schema};

    use diesel_clickhouse::arrow::{ArrowRow, build_column_index, for_each_row};

    fn create_test_batch() -> (RecordBatch, HashMap<Arc<str>, usize>) {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("score", DataType::Float64, false),
            Field::new("active", DataType::Boolean, false),
        ]));

        let id_array = Int64Array::from(vec![1, 2, 3]);
        let name_array = StringArray::from(vec!["alice", "bob", "charlie"]);
        let score_array = Float64Array::from(vec![100.0, 200.5, 300.25]);
        let active_array = BooleanArray::from(vec![true, false, true]);

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_array),
                Arc::new(name_array),
                Arc::new(score_array),
                Arc::new(active_array),
            ],
        )
        .unwrap();

        let column_indices = build_column_index(&schema);
        (batch, column_indices)
    }

    #[test]
    fn test_arrow_row_get_i64() {
        let (batch, indices) = create_test_batch();
        let row = ArrowRow::new(&batch, 0, &indices);

        assert_eq!(row.get_i64("id").unwrap(), 1);
    }

    #[test]
    fn test_arrow_row_get_str() {
        let (batch, indices) = create_test_batch();
        let row = ArrowRow::new(&batch, 1, &indices);

        assert_eq!(row.get_str("name").unwrap(), "bob");
    }

    #[test]
    fn test_arrow_row_get_f64() {
        let (batch, indices) = create_test_batch();
        let row = ArrowRow::new(&batch, 2, &indices);

        let score = row.get_f64("score").unwrap();
        assert!((score - 300.25).abs() < 0.01);
    }

    #[test]
    fn test_arrow_row_get_bool() {
        let (batch, indices) = create_test_batch();

        let row0 = ArrowRow::new(&batch, 0, &indices);
        assert!(row0.get_bool("active").unwrap());

        let row1 = ArrowRow::new(&batch, 1, &indices);
        assert!(!row1.get_bool("active").unwrap());
    }

    #[test]
    fn test_arrow_row_column_not_found() {
        let (batch, indices) = create_test_batch();
        let row = ArrowRow::new(&batch, 0, &indices);

        let result = row.get_str("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_arrow_row_wrong_type() {
        let (batch, indices) = create_test_batch();
        let row = ArrowRow::new(&batch, 0, &indices);

        // id is Int64, not String
        let result = row.get_str("id");
        assert!(result.is_err());
    }

    #[test]
    fn test_for_each_row() {
        let (batch, indices) = create_test_batch();

        let mut rows = Vec::new();
        for_each_row(&batch, &indices, |row| {
            rows.push((
                row.get_i64("id").unwrap(),
                row.get_str("name").unwrap().to_string(),
                row.get_f64("score").unwrap(),
            ));
            Ok(())
        })
        .unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], (1, "alice".to_string(), 100.0));
        assert_eq!(rows[1], (2, "bob".to_string(), 200.5));
        assert_eq!(rows[2], (3, "charlie".to_string(), 300.25));
    }

    #[test]
    fn test_build_column_index() {
        let schema = Schema::new(vec![
            Field::new("a", DataType::Int32, false),
            Field::new("b", DataType::Utf8, false),
            Field::new("c", DataType::Float64, false),
        ]);

        let indices = build_column_index(&schema);

        let key_a: Arc<str> = Arc::from("a");
        let key_b: Arc<str> = Arc::from("b");
        let key_c: Arc<str> = Arc::from("c");

        assert_eq!(indices.get(&key_a), Some(&0));
        assert_eq!(indices.get(&key_b), Some(&1));
        assert_eq!(indices.get(&key_c), Some(&2));
    }

    #[test]
    fn test_arrow_row_num_columns() {
        let (batch, indices) = create_test_batch();
        let row = ArrowRow::new(&batch, 0, &indices);

        assert_eq!(row.num_columns(), 4);
    }

    #[test]
    fn test_arrow_row_row_index() {
        let (batch, indices) = create_test_batch();

        let row0 = ArrowRow::new(&batch, 0, &indices);
        assert_eq!(row0.row_index(), 0);

        let row2 = ArrowRow::new(&batch, 2, &indices);
        assert_eq!(row2.row_index(), 2);
    }
}

// =============================================================================
// Pool Tests (unit tests only - no ClickHouse connection)
// =============================================================================

#[cfg(any(feature = "http", feature = "native"))]
mod pool_tests {
    use diesel_clickhouse::pool::PoolConfig;

    #[test]
    fn test_pool_config_new() {
        let config = PoolConfig::new(10);

        assert_eq!(config.max_size, 10);
    }

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();

        assert!(config.max_size > 0);
        assert!(config.connection_timeout_ms > 0);
    }
}
