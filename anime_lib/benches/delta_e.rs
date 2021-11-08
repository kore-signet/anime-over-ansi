use anime_telnet::color_calc::{closest_ansi_avx, closest_ansi_scalar, closest_ansi_sse};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::prelude::*;
use std::time::Duration;

fn delta_e(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    let (r, g, b) = (rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>());
    c.bench_function("delta E - scalar", |bench| {
        bench.iter(|| black_box(closest_ansi_scalar(r, g, b)))
    });
    c.bench_function("delta E - sse (128bit)", |bench| {
        bench.iter(|| black_box(unsafe { closest_ansi_sse(r, g, b) }))
    });
    c.bench_function("delta E - avx (256bit)", |bench| {
        bench.iter(|| black_box(unsafe { closest_ansi_avx(r, g, b) }))
    });
}

criterion_group! {
    name = delta;
    config = Criterion::default().warm_up_time(Duration::from_secs(7)).measurement_time(Duration::from_secs(10));
    targets = delta_e
}

criterion_main!(delta);
