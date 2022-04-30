use criterion::*;
use simd_sexp::*;

fn bench(c: &mut Criterion) {
    let a = rand::random::<u64>();
    let b = rand::random::<u64>();
    let unescaped = a & (a ^ b);
    let escaped = b & (a ^ b);
    let prev_state = rand::random();

    let mut group = c.benchmark_group("find_quote_transitions");
    group.throughput(Throughput::Bytes(8));
    let generic_clmul = clmul::Generic::new();
    let generic_xor_masked_adjacent = xor_masked_adjacent::Generic::new();
    group.bench_function("generic",
                         |b| b.iter(|| black_box(find_quote_transitions::find_quote_transitions(
                             &generic_clmul, &generic_xor_masked_adjacent,
                             unescaped, escaped, prev_state))));
    let runtime_detect_clmul = clmul::runtime_detect();
    let runtime_detect_xor_masked_adjacent = xor_masked_adjacent::runtime_detect();
    group.bench_function("runtime-detect",
                         |b| b.iter(|| black_box(find_quote_transitions::find_quote_transitions(
                             &runtime_detect_clmul, &runtime_detect_xor_masked_adjacent,
                             unescaped, escaped, prev_state))));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
