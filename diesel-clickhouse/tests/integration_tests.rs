//! Integration tests for diesel-clickhouse.
//!
//! These tests require a running ClickHouse instance.
//! Use `docker-compose up -d` to start the test database.
//!
//! Run with: `cargo test --test integration_tests`
//! Run integration tests: `cargo test --test integration_tests --features integration`

// =============================================================================
// Type System Integration Tests
// =============================================================================

mod type_tests {
    use diesel_clickhouse_types::*;

    #[test]
    fn test_type_names_match_clickhouse() {
        // Verify our type names match what ClickHouse expects
        assert_eq!(UInt8::type_name(), "UInt8");
        assert_eq!(UInt16::type_name(), "UInt16");
        assert_eq!(UInt32::type_name(), "UInt32");
        assert_eq!(UInt64::type_name(), "UInt64");
        assert_eq!(UInt128::type_name(), "UInt128");
        assert_eq!(UInt256::type_name(), "UInt256");

        assert_eq!(Int8::type_name(), "Int8");
        assert_eq!(Int16::type_name(), "Int16");
        assert_eq!(Int32::type_name(), "Int32");
        assert_eq!(Int64::type_name(), "Int64");
        assert_eq!(Int128::type_name(), "Int128");
        assert_eq!(Int256::type_name(), "Int256");

        assert_eq!(Float32::type_name(), "Float32");
        assert_eq!(Float64::type_name(), "Float64");

        assert_eq!(CHString::type_name(), "String");
        assert_eq!(UUID::type_name(), "UUID");
        assert_eq!(Bool::type_name(), "Bool");

        assert_eq!(Date::type_name(), "Date");
        assert_eq!(Date32::type_name(), "Date32");
        assert_eq!(DateTime::type_name(), "DateTime");
        // type_name() returns just the base type name without parameters
        assert_eq!(<DateTime64<3>>::type_name(), "DateTime64");
        assert_eq!(<DateTime64<6>>::type_name(), "DateTime64");

        assert_eq!(<Array<UInt64>>::type_name(), "Array");
        assert_eq!(<Array<CHString>>::type_name(), "Array");
        assert_eq!(<Nullable<UInt64>>::type_name(), "Nullable");
        assert_eq!(<Map<CHString, UInt64>>::type_name(), "Map");
        assert_eq!(<LowCardinality<CHString>>::type_name(), "LowCardinality");
    }

    #[test]
    fn test_fixed_string_type_names() {
        // type_name() returns just the base type name
        assert_eq!(<FixedString<16>>::type_name(), "FixedString");
        assert_eq!(<FixedString<32>>::type_name(), "FixedString");
        assert_eq!(<FixedString<64>>::type_name(), "FixedString");
    }

    #[test]
    fn test_nested_complex_types() {
        // type_name() returns just the base type name
        assert_eq!(<Array<Array<UInt64>>>::type_name(), "Array");
        assert_eq!(<Nullable<Array<CHString>>>::type_name(), "Nullable");
        assert_eq!(<Map<CHString, Array<UInt64>>>::type_name(), "Map");
    }
}

// =============================================================================
// Query Builder Integration Tests
// =============================================================================

mod query_builder_tests {
    use diesel_clickhouse_core::backend::*;
    use diesel_clickhouse_core::expression::*;
    use diesel_clickhouse_core::query_builder::*;
    use diesel_clickhouse_types::*;

    fn build_sql<T: QueryFragment<ClickHouse>>(fragment: &T) -> String {
        let mut builder = GenericQueryBuilder::default();
        let mut collector = GenericBindCollector::default();
        let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
        fragment.walk_ast(pass).unwrap();

        // Inline bindings into the SQL for easier test assertions
        // GenericQueryBuilder uses '?' as placeholder
        let mut sql = builder.finish();
        for binding in collector.bindable_values().iter().rev() {
            if let Some(pos) = sql.rfind('?') {
                sql.replace_range(pos..pos + 1, &binding.sql_literal());
            }
        }
        sql
    }

