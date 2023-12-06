use ark_ec::{AffineCurve, ProjectiveCurve, msm::VariableBaseMSM};
use ark_ec::models::short_weierstrass_jacobian::GroupAffine;
use ark_ff::{BigInteger256, fields::*, BigInteger, PrimeField};
use ark_bn254::{G1Affine as ark254GA, Fr, G1Projective as ark254GP};
use ark_std::{UniformRand, Zero};
use group::Group;
use core::panic;
use std::ops::Add;
use std::time::Instant;
use halo2curves::{bn256::G1, serde::SerdeObject, bn256::G1Affine as h256GA};
use halo2curves::{CurveAffine, CurveExt, CurveAffineExt};
use group::{prime::PrimeCurveAffine, GroupEncoding, Curve};
use rand_core::OsRng;
use rand::Rng;
use halo2_proofs::transform::{self, h2c_affine_to_ark_point};

fn ark_rand_gen_points<G: AffineCurve>(len: usize) -> Vec<G> {
    let rand_gen: usize = 1 << 12;
    let mut rng = rand::thread_rng();;

    let mut points =
    <G::Projective as ProjectiveCurve>::batch_normalization_into_affine(
        &(0..rand_gen)
            .map(|_| G::Projective::rand(&mut rng))
            .collect::<Vec<_>>(),
    );

    while points.len() < len {
        points.append(&mut points.clone());
    }

    points.truncate(len);
    points
}

fn h2c_rand_gen(len: usize) -> Vec<h256GA> {
    let mut h2c_ret : Vec<h256GA> = Vec::new();
    let mut rng = rand::thread_rng();
    for _i in 0..len {
        h2c_ret.push(h256GA::random(&mut rng));
    }

    h2c_ret
}

fn main() {
    // 无穷远点测试
    let h2c_inf_point = G1::identity();
    let ark_inf_point = transform::h2c_affine_to_ark_point(h2c_inf_point.to_affine());
    assert!(ark_inf_point.is_zero());
    println!("是否为无穷远点?{:?}", ark_inf_point.is_zero());

    // 点转换测试
    let mut rng = rand::thread_rng();
    let ark_point = ark254GP::rand(& mut rng);
    let h2c_point = transform::ark_to_h2c_point(ark_point);
    assert_eq!(ark_point, transform::h2c_to_ark_point(h2c_point));

    assert_eq!(ark_inf_point + ark_point, ark_point);

    // 点加测试
    let test_times = 1;
    for _i in 0..test_times {
        let mut rng = rand::thread_rng();

        let ark_point_1 = ark254GP::rand(& mut rng);
        let ark_point_2 = ark254GP::rand(& mut rng);
    
        let ark_sum = ark_point_1 + ark_point_2;
    
        let h2c_point_1 = transform::ark_to_h2c_point(ark_point_1);
        let h2c_point_2 = transform::ark_to_h2c_point(ark_point_2);
    
        let h2c_point_sum = h2c_point_1 + h2c_point_2;
    
        let ark_trans_from_h2c = transform::h2c_to_ark_point(h2c_point_sum);
    
        assert_eq!(ark_sum, ark_trans_from_h2c);
    }
    // println!("{:?}次点加测试通过", test_times);

    // 倍点测试
    for _i in 0..test_times {
        let mut rng = rand::thread_rng();

        let ark_point = ark254GP::rand(&mut rng);
        let ark_double = ark_point + ark_point;

        let h2c_point = transform::ark_to_h2c_point(ark_point);
        let h2c_double = h2c_point + h2c_point;

        let h2c_trans_from_ark = transform::ark_to_h2c_point(ark_double);

        assert_eq!(h2c_double, h2c_trans_from_ark);
    }
    // println!("{:?}次倍点测试通过", test_times);

    // 数乘测试
    let mut rng = rand::thread_rng();
    let mut range : u8 = rng.gen();
    let ark_point = ark254GP::rand(&mut rng);
    let h2c_point = transform::ark_to_h2c_point(ark_point);
    let mut ark_sum = ark_point;
    let mut h2c_sum = h2c_point;

    for _i in 0..range {
        ark_sum += ark_point;
        h2c_sum += h2c_point;
        assert_eq!(h2c_sum, transform::ark_to_h2c_point(ark_sum));
    }
    println!("数乘测试通过，测试倍数为: {:?}", range);

    // MSM测试
    let mut h2c_coeff : Vec<u8> = Vec::new();
    let mut h2c_base : Vec<h256GA> = Vec::new();
    let test_scale = 1 << 10;

    for _i in 0..test_scale {
        h2c_coeff.push(rng.gen());
    }
    h2c_base = h2c_rand_gen(test_scale);

    let mut ark_coeff : Vec<BigInteger256> = Vec::new();
    let mut ark_base : Vec<ark254GA> = Vec::new();

    for _i in 0..test_scale {
        let mut tmp = [0u8; 32];
        tmp[0] = *h2c_coeff.get(_i).unwrap();
        ark_coeff.push(BigInteger256(transform::from_u8_to_big_int256(&tmp)));
        ark_base.push(h2c_affine_to_ark_point(*h2c_base.get(_i).unwrap()).into_affine());
    }

    let ark_msm_result = VariableBaseMSM::multi_scalar_mul(&ark_base.as_slice(), &ark_coeff.as_slice());
    let mut h2c_msm_result = h256GA::identity();

    for _i in 0..test_scale {
        for _j in 0..*h2c_coeff.get(_i).unwrap() {
            h2c_msm_result = (h2c_msm_result + *h2c_base.get(_i).unwrap()).to_affine();
        }
    }

    assert_eq!(h2c_msm_result, transform::ark_to_h2c_point(ark_msm_result).to_affine()) ;
}