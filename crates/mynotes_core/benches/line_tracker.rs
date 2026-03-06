use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
// Adjust these to point to your actual crate/module paths!
use mynotes_core::line_tracker::{LineChunk, LineTracker, LineTrackerSummary};
use rand::prelude::*;
use rand::rngs::StdRng;
use std::hint::black_box;

/// Helper to quickly create valid chunks for benchmarking
fn make_chunk(text: &str) -> LineChunk {
    LineChunk {
        byte_length: text.len(),
        newlines: text
            .bytes()
            .enumerate()
            .filter(|(_, b)| *b == b'\n')
            .map(|(i, _)| i)
            .collect(),
    }
}

fn bench_insert(c: &mut Criterion) {
    c.bench_function("line_tracker_insert_10k_sequential", |b| {
        b.iter(|| {
            let mut tracker = LineTracker::new();
            let mut current_bytes = 0;
            let mut current_lines = 0;

            #[allow(clippy::explicit_counter_loop)]
            for _ in 0..10_000 {
                let text = "Sequential text insert\n";
                tracker.insert(
                    LineTrackerSummary {
                        byte_count: current_bytes,
                        line_count: current_lines,
                    },
                    make_chunk(text),
                );
                current_bytes += text.len();
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
                let mut total_bytes = 0;
                let mut total_lines = 0;

                #[allow(clippy::explicit_counter_loop)]
                for _ in 0..10_000 {
                    let text = "Random text insert\n";

                    let insert_byte = if total_bytes == 0 {
                        0
                    } else {
                        rng.random_range(0..=total_bytes)
                    };

                    // Calculate correct line insertion point using your exact API
                    let insert_line = if insert_byte == total_bytes {
                        total_lines
                    } else if insert_byte == 0 {
                        0
                    } else {
                        let res = tracker.find_by_byte(insert_byte).unwrap();
                        let local_byte = insert_byte - res.start_byte;
                        let local_lines = res
                            .chunk
                            .newlines
                            .iter()
                            .filter(|&&p| p < local_byte)
                            .count();
                        res.start_line + local_lines
                    };

                    tracker.insert(
                        LineTrackerSummary {
                            byte_count: insert_byte,
                            line_count: insert_line,
                        },
                        make_chunk(text),
                    );

                    total_bytes += text.len();
                    total_lines += 1;
                }
                black_box(tracker);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_queries(c: &mut Criterion) {
    // 1. Setup a massive 50k line tree outside the timer
    let mut tracker = LineTracker::new();
    let mut current_bytes = 0;

    for i in 0..50_000 {
        let text = "Editor line content here\n";
        tracker.insert(
            LineTrackerSummary {
                byte_count: current_bytes,
                line_count: i,
            },
            make_chunk(text),
        );
        current_bytes += text.len();
    }

    let target_byte = current_bytes / 2;
    let target_line = 25_000;

    let mut group = c.benchmark_group("line_tracker_queries");

    // 2. Benchmark finding by absolute byte offset
    group.bench_function("find_by_byte_middle", |b| {
        b.iter(|| {
            black_box(tracker.find_by_byte(target_byte).unwrap());
        });
    });

    // 3. Benchmark finding by line number
    group.bench_function("find_by_line_middle", |b| {
        b.iter(|| {
            black_box(tracker.find_by_line(target_line).unwrap());
        });
    });

    // 4. Benchmark resolving a line's starting byte
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
                let mut current_bytes = 0;
                for i in 0..10_000 {
                    let text = "Line to be partially deleted\n";
                    tracker.insert(
                        LineTrackerSummary {
                            byte_count: current_bytes,
                            line_count: i,
                        },
                        make_chunk(text),
                    );
                    current_bytes += text.len();
                }
                (tracker, current_bytes)
            },
            |(mut tracker, total_bytes)| {
                // Delete the entire middle 50% of the document.
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
