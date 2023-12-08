//! This module provides common utilities, traits and structures for group,
//! field and polynomial arithmetic.

use super::multicore;
pub use ff::Field;
use group::{
    ff::{BatchInvert, PrimeField},
    Curve, Group, GroupOpsOwned, ScalarMulOwned,
    prime::PrimeCurveAffine,
};

pub use halo2curves::{CurveAffine, CurveExt, };
use halo2curves::bn256::{G1Affine, Fr};
use ark_bn254::{G1Affine as ark254GA};
use ark_ec::ProjectiveCurve;
use ark_ff::BigInteger256;
use ark_ec::msm::VariableBaseMSM;
use super::transform;
use std::sync::mpsc::channel;
use std::sync::{Arc, RwLock};
use std::thread;
use std::any::Any;
use msm_cuda::multi_scalar_mult_arkworks;

/// This represents an element of a group with basic operations that can be
/// performed. This allows an FFT implementation (for example) to operate
/// generically over either a field or elliptic curve group.
pub trait FftGroup<Scalar: Field>:
    Copy + Send + Sync + 'static + GroupOpsOwned + ScalarMulOwned<Scalar>
{
}

impl<T, Scalar> FftGroup<Scalar> for T
where
    Scalar: Field,
    T: Copy + Send + Sync + 'static + GroupOpsOwned + ScalarMulOwned<Scalar>,
{
}

fn multiexp_serial<C: CurveAffine>(coeffs: &[C::Scalar], bases: &[C], acc: &mut C::Curve) {
    let coeffs: Vec<_> = coeffs.iter().map(|a| a.to_repr()).collect();

    let c = if bases.len() < 4 {
        1
    } else if bases.len() < 32 {
        3
    } else {
        (f64::from(bases.len() as u32)).ln().ceil() as usize
    };

    fn get_at<F: PrimeField>(segment: usize, c: usize, bytes: &F::Repr) -> usize {
        let skip_bits = segment * c;
        let skip_bytes = skip_bits / 8;

        if skip_bytes >= 32 {
            return 0;
        }

        let mut v = [0; 8];
        for (v, o) in v.iter_mut().zip(bytes.as_ref()[skip_bytes..].iter()) {
            *v = *o;
        }

        let mut tmp = u64::from_le_bytes(v);
        tmp >>= skip_bits - (skip_bytes * 8);
        tmp = tmp % (1 << c);

        tmp as usize
    }

    let segments = (256 / c) + 1;

    for current_segment in (0..segments).rev() {
        for _ in 0..c {
            *acc = acc.double();
        }

        #[derive(Clone, Copy)]
        enum Bucket<C: CurveAffine> {
            None,
            Affine(C),
            Projective(C::Curve),
        }

        impl<C: CurveAffine> Bucket<C> {
            fn add_assign(&mut self, other: &C) {
                *self = match *self {
                    Bucket::None => Bucket::Affine(*other),
                    Bucket::Affine(a) => Bucket::Projective(a + *other),
                    Bucket::Projective(mut a) => {
                        a += *other;
                        Bucket::Projective(a)
                    }
                }
            }

            fn add(self, mut other: C::Curve) -> C::Curve {
                match self {
                    Bucket::None => other,
                    Bucket::Affine(a) => {
                        other += a;
                        other
                    }
                    Bucket::Projective(a) => other + &a,
                }
            }
        }

        let mut buckets: Vec<Bucket<C>> = vec![Bucket::None; (1 << c) - 1];

        for (coeff, base) in coeffs.iter().zip(bases.iter()) {
            let coeff = get_at::<C::Scalar>(current_segment, c, coeff);
            if coeff != 0 {
                buckets[coeff - 1].add_assign(base);
            }
        }

        // Summation by parts
        // e.g. 3a + 2b + 1c = a +
        //                    (a) + b +
        //                    ((a) + b) + c
        let mut running_sum = C::Curve::identity();
        for exp in buckets.into_iter().rev() {
            running_sum = exp.add(running_sum);
            *acc = *acc + &running_sum;
        }
    }
}

/// Performs a small multi-exponentiation operation.
/// Uses the double-and-add algorithm with doublings shared across points.
pub fn small_multiexp<C: CurveAffine>(coeffs: &[C::Scalar], bases: &[C]) -> C::Curve {
    let coeffs: Vec<_> = coeffs.iter().map(|a| a.to_repr()).collect();
    let mut acc = C::Curve::identity();

    // for byte idx
    for byte_idx in (0..32).rev() {
        // for bit idx
        for bit_idx in (0..8).rev() {
            acc = acc.double();
            // for each coeff
            for coeff_idx in 0..coeffs.len() {
                let byte = coeffs[coeff_idx].as_ref()[byte_idx];
                if ((byte >> bit_idx) & 1) != 0 {
                    acc += bases[coeff_idx];
                }
            }
        }
    }

    acc
}


/// This function is used for ark transform and calculation
pub fn small_multiexp_ark_254(coeffs: &[Fr], bases: &[G1Affine]) -> G1Affine {
    let mut ark_coeff : Vec<BigInteger256> = Vec::new();
    let mut ark_base : Vec<ark254GA> = Vec::new();

    for _i in 0..bases.len() {
        let temp = coeffs.get(_i).unwrap().to_bytes();
        ark_coeff.push(BigInteger256(transform::from_u8_to_big_int256(&temp)));
        ark_base.push(transform::h2c_affine_to_ark_point(*bases.get(_i).unwrap()).into_affine());
    }
    let ark_msm_result = VariableBaseMSM::multi_scalar_mul(&ark_base.as_slice(), &ark_coeff.as_slice());
    let ark_back = transform::ark_to_h2c_point(ark_msm_result).to_affine();

    ark_back
}

/// Performs a multi-exponentiation operation.
pub fn msm_trans_c_to_fr<C: CurveAffine>(
    a: &[C::Scalar]
) -> Vec<Fr> {
    let mut h = vec![];
    a.to_vec().iter().for_each(|x| {
        let x_as_any: &dyn Any = x;

        if let Some(temp )= x_as_any.downcast_ref::<Fr>(){
            h.push(*temp);
        } else{
            panic!("error in downcast");
        }
    });
    h
}
/// Performs a multi-exponentiation operation.
pub fn msm_trans_c_to_g1<C: CurveAffine>(
    a: &[C]
) -> Vec<G1Affine> {
    let mut h = vec![];
    a.to_vec().iter().for_each(|x| {
        let x_as_any: &dyn Any = x;

        if let Some(temp )= x_as_any.downcast_ref::<G1Affine>(){
            h.push(*temp);
        } else{
            panic!("error in downcast");
        }
    });
    h
}

/// Performs a multi-exponentiation operation.
pub fn msm_trans_g_to_c<C: CurveAffine>(
    a: halo2curves::bn256::G1
    ) -> C::Curve {
    let original_vec = /* 原始的 Vec */a;

    // 使用 into_iter 转换所有权并进行类型转换
    let converted_vec: C::Curve = unsafe {
    std::mem::transmute_copy(&original_vec)
    };
    converted_vec
}

