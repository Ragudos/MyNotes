use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
// Adjust these to point to your actual crate/module paths!
use mynotes_core::line_tracker::{LineChunk, LineTracker, LineTrackerSummary};
use rand::prelude::*;
use rand::rngs::StdRng;
use std::hint::black_box;

/// Helper to quickly create valid chunks for benchmarking
fn make_chunk(text: &str) -> LineChunk {
    LineChunk {
        // Safely convert usize to u32, panicking if the text is somehow > 4GB
        byte_length: u32::try_from(text.len()).expect("Chunk length exceeds u32::MAX"),
        newlines: text
            .bytes()
            .enumerate()
            .filter(|(_, b)| *b == b'\n')
            .map(|(i, _)| u32::try_from(i).expect("Newline offset exceeds u32::MAX"))
            .collect(),
    }
}

fn bench_insert(c: &mut Criterion) {
    c.bench_function("line_tracker_insert_10k_sequential", |b| {
        b.iter(|| {
            let mut tracker = LineTracker::new();
            let mut current_bytes: u64 = 0;
            let mut current_lines: u64 = 0;

            #[allow(clippy::explicit_counter_loop)]
            for _ in 0..10_000 {
                let text = "Sequential text insert\n";
                tracker.insert(
                    LineTrackerSummary::new(current_bytes, current_lines),
                    make_chunk(text),
                );
                current_bytes += text.len() as u64;
                current_lines += 1;
            }
            black_box(tracker);
        });
    });
}

fn bench_random_insert(c: &mut Criterion) {
    c.bench_function("line_tracker_insert_10k_random", |b| {
        b.iter_batched(
            || {
                let tracker = LineTracker::new();
                let rng = StdRng::seed_from_u64(42);
                (tracker, rng)
            },
            |(mut tracker, mut rng)| {
                let mut total_bytes: u64 = 0;
                let mut total_lines: u64 = 0;

                #[allow(clippy::explicit_counter_loop)]
                for _ in 0..10_000 {
                    let text = "Random text insert\n";

                    let insert_byte = if total_bytes == 0 {
                        0
                    } else {
                        rng.random_range(0..=total_bytes)
                    };

                    let insert_line = if insert_byte == total_bytes {
                        total_lines
                    } else if insert_byte == 0 {
                        0
                    } else {
                        let res = tracker.find_by_byte(insert_byte).unwrap();
                        let local_byte = insert_byte - res.start_byte;

                        // Safely cast the local offset to u32 for chunk-internal comparison
                        let local_byte_u32 =
                            u32::try_from(local_byte).expect("Local offset exceeds u32::MAX");

                        let local_lines = res
                            .chunk
                            .newlines
                            .iter()
                            .filter(|&&p| p < local_byte_u32)
                            .count() as u64;

                        res.start_line + local_lines
                    };

                    tracker.insert(
                        LineTrackerSummary::new(insert_byte, insert_line),
                        make_chunk(text),
                    );

                    total_bytes += text.len() as u64;
                    total_lines += 1;
                }
                black_box(tracker);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_queries(c: &mut Criterion) {
    let mut tracker = LineTracker::new();
    let mut current_bytes: u64 = 0;

    for i in 0..50_000 {
        let text = "Editor line content here\n";
        tracker.insert(
            LineTrackerSummary::new(current_bytes, i as u64),
            make_chunk(text),
        );
        current_bytes += text.len() as u64;
    }

    let target_byte = current_bytes / 2;
    let target_line: u64 = 25_000;

    let mut group = c.benchmark_group("line_tracker_queries");

    group.bench_function("find_by_byte_middle", |b| {
        b.iter(|| {
            black_box(tracker.find_by_byte(target_byte).unwrap());
        });
    });

    group.bench_function("find_by_line_middle", |b| {
        b.iter(|| {
            black_box(tracker.find_by_line(target_line).unwrap());
        });
    });

    group.bench_function("byte_offset_of_line_middle", |b| {
        b.iter(|| {
            black_box(tracker.byte_offset_of_line(target_line).unwrap());
        });
    });

    group.finish();
}

fn bench_delete(c: &mut Criterion) {
    c.bench_function("line_tracker_delete_massive_range", |b| {
        b.iter_batched(
            || {
                let mut tracker = LineTracker::new();
                let mut current_bytes: u64 = 0;
                for i in 0..10_000 {
                    let text = "Line to be partially deleted\n";
                    tracker.insert(
                        LineTrackerSummary::new(current_bytes, i as u64),
                        make_chunk(text),
                    );
                    current_bytes += text.len() as u64;
                }
                (tracker, current_bytes)
            },
            |(mut tracker, total_bytes)| {
                let start_byte = total_bytes / 4;
                let end_byte = total_bytes - (total_bytes / 4);

                tracker.delete_range(start_byte, end_byte);
                black_box(tracker);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_insert,
    bench_random_insert,
    bench_queries,
    bench_delete
);
criterion_main!(benches);
