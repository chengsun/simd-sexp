use std::collections::HashSet;
use criterion::*;
use simd_sexp::vector_classifier::*;

fn bench(c: &mut Criterion) {
    let accept_chars = " \t\r\n()\\\"".as_bytes();

    let accept: Vec<bool> = (0u8..=255).map(|i| {
        accept_chars.contains(&i)
    }).collect();
    let classifier = VectorClassifier::new(&accept[..]).unwrap();

    let mut naive_classifier: HashSet<u8> = HashSet::new();
    for &c in accept_chars {
        naive_classifier.insert(c);
    }

    const SIZE: usize = 1000;
    let bytes = &[0; SIZE];

    let mut group = c.benchmark_group("vector_classifier");
    group.throughput(Throughput::Bytes(SIZE.try_into().unwrap()));
    group.bench_function("one-by-one-hash-set",
                         |b| b.iter(|| {
                             for byte in bytes {
                                 let _ = black_box(naive_classifier.contains(byte));
                             }
                         }));
    group.bench_function("one-by-one",
                         |b| b.iter(|| {
                             for byte in bytes {
                                 let _ = black_box(accept_chars.contains(byte));
                             }
                         }));
    group.bench_function("generic",
                         |b| b.iter_batched(|| bytes.clone(), |mut bytes| {
                             black_box(classifier.classify_generic(&mut bytes));
                         }, BatchSize::SmallInput));
    group.bench_function("vector",
                         |b| b.iter_batched(|| bytes.clone(), |mut bytes| {
                             black_box(classifier.classify(&mut bytes));
                         }, BatchSize::SmallInput));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
