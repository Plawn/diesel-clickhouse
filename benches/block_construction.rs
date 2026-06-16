//! Benchmark comparing block construction approaches:
//! - N-pass with collect() (vectorizable)
//! - 1-pass with push() (better cache locality for rows)

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use clickhouse_rs::Block;

// =============================================================================
// Test Data Structures
// =============================================================================

/// Row with only primitive types (best case for vectorization)
#[derive(Clone)]
struct PrimitiveRow {
    id: u64,
    count: u32,
    value: f64,
    flag: bool,
    score: i64,
}

/// Row with String fields (requires cloning in borrow case)
#[derive(Clone)]
struct MixedRow {
    id: u64,
    name: String,
    email: String,
    count: u32,
    active: bool,
}

/// Row with many fields (tests scaling)
#[derive(Clone)]
struct WideRow {
    f1: u64, f2: u64, f3: u64, f4: u64,
    f5: u32, f6: u32, f7: u32, f8: u32,
    f9: f64, f10: f64, f11: f64, f12: f64,
    f13: bool, f14: bool, f15: bool, f16: bool,
}

// =============================================================================
// Approach 1: N-pass with collect() - Vectorizable
// =============================================================================

fn primitive_rows_to_block_collect(rows: &[PrimitiveRow]) -> Block {
    let col_id: Vec<u64> = rows.iter().map(|r| r.id).collect();
    let col_count: Vec<u32> = rows.iter().map(|r| r.count).collect();
    let col_value: Vec<f64> = rows.iter().map(|r| r.value).collect();
    let col_flag: Vec<u8> = rows.iter().map(|r| r.flag as u8).collect();
    let col_score: Vec<i64> = rows.iter().map(|r| r.score).collect();

    Block::new()
        .column("id", col_id)
        .column("count", col_count)
        .column("value", col_value)
        .column("flag", col_flag)
        .column("score", col_score)
}

fn mixed_rows_to_block_collect(rows: &[MixedRow]) -> Block {
    let col_id: Vec<u64> = rows.iter().map(|r| r.id).collect();
    let col_name: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    let col_email: Vec<String> = rows.iter().map(|r| r.email.clone()).collect();
    let col_count: Vec<u32> = rows.iter().map(|r| r.count).collect();
    let col_active: Vec<u8> = rows.iter().map(|r| r.active as u8).collect();

    Block::new()
        .column("id", col_id)
        .column("name", col_name)
        .column("email", col_email)
        .column("count", col_count)
        .column("active", col_active)
}

fn wide_rows_to_block_collect(rows: &[WideRow]) -> Block {
    let col_f1: Vec<u64> = rows.iter().map(|r| r.f1).collect();
    let col_f2: Vec<u64> = rows.iter().map(|r| r.f2).collect();
    let col_f3: Vec<u64> = rows.iter().map(|r| r.f3).collect();
    let col_f4: Vec<u64> = rows.iter().map(|r| r.f4).collect();
    let col_f5: Vec<u32> = rows.iter().map(|r| r.f5).collect();
    let col_f6: Vec<u32> = rows.iter().map(|r| r.f6).collect();
    let col_f7: Vec<u32> = rows.iter().map(|r| r.f7).collect();
    let col_f8: Vec<u32> = rows.iter().map(|r| r.f8).collect();
    let col_f9: Vec<f64> = rows.iter().map(|r| r.f9).collect();
    let col_f10: Vec<f64> = rows.iter().map(|r| r.f10).collect();
    let col_f11: Vec<f64> = rows.iter().map(|r| r.f11).collect();
    let col_f12: Vec<f64> = rows.iter().map(|r| r.f12).collect();
    let col_f13: Vec<u8> = rows.iter().map(|r| r.f13 as u8).collect();
    let col_f14: Vec<u8> = rows.iter().map(|r| r.f14 as u8).collect();
    let col_f15: Vec<u8> = rows.iter().map(|r| r.f15 as u8).collect();
    let col_f16: Vec<u8> = rows.iter().map(|r| r.f16 as u8).collect();

    Block::new()
        .column("f1", col_f1)
        .column("f2", col_f2)
        .column("f3", col_f3)
        .column("f4", col_f4)
        .column("f5", col_f5)
        .column("f6", col_f6)
        .column("f7", col_f7)
        .column("f8", col_f8)
        .column("f9", col_f9)
        .column("f10", col_f10)
        .column("f11", col_f11)
        .column("f12", col_f12)
        .column("f13", col_f13)
        .column("f14", col_f14)
        .column("f15", col_f15)
        .column("f16", col_f16)
}