    // Simple table representation for tests
    struct UsersTable;
    impl QueryFragment<ClickHouse> for UsersTable {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, ClickHouse>) -> diesel_clickhouse_core::result::QueryResult<()> {
            pass.push_identifier("users");
            Ok(())
        }
    }

    struct EventsTable;
    impl QueryFragment<ClickHouse> for EventsTable {
        fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, ClickHouse>) -> diesel_clickhouse_core::result::QueryResult<()> {
            pass.push_identifier("events");
            Ok(())
        }
    }

    #[test]
    fn test_simple_select_query() {
        let stmt = SelectStatement::new(UsersTable);
        let sql = build_sql(&stmt);
        assert_eq!(sql, "SELECT * FROM `users`");
    }

    #[test]
    fn test_select_with_filter() {
        let predicate: SqlLiteral<Bool> = sql("id > 100");
        let stmt = SelectStatement::new(UsersTable).filter(predicate);
        let sql = build_sql(&stmt);
        assert_eq!(sql, "SELECT * FROM `users` WHERE id > 100");
    }

    #[test]
    fn test_select_with_multiple_clauses() {
        let columns: SqlLiteral<UInt64> = sql("id, name, created_at");
        let predicate: SqlLiteral<Bool> = sql("active = 1");
        let order: SqlLiteral<UInt64> = sql("created_at DESC");

        let stmt = SelectStatement::new(UsersTable)
            .select(columns)
            .filter(predicate)
            .order_by(order)
            .limit(100);

        let sql = build_sql(&stmt);
        assert_eq!(sql, "SELECT id, name, created_at FROM `users` WHERE active = 1 ORDER BY created_at DESC LIMIT 100");
    }

    #[test]
    fn test_group_by_query() {
        let columns: SqlLiteral<UInt64> = sql("country, count(*) as cnt");
        let group: SqlLiteral<CHString> = sql("country");
        let having: SqlLiteral<Bool> = sql("cnt > 10");
        let order: SqlLiteral<UInt64> = sql("cnt DESC");

        let stmt = SelectStatement::new(UsersTable)
            .select(columns)
            .group_by(group)
            .having(having)
            .order_by(order);

        let sql = build_sql(&stmt);
        assert_eq!(sql, "SELECT country, count(*) as cnt FROM `users` GROUP BY country HAVING cnt > 10 ORDER BY cnt DESC");
    }

    #[test]
    fn test_clickhouse_final() {
        let base: SqlLiteral<UInt64> = sql("SELECT * FROM events");
        let query = base.final_();
        let sql = build_sql(&query);
        assert_eq!(sql, "SELECT * FROM events FINAL");
    }

    #[test]
    fn test_clickhouse_sample() {
        let base: SqlLiteral<UInt64> = sql("SELECT count() FROM events");
        let query = base.sample(0.01);
        let sql = build_sql(&query);
        assert_eq!(sql, "SELECT count() FROM events SAMPLE 0.01");
    }

    #[test]
    fn test_clickhouse_prewhere() {
        let base: SqlLiteral<UInt64> = sql("SELECT * FROM events");
        let pred: SqlLiteral<Bool> = sql("toDate(timestamp) = today()");
        let query = Prewhere::new(base, pred);
        let sql = build_sql(&query);
        assert_eq!(sql, "SELECT * FROM events PREWHERE toDate(timestamp) = today()");
    }

    #[test]
    fn test_clickhouse_settings() {
        let base: SqlLiteral<UInt64> = sql("SELECT * FROM events");
        let query = base.settings()
            .set("max_threads", "4")
            .set("max_memory_usage", "10000000000");
        let sql = build_sql(&query);
        assert_eq!(sql, "SELECT * FROM events SETTINGS max_threads = 4, max_memory_usage = 10000000000");
    }

    #[test]
    fn test_clickhouse_format() {
        let base: SqlLiteral<UInt64> = sql("SELECT * FROM events");
        let query = base.format("JSONEachRow");
        let sql = build_sql(&query);
        assert_eq!(sql, "SELECT * FROM events FORMAT JSONEachRow");
    }

    #[test]
    fn test_with_totals() {
        let base: SqlLiteral<UInt64> = sql("SELECT type, count() FROM events GROUP BY type");
        let query = base.with_totals();
        let sql = build_sql(&query);
        assert_eq!(sql, "SELECT type, count() FROM events GROUP BY type WITH TOTALS");
    }

    #[test]
    fn test_combined_clickhouse_features() {
        // This is the kind of query you might run in production
        let base: SqlLiteral<UInt64> = sql("SELECT user_id, count() as event_count FROM events");
        let pred: SqlLiteral<Bool> = sql("toDate(timestamp) >= today() - 7");

        let query = Prewhere::new(base, pred)
            .sample(0.1)
            .final_()
            .settings()
            .set("max_threads", "8");

        let sql = build_sql(&query);
        assert_eq!(
            sql,
            "SELECT user_id, count() as event_count FROM events PREWHERE toDate(timestamp) >= today() - 7 SAMPLE 0.1 FINAL SETTINGS max_threads = 8"
        );
    }
}