/// Performs a multi-exponentiation operation.
///
/// This function will panic if coeffs and bases have a different length.
///
/// This will use multithreading if beneficial.
pub fn best_multiexp<C: CurveAffine>(coeffs: &[C::Scalar], bases: &[C]) -> C::Curve {
    // assert_eq!(coeffs.len(), bases.len());

    // // #[cfg(feature = "gpu")]
    // // 2^14次方以上调用GPU计算
    // if coeffs.len() > 16384
    // {    
    //     let cpu_num = num_cpus::get();
    //     let scale = coeffs.len();
    //     // 开辟线程
    //     let mut handles = Vec::new();
    //     let avg = scale / (cpu_num * 2 / 5);
    //     let thread_num = scale / avg + 1;

    //     // 创建可复制指针
    //     let bases_254 = Arc::new(msm_trans_c_to_g1(&bases));
    //     let coeffs_254 : Arc<Vec<Fr>> = Arc::new(msm_trans_c_to_fr::<C>(&coeffs)); 

    //     // 基本数据通道
    //     let (tx                       , rx) = channel();

    //     for variable_i in 0..thread_num {
    //         let base_pointer = bases_254.clone();
    //         let coeff_pointer = coeffs_254.clone();
    //         let tx_pointer = tx.clone();

    //         handles.push(thread::spawn(move || {
    //             let lower_bound = avg * variable_i;
    //             let mut upper_bound = avg * (variable_i + 1);
    //             if upper_bound > scale {
    //                 upper_bound = scale;
    //             }
    //             // println!("lower_bound:{:?}, upper_bound:{:?}\n", lower_bound, upper_bound);
    //             for _j in lower_bound..upper_bound {
    //                 let a = BigInteger256(transform::from_u8_to_big_int256(
    //                     &coeff_pointer.get(_j).unwrap().to_bytes(),
    //                 ));
    //                 let b = transform::h2c_affine_to_ark_point(*base_pointer.get(_j).unwrap())
    //                     .into_affine();
    //                 tx_pointer.send((a, b)).unwrap();
    //             }
    //         }));
    //     }

    //     let handle = thread::spawn(move || {
    //         // 设定转换接收数组
    //         let mut ark_coeff = Vec::with_capacity(scale);
    //         let mut ark_base = Vec::with_capacity(scale);
    //         // let mut count = 0;
    //         loop {
    //             if let Ok((coeff, base)) = rx.recv() {
    //                 // count = count + 1;
    //                 // if count == test_scale {
    //                 //     break;
    //                 // }
    //                 ark_coeff.push(coeff);
    //                 ark_base.push(base);
    //             }
    //             if ark_coeff.len() == scale && ark_base.len() == scale {
    //                 break;
    //             }
    //         }
    //         return (ark_coeff, ark_base);
    //     });

    //     for handle in handles {
    //         handle.join().unwrap();
    //     }

    //     let (ark_coeff, ark_base) = handle.join().unwrap();

    //     println!("Before Computing\n");

    //     let ark_msm_result =
    //         multi_scalar_mult_arkworks(&ark_base.as_slice(), &ark_coeff.as_slice());

    //     println!("Before\n");

    //     let ark_back = transform::ark_to_h2c_point(ark_msm_result);

    //     msm_trans_g_to_c::<C>(ark_back)
    // }  
    // else {
    //     let num_threads = multicore::current_num_threads();
    //     if coeffs.len() > num_threads {
    //         let chunk = coeffs.len() / num_threads;
    //         let num_chunks = coeffs.chunks(chunk).len();
    //         let mut results = vec![C::Curve::identity(); num_chunks];
    //         multicore::scope(|scope| {
    //             let chunk = coeffs.len() / num_threads;

    //             for ((coeffs, bases), acc) in coeffs
    //                 .chunks(chunk)
    //                 .zip(bases.chunks(chunk))
    //                 .zip(results.iter_mut())
    //             {
    //                 scope.spawn(move |_| {
    //                     multiexp_serial(coeffs, bases, acc);
    //                 });
    //             }
    //         });
    //         results.iter().fold(C::Curve::identity(), |a, b| a + b)
    //     } else {
    //         let mut acc = C::Curve::identity();
    //         multiexp_serial(coeffs, bases, &mut acc);
    //         acc
    //     }

        // // 创建可复制指针
        let bases_254 = msm_trans_c_to_g1(&bases);
        let coeffs_254 = msm_trans_c_to_fr::<C>(&coeffs); 
        let mut ark_coeff = Vec::with_capacity(coeffs.len());
        let mut ark_base = Vec::with_capacity(coeffs.len());

        for _i in 0..coeffs.len() {
            let temp = coeffs_254.get(_i).unwrap().to_bytes();
            let a = BigInteger256(transform::from_u8_to_big_int256(&temp));
            let b = transform::h2c_affine_to_ark_point(*bases_254.get(_i).unwrap()).into_affine();
            ark_coeff.push(a);
            ark_base.push(b);
        }
        let ark_msm_result =
            multi_scalar_mult_arkworks(&ark_base.as_slice(), &ark_coeff.as_slice());

        let ark_back = transform::ark_to_h2c_point(ark_msm_result);

        msm_trans_g_to_c::<C>(ark_back)
      

}

