use criterion::*;
use simd_sexp::*;

fn bench(c: &mut Criterion) {
    let input_all_parens = [b'('; 64];
    let input_all_misc = [b'x'; 64];

    let mut output_scratch = [0usize; 64];

    let mut group = c.benchmark_group("lib");
    group.throughput(Throughput::Bytes(64));
    group.bench_function("all-parens",
                         |b| b.iter(|| black_box(extract_structural_indices(&input_all_parens[..], &mut output_scratch, 0))));
    group.bench_function("all-misc",
                         |b| b.iter(|| black_box(extract_structural_indices(&input_all_misc[..], &mut output_scratch, 0))));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
