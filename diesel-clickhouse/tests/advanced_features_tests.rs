//! Integration tests for advanced features.
//!
//! Tests for:
//! - Zero-copy parsing
//! - Pool
//! - Arena allocator

// =============================================================================
// Zero-Copy Parsing Tests
// =============================================================================

#[cfg(feature = "http")]
mod zero_copy_tests {
    use diesel_clickhouse::zero_copy::{BorrowedValue, TsvParser};

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
        // ClickHouse TSV uses \N for NULL
        assert!(BorrowedValue::new(b"\\N").is_null());
        assert!(!BorrowedValue::new(b"hello").is_null());
    }

    #[test]
    fn test_borrowed_value_as_str() {
        let val = BorrowedValue::new(b"hello world");
        assert_eq!(val.as_str().unwrap(), "hello world");
    }

    #[test]
    fn test_tsv_parser_for_each() {
        let data = b"1\talice\t100\n2\tbob\t200\n";
        let parser = TsvParser::new(data, &["id", "name", "score"]);

        let mut rows = Vec::new();
        parser
            .for_each(|row| {
                rows.push((
                    row.get_u64("id").unwrap(),
                    row.get_str("name").unwrap().to_string(),
                    row.get_u64("score").unwrap(),
                ));
                Ok(())
            })
            .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], (1, "alice".to_string(), 100));
        assert_eq!(rows[1], (2, "bob".to_string(), 200));
    }

    #[test]
    fn test_tsv_parser_with_null() {
        let data = b"1\t\\N\t100\n";
        let parser = TsvParser::new(data, &["id", "name", "score"]);

        parser
            .for_each(|row| {
                assert_eq!(row.get_u64("id").unwrap(), 1);
                assert!(row.is_null("name").unwrap());
                assert_eq!(row.get_optional_str("name").unwrap(), None);
                assert_eq!(row.get_u64("score").unwrap(), 100);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn test_tsv_parser_by_index() {
        let data = b"42\thello\t3.14\n";
        let parser = TsvParser::new(data, &["a", "b", "c"]);

        parser
            .for_each(|row| {
                let val0 = row.get_by_index(0).unwrap();
                let val1 = row.get_by_index(1).unwrap();
                let val2 = row.get_by_index(2).unwrap();

                assert_eq!(val0.parse_u64().unwrap(), 42);
                assert_eq!(val1.as_str().unwrap(), "hello");
                assert!((val2.parse_f64().unwrap() - 3.14).abs() < 0.01);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn test_streaming_tsv_parser() {
        use diesel_clickhouse::zero_copy::StreamingTsvParser;

        let columns = ["id", "name"];
        let mut parser = StreamingTsvParser::new(&columns);

        // First chunk ends mid-row
        let chunk1 = b"1\talice\n2\tbo";
        let mut count1 = 0;
        parser
            .process_chunk(chunk1, |_row| {
                count1 += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(count1, 1); // Only first row complete

        // Second chunk completes the row
        let chunk2 = b"b\n";
        let mut count2 = 0;
        parser
            .process_chunk(chunk2, |row| {
                assert_eq!(row.get_str("name").unwrap(), "bob");
                count2 += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(count2, 1);
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
