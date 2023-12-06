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

use halo2_proofs::arithmetic::best_multiexp;
use halo2_proofs::poly::{commitment::ParamsProver, ipa::commitment::ParamsIPA};

use halo2curves::{bn256, CurveAffine};
use rand_core::OsRng;
use rustacuda::{prelude::*, launch};
use rustacuda::memory::DeviceBox;
use std::error::Error;
use std::ffi::CString;


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
fn main() -> Result<(), Box<dyn Error>> {
    let mut rng = OsRng;

    {
        

        let test_scale = 1 << 1;
        let base_254: Vec<bn256::G1Affine> = h2c_rand_gen(test_scale);
        let coeff_254 = scalar_rand_gen(test_scale);
       
        let ret_new = best_multiexp(&coeff_254, &base_254);

        for _i in 0..1 {
            println!("坐标点{:?}：{:?}\n", _i, base_254.get(_i).unwrap().x);
            println!("坐标x点按位转换成bytes{:?}",base_254.get(_i).unwrap().x.to_bytes());


            let qq = ark_bn254::Fq::from_le_bytes_mod_order(&base_254.get(_i).unwrap().x.to_bytes());
            println!("坐标点经过完整转换后产生的值{:?}",qq);

            println!("标量值{:?}：{:?}\n", _i, coeff_254.get(_i));
        }

        let a = unsafe {
            std::mem::transmute::<halo2curves::bn256::G1, <halo2curves::bn256::G1Affine as PrimeCurveAffine>::Curve>(ret_new);
        };
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
        
    }

    Ok(())
}