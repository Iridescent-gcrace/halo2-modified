#[macro_use]
extern crate criterion;

use crate::arithmetic::best_fft;
use group::ff::Field;
use halo2_proofs::*;
use halo2curves::bn256::Fq;

use criterion::{BenchmarkId, Criterion};
use rand_core::OsRng;

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft");
    for k in 18..19 {
        group.bench_function(BenchmarkId::new("k", k), |b| {
            let mut a = (0..(1 << k)).map(|_| Fq::random(OsRng)).collect::<Vec<_>>();
            let omega = Fq::random(OsRng); // would be weird if this mattered
            b.iter(|| {
                best_fft(&mut a, omega, k as u32);
            });
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
