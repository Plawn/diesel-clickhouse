//! Unit tests for diesel-clickhouse-types.

use diesel_clickhouse_types::*;

// =============================================================================
// Integer Type Tests
// =============================================================================

mod integer_tests {
    use super::*;

    #[test]
    fn test_sql_type_names() {
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
    }

    #[test]
    fn test_u8_roundtrip() {
        for val in [0u8, 1, 127, 128, 255] {
            let mut buf = Vec::new();
            <u8 as ToClickHouse<UInt8>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <u8 as FromClickHouse<UInt8>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_u16_roundtrip() {
        for val in [0u16, 1, 255, 256, 65535] {
            let mut buf = Vec::new();
            <u16 as ToClickHouse<UInt16>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <u16 as FromClickHouse<UInt16>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_u32_roundtrip() {
        for val in [0u32, 1, u16::MAX as u32, u32::MAX] {
            let mut buf = Vec::new();
            <u32 as ToClickHouse<UInt32>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <u32 as FromClickHouse<UInt32>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_u64_roundtrip() {
        for val in [0u64, 1, u32::MAX as u64, u64::MAX] {
            let mut buf = Vec::new();
            <u64 as ToClickHouse<UInt64>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <u64 as FromClickHouse<UInt64>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_u128_roundtrip() {
        for val in [0u128, 1, u64::MAX as u128, u128::MAX] {
            let mut buf = Vec::new();
            <u128 as ToClickHouse<UInt128>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <u128 as FromClickHouse<UInt128>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_i8_roundtrip() {
        for val in [i8::MIN, -1, 0, 1, i8::MAX] {
            let mut buf = Vec::new();
            <i8 as ToClickHouse<Int8>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <i8 as FromClickHouse<Int8>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_i16_roundtrip() {
        for val in [i16::MIN, -1, 0, 1, i16::MAX] {
            let mut buf = Vec::new();
            <i16 as ToClickHouse<Int16>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <i16 as FromClickHouse<Int16>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_i32_roundtrip() {
        for val in [i32::MIN, -1, 0, 1, i32::MAX] {
            let mut buf = Vec::new();
            <i32 as ToClickHouse<Int32>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <i32 as FromClickHouse<Int32>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_i64_roundtrip() {
        for val in [i64::MIN, -1, 0, 1, i64::MAX] {
            let mut buf = Vec::new();
            <i64 as ToClickHouse<Int64>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <i64 as FromClickHouse<Int64>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_i128_roundtrip() {
        for val in [i128::MIN, -1, 0, 1, i128::MAX] {
            let mut buf = Vec::new();
            <i128 as ToClickHouse<Int128>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <i128 as FromClickHouse<Int128>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_u256_roundtrip() {
        let val = U256::from_u128(u128::MAX);
        let mut buf = Vec::new();
        <U256 as ToClickHouse<UInt256>>::to_clickhouse(&val, &mut buf).unwrap();
        let result = <U256 as FromClickHouse<UInt256>>::from_clickhouse(&buf).unwrap();
        assert_eq!(result, val);
    }

    #[test]
    fn test_i256_roundtrip() {
        for i128_val in [i128::MIN, -1, 0, 1, i128::MAX] {
            let val = I256::from_i128(i128_val);
            let mut buf = Vec::new();
            <I256 as ToClickHouse<Int256>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <I256 as FromClickHouse<Int256>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result.to_i128(), Some(i128_val));
        }
    }

    #[test]
    fn test_bool_roundtrip() {
        for val in [true, false] {
            let mut buf = Vec::new();
            <bool as ToClickHouse<Bool>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <bool as FromClickHouse<Bool>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_invalid_byte_length_errors() {
        // Too few bytes
        assert!(<u32 as FromClickHouse<UInt32>>::from_clickhouse(&[0, 0]).is_err());
        // Too many bytes
        assert!(<u32 as FromClickHouse<UInt32>>::from_clickhouse(&[0, 0, 0, 0, 0]).is_err());
    }
}

// =============================================================================
// Float Type Tests
// =============================================================================

mod float_tests {
    use super::*;

    #[test]
    fn test_sql_type_names() {
        assert_eq!(Float32::type_name(), "Float32");
        assert_eq!(Float64::type_name(), "Float64");
    }

    #[test]
    fn test_f32_roundtrip() {
        for val in [0.0f32, 1.0, -1.0, f32::MIN, f32::MAX, std::f32::consts::PI] {
            let mut buf = Vec::new();
            <f32 as ToClickHouse<Float32>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <f32 as FromClickHouse<Float32>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_f64_roundtrip() {
        for val in [0.0f64, 1.0, -1.0, f64::MIN, f64::MAX, std::f64::consts::PI] {
            let mut buf = Vec::new();
            <f64 as ToClickHouse<Float64>>::to_clickhouse(&val, &mut buf).unwrap();
            let result = <f64 as FromClickHouse<Float64>>::from_clickhouse(&buf).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_f32_special_values() {
        // Test NaN and infinity
        let nan = f32::NAN;
        let mut buf = Vec::new();
        <f32 as ToClickHouse<Float32>>::to_clickhouse(&nan, &mut buf).unwrap();
        let result = <f32 as FromClickHouse<Float32>>::from_clickhouse(&buf).unwrap();
        assert!(result.is_nan());

        buf.clear();
        let inf = f32::INFINITY;
        <f32 as ToClickHouse<Float32>>::to_clickhouse(&inf, &mut buf).unwrap();
        let result = <f32 as FromClickHouse<Float32>>::from_clickhouse(&buf).unwrap();
        assert!(result.is_infinite());
    }
}

// =============================================================================
// String Type Tests
// =============================================================================

mod string_tests {
    use super::*;

    #[test]
    fn test_sql_type_names() {
        assert_eq!(CHString::type_name(), "String");
        // FixedString returns just "FixedString" from type_name()
        assert_eq!(<FixedString<10>>::type_name(), "FixedString");
        assert_eq!(UUID::type_name(), "UUID");
    }

    #[test]
    fn test_string_roundtrip() {
        // Note: ToClickHouse for String includes a varint length prefix
        // FromClickHouse expects just the raw bytes, so we need to skip the prefix
        for val in ["hello", "hello world", "🎉 Unicode! 日本語"] {
            let s = val.to_string();
            let mut buf = Vec::new();
            <String as ToClickHouse<CHString>>::to_clickhouse(&s, &mut buf).unwrap();
            // The first byte(s) are the varint length, skip them for FromClickHouse
            // For strings < 128 bytes, it's just 1 byte
            let len_byte = buf[0] as usize;
            let result = <String as FromClickHouse<CHString>>::from_clickhouse(&buf[1..]).unwrap();
            assert_eq!(result.len(), len_byte);
            assert_eq!(result, val);
        }
    }

    #[test]
    fn test_string_with_nulls() {
        let val = "hello\0world";
        let s = val.to_string();
        let mut buf = Vec::new();
        <String as ToClickHouse<CHString>>::to_clickhouse(&s, &mut buf).unwrap();
        // Skip the varint length prefix
        let result = <String as FromClickHouse<CHString>>::from_clickhouse(&buf[1..]).unwrap();
        assert_eq!(result, val);
    }
}

// =============================================================================
// Complex Type Tests
// =============================================================================

mod complex_tests {
    use super::*;

    #[test]
    fn test_array_type_name() {
        // type_name() returns just the base type name
        assert_eq!(<Array<UInt64>>::type_name(), "Array");
        assert_eq!(<Array<CHString>>::type_name(), "Array");
        assert_eq!(<Array<Array<Int32>>>::type_name(), "Array");
    }

    #[test]
    fn test_nullable_type_name() {
        // type_name() returns just the base type name
        assert_eq!(<Nullable<UInt64>>::type_name(), "Nullable");
        assert_eq!(<Nullable<CHString>>::type_name(), "Nullable");
    }

    #[test]
    fn test_low_cardinality_type_name() {
        assert_eq!(<LowCardinality<CHString>>::type_name(), "LowCardinality");
    }

    #[test]
    fn test_map_type_name() {
        assert_eq!(<Map<CHString, UInt64>>::type_name(), "Map");
    }

    #[test]
    fn test_tuple_type_name() {
        assert_eq!(<Tuple<(UInt64, CHString)>>::type_name(), "Tuple");
    }
}

// =============================================================================
// Temporal Type Tests
// =============================================================================

mod temporal_tests {
    use super::*;

    #[test]
    fn test_sql_type_names() {
        assert_eq!(Date::type_name(), "Date");
        assert_eq!(Date32::type_name(), "Date32");
        assert_eq!(DateTime::type_name(), "DateTime");
        // type_name() returns just the base type name
        assert_eq!(<DateTime64<3>>::type_name(), "DateTime64");
        assert_eq!(<DateTime64<6>>::type_name(), "DateTime64");
    }
}

// =============================================================================
// SqlType Tuple Tests
// =============================================================================

mod tuple_sql_type_tests {
    use super::*;

    #[test]
    fn test_tuple_sql_type() {
        // Verify that tuples of SqlTypes are themselves SqlTypes
        fn assert_sql_type<T: SqlType>() {}

        assert_sql_type::<(UInt64,)>();
        assert_sql_type::<(UInt64, CHString)>();
        assert_sql_type::<(UInt64, CHString, Bool)>();
        assert_sql_type::<(UInt64, CHString, Bool, Float64)>();
    }
}

// =============================================================================
// Error Type Tests
// =============================================================================

mod error_tests {
    use super::*;

    #[test]
    fn test_deserialize_error_display() {
        let err = DeserializeError::InvalidData("test error".into());
        assert!(err.to_string().contains("test error"));

        let err = DeserializeError::TypeMismatch {
            expected: "UInt64".into(),
            actual: "String".into(),
        };
        assert!(err.to_string().contains("UInt64"));
        assert!(err.to_string().contains("String"));

        let err = DeserializeError::UnexpectedNull;
        // Message is "Null value for non-nullable type"
        assert!(err.to_string().contains("Null") || err.to_string().contains("null"));
    }

    #[test]
    fn test_serialize_error_display() {
        let err = SerializeError::InvalidValue("test value".into());
        assert!(err.to_string().contains("test value"));

        let err = SerializeError::OutOfRange {
            type_name: "UInt8".into(),
            value: "256".into(),
        };
        assert!(err.to_string().contains("UInt8"));
        assert!(err.to_string().contains("256"));
    }
}

// =============================================================================
// TypeMetadata Tests
// =============================================================================

mod metadata_tests {
    use super::*;

    #[test]
    fn test_simple_metadata() {
        let meta = TypeMetadata::simple("UInt64");
        assert_eq!(meta.name, "UInt64");
        assert!(!meta.nullable);
        assert!(meta.parameters.is_empty());
    }

    #[test]
    fn test_nullable_metadata() {
        let inner = TypeMetadata::simple("String");
        let meta = TypeMetadata::nullable(inner);
        assert!(meta.name.contains("Nullable"));
        assert!(meta.nullable);
        assert_eq!(meta.parameters.len(), 1);
    }

    #[test]
    fn test_parameterized_metadata() {
        let params = vec![
            TypeMetadata::simple("String"),
            TypeMetadata::simple("UInt64"),
        ];
        let meta = TypeMetadata::parameterized("Map", params);
        assert!(meta.name.contains("Map"));
        assert!(meta.name.contains("String"));
        assert!(meta.name.contains("UInt64"));
    }

    #[test]
    fn test_metadata_display() {
        let meta = TypeMetadata::simple("Int32");
        assert_eq!(format!("{}", meta), "Int32");
    }
}
