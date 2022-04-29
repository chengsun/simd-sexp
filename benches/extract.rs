use criterion::*;
use simd_sexp::*;

fn bench(c: &mut Criterion) {
    let mut output_scratch = [0usize; 1024];

    let mut group = c.benchmark_group("extract-64");
    group.throughput(Throughput::Bytes(64));
    group.bench_function("fast-ones",
                         |b| b.iter(|| black_box(extract::fast(&mut output_scratch, 0, !0u64))));
    group.bench_function("fast-zeros",
                         |b| b.iter(|| black_box(extract::fast(&mut output_scratch, 0, 0u64))));
    group.bench_function("safe-ones",
                         |b| b.iter(|| black_box(extract::safe(&mut output_scratch, 0, !0u64))));
    group.bench_function("safe-zeros",
                         |b| b.iter(|| black_box(extract::safe(&mut output_scratch, 0, 0u64))));
    group.finish();

    let mut group = c.benchmark_group("extract-1024");
    group.throughput(Throughput::Bytes(1024));
    group.bench_function("fast-ones",
                         |b| b.iter(|| {
                             let mut offset = 0;
                             let mut output_i = 0;
                             for _ in 0..16 {
                                 output_i += extract::fast(&mut output_scratch[output_i..], offset, !0u64);
                                 offset += 64;
                             }
                             let _ = black_box(output_i);
                         } ));
    group.bench_function("fast-zeros",
                         |b| b.iter(|| {
                             let mut offset = 0;
                             let mut output_i = 0;
                             for _ in 0..16 {
                                 output_i += extract::fast(&mut output_scratch[output_i..], offset, 0u64);
                                 offset += 64;
                             }
                             let _ = black_box(output_i);
                         } ));
    group.bench_function("safe-ones",
                         |b| b.iter(|| {
                             let mut offset = 0;
                             let mut output_i = 0;
                             for _ in 0..16 {
                                 output_i += extract::safe(&mut output_scratch[output_i..], offset, !0u64);
                                 offset += 64;
                             }
                             let _ = black_box(output_i);
                         } ));
    group.bench_function("safe-zeros",
                         |b| b.iter(|| {
                             let mut offset = 0;
                             let mut output_i = 0;
                             for _ in 0..16 {
                                 output_i += extract::safe(&mut output_scratch[output_i..], offset, 0u64);
                                 offset += 64;
                             }
                             let _ = black_box(output_i);
                         } ));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