/// Performs a multi-exponentiation operation.
///
/// This function will panic if coeffs and bases have a different length.
///
/// This will use multithreading if beneficial.
pub fn best_multiexp1<C: CurveAffine>(coeffs: &[C::Scalar], bases: &[C]) -> C::Curve {
    assert_eq!(coeffs.len(), bases.len());

    let num_threads = multicore::current_num_threads();
    if coeffs.len() > num_threads {
        let chunk = coeffs.len() / num_threads;
        let num_chunks = coeffs.chunks(chunk).len();
        let mut results = vec![C::Curve::identity(); num_chunks];
        multicore::scope(|scope| {
            let chunk = coeffs.len() / num_threads;

            for ((coeffs, bases), acc) in coeffs
                .chunks(chunk)
                .zip(bases.chunks(chunk))
                .zip(results.iter_mut())
            {
                scope.spawn(move |_| {
                    multiexp_serial(coeffs, bases, acc);
                });
            }
        });
        results.iter().fold(C::Curve::identity(), |a, b| a + b)
    } else {
        let mut acc = C::Curve::identity();
        multiexp_serial(coeffs, bases, &mut acc);
        acc
    }
}


/// Performs a radix-$2$ Fast-Fourier Transformation (FFT) on a vector of size
/// $n = 2^k$, when provided `log_n` = $k$ and an element of multiplicative
/// order $n$ called `omega` ($\omega$). The result is that the vector `a`, when
/// interpreted as the coefficients of a polynomial of degree $n - 1$, is
/// transformed into the evaluations of this polynomial at each of the $n$
/// distinct powers of $\omega$. This transformation is invertible by providing
/// $\omega^{-1}$ in place of $\omega$ and dividing each resulting field element
/// by $n$.
///
/// This will use multithreading if beneficial.
pub fn best_fft
<Scalar: Field, G: FftGroup<Scalar>>
(a: &mut [G], omega: Scalar, log_n: u32) {
    fn bitreverse(mut n: usize, l: usize) -> usize {
        let mut r = 0;
        for _ in 0..l {
            r = (r << 1) | (n & 1);
            n >>= 1;
        }
        r
    }

    let threads = multicore::current_num_threads();
    let log_threads = log2_floor(threads);
    let n = a.len() as usize;
    assert_eq!(n, 1 << log_n);

    for k in 0..n {
        let rk = bitreverse(k, log_n as usize);
        if k < rk {
            a.swap(rk, k);
        }
    }

    // precompute twiddle factors
    let twiddles: Vec<_> = (0..(n / 2) as usize)
        .scan(Scalar::ONE, |w, _| {
            let tw = *w;
            *w *= &omega;
            Some(tw)
        })
        .collect();

    if log_n <= log_threads {
        let mut chunk = 2_usize;
        let mut twiddle_chunk = (n / 2) as usize;
        for _ in 0..log_n {
            a.chunks_mut(chunk).for_each(|coeffs| {
                let (left, right) = coeffs.split_at_mut(chunk / 2);

                // case when twiddle factor is one
                let (a, left) = left.split_at_mut(1);
                let (b, right) = right.split_at_mut(1);
                let t = b[0];
                b[0] = a[0];
                a[0] += &t;
                b[0] -= &t;

                left.iter_mut()
                    .zip(right.iter_mut())
                    .enumerate()
                    .for_each(|(i, (a, b))| {
                        let mut t = *b;
                        t *= &twiddles[(i + 1) * twiddle_chunk];
                        *b = *a;
                        *a += &t;
                        *b -= &t;
                    });
            });
            chunk *= 2;
            twiddle_chunk /= 2;
        }
    } else {
        recursive_butterfly_arithmetic(a, n, 1, &twiddles)
    }
}

/// This perform recursive butterfly arithmetic
pub fn recursive_butterfly_arithmetic<Scalar: Field, G: FftGroup<Scalar>>(
    a: &mut [G],
    n: usize,
    twiddle_chunk: usize,
    twiddles: &[Scalar],
) {
    if n == 2 {
        let t = a[1];
        a[1] = a[0];
        a[0] += &t;
        a[1] -= &t;
    } else {
        let (left, right) = a.split_at_mut(n / 2);
        rayon::join(
            || recursive_butterfly_arithmetic(left, n / 2, twiddle_chunk * 2, twiddles),
            || recursive_butterfly_arithmetic(right, n / 2, twiddle_chunk * 2, twiddles),
        );

        // case when twiddle factor is one
        let (a, left) = left.split_at_mut(1);
        let (b, right) = right.split_at_mut(1);
        let t = b[0];
        b[0] = a[0];
        a[0] += &t;
        b[0] -= &t;

        left.iter_mut()
            .zip(right.iter_mut())
            .enumerate()
            .for_each(|(i, (a, b))| {
                let mut t = *b;
                t *= &twiddles[(i + 1) * twiddle_chunk];
                *b = *a;
                *a += &t;
                *b -= &t;
            });
    }
}

