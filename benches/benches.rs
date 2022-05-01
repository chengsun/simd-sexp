use std::collections::HashSet;
use criterion::*;
use simd_sexp::*;

fn bench_lib(c: &mut Criterion) {
    let input_all_parens_64 = [b'('; 64];
    let input_all_misc_64 = [b'x'; 64];
    let input_all_parens_6400 = [b'('; 6400];
    let input_all_misc_6400 = [b'x'; 6400];

    let mut output_scratch = [0usize; 6400];

    let mut group = c.benchmark_group("lib-64");
    group.throughput(Throughput::Bytes(64));
    group.bench_function("all-parens",
                         |b| b.iter(|| black_box(extract_structural_indices(&input_all_parens_64[..], &mut output_scratch, 0))));
    group.bench_function("all-misc",
                         |b| b.iter(|| black_box(extract_structural_indices(&input_all_misc_64[..], &mut output_scratch, 0))));
    group.finish();

    let mut group = c.benchmark_group("lib-6400");
    group.throughput(Throughput::Bytes(6400));
    group.bench_function("all-parens-6400",
                         |b| b.iter(|| black_box(extract_structural_indices(&input_all_parens_6400[..], &mut output_scratch, 0))));
    group.bench_function("all-misc-6400",
                         |b| b.iter(|| black_box(extract_structural_indices(&input_all_misc_6400[..], &mut output_scratch, 0))));
    group.finish();
}

fn bench_extract(c: &mut Criterion) {
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

fn bench_find_quote_transitions(c: &mut Criterion) {
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
    match (clmul::Sse2Pclmulqdq::new(), xor_masked_adjacent::Bmi2::new()) {
        (Some(clmul), Some(xor_masked_adjacent)) => {
            group.bench_function("haswell",
                                 |b| b.iter(|| black_box(find_quote_transitions::find_quote_transitions(
                                     &clmul, &xor_masked_adjacent,
                                     unescaped, escaped, prev_state))));
        },
        _ => (),
    }
    let runtime_detect_clmul = clmul::runtime_detect();
    let runtime_detect_xor_masked_adjacent = xor_masked_adjacent::runtime_detect();
    group.bench_function("runtime-detect",
                         |b| b.iter(|| black_box(find_quote_transitions::find_quote_transitions(
                             &runtime_detect_clmul, &runtime_detect_xor_masked_adjacent,
                             unescaped, escaped, prev_state))));
    group.finish();
}

fn bench_start_stop_transitions(c: &mut Criterion) {
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

fn bench_vector_classifier(c: &mut Criterion) {
    use simd_sexp::vector_classifier::{Classifier, ClassifierBuilder};
    let accepting_chars = b" \t\n";
    let lookup_tables = vector_classifier::LookupTables::from_accepting_chars(accepting_chars).unwrap();

    let mut naive_classifier: HashSet<u8> = HashSet::new();
    for &c in accepting_chars {
        naive_classifier.insert(c);
    }

    const SIZE: usize = 64000;
    let bytes: Vec<u8> = (0..SIZE).map(|_| 0u8).collect();

    let mut group = c.benchmark_group("vector_classifier");
    group.throughput(Throughput::Bytes(SIZE.try_into().unwrap()));
    group.bench_function("one-by-one-hash-set",
                         |b| b.iter(|| {
                             for byte in bytes.iter() {
                                 let _ = black_box(naive_classifier.contains(&byte));
                             }
                         }));
    group.bench_function("one-by-one",
                         |b| b.iter(|| {
                             for byte in bytes.iter() {
                                 let _ = black_box(accepting_chars.contains(&byte));
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
            group.bench_function("ssse3",
                                 |b| b.iter_batched(|| bytes.clone(), |mut bytes| {
                                     ssse3_classifier.classify(&mut bytes);
                                     black_box(bytes);
                                 }, BatchSize::SmallInput));
        }
    }
    match vector_classifier::Avx2Builder::new() {
        None => (),
        Some(avx2_builder) => {
            let avx2_classifier = avx2_builder.build(&lookup_tables);
            group.bench_function("avx2",
                                 |b| b.iter_batched(|| bytes.clone(), |mut bytes| {
                                     avx2_classifier.classify(&mut bytes);
                                     black_box(bytes);
                                 }, BatchSize::SmallInput));
        }
    }

    group.finish();
}

fn bench_xor_masked_adjacent(c: &mut Criterion) {
    use xor_masked_adjacent::XorMaskedAdjacent;

    let bitstring = rand::random::<u64>();
    let mut mask;
    loop {
        mask = rand::random::<u64>();
        if mask.count_ones() == 32 { break; }
    }
    let lo_fill = rand::random();

    let mut group = c.benchmark_group("xor_masked_adjacent");
    group.throughput(Throughput::Bytes(8));

    let generic = xor_masked_adjacent::Generic::new();
    group.bench_function("generic",
                         |b| b.iter(|| black_box(generic.xor_masked_adjacent(bitstring, mask, lo_fill))));
    let runtime_detect = xor_masked_adjacent::runtime_detect();
    group.bench_function("runtime-detect",
                         |b| b.iter(|| black_box(runtime_detect.xor_masked_adjacent(bitstring, mask, lo_fill))));
    match xor_masked_adjacent::Bmi2::new() {
        None => (),
        Some(bmi2) => {
            group.bench_function("bmi2",
                                 |b| b.iter(|| black_box(bmi2.xor_masked_adjacent(bitstring, mask, lo_fill))));
        }
    }
    group.finish();
}

criterion_group!(benches,
                 bench_lib,
                 bench_extract,
                 bench_find_quote_transitions,
                 bench_start_stop_transitions,
                 bench_vector_classifier,
                 bench_xor_masked_adjacent);

criterion_main!(benches);
