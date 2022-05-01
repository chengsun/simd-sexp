use criterion::*;
use simd_sexp::*;

fn bench(c: &mut Criterion) {
    let input_all_parens_64 = [b'('; 64];
    let input_all_misc_64 = [b'x'; 64];
    let input_all_parens_6400 = [b'('; 6400];
    let input_all_misc_6400 = [b'x'; 6400];

    let mut output_scratch = [0usize; 6400];

    let mut group = c.benchmark_group("lib-64");
    group.throughput(Throughput::Bytes(64));
    group.bench_function("all-parens",
                         |b| b.iter(|| black_box(extract_structural_indices(&input_all_parens_64[..], &mut output_scratch, 0))));
    group.bench_function("all-misc",
                         |b| b.iter(|| black_box(extract_structural_indices(&input_all_misc_64[..], &mut output_scratch, 0))));
    group.finish();

    let mut group = c.benchmark_group("lib-6400");
    group.throughput(Throughput::Bytes(6400));
    group.bench_function("all-parens-6400",
                         |b| b.iter(|| black_box(extract_structural_indices(&input_all_parens_6400[..], &mut output_scratch, 0))));
    group.bench_function("all-misc-6400",
                         |b| b.iter(|| black_box(extract_structural_indices(&input_all_misc_6400[..], &mut output_scratch, 0))));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
