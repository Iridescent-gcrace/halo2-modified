#[macro_use]
extern crate criterion;

use crate::arithmetic::small_multiexp;
use crate::halo2curves::bn256::{Fr, G1Affine};
use crate::halo2curves::pasta::{EqAffine, Fp};
use ark_bn254::G1Affine as ark254GA;
use ark_ec::msm::VariableBaseMSM;
use ark_ec::ProjectiveCurve;
use ark_ec::short_weierstrass_jacobian::GroupAffine;
use ark_ff::{BigInteger256, PrimeField};
use group::ff::Field;
use group::prime::PrimeCurveAffine;
use group::{Curve, GroupEncoding};
use halo2_proofs::*;
use halo2curves::bn256::G1;
use std::sync::mpsc::channel;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Instant;
use msm_cuda::{multi_scalar_mult_arkworks, multi_scalar_mult_halo2curve};

use halo2_proofs::arithmetic::{best_multiexp, best_multiexp1};
use halo2_proofs::poly::{commitment::ParamsProver, ipa::commitment::ParamsIPA};

use criterion::{black_box, Criterion};
use halo2curves::{bn256, CurveAffine};
use rand_core::OsRng;

fn h2c_rand_gen(len: usize) -> Vec<bn256::G1Affine> {
    let mut h2c_ret = Vec::new();
    let mut rng = rand::thread_rng();
    for _i in 0..len {
        h2c_ret.push(bn256::G1Affine::random(&mut rng));
    }

    h2c_ret
}

fn scalar_rand_gen(len: usize) -> Vec<bn256::Fr> {
    let mut scalar_ret: Vec<bn256::Fr> = Vec::new();
    let mut rng = rand::thread_rng();
    for _i in 0..len {
        scalar_ret.push(bn256::Fr::random(&mut rng));
    }

    scalar_ret
}