// =============================================================================
// Approach 2: 1-pass with push() - Better cache locality
// =============================================================================

fn primitive_rows_to_block_single_pass(rows: &[PrimitiveRow]) -> Block {
    let capacity = rows.len();
    let mut col_id = Vec::with_capacity(capacity);
    let mut col_count = Vec::with_capacity(capacity);
    let mut col_value = Vec::with_capacity(capacity);
    let mut col_flag = Vec::with_capacity(capacity);
    let mut col_score = Vec::with_capacity(capacity);

    for row in rows {
        col_id.push(row.id);
        col_count.push(row.count);
        col_value.push(row.value);
        col_flag.push(row.flag as u8);
        col_score.push(row.score);
    }

    Block::new()
        .column("id", col_id)
        .column("count", col_count)
        .column("value", col_value)
        .column("flag", col_flag)
        .column("score", col_score)
}

fn mixed_rows_to_block_single_pass(rows: &[MixedRow]) -> Block {
    let capacity = rows.len();
    let mut col_id = Vec::with_capacity(capacity);
    let mut col_name = Vec::with_capacity(capacity);
    let mut col_email = Vec::with_capacity(capacity);
    let mut col_count = Vec::with_capacity(capacity);
    let mut col_active = Vec::with_capacity(capacity);

    for row in rows {
        col_id.push(row.id);
        col_name.push(row.name.clone());
        col_email.push(row.email.clone());
        col_count.push(row.count);
        col_active.push(row.active as u8);
    }

    Block::new()
        .column("id", col_id)
        .column("name", col_name)
        .column("email", col_email)
        .column("count", col_count)
        .column("active", col_active)
}

fn wide_rows_to_block_single_pass(rows: &[WideRow]) -> Block {
    let capacity = rows.len();
    let mut col_f1 = Vec::with_capacity(capacity);
    let mut col_f2 = Vec::with_capacity(capacity);
    let mut col_f3 = Vec::with_capacity(capacity);
    let mut col_f4 = Vec::with_capacity(capacity);
    let mut col_f5 = Vec::with_capacity(capacity);
    let mut col_f6 = Vec::with_capacity(capacity);
    let mut col_f7 = Vec::with_capacity(capacity);
    let mut col_f8 = Vec::with_capacity(capacity);
    let mut col_f9 = Vec::with_capacity(capacity);
    let mut col_f10 = Vec::with_capacity(capacity);
    let mut col_f11 = Vec::with_capacity(capacity);
    let mut col_f12 = Vec::with_capacity(capacity);
    let mut col_f13 = Vec::with_capacity(capacity);
    let mut col_f14 = Vec::with_capacity(capacity);
    let mut col_f15 = Vec::with_capacity(capacity);
    let mut col_f16 = Vec::with_capacity(capacity);

    for row in rows {
        col_f1.push(row.f1);
        col_f2.push(row.f2);
        col_f3.push(row.f3);
        col_f4.push(row.f4);
        col_f5.push(row.f5);
        col_f6.push(row.f6);
        col_f7.push(row.f7);
        col_f8.push(row.f8);
        col_f9.push(row.f9);
        col_f10.push(row.f10);
        col_f11.push(row.f11);
        col_f12.push(row.f12);
        col_f13.push(row.f13 as u8);
        col_f14.push(row.f14 as u8);
        col_f15.push(row.f15 as u8);
        col_f16.push(row.f16 as u8);
    }

    Block::new()
        .column("f1", col_f1)
        .column("f2", col_f2)
        .column("f3", col_f3)
        .column("f4", col_f4)
        .column("f5", col_f5)
        .column("f6", col_f6)
        .column("f7", col_f7)
        .column("f8", col_f8)
        .column("f9", col_f9)
        .column("f10", col_f10)
        .column("f11", col_f11)
        .column("f12", col_f12)
        .column("f13", col_f13)
        .column("f14", col_f14)
        .column("f15", col_f15)
        .column("f16", col_f16)
}

// =============================================================================
// Approach 3: Owned version (1-pass, no cloning)
// =============================================================================

