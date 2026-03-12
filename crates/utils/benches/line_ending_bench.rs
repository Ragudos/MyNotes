use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use utils::line_ending::create_line_ending;

fn bench_calculate_score_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("Creating Line Ending from Different Sizes");

    // 1 KB, 100 KB, 1 MB, and 10 MB
    let sizes = [1024, 100 * 1024, 1024 * 1024, 10 * 1024 * 1024];

    for size in sizes {
        let base_pattern = "a\r\nb\nc\rd";
        let repeat_count = size / base_pattern.len();
        let text = base_pattern.repeat(repeat_count);
        let bytes = text.as_bytes();

        // Tell Criterion how much data we are processing to get MB/s metrics
        group.throughput(Throughput::Bytes(bytes.len() as u64));

        // Benchmark this specific size
        group.bench_with_input(
            BenchmarkId::new("create_line_ending", format!("{} bytes", size)),
            &bytes,
            |b, input_bytes| {
                b.iter(|| {
                    // Call the public function instead
                    create_line_ending(black_box(input_bytes))
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_calculate_score_sizes);
criterion_main!(benches);