// =============================================================================
// Expression Integration Tests
// =============================================================================

mod expression_tests {
    use diesel_clickhouse_core::backend::*;
    use diesel_clickhouse_core::expression::*;
    use diesel_clickhouse_core::query_builder::*;
    use diesel_clickhouse_types::*;

    fn build_sql<T: QueryFragment<ClickHouse>>(fragment: &T) -> String {
        let mut builder = GenericQueryBuilder::default();
        let mut collector = GenericBindCollector::default();
        let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
        fragment.walk_ast(pass).unwrap();

        // Inline bindings into the SQL for easier test assertions
        // GenericQueryBuilder uses '?' as placeholder
        let mut sql = builder.finish();
        for binding in collector.bindable_values().iter().rev() {
            if let Some(pos) = sql.rfind('?') {
                sql.replace_range(pos..pos + 1, &binding.sql_literal());
            }
        }
        sql
    }

    #[test]
    fn test_complex_where_clause() {
        // (status = 'active' AND age >= 18) OR role = 'admin'
        let status_check: SqlLiteral<Bool> = sql("status = 'active'");
        let age_check: SqlLiteral<Bool> = sql("age >= 18");
        let role_check: SqlLiteral<Bool> = sql("role = 'admin'");

        let and_clause = And {
            left: status_check,
            right: age_check,
        };
        let or_clause = Or {
            left: and_clause,
            right: role_check,
        };

        let sql = build_sql(&or_clause);
        assert_eq!(sql, "((status = 'active' AND age >= 18) OR role = 'admin')");
    }

    #[test]
    fn test_comparison_operators() {
        let a: SqlLiteral<UInt64> = sql("a");
        let b: SqlLiteral<UInt64> = sql("b");

        assert_eq!(build_sql(&Eq { left: a.clone(), right: b.clone() }), "a = b");
        assert_eq!(build_sql(&NotEq { left: a.clone(), right: b.clone() }), "a != b");
        assert_eq!(build_sql(&Gt { left: a.clone(), right: b.clone() }), "a > b");
        assert_eq!(build_sql(&Lt { left: a.clone(), right: b.clone() }), "a < b");
        assert_eq!(build_sql(&GtEq { left: a.clone(), right: b.clone() }), "a >= b");
        assert_eq!(build_sql(&LtEq { left: a, right: b }), "a <= b");
    }

    #[test]
    fn test_null_checks() {
        let col: SqlLiteral<Nullable<UInt64>> = sql("optional_field");

        let is_null = IsNull { expr: col.clone() };
        let is_not_null = IsNotNull { expr: col };

        assert_eq!(build_sql(&is_null), "optional_field IS NULL");
        assert_eq!(build_sql(&is_not_null), "optional_field IS NOT NULL");
    }

    #[test]
    fn test_between_operator() {
        let expr: SqlLiteral<UInt64> = sql("price");
        let low: SqlLiteral<UInt64> = sql("100");
        let high: SqlLiteral<UInt64> = sql("1000");

        let between = Between { expr, low, high };
        assert_eq!(build_sql(&between), "price BETWEEN 100 AND 1000");
    }

    #[test]
    fn test_like_operators() {
        let name: SqlLiteral<CHString> = sql("name");
        let pattern: SqlLiteral<CHString> = sql("'%test%'");

        let like = Like {
            left: name.clone(),
            right: pattern.clone(),
        };
        let ilike = ILike { left: name, right: pattern };

        assert_eq!(build_sql(&like), "name LIKE '%test%'");
        assert_eq!(build_sql(&ilike), "name ILIKE '%test%'");
    }

    #[test]
    fn test_not_expression() {
        let expr: SqlLiteral<Bool> = sql("is_deleted");
        let not_expr = Not { expr };
        assert_eq!(build_sql(&not_expr), "NOT (is_deleted)");
    }
}

// =============================================================================
// Migration Integration Tests
// =============================================================================

mod migration_tests {
    use diesel_clickhouse_migrations::migration::*;
    use diesel_clickhouse_migrations::source::*;

    #[test]
    fn test_migration_creation_and_loading() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let source = FileBasedMigrations::new(temp_dir.path());

        // Create a migration
        let path = source.create_migration("test_migration").unwrap();
        assert!(path.exists());

        // Write some SQL
        std::fs::write(path.join("up.sql"), "CREATE TABLE test (id UInt64) ENGINE = MergeTree ORDER BY id").unwrap();
        std::fs::write(path.join("down.sql"), "DROP TABLE test").unwrap();

