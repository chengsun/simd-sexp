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
    group.bench_function("runtime-detect",
                         |b| b.iter(|| black_box(find_quote_transitions::find_quote_transitions(unescaped, escaped, prev_state))));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
