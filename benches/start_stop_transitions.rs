use criterion::*;
use simd_sexp::*;

fn bench(c: &mut Criterion) {
    let a = rand::random::<u64>();
    let b = rand::random::<u64>();
    let start = a & (a ^ b);
    let stop = b & (a ^ b);
    let prev_state = rand::random();

    let mut group = c.benchmark_group("start_stop_transitions");
    group.throughput(Throughput::Bytes(8));
    group.bench_function("runtime-detect",
                         |b| b.iter(|| black_box(start_stop_transitions::start_stop_transitions(start, stop, prev_state))));
    group.bench_function("generic",
                         |b| b.iter(|| black_box(start_stop_transitions::start_stop_transitions_generic(start, stop, prev_state))));
    group.bench_function("bmi2",
                         |b| b.iter(|| black_box(unsafe { start_stop_transitions::start_stop_transitions_bmi2(start, stop, prev_state) })));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
