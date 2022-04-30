use criterion::*;
use simd_sexp::*;

fn bench(c: &mut Criterion) {
    use xor_masked_adjacent::XorMaskedAdjacent;

    let bitstring = rand::random::<u64>();
    let mask = rand::random::<u64>();
    let lo_fill = rand::random();

    let mut group = c.benchmark_group("xor_masked_adjacent");
    group.throughput(Throughput::Bytes(8));

    let generic = xor_masked_adjacent::Generic::new();
    group.bench_function("generic",
                         |b| b.iter(|| black_box(generic.xor_masked_adjacent(bitstring, mask, lo_fill))));
    match xor_masked_adjacent::Bmi2::new() {
        None => (),
        Some(bmi2) => {
            group.bench_function("bmi2",
                                 |b| b.iter(|| black_box(bmi2.xor_masked_adjacent(bitstring, mask, lo_fill))));
        }
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