        // Load and verify
        let migrations = source.migrations().unwrap();
        assert_eq!(migrations.len(), 1);
        assert!(migrations[0].up_sql.contains("CREATE TABLE"));
        assert!(migrations[0].down_sql.contains("DROP TABLE"));
    }

    #[test]
    fn test_in_memory_migrations_ordering() {
        let source = InMemoryMigrations::new()
            .with_migration(Migration::new("00003", "third", "SELECT 3", ""))
            .with_migration(Migration::new("00001", "first", "SELECT 1", ""))
            .with_migration(Migration::new("00002", "second", "SELECT 2", ""));

        let migrations = source.migrations().unwrap();

        assert_eq!(migrations.len(), 3);
        assert_eq!(migrations[0].version.as_str(), "00001");
        assert_eq!(migrations[1].version.as_str(), "00002");
        assert_eq!(migrations[2].version.as_str(), "00003");
    }

    #[test]
    fn test_migration_checksum_stability() {
        let m1 = Migration::new("1", "test", "SELECT 1", "");
        let m2 = Migration::new("1", "test", "SELECT 1", "");
        let m3 = Migration::new("1", "test", "SELECT 2", "");

        // Same SQL should produce same checksum
        assert_eq!(m1.checksum(), m2.checksum());

        // Different SQL should produce different checksum
        assert_ne!(m1.checksum(), m3.checksum());
    }
}

// =============================================================================
// Library Integration Tests (requires running ClickHouse)
// =============================================================================

#[cfg(feature = "integration")]
mod clickhouse_integration {
    use std::env;
    use diesel_clickhouse::{Connection, ConnectionBuilder};

    /// Parse URL and create HTTP connection using builder.
    async fn connect_http() -> Option<diesel_clickhouse::http::ClickHouseConnection> {
        let url = env::var("CLICKHOUSE_URL")
            .unwrap_or_else(|_| "http://default:default@localhost:8123/test_db".to_string());

        let parsed = url::Url::parse(&url).ok()?;
        let host = parsed.host_str()?;
        let port = parsed.port().unwrap_or(8123);
        let database = parsed.path().trim_start_matches('/');
        let user = if parsed.username().is_empty() { "default" } else { parsed.username() };
        let password = parsed.password().unwrap_or("");

        let conn = Connection::http()
            .host(host)
            .port(port)
            .database(database)
            .user(user)
            .password(password)
            .build()
            .await
            .ok()?;

        conn.as_http().cloned()
    }

    /// Parse URL and create unified connection using builder.
    async fn connect_unified() -> Option<Connection> {
        let url = env::var("CLICKHOUSE_URL")
            .unwrap_or_else(|_| "http://default:default@localhost:8123/test_db".to_string());

        let parsed = url::Url::parse(&url).ok()?;
        let host = parsed.host_str()?;
        let port = parsed.port().unwrap_or(8123);
        let database = parsed.path().trim_start_matches('/');
        let user = if parsed.username().is_empty() { "default" } else { parsed.username() };
        let password = parsed.password().unwrap_or("");

        Connection::http()
            .host(host)
            .port(port)
            .database(database)
            .user(user)
            .password(password)
            .build()
            .await
            .ok()
    }

    #[tokio::test]
    async fn test_zero_copy_load() {
        let conn = match connect_http().await {
            Some(c) => c,
            None => {
                eprintln!("ClickHouse not available, skipping integration test");
                return;
            }
        };
        let table_name = format!("test_zero_copy_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis());

        // Create table
        conn.execute_raw(&format!(
            "CREATE TABLE IF NOT EXISTS {} (id UInt64, name String, score Float64) ENGINE = MergeTree ORDER BY id",
            table_name
        )).await.unwrap();

        // Insert data
        conn.execute_raw(&format!(
            "INSERT INTO {} VALUES (1, 'Alice', 95.5), (2, 'Bob', 87.3), (3, 'Charlie', 92.1)",
            table_name
        )).await.unwrap();

        // Test zero-copy load
        let mut results: Vec<(u64, String, f64)> = Vec::new();
        let count = conn.load_zero_copy(
            &format!("SELECT id, name, score FROM {} ORDER BY id", table_name),
            |row| {
                let id = row.get_u64("id")?;
                let name = row.get_str("name")?.to_string();
                let score = row.get_f64("score")?;
                results.push((id, name, score));
                Ok(())
            }
        ).await.unwrap();

        assert_eq!(count, 3);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], (1, "Alice".to_string(), 95.5));
        assert_eq!(results[1], (2, "Bob".to_string(), 87.3));
        assert_eq!(results[2], (3, "Charlie".to_string(), 92.1));

