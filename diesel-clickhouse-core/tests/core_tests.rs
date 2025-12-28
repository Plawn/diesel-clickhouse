//! Unit tests for diesel-clickhouse-core.

use diesel_clickhouse_core::backend::*;
use diesel_clickhouse_core::expression::*;
use diesel_clickhouse_core::query_builder::*;
use diesel_clickhouse_core::test_utils::{build_sql_inlined, TestTable};
use diesel_clickhouse_types::*;

// =============================================================================
// Query Builder Tests
// =============================================================================

mod query_builder_tests {
    use super::*;

    #[test]
    fn test_http_query_builder_basic() {
        let mut builder = HttpQueryBuilder::default();
        builder.push_sql("SELECT ");
        builder.push_identifier("id");
        builder.push_sql(", ");
        builder.push_identifier("name");
        builder.push_sql(" FROM ");
        builder.push_identifier("users");

        let sql = builder.finish();
        assert_eq!(sql, "SELECT `id`, `name` FROM `users`");
    }

    #[test]
    fn test_http_query_builder_with_bind_params() {
        let mut builder = HttpQueryBuilder::default();
        builder.push_sql("SELECT * FROM ");
        builder.push_identifier("users");
        builder.push_sql(" WHERE ");
        builder.push_identifier("id");
        builder.push_sql(" = ");
        builder.push_bind_param();
        builder.push_sql(" AND ");
        builder.push_identifier("name");
        builder.push_sql(" = ");
        builder.push_bind_param();

        let sql = builder.finish();
        assert_eq!(sql, "SELECT * FROM `users` WHERE `id` = {p0:String} AND `name` = {p1:String}");
    }

    #[test]
    fn test_native_query_builder_basic() {
        let mut builder = NativeQueryBuilder::default();
        builder.push_sql("SELECT * FROM ");
        builder.push_identifier("events");

        let sql = builder.finish();
        assert_eq!(sql, "SELECT * FROM `events`");
    }

    #[test]
    fn test_native_query_builder_with_bind_params() {
        let mut builder = NativeQueryBuilder::default();
        builder.push_sql("INSERT INTO ");
        builder.push_identifier("users");
        builder.push_sql(" VALUES (");
        builder.push_bind_param();
        builder.push_sql(", ");
        builder.push_bind_param();
        builder.push_sql(")");

        let sql = builder.finish();
        assert_eq!(sql, "INSERT INTO `users` VALUES ($1, $2)");
    }

    #[test]
    fn test_identifier_with_special_chars() {
        let mut builder = HttpQueryBuilder::default();
        builder.push_identifier("table`name");
        let sql = builder.finish();
        assert_eq!(sql, "`table``name`");
    }

    #[test]
    fn test_generic_query_builder() {
        let mut builder = GenericQueryBuilder::default();
        builder.push_sql("SELECT 1 + ");
        builder.push_bind_param();

        let sql = builder.finish();
        assert_eq!(sql, "SELECT 1 + ?");
    }
}

// =============================================================================
// Expression Tests
// =============================================================================

mod expression_tests {
    use super::*;

    #[test]
    fn test_sql_literal_type() {
        let lit: SqlLiteral<UInt64> = sql("42");
        // Verify it implements Expression with the correct SqlType
        fn check_sql_type<E: Expression<SqlType = UInt64>>(_e: E) {}
        check_sql_type(lit);
    }

    #[test]
    fn test_sql_literal_query_fragment() {
        let lit: SqlLiteral<UInt64> = sql("1 + 2");

        let mut builder = GenericQueryBuilder::default();
        let mut collector = GenericBindCollector::default();
        let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);

        lit.walk_ast(pass).unwrap();
        assert_eq!(builder.finish(), "1 + 2");
    }

    #[test]
    fn test_bound_expression_type() {
        let bound = Bound::<u64, UInt64>::new(42u64);
        fn check_sql_type<E: Expression<SqlType = UInt64>>(_e: E) {}
        check_sql_type(bound);
    }

    #[test]
    fn test_expression_tuple_types() {
        // Single element tuple
        fn check_tuple1<E: Expression>(_e: E) {}
        let tuple1: (SqlLiteral<UInt64>,) = (sql("1"),);
        check_tuple1(tuple1);

        // Two element tuple
        let tuple2: (SqlLiteral<UInt64>, SqlLiteral<CHString>) = (sql("1"), sql("'hello'"));
        check_tuple1(tuple2);
    }
}

