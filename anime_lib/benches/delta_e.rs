use anime_telnet::color_calc::{closest_ansi_avx, closest_ansi_scalar, closest_ansi_sse};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::prelude::*;
use std::time::Duration;

fn delta_e(c: &mut Criterion) {
    let mut group = c.benchmark_group("delta E");
    let mut rng = rand::thread_rng();
    let (r, g, b) = (rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>());

    group.bench_function("scalar", |bench| {
        bench.iter(|| black_box(closest_ansi_scalar(r, g, b)))
    });
    group.bench_function("sse (128bit)", |bench| {
        bench.iter(|| black_box(unsafe { closest_ansi_sse(r, g, b) }))
    });
    group.bench_function("avx (256bit)", |bench| {
        bench.iter(|| black_box(unsafe { closest_ansi_avx(r, g, b) }))
    });

    group.finish();
}

criterion_group! {
    name = delta;
    config = Criterion::default().sample_size(1_000).warm_up_time(Duration::from_secs(7)).measurement_time(Duration::from_secs(12));
    targets = delta_e
}

criterion_main!(delta);
