use criterion::*;
use simd_sexp::*;

fn bench(c: &mut Criterion) {
    use start_stop_transitions::StartStopTransitions;

    let a = rand::random::<u64>();
    let b = rand::random::<u64>();
    let start = a & (a ^ b);
    let stop = b & (a ^ b);
    let prev_state = rand::random();

    let mut group = c.benchmark_group("start_stop_transitions");
    group.throughput(Throughput::Bytes(8));

    let runtime_detect_start_stop_transitions = start_stop_transitions::runtime_detect();
    group.bench_function("runtime-detect",
                         |b| b.iter(|| black_box(runtime_detect_start_stop_transitions.start_stop_transitions(start, stop, prev_state))));
    let generic_start_stop_transitions = start_stop_transitions::Generic::new(clmul::Generic::new(), xor_masked_adjacent::Generic::new());
    group.bench_function("generic",
                         |b| b.iter(|| black_box(generic_start_stop_transitions.start_stop_transitions(start, stop, prev_state))));
    match start_stop_transitions::Bmi2::new() {
        None => (),
        Some(bmi2_start_stop_transitions) => {
            group.bench_function("bmi2",
                                 |b| b.iter(|| black_box(bmi2_start_stop_transitions.start_stop_transitions(start, stop, prev_state))));
        }
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