/// Convert coefficient bases group elements to lagrange basis by inverse FFT.
pub fn g_to_lagrange<C: CurveAffine>(g_projective: Vec<C::Curve>, k: u32) -> Vec<C> {
    let n_inv = C::Scalar::TWO_INV.pow_vartime(&[k as u64, 0, 0, 0]);
    let mut omega_inv = C::Scalar::ROOT_OF_UNITY_INV;
    for _ in k..C::Scalar::S {
        omega_inv = omega_inv.square();
    }

    let mut g_lagrange_projective = g_projective;
    best_fft(&mut g_lagrange_projective, omega_inv, k);
    parallelize(&mut g_lagrange_projective, |g, _| {
        for g in g.iter_mut() {
            *g *= n_inv;
        }
    });

    let mut g_lagrange = vec![C::identity(); 1 << k];
    parallelize(&mut g_lagrange, |g_lagrange, starts| {
        C::Curve::batch_normalize(
            &g_lagrange_projective[starts..(starts + g_lagrange.len())],
            g_lagrange,
        );
    });

    g_lagrange
}

/// This evaluates a provided polynomial (in coefficient form) at `point`.
pub fn eval_polynomial<F: Field>(poly: &[F], point: F) -> F {
    fn evaluate<F: Field>(poly: &[F], point: F) -> F {
        poly.iter()
            .rev()
            .fold(F::ZERO, |acc, coeff| acc * point + coeff)
    }
    let n = poly.len();
    let num_threads = multicore::current_num_threads();
    if n * 2 < num_threads {
        evaluate(poly, point)
    } else {
        let chunk_size = (n + num_threads - 1) / num_threads;
        let mut parts = vec![F::ZERO; num_threads];
        multicore::scope(|scope| {
            for (chunk_idx, (out, poly)) in
                parts.chunks_mut(1).zip(poly.chunks(chunk_size)).enumerate()
            {
                scope.spawn(move |_| {
                    let start = chunk_idx * chunk_size;
                    out[0] = evaluate(poly, point) * point.pow_vartime(&[start as u64, 0, 0, 0]);
                });
            }
        });
        parts.iter().fold(F::ZERO, |acc, coeff| acc + coeff)
    }
}

/// This computes the inner product of two vectors `a` and `b`.
///
/// This function will panic if the two vectors are not the same size.
pub fn compute_inner_product<F: Field>(a: &[F], b: &[F]) -> F {
    // TODO: parallelize?
    assert_eq!(a.len(), b.len());

    let mut acc = F::ZERO;
    for (a, b) in a.iter().zip(b.iter()) {
        acc += (*a) * (*b);
    }

    acc
}

/// Divides polynomial `a` in `X` by `X - b` with
/// no remainder.
pub fn kate_division<'a, F: Field, I: IntoIterator<Item = &'a F>>(a: I, mut b: F) -> Vec<F>
where
    I::IntoIter: DoubleEndedIterator + ExactSizeIterator,
{
    b = -b;
    let a = a.into_iter();

    let mut q = vec![F::ZERO; a.len() - 1];

    let mut tmp = F::ZERO;
    for (q, r) in q.iter_mut().rev().zip(a.rev()) {
        let mut lead_coeff = *r;
        lead_coeff.sub_assign(&tmp);
        *q = lead_coeff;
        tmp = lead_coeff;
        tmp.mul_assign(&b);
    }

    q
}

/// This simple utility function will parallelize an operation that is to be
/// performed over a mutable slice.
pub fn parallelize<T: Send, F: Fn(&mut [T], usize) + Send + Sync + Clone>(v: &mut [T], f: F) {
    let n = v.len();
    let num_threads = multicore::current_num_threads();
    let mut chunk = (n as usize) / num_threads;
    if chunk < num_threads {
        chunk = 1;
    }

    multicore::scope(|scope| {
        for (chunk_num, v) in v.chunks_mut(chunk).enumerate() {
            let f = f.clone();
            scope.spawn(move |_| {
                let start = chunk_num * chunk;
                f(v, start);
            });
        }
    });
}