// =============================================================================
// Operator Tests
// =============================================================================

mod operator_tests {
    use super::*;

    #[test]
    fn test_eq_operator() {
        let left: SqlLiteral<UInt64> = sql("a");
        let right: SqlLiteral<UInt64> = sql("b");
        let eq = Eq { left, right };
        assert_eq!(build_sql_inlined(&eq), "a = b");
    }

    #[test]
    fn test_not_eq_operator() {
        let left: SqlLiteral<UInt64> = sql("a");
        let right: SqlLiteral<UInt64> = sql("b");
        let ne = NotEq { left, right };
        assert_eq!(build_sql_inlined(&ne), "a != b");
    }

    #[test]
    fn test_gt_operator() {
        let left: SqlLiteral<UInt64> = sql("x");
        let right: SqlLiteral<UInt64> = sql("10");
        let gt = Gt { left, right };
        assert_eq!(build_sql_inlined(&gt), "x > 10");
    }

    #[test]
    fn test_lt_operator() {
        let left: SqlLiteral<UInt64> = sql("x");
        let right: SqlLiteral<UInt64> = sql("10");
        let lt = Lt { left, right };
        assert_eq!(build_sql_inlined(&lt), "x < 10");
    }

    #[test]
    fn test_gte_operator() {
        let left: SqlLiteral<UInt64> = sql("x");
        let right: SqlLiteral<UInt64> = sql("10");
        let gte = GtEq { left, right };
        assert_eq!(build_sql_inlined(&gte), "x >= 10");
    }

    #[test]
    fn test_lte_operator() {
        let left: SqlLiteral<UInt64> = sql("x");
        let right: SqlLiteral<UInt64> = sql("10");
        let lte = LtEq { left, right };
        assert_eq!(build_sql_inlined(&lte), "x <= 10");
    }

    #[test]
    fn test_and_operator() {
        let left: SqlLiteral<Bool> = sql("a = 1");
        let right: SqlLiteral<Bool> = sql("b = 2");
        let and = And { left, right };
        assert_eq!(build_sql_inlined(&and), "(a = 1 AND b = 2)");
    }

    #[test]
    fn test_or_operator() {
        let left: SqlLiteral<Bool> = sql("a = 1");
        let right: SqlLiteral<Bool> = sql("b = 2");
        let or = Or { left, right };
        assert_eq!(build_sql_inlined(&or), "(a = 1 OR b = 2)");
    }

    #[test]
    fn test_not_operator() {
        let expr: SqlLiteral<Bool> = sql("active");
        let not = Not { expr };
        assert_eq!(build_sql_inlined(&not), "NOT (active)");
    }

    #[test]
    fn test_is_null() {
        let expr: SqlLiteral<UInt64> = sql("column_name");
        let is_null = IsNull { expr };
        assert_eq!(build_sql_inlined(&is_null), "column_name IS NULL");
    }

    #[test]
    fn test_is_not_null() {
        let expr: SqlLiteral<UInt64> = sql("column_name");
        let is_not_null = IsNotNull { expr };
        assert_eq!(build_sql_inlined(&is_not_null), "column_name IS NOT NULL");
    }

    #[test]
    fn test_like_operator() {
        let left: SqlLiteral<CHString> = sql("name");
        let right: SqlLiteral<CHString> = sql("'%test%'");
        let like = Like { left, right };
        assert_eq!(build_sql_inlined(&like), "name LIKE '%test%'");
    }

    #[test]
    fn test_ilike_operator() {
        let left: SqlLiteral<CHString> = sql("name");
        let right: SqlLiteral<CHString> = sql("'%TEST%'");
        let ilike = ILike { left, right };
        assert_eq!(build_sql_inlined(&ilike), "name ILIKE '%TEST%'");
    }

    #[test]
    fn test_between_operator() {
        let expr: SqlLiteral<UInt64> = sql("age");
        let low: SqlLiteral<UInt64> = sql("18");
        let high: SqlLiteral<UInt64> = sql("65");
        let between = Between { expr, low, high };
        assert_eq!(build_sql_inlined(&between), "age BETWEEN 18 AND 65");
    }

    #[test]
    fn test_nested_operators() {
        // (a = 1 AND b = 2) OR c = 3
        let eq1: Eq<SqlLiteral<UInt64>, SqlLiteral<UInt64>> = Eq {
            left: sql("a"),
            right: sql("1"),
        };
        let eq2: Eq<SqlLiteral<UInt64>, SqlLiteral<UInt64>> = Eq {
            left: sql("b"),
            right: sql("2"),
        };
        let and = And {
            left: eq1,
            right: eq2,
        };
        let eq3: Eq<SqlLiteral<UInt64>, SqlLiteral<UInt64>> = Eq {
            left: sql("c"),
            right: sql("3"),
        };
        let or = Or { left: and, right: eq3 };
        assert_eq!(build_sql_inlined(&or), "((a = 1 AND b = 2) OR c = 3)");
    }
}

// =============================================================================
// ClickHouse Extension Tests
// =============================================================================

mod clickhouse_extension_tests {
    use super::*;

    #[test]
    fn test_final_modifier() {
        let base: SqlLiteral<UInt64> = sql("SELECT * FROM users");
        let final_query = base.final_();
        assert_eq!(build_sql_inlined(&final_query), "SELECT * FROM users FINAL");
    }

    #[test]
    fn test_sample_modifier() {
        let base: SqlLiteral<UInt64> = sql("SELECT * FROM events");
        let sampled = base.sample(0.1);
        assert_eq!(build_sql_inlined(&sampled), "SELECT * FROM events SAMPLE 0.1");
    }

    #[test]
    fn test_sample_with_offset() {
        let base: SqlLiteral<UInt64> = sql("SELECT * FROM events");
        let sampled = base.sample_with_offset(0.1, 0.5);
        assert_eq!(build_sql_inlined(&sampled), "SELECT * FROM events SAMPLE 0.1 OFFSET 0.5");
    }

    #[test]
    fn test_with_totals() {
        let base: SqlLiteral<UInt64> = sql("SELECT count() FROM events GROUP BY type");
        let with_totals = base.with_totals();
        assert_eq!(build_sql_inlined(&with_totals), "SELECT count() FROM events GROUP BY type WITH TOTALS");
    }

    #[test]
    fn test_format() {
        let base: SqlLiteral<UInt64> = sql("SELECT * FROM users");
        let formatted = base.format("JSONEachRow");
        assert_eq!(build_sql_inlined(&formatted), "SELECT * FROM users FORMAT JSONEachRow");
    }

    #[test]
    fn test_settings() {
        let base: SqlLiteral<UInt64> = sql("SELECT * FROM users");
        let with_settings = base.settings()
            .set("max_threads", "4")
            .set("optimize_read_in_order", "1");
        assert_eq!(build_sql_inlined(&with_settings), "SELECT * FROM users SETTINGS max_threads = 4, optimize_read_in_order = 1");
    }

    #[test]
    fn test_combined_modifiers() {
        let base: SqlLiteral<UInt64> = sql("SELECT * FROM events");
        let query = base
            .sample(0.5)
            .final_()
            .format("TabSeparated");
        assert_eq!(build_sql_inlined(&query), "SELECT * FROM events SAMPLE 0.5 FINAL FORMAT TabSeparated");
    }
}

// =============================================================================
// Select Statement Tests
// =============================================================================

mod select_statement_tests {
    use super::*;

    #[test]
    fn test_simple_select() {
        let stmt = SelectStatement::new(TestTable("users"));
        assert_eq!(build_sql_inlined(&stmt), "SELECT * FROM `users`");
    }

    #[test]
    fn test_select_with_columns() {
        let columns: SqlLiteral<UInt64> = sql("id, name");
        let stmt = SelectStatement::new(TestTable("users")).select(columns);
        assert_eq!(build_sql_inlined(&stmt), "SELECT id, name FROM `users`");
    }

    #[test]
    fn test_select_with_where() {
        let predicate: SqlLiteral<Bool> = sql("id > 10");
        let stmt = SelectStatement::new(TestTable("users")).filter(predicate);
        assert_eq!(build_sql_inlined(&stmt), "SELECT * FROM `users` WHERE id > 10");
    }

    #[test]
    fn test_select_with_order_by() {
        let order: SqlLiteral<UInt64> = sql("created_at DESC");
        let stmt = SelectStatement::new(TestTable("users")).order_by(order);
        assert_eq!(build_sql_inlined(&stmt), "SELECT * FROM `users` ORDER BY created_at DESC");
    }

