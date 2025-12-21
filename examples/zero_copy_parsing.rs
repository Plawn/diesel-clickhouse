//! Zero-copy parsing example for diesel-clickhouse.
//!
//! This example demonstrates efficient parsing of ClickHouse responses
//! without unnecessary memory allocations.
//!
//! Run with: cargo run --example zero_copy_parsing

use diesel_clickhouse::zero_copy::{
    BorrowedValue, ParseError, ZeroCopyRow, TsvParser, CsvParser, JsonRowParser, ZeroCopyParser
};
use std::time::Instant;

fn main() {
    println!("=== Zero-Copy Parsing Example ===\n");

    // -------------------------------------------------------------------------
    // 1. BorrowedValue - The Core Type
    // -------------------------------------------------------------------------
    println!("1. BorrowedValue basics:");

    let data = b"42";
    let value = BorrowedValue::new(data);

    println!("   Raw bytes: {:?}", value.as_bytes());
    println!("   As string: {:?}", value.as_str());
    println!("   Parse as i64: {:?}", value.parse_i64());
    println!("   Parse as u64: {:?}", value.parse_u64());
    println!("   Parse as f64: {:?}", value.parse_f64());
    println!("   Length: {} bytes", value.len());
    println!("   Is empty: {}", value.is_empty());
    println!("   Is null: {}", value.is_null());
    println!();

    // -------------------------------------------------------------------------
    // 2. Parsing Different Types
    // -------------------------------------------------------------------------
    println!("2. Parsing different value types:");

    // Integer
    let val = BorrowedValue::new(b"12345");
    println!("   Integer '12345': {:?}", val.parse_i64());

    // Negative integer
    let val = BorrowedValue::new(b"-999");
    println!("   Negative '-999': {:?}", val.parse_i64());

    // Unsigned integer
    let val = BorrowedValue::new(b"4294967295");
    println!("   Unsigned '4294967295': {:?}", val.parse_u64());

    // Float
    let val = BorrowedValue::new(b"3.14159");
    println!("   Float '3.14159': {:?}", val.parse_f64());

    // Scientific notation
    let val = BorrowedValue::new(b"1.5e-10");
    println!("   Scientific '1.5e-10': {:?}", val.parse_f64());

    // Boolean values
    let cases = [b"1".as_slice(), b"0", b"true", b"false", b"TRUE", b"FALSE"];
    for case in &cases {
        let val = BorrowedValue::new(case);
        println!("   Boolean '{}': {:?}", val.as_str().unwrap(), val.parse_bool());
    }
    println!();

    // -------------------------------------------------------------------------
    // 3. Null Handling
    // -------------------------------------------------------------------------
    println!("3. Null handling:");

    let null_cases = [
        (b"\\N".as_slice(), "\\\\N (ClickHouse TSV null)"),
        (b"NULL", "NULL"),
        (b"", "empty string"),
        (b"hello", "hello"),
    ];

    for (bytes, desc) in &null_cases {
        let val = BorrowedValue::new(*bytes);
        println!("   '{}' is_null: {}", desc, val.is_null());
    }
    println!();

    // -------------------------------------------------------------------------
    // 4. String Handling
    // -------------------------------------------------------------------------
    println!("4. String handling:");

    let val = BorrowedValue::new(b"Hello, World!");
    println!("   as_str(): {:?}", val.as_str());
    println!("   to_string_owned(): {:?}", val.to_string_owned());

    // Lossy conversion for invalid UTF-8
    let invalid_utf8 = BorrowedValue::new(&[0xff, 0xfe, b'A', b'B']);
    println!("   Invalid UTF-8 as_str(): {:?}", invalid_utf8.as_str());
    println!("   Invalid UTF-8 as_str_lossy(): {}", invalid_utf8.as_str_lossy());
    println!();

    // -------------------------------------------------------------------------
    // 5. TSV Parser
    // -------------------------------------------------------------------------
    println!("5. TSV Parser (TabSeparated format):");

    let tsv_data = b"1\talice\t100\n2\tbob\t200\n3\tcharlie\t300\n";
    let mut parser = TsvParser::new(tsv_data);

    while let Some(row) = parser.next_row() {
        let id = row.get_u64(0).unwrap();
        let name = row.get_str(1).unwrap();
        let score = row.get_u64(2).unwrap();
        println!("   Row: id={}, name={}, score={}", id, name, score);
    }
    println!();

    // -------------------------------------------------------------------------
    // 6. CSV Parser
    // -------------------------------------------------------------------------
    println!("6. CSV Parser:");

    let csv_data = b"1,alice,100\n2,bob,200\n3,charlie,300\n";
    let mut parser = CsvParser::new(csv_data);

    while let Some(row) = parser.next_row() {
        let id = row.get_u64(0).unwrap();
        let name = row.get_str(1).unwrap();
        let score = row.get_u64(2).unwrap();
        println!("   Row: id={}, name={}, score={}", id, name, score);
    }
    println!();

    // -------------------------------------------------------------------------
    // 7. CSV with Quoted Values
    // -------------------------------------------------------------------------
    println!("7. CSV with quoted values:");

    let csv_quoted = b"1,\"hello, world\",100\n2,\"O'Brien\",200\n";
    let mut parser = CsvParser::new(csv_quoted);

    while let Some(row) = parser.next_row() {
        let id = row.get_u64(0).unwrap();
        let name = row.get_str(1).unwrap();
        let score = row.get_u64(2).unwrap();
        println!("   Row: id={}, name='{}', score={}", id, name, score);
    }
    println!();

    // -------------------------------------------------------------------------
    // 8. JSON Row Parser (JSONEachRow format)
    // -------------------------------------------------------------------------
    println!("8. JSON Row Parser (JSONEachRow format):");

    let json_data = br#"{"id":1,"name":"alice","score":100}
{"id":2,"name":"bob","score":200}
"#;

    let mut parser = JsonRowParser::new(json_data);

    while let Some(result) = parser.next_row() {
        match result {
            Ok(fields) => {
                print!("   Row:");
                for (key, value) in &fields {
                    print!(" {}={}", key, value.as_str_lossy());
                }
                println!();
            }
            Err(e) => println!("   Parse error: {:?}", e),
        }
    }
    println!();

    // -------------------------------------------------------------------------
    // 9. ZeroCopyRow Methods
    // -------------------------------------------------------------------------
    println!("9. ZeroCopyRow methods:");

    let values = vec![
        BorrowedValue::new(b"42"),
        BorrowedValue::new(b"hello"),
        BorrowedValue::new(b"3.14"),
        BorrowedValue::new(b"true"),
        BorrowedValue::new(b"\\N"),
    ];
    let row = ZeroCopyRow::new(values);

    println!("   Row length: {} columns", row.len());
    println!("   get_i64(0): {:?}", row.get_i64(0));
    println!("   get_str(1): {:?}", row.get_str(1));
    println!("   get_f64(2): {:?}", row.get_f64(2));
    println!("   get_bool(3): {:?}", row.get_bool(3));
    println!("   is_null(4): {:?}", row.is_null(4));
    println!("   get(10): {:?}", row.get(10)); // Out of bounds
    println!();

    // -------------------------------------------------------------------------
    // 10. Auto-detect Parser
    // -------------------------------------------------------------------------
    println!("10. Auto-detect format:");

    let samples = [
        (b"1\t2\t3\n".as_slice(), "TSV"),
        (b"{\"a\":1}\n".as_slice(), "JSONEachRow"),
        (b"1,2,3\n".as_slice(), "CSV"),
    ];

    for (data, expected) in &samples {
        let detected = match ZeroCopyParser::auto_detect(data) {
            ZeroCopyParser::Tsv(_) => "TSV",
            ZeroCopyParser::Csv(_) => "CSV",
            ZeroCopyParser::JsonEachRow(_) => "JSONEachRow",
        };
        println!("   {:?} -> detected: {}, expected: {}",
            String::from_utf8_lossy(data).trim(), detected, expected);
    }
    println!();

    // -------------------------------------------------------------------------
    // 11. Error Handling
    // -------------------------------------------------------------------------
    println!("11. Error handling:");

    let bad_int = BorrowedValue::new(b"not_a_number");
    match bad_int.parse_i64() {
        Ok(n) => println!("   Parsed: {}", n),
        Err(ParseError::InvalidInteger) => println!("   Error: InvalidInteger (expected)"),
        Err(e) => println!("   Error: {:?}", e),
    }

    let bad_float = BorrowedValue::new(b"3.14.15");
    match bad_float.parse_f64() {
        Ok(f) => println!("   Parsed: {}", f),
        Err(ParseError::InvalidFloat) => println!("   Error: InvalidFloat (expected)"),
        Err(e) => println!("   Error: {:?}", e),
    }

    let bad_bool = BorrowedValue::new(b"maybe");
    match bad_bool.parse_bool() {
        Ok(b) => println!("   Parsed: {}", b),
        Err(ParseError::InvalidBoolean) => println!("   Error: InvalidBoolean (expected)"),
        Err(e) => println!("   Error: {:?}", e),
    }
    println!();

    // -------------------------------------------------------------------------
    // 12. Performance Comparison
    // -------------------------------------------------------------------------
    println!("12. Performance comparison:");

    // Generate test data
    let mut tsv_lines = String::new();
    for i in 0..10_000 {
        tsv_lines.push_str(&format!("{}\tuser_{}\t{}\n", i, i, i * 10));
    }
    let tsv_bytes = tsv_lines.as_bytes();

    // Zero-copy parsing
    let start = Instant::now();
    let mut sum = 0u64;
    for row in TsvParser::new(tsv_bytes) {
        if let Ok(val) = row.get_u64(2) {
            sum += val;
        }
    }
    let zero_copy_time = start.elapsed();

    // String-based parsing (simulate traditional approach)
    let start = Instant::now();
    let mut sum2 = 0u64;
    for line in tsv_lines.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() > 2 {
            if let Ok(val) = parts[2].parse::<u64>() {
                sum2 += val;
            }
        }
    }
    let string_time = start.elapsed();

    println!("   Parsed 10,000 rows:");
    println!("   - Zero-copy: {:?} (sum={})", zero_copy_time, sum);
    println!("   - String split: {:?} (sum={})", string_time, sum2);
    println!("   - Speedup: {:.2}x",
        string_time.as_nanos() as f64 / zero_copy_time.as_nanos() as f64);
    println!();

    // -------------------------------------------------------------------------
    // 13. Best Practices
    // -------------------------------------------------------------------------
    println!("13. Best practices:");
    println!();
    println!("   - Use TSV format (FORMAT TabSeparated) for best performance");
    println!("   - Process rows in streaming fashion to minimize memory");
    println!("   - Only call to_string_owned() for data you need to keep");
    println!("   - Use parse_i64/parse_u64/parse_f64 for type conversions");
    println!("   - Check is_null() before parsing nullable columns");
    println!("   - Keep response bytes alive while using BorrowedValues");
    println!("   - Use as_str_lossy() for potentially invalid UTF-8");
    println!();

    println!("=== End of Zero-Copy Parsing Example ===");
}