        // Cleanup
        conn.execute_raw(&format!("DROP TABLE IF EXISTS {}", table_name)).await.unwrap();
    }

    #[tokio::test]
    async fn test_zero_copy_streaming() {
        let conn = match connect_http().await {
            Some(c) => c,
            None => {
                eprintln!("ClickHouse not available, skipping integration test");
                return;
            }
        };
        let table_name = format!("test_zero_copy_stream_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis());

        // Create table with more data for streaming test
        conn.execute_raw(&format!(
            "CREATE TABLE IF NOT EXISTS {} (id UInt64, value String) ENGINE = MergeTree ORDER BY id",
            table_name
        )).await.unwrap();

        // Insert more rows to better test streaming
        conn.execute_raw(&format!(
            "INSERT INTO {} SELECT number, concat('value_', toString(number)) FROM numbers(100)",
            table_name
        )).await.unwrap();

        // Test streaming zero-copy load
        let mut sum: u64 = 0;
        let count = conn.load_zero_copy(
            &format!("SELECT id, value FROM {} ORDER BY id", table_name),
            |row| {
                let id = row.get_u64("id")?;
                sum += id;
                Ok(())
            }
        ).await.unwrap();

        assert_eq!(count, 100);
        // Sum of 0..100 = 4950
        assert_eq!(sum, 4950);

        // Cleanup
        conn.execute_raw(&format!("DROP TABLE IF EXISTS {}", table_name)).await.unwrap();
    }

    #[tokio::test]
    async fn test_zero_copy_with_nulls() {
        let conn = match connect_http().await {
            Some(c) => c,
            None => {
                eprintln!("ClickHouse not available, skipping integration test");
                return;
            }
        };
        let table_name = format!("test_zero_copy_null_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis());

        // Create table with nullable column
        conn.execute_raw(&format!(
            "CREATE TABLE IF NOT EXISTS {} (id UInt64, name Nullable(String)) ENGINE = MergeTree ORDER BY id",
            table_name
        )).await.unwrap();

        // Insert data with NULL values
        conn.execute_raw(&format!(
            "INSERT INTO {} VALUES (1, 'Alice'), (2, NULL), (3, 'Charlie')",
            table_name
        )).await.unwrap();

        // Test zero-copy load with NULLs
        let mut results: Vec<(u64, Option<String>)> = Vec::new();
        let count = conn.load_zero_copy(
            &format!("SELECT id, name FROM {} ORDER BY id", table_name),
            |row| {
                let id = row.get_u64("id")?;
                let name = row.get_optional_str("name")?.map(|s| s.to_string());
                results.push((id, name));
                Ok(())
            }
        ).await.unwrap();

        assert_eq!(count, 3);
        assert_eq!(results[0], (1, Some("Alice".to_string())));
        assert_eq!(results[1], (2, None));
        assert_eq!(results[2], (3, Some("Charlie".to_string())));

        // Cleanup
        conn.execute_raw(&format!("DROP TABLE IF EXISTS {}", table_name)).await.unwrap();
    }

    #[tokio::test]
    async fn test_unified_connection_load() {
        let conn = match connect_unified().await {
            Some(c) => c,
            None => {
                eprintln!("ClickHouse not available, skipping integration test");
                return;
            }
        };
        let table_name = format!("test_unified_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis());

        // Create and populate table
        conn.execute(&format!(
            "CREATE TABLE IF NOT EXISTS {} (id UInt64, name String) ENGINE = MergeTree ORDER BY id",
            table_name
        )).await.unwrap();

        conn.execute(&format!(
            "INSERT INTO {} VALUES (1, 'Test1'), (2, 'Test2')",
            table_name
        )).await.unwrap();

        // Test unified connection load
        let mut results: Vec<(u64, String)> = Vec::new();
        let count = conn.load_zero_copy(
            &format!("SELECT id, name FROM {} ORDER BY id", table_name),
            |row| {
                let id = row.get_u64("id")?;
                let name = row.get_str("name")?.to_string();
                results.push((id, name));
                Ok(())
            }
        ).await.unwrap();

        assert_eq!(count, 2);
        assert_eq!(results[0], (1, "Test1".to_string()));
        assert_eq!(results[1], (2, "Test2".to_string()));

        // Cleanup
        conn.execute(&format!("DROP TABLE IF EXISTS {}", table_name)).await.unwrap();
    }
}