    #[test]
    fn test_select_with_limit() {
        let stmt = SelectStatement::new(TestTable("users")).limit(100);
        assert_eq!(build_sql_inlined(&stmt), "SELECT * FROM `users` LIMIT 100");
    }

    #[test]
    fn test_select_with_offset() {
        let stmt = SelectStatement::new(TestTable("users")).offset(50);
        assert_eq!(build_sql_inlined(&stmt), "SELECT * FROM `users` OFFSET 50");
    }

    #[test]
    fn test_select_with_group_by() {
        let group: SqlLiteral<CHString> = sql("country");
        let stmt = SelectStatement::new(TestTable("users")).group_by(group);
        assert_eq!(build_sql_inlined(&stmt), "SELECT * FROM `users` GROUP BY country");
    }

    #[test]
    fn test_select_with_having() {
        let group: SqlLiteral<CHString> = sql("country");
        let having: SqlLiteral<Bool> = sql("count(*) > 10");
        let stmt = SelectStatement::new(TestTable("users"))
            .group_by(group)
            .having(having);
        assert_eq!(build_sql_inlined(&stmt), "SELECT * FROM `users` GROUP BY country HAVING count(*) > 10");
    }

    #[test]
    fn test_complex_select() {
        let columns: SqlLiteral<UInt64> = sql("country, count(*) as cnt");
        let predicate: SqlLiteral<Bool> = sql("active = 1");
        let group: SqlLiteral<CHString> = sql("country");
        let having: SqlLiteral<Bool> = sql("cnt > 5");
        let order: SqlLiteral<UInt64> = sql("cnt DESC");

        let stmt = SelectStatement::new(TestTable("users"))
            .select(columns)
            .filter(predicate)
            .group_by(group)
            .having(having)
            .order_by(order)
            .limit(10)
            .offset(0);

        assert_eq!(
            build_sql_inlined(&stmt),
            "SELECT country, count(*) as cnt FROM `users` WHERE active = 1 GROUP BY country HAVING cnt > 5 ORDER BY cnt DESC LIMIT 10 OFFSET 0"
        );
    }
}

// =============================================================================
// AsExpression Tests
// =============================================================================

mod as_expression_tests {
    use super::*;

    #[test]
    fn test_u8_as_expression() {
        let val = 42u8;
        let expr = val.as_expression();
        fn check<E: Expression<SqlType = UInt8>>(_: E) {}
        check(expr);
    }

    #[test]
    fn test_u16_as_expression() {
        let val = 1000u16;
        let expr = val.as_expression();
        fn check<E: Expression<SqlType = UInt16>>(_: E) {}
        check(expr);
    }

    #[test]
    fn test_u32_as_expression() {
        let val = 100000u32;
        let expr = val.as_expression();
        fn check<E: Expression<SqlType = UInt32>>(_: E) {}
        check(expr);
    }

    #[test]
    fn test_u64_as_expression() {
        let val = 1_000_000_000u64;
        let expr = val.as_expression();
        fn check<E: Expression<SqlType = UInt64>>(_: E) {}
        check(expr);
    }

    #[test]
    fn test_i64_as_expression() {
        let val = -1000i64;
        let expr = val.as_expression();
        fn check<E: Expression<SqlType = Int64>>(_: E) {}
        check(expr);
    }

    #[test]
    fn test_f32_as_expression() {
        let val = 3.14f32;
        let expr = val.as_expression();
        fn check<E: Expression<SqlType = Float32>>(_: E) {}
        check(expr);
    }

    #[test]
    fn test_f64_as_expression() {
        let val = std::f64::consts::PI;
        let expr = val.as_expression();
        fn check<E: Expression<SqlType = Float64>>(_: E) {}
        check(expr);
    }

    #[test]
    fn test_bool_as_expression() {
        let val = true;
        let expr = val.as_expression();
        fn check<E: Expression<SqlType = Bool>>(_: E) {}
        check(expr);
    }

    #[test]
    fn test_string_as_expression() {
        let val = String::from("hello");
        let expr = val.as_expression();
        fn check<E: Expression<SqlType = CHString>>(_: E) {}
        check(expr);
    }

    #[test]
    fn test_str_as_expression() {
        let val = "world";
        let expr = val.as_expression();
        fn check<E: Expression<SqlType = CHString>>(_: E) {}
        check(expr);
    }
}

