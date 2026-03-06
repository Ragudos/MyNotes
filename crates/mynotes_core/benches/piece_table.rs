use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
// Adjust these imports to match your actual crate structure!
use mynotes_core::piece_table::{BufferKind, PieceTable};
use rand::prelude::*;
use rand::rngs::StdRng;
use std::hint::black_box;

fn bench_insert(c: &mut Criterion) {
    c.bench_function("insert_10k_pieces", |b| {
        b.iter(|| {
            let mut table = PieceTable::new();
            // Insert sequentially.
            // (You could also write a bench for random insertions to force middle-splits!)
            for i in 0..10_000 {
                table.insert(i, i, 1, BufferKind::Add).unwrap();
            }
            // black_box prevents the compiler from optimizing away the unused table
            black_box(table);
        });
    });
}

fn bench_get(c: &mut Criterion) {
    // 1. Setup a large tree outside the timer
    let mut table = PieceTable::new();
    for i in 0..10_000 {
        table.insert(i, i, 1, BufferKind::Original).unwrap();
    }

    c.bench_function("get_piece_middle", |b| {
        b.iter(|| {
            // 2. Measure exactly how long it takes to traverse down to the middle
            // Replace `get` with whatever your actual read/query method is named
            black_box(table.get_at(5000).unwrap());
        });
    });
}

fn bench_delete(c: &mut Criterion) {
    c.bench_function("delete_triggering_merges", |b| {
        b.iter_batched(
            || {
                // Setup: Build a fresh 10k piece tree
                let mut table = PieceTable::new();
                for i in 0..10_000 {
                    table.insert(i, i, 1, BufferKind::Add).unwrap();
                }
                table
            },
            |mut table| {
                // Measurement: Delete a massive chunk from the middle.
                // This will force massive splits, underflows, and merges!
                table.delete(2500, 5000).unwrap();
                black_box(table);
            },
            // Tells Criterion that the setup phase is expensive and shouldn't be timed
            BatchSize::SmallInput,
        );
    });
}

fn bench_random_insert(c: &mut Criterion) {
    c.bench_function("insert_10k_random", |b| {
        b.iter_batched(
            || {
                let table = PieceTable::new();
                // We use a fixed seed (42) so the "randomness" is exactly
                // the same on every run. This makes the benchmark reproducible!
                let rng = StdRng::seed_from_u64(42);
                (table, rng)
            },
            |(mut table, mut rng)| {
                for (current_length, i) in (0..10_000).enumerate() {
                    // Pick a random spot between 0 and the current total length
                    let insert_pos = if current_length == 0 {
                        0
                    } else {
                        rng.random_range(0..=current_length)
                    };

                    table.insert(insert_pos, i, 1, BufferKind::Add).unwrap();
                }
                black_box(table);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_insert,
    bench_get,
    bench_delete,
    bench_random_insert
);
criterion_main!(benches);