fn test_small_exp(c: &mut Criterion) {
    let mut rng = OsRng;

    // {
    //     /// 数据点操作

    //     let test_scale = 1 << 1;
    //     let base_254: Vec<bn256::G1Affine> = h2c_rand_gen(test_scale);
    //     let coeff_254 = scalar_rand_gen(test_scale);
       
    //     let ret_new = best_multiexp(&coeff_254, &base_254);

    //     for _i in 0..1 {
    //         println!("坐标点{:?}：{:?}\n", _i, base_254.get(_i).unwrap().x);
    //         println!("坐标x点按位转换成bytes{:?}",base_254.get(_i).unwrap().x.to_bytes());


    //         let qq = ark_bn254::Fq::from_le_bytes_mod_order(&base_254.get(_i).unwrap().x.to_bytes());
    //         println!("坐标点经过完整转换后产生的值{:?}",qq);

    //         println!("标量值{:?}：{:?}\n", _i, coeff_254.get(_i));
    //     }

    //     let a = unsafe {
    //         std::mem::transmute::<halo2curves::bn256::G1, <halo2curves::bn256::G1Affine as PrimeCurveAffine>::Curve>(ret_new);
    //     };
        // println!("best_multiexp時长: {}", start_time_1.elapsed().as_millis());

        // let mut ark_coeff: Vec<BigInteger256> = Vec::new();
        // let mut ark_base: Vec<ark254GA> = Vec::new();

        // let start_time_2 = Instant::now();

        // for _i in 0..test_scale {

        //     let temp = coeff_254.get(_i).unwrap().to_bytes();
        //     let instant1 = Instant::now();

        //     let a = BigInteger256(transform::from_u8_to_big_int256(&temp));

        //     println!("转换1：{}",instant1.elapsed().as_nanos());
        //     let instant2 = Instant::now();

        //     let b = transform::h2c_affine_to_ark_point(*base_254.get(_i).unwrap()).into_affine();

        //     println!("转换2：{}",instant2.elapsed().as_nanos());

        //     ark_coeff.push(a);
        //     ark_base.push(b);
        // }
        
    // }

    // // 获取当前CPU的内核数（逻辑），执行多线程并行策略
    let cpu_num = num_cpus::get();
    println!("cpu_num: {:?}", cpu_num);
    // 产生测试数据集
    let test_scale = 1 << 18;
    let instant = Instant::now();
    let base_254 = Arc::new(h2c_rand_gen(test_scale));
    let coeff_254 = Arc::new(scalar_rand_gen(test_scale));
    println!("产生数据集时间：{}",instant.elapsed().as_millis());
    // let ret_254 = small_multiexp(&coeff_254.as_slice(), &base_254.as_slice());
    let start_time_1 = Instant::now();
    let ret_best = best_multiexp1(&coeff_254, &base_254);
    println!("start_time_1 {:?}",start_time_1.elapsed().as_millis());

    let ret_h2c_best = multi_scalar_mult_halo2curve(&base_254,&coeff_254);

    // assert_eq!(ret_best.to_affine(), ret_h2c_best.to_affine());


    // 开辟线程
    let mut handles = Vec::new();
    let avg = test_scale / (cpu_num * 2 / 5);
    let thread_num = test_scale / avg + 1;
    println!("avg: {:?}", avg);

    let start_time_3 = Instant::now();

    // 基本数据通道
    let (tx, rx) = channel();
    let instantX = Instant::now();
    for variable_i in 0..thread_num {
        let base_pointer = base_254.clone();
        let coeff_pointer = coeff_254.clone();
        let tx_pointer = tx.clone();

        handles.push(thread::spawn(move || {
            let lower_bound = avg * variable_i;
            let mut upper_bound = avg * (variable_i + 1);
            if upper_bound > test_scale {
                upper_bound = test_scale;
            }
            // println!("lower_bound:{:?}, upper_bound:{:?}\n", lower_bound, upper_bound);
            let instant = Instant::now();
            for _j in lower_bound..upper_bound {
                let a = BigInteger256(transform::from_u8_to_big_int256(
                    &coeff_pointer.get(_j).unwrap().to_bytes(),
                ));
                let b = transform::h2c_affine_to_ark_point(*base_pointer.get(_j).unwrap())
                    .into_affine();
                tx_pointer.send((a, b)).unwrap();
            }           
            println!("lower_bound:{:?}, upper_bound:{:?}, cost {:?}\n", lower_bound, upper_bound, instant.elapsed().as_millis());

        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    println!("数据通道时间：{}",instantX.elapsed().as_millis());
    let handle = thread::spawn(move || {
        // 设定转换接收数组
        let mut ark_coeff = Vec::with_capacity(test_scale);
        let mut ark_base = Vec::with_capacity(test_scale);
        // let mut count = 0;
        loop {
            if let Ok((coeff, base)) = rx.recv() {
                // count = count + 1;
                // if count == test_scale {
                //     break;
                // }
                ark_coeff.push(coeff);
                ark_base.push(base);
            }
            if ark_coeff.len() == test_scale && ark_base.len() == test_scale {
                break;
            }
        }
        return (ark_coeff, ark_base);
    });



    let (ark_coeff, ark_base) = handle.join().unwrap();

    println!("多线程時长: {}", start_time_3.elapsed().as_millis());

    // let ark_msm_result =
    //         VariableBaseMSM::multi_scalar_mul(&ark_base.as_slice(), &ark_coeff.as_slice());
    let start_time_3 = Instant::now();

    let gpu_result = multi_scalar_mult_arkworks(&ark_base.as_slice(), &ark_coeff.as_slice());

    println!("计算时长: {}", start_time_3.elapsed().as_millis());

    let start_time_3 = Instant::now();

    let ark_back = transform::ark_to_h2c_point(gpu_result).to_affine();

    println!("转换回时长: {}", start_time_3.elapsed().as_millis());


    assert_eq!(ret_best.to_affine(), ark_back);



    // {
    //     // 获取当前CPU的内核数（逻辑），执行多线程并行策略
    //     let cpu_num = num_cpus::get();

    //     // 产生测试数据集
    //     let test_scale = 1 << 20;
    //     let base_254 = Arc::new(h2c_rand_gen(test_scale));
    //     let coeff_254 = Arc::new(scalar_rand_gen(test_scale));

    //     // 开辟线程
    //     let avg = test_scale / (cpu_num * 2 / 5);
    //     let thread_num = test_scale / avg + 1;
    //     let mut handles = Vec::with_capacity(thread_num);

    //     // 设定转换接收数组
    //     let mut ark_coeff : Vec<BigInteger256> = Vec::with_capacity(test_scale);
    //     let mut ark_base : Vec<ark254GA> = Vec::with_capacity(test_scale);

    //     unsafe{
    //         for _i in 0..thread_num {
    //             let base_pointer = base_254.clone();
    //             let coeff_pointer = coeff_254.clone();
    //             let coeff_ptr = ark_coeff.as_mut_ptr();
    //             let base_ptr = ark_base.as_mut_ptr();
    //             handles.push(thread::spawn(move || {
    //                 let lower_bound = avg * _i;
    //                 let mut upper_bound = avg * (_i + 1);
    //                 if upper_bound > test_scale {
    //                     upper_bound = test_scale;
    //                 }
    //                 // println!("lower_bound:{:?}, upper_bound:{:?}\n", lower_bound, upper_bound);
    //                 for _j in lower_bound..upper_bound {
    //                     *coeff_ptr.add(_j) = BigInteger256(
    //                         transform::from_u8_to_big_int256(&coeff_pointer.get(_j).unwrap().to_bytes())) as BigInteger256;
    //                     *base_ptr.add(_j) = transform::h2c_affine_to_ark_point
    //                     (*base_pointer.get(_j).unwrap()).into_affine() as ark254GA;
    //                 }
    //                 })
    //             );
    //         }
    //     }

    //     for handle in handles {
    //         handle.join().unwrap();
    //     }

    // }
}

criterion_group!(benches, test_small_exp);
criterion_main!(benches);
