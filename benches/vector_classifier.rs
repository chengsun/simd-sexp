use std::collections::HashSet;
use criterion::*;
use simd_sexp::*;
use simd_sexp::vector_classifier::{Classifier, ClassifierBuilder};

fn bench(c: &mut Criterion) {
    let accepting_chars = b" \t\r\n()\\\"";
    let lookup_tables = vector_classifier::LookupTables::from_accepting_chars(accepting_chars).unwrap();

    let mut naive_classifier: HashSet<u8> = HashSet::new();
    for &c in accepting_chars {
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
                                 let _ = black_box(accepting_chars.contains(byte));
                             }
                         }));
    let generic_classifier = vector_classifier::GenericBuilder::new().build(&lookup_tables);
    group.bench_function("generic",
                         |b| b.iter_batched(|| bytes.clone(), |mut bytes| {
                             black_box(generic_classifier.classify(&mut bytes));
                         }, BatchSize::SmallInput));
    match vector_classifier::Ssse3Builder::new() {
        None => (),
        Some(ssse3_builder) => {
            let ssse3_classifier = ssse3_builder.build(&lookup_tables);
            group.bench_function("vector",
                                 |b| b.iter_batched(|| bytes.clone(), |mut bytes| {
                                     black_box(ssse3_classifier.classify(&mut bytes));
                                 }, BatchSize::SmallInput));
        }
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
