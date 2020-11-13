use criterion::*;
use simd_sexp::*;

fn bench(c: &mut Criterion) {
    let bitstring = rand::random::<u64>();
    let mask = rand::random::<u64>();
    let lo_fill = rand::random();

    let mut group = c.benchmark_group("xor_masked_adjacent");
    group.throughput(Throughput::Bytes(8));
    group.bench_function("runtime-detect",
                         |b| b.iter(|| black_box(xor_masked_adjacent::xor_masked_adjacent(bitstring, mask, lo_fill))));
    group.bench_function("generic",
                         |b| b.iter(|| black_box(xor_masked_adjacent::xor_masked_adjacent_generic(bitstring, mask, lo_fill))));
    group.bench_function("bmi2",
                         |b| b.iter(|| black_box(unsafe { xor_masked_adjacent::xor_masked_adjacent_bmi2(bitstring, mask, lo_fill) })));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