fn mixed_rows_into_block_owned(rows: Vec<MixedRow>) -> Block {
    let capacity = rows.len();
    let mut col_id = Vec::with_capacity(capacity);
    let mut col_name = Vec::with_capacity(capacity);
    let mut col_email = Vec::with_capacity(capacity);
    let mut col_count = Vec::with_capacity(capacity);
    let mut col_active = Vec::with_capacity(capacity);

    for row in rows {
        col_id.push(row.id);
        col_name.push(row.name);  // Move, no clone!
        col_email.push(row.email); // Move, no clone!
        col_count.push(row.count);
        col_active.push(row.active as u8);
    }

    Block::new()
        .column("id", col_id)
        .column("name", col_name)
        .column("email", col_email)
        .column("count", col_count)
        .column("active", col_active)
}

// =============================================================================
// Data Generation
// =============================================================================

fn generate_primitive_rows(n: usize) -> Vec<PrimitiveRow> {
    (0..n)
        .map(|i| PrimitiveRow {
            id: i as u64,
            count: (i % 1000) as u32,
            value: i as f64 * 0.5,
            flag: i % 2 == 0,
            score: i as i64 - 500,
        })
        .collect()
}

fn generate_mixed_rows(n: usize) -> Vec<MixedRow> {
    (0..n)
        .map(|i| MixedRow {
            id: i as u64,
            name: format!("user_{}", i),
            email: format!("user_{}@example.com", i),
            count: (i % 1000) as u32,
            active: i % 2 == 0,
        })
        .collect()
}

fn generate_wide_rows(n: usize) -> Vec<WideRow> {
    (0..n)
        .map(|i| WideRow {
            f1: i as u64,
            f2: i as u64 + 1,
            f3: i as u64 + 2,
            f4: i as u64 + 3,
            f5: i as u32,
            f6: i as u32 + 1,
            f7: i as u32 + 2,
            f8: i as u32 + 3,
            f9: i as f64,
            f10: i as f64 + 0.1,
            f11: i as f64 + 0.2,
            f12: i as f64 + 0.3,
            f13: i % 2 == 0,
            f14: i % 3 == 0,
            f15: i % 5 == 0,
            f16: i % 7 == 0,
        })
        .collect()
}

// =============================================================================
// Benchmarks
// =============================================================================

fn bench_primitive_rows(c: &mut Criterion) {
    let mut group = c.benchmark_group("primitive_rows");

    for size in [100, 1_000, 10_000, 100_000] {
        let rows = generate_primitive_rows(size);
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("collect", size),
            &rows,
            |b, rows| b.iter(|| primitive_rows_to_block_collect(black_box(rows))),
        );

        group.bench_with_input(
            BenchmarkId::new("single_pass", size),
            &rows,
            |b, rows| b.iter(|| primitive_rows_to_block_single_pass(black_box(rows))),
        );
    }

    group.finish();
}

fn bench_mixed_rows(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_rows");

    for size in [100, 1_000, 10_000, 100_000] {
        let rows = generate_mixed_rows(size);
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("collect", size),
            &rows,
            |b, rows| b.iter(|| mixed_rows_to_block_collect(black_box(rows))),
        );

        group.bench_with_input(
            BenchmarkId::new("single_pass", size),
            &rows,
            |b, rows| b.iter(|| mixed_rows_to_block_single_pass(black_box(rows))),
        );

        // For owned version, we need to clone the data each iteration
        group.bench_with_input(
            BenchmarkId::new("owned", size),
            &rows,
            |b, rows| b.iter(|| mixed_rows_into_block_owned(black_box(rows.clone()))),
        );
    }

    group.finish();
}

fn bench_wide_rows(c: &mut Criterion) {
    let mut group = c.benchmark_group("wide_rows");

    for size in [100, 1_000, 10_000, 100_000] {
        let rows = generate_wide_rows(size);
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("collect", size),
            &rows,
            |b, rows| b.iter(|| wide_rows_to_block_collect(black_box(rows))),
        );

        group.bench_with_input(
            BenchmarkId::new("single_pass", size),
            &rows,
            |b, rows| b.iter(|| wide_rows_to_block_single_pass(black_box(rows))),
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_primitive_rows,
    bench_mixed_rows,
    bench_wide_rows
);

criterion_main!(benches);