fn log2_floor(num: usize) -> u32 {
    assert!(num > 0);

    let mut pow = 0;

    while (1 << (pow + 1)) <= num {
        pow += 1;
    }

    pow
}

/// Returns coefficients of an n - 1 degree polynomial given a set of n points
/// and their evaluations. This function will panic if two values in `points`
/// are the same.
pub fn lagrange_interpolate<F: Field>(points: &[F], evals: &[F]) -> Vec<F> {
    assert_eq!(points.len(), evals.len());
    if points.len() == 1 {
        // Constant polynomial
        vec![evals[0]]
    } else {
        let mut denoms = Vec::with_capacity(points.len());
        for (j, x_j) in points.iter().enumerate() {
            let mut denom = Vec::with_capacity(points.len() - 1);
            for x_k in points
                .iter()
                .enumerate()
                .filter(|&(k, _)| k != j)
                .map(|a| a.1)
            {
                denom.push(*x_j - x_k);
            }
            denoms.push(denom);
        }
        // Compute (x_j - x_k)^(-1) for each j != i
        denoms.iter_mut().flat_map(|v| v.iter_mut()).batch_invert();

        let mut final_poly = vec![F::ZERO; points.len()];
        for (j, (denoms, eval)) in denoms.into_iter().zip(evals.iter()).enumerate() {
            let mut tmp: Vec<F> = Vec::with_capacity(points.len());
            let mut product = Vec::with_capacity(points.len() - 1);
            tmp.push(F::ONE);
            for (x_k, denom) in points
                .iter()
                .enumerate()
                .filter(|&(k, _)| k != j)
                .map(|a| a.1)
                .zip(denoms.into_iter())
            {
                product.resize(tmp.len() + 1, F::ZERO);
                for ((a, b), product) in tmp
                    .iter()
                    .chain(std::iter::once(&F::ZERO))
                    .zip(std::iter::once(&F::ZERO).chain(tmp.iter()))
                    .zip(product.iter_mut())
                {
                    *product = *a * (-denom * x_k) + *b * denom;
                }
                std::mem::swap(&mut tmp, &mut product);
            }
            assert_eq!(tmp.len(), points.len());
            assert_eq!(product.len(), points.len() - 1);
            for (final_coeff, interpolation_coeff) in final_poly.iter_mut().zip(tmp.into_iter()) {
                *final_coeff += interpolation_coeff * eval;
            }
        }
        final_poly
    }
}

pub(crate) fn evaluate_vanishing_polynomial<F: Field>(roots: &[F], z: F) -> F {
    fn evaluate<F: Field>(roots: &[F], z: F) -> F {
        roots.iter().fold(F::ONE, |acc, point| (z - point) * acc)
    }
    let n = roots.len();
    let num_threads = multicore::current_num_threads();
    if n * 2 < num_threads {
        evaluate(roots, z)
    } else {
        let chunk_size = (n + num_threads - 1) / num_threads;
        let mut parts = vec![F::ONE; num_threads];
        multicore::scope(|scope| {
            for (out, roots) in parts.chunks_mut(1).zip(roots.chunks(chunk_size)) {
                scope.spawn(move |_| out[0] = evaluate(roots, z));
            }
        });
        parts.iter().fold(F::ONE, |acc, part| acc * part)
    }
}

pub(crate) fn powers<F: Field>(base: F) -> impl Iterator<Item = F> {
    std::iter::successors(Some(F::ONE), move |power| Some(base * power))
}

#[cfg(test)]
use rand_core::OsRng;

#[cfg(test)]
use crate::halo2curves::pasta::Fp;

#[test]
fn test_lagrange_interpolate() {
    let rng = OsRng;

    let points = (0..5).map(|_| Fp::random(rng)).collect::<Vec<_>>();
    let evals = (0..5).map(|_| Fp::random(rng)).collect::<Vec<_>>();

    for coeffs in 0..5 {
        let points = &points[0..coeffs];
        let evals = &evals[0..coeffs];

        let poly = lagrange_interpolate(points, evals);
        assert_eq!(poly.len(), points.len());

        for (point, eval) in points.iter().zip(evals) {
            assert_eq!(eval_polynomial(&poly, *point), *eval);
        }
    }
}
extern crate rustacuda;

