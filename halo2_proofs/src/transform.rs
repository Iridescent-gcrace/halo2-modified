//! Tools for curve transformation.

use ark_ec::{AffineCurve, ProjectiveCurve, msm::VariableBaseMSM};
use ark_ec::models::short_weierstrass_jacobian::GroupAffine;
use ark_ff::{BigInteger256, fields::*, BigInteger, PrimeField};
use ark_bn254::{G1Affine as ark254GA, Fr, G1Projective as ark254GP};
use ark_std::{UniformRand, Zero};
use group::Group;
use core::panic;
use std::ops::Add;
use std::str::FromStr;
use std::time::Instant;
use halo2curves::{bn256::G1, serde::SerdeObject, bn256::G1Affine as h256GA};
use halo2curves::{CurveAffine, CurveExt, bn256, CurveAffineExt};
use ff::{Field, PrimeField as PField, BitViewSized};
use group::{prime::PrimeCurveAffine, GroupEncoding, Curve};
use std::thread;


/// These functions are designed to transform data between halo2curves and ark-works
/// on curve BN254 (or BN256).

/// 使用射影坐标，实现halo2curves向ark的转换
pub fn h2c_to_ark_point(h2c_point: bn256::G1) -> ark254GP {
    // First get the coordinates
    let ark_point_x = ark_bn254::Fq::from_le_bytes_mod_order(&h2c_point.to_affine().x.to_bytes());
    let ark_point_y = ark_bn254::Fq::from_le_bytes_mod_order(&h2c_point.to_affine().y.to_bytes());
    // Second get infinity
    let inf : bool = bool::from(h2c_point.is_identity());
    // Finally generate points in ark
    let ark_point: GroupAffine<ark_bn254::g1::Parameters> = ark254GA::new(ark_point_x, ark_point_y, inf);
    
    ark_point.into_projective()
}


/// 使用仿射和射影坐标，实现halo2curves向ark的转换
pub fn h2c_affine_to_ark_point(h2c_point: bn256::G1Affine) -> ark254GP {
    // let flag = h2c_point.x.clone();
    // First get the coordinates
    let ark_point_x = ark_bn254::Fq::from_le_bytes_mod_order(&h2c_point.x.to_bytes());
    let ark_point_y = ark_bn254::Fq::from_le_bytes_mod_order(&h2c_point.y.to_bytes());
    // Second get infinity
    let inf = bool::from(h2c_point.is_identity());
    // Finally generate points in ark
    let ark_point = ark254GA::new(ark_point_x, ark_point_y, inf);
    // println!("h2c_affine_to_ark_affine_point time:{:?}(flag) {:?}", flag,now.elapsed());

    ark_point.into_projective()
}

/// 使用仿射坐标，实现halo2curves向ark的转换
pub fn h2c_affine_to_ark_affine_point(h2c_point: h256GA) -> ark254GA {
    let flag = h2c_point.x.clone();
    let now = Instant::now();
    // First get the coordinates
    let ark_point_x = ark_bn254::Fq::from_le_bytes_mod_order(&h2c_point.x.to_bytes());
    let ark_point_y = ark_bn254::Fq::from_le_bytes_mod_order(&h2c_point.y.to_bytes());
    // Second get infinity
    let inf = bool::from(h2c_point.is_identity());
    // Finally generate points in ark
    let ark_point = ark254GA::new(ark_point_x, ark_point_y, inf);
    println!("h2c_affine_to_ark_affine_point time:{:?}(flag) {:?}", flag,now.elapsed());
    ark_point
}

/// 使用射影坐标，实现ark向halo2curves的转换
pub fn ark_to_h2c_point(ark_point: ark254GP) -> bn256::G1 {

    if (ark_point.x.into_repr().to_bytes_le().len() < 32 ||
    ark_point.y.into_repr().to_bytes_le().len() < 32) {
        panic!("ark_point input length panic at data transform");
    }
    let mut h2c_x_u8 = [0u8; 32];
    h2c_x_u8.copy_from_slice(&ark_point.x.into_repr().to_bytes_le().as_slice()[0..32]);
    let mut h2c_y_u8 = [0u8; 32];
    h2c_y_u8.clone_from_slice(&ark_point.y.into_repr().to_bytes_le().as_slice()[0..32]);
    let mut h2c_z_u8 = [0u8; 32];
    h2c_z_u8.clone_from_slice(&ark_point.z.into_repr().to_bytes_le().as_slice()[0..32]);

    let h2c_point_x = bn256::Fq::from_bytes(&h2c_x_u8).unwrap();
    let h2c_point_y = bn256::Fq::from_bytes(&h2c_y_u8).unwrap();
    let h2c_point_z = bn256::Fq::from_bytes(&h2c_z_u8).unwrap();

    let h2c_point = bn256::G1::new_jacobian(h2c_point_x, h2c_point_y, h2c_point_z).unwrap();
    h2c_point
}

/// From U8 to boolean type
pub fn from_u8_to_bool(data : u8) 
    -> [bool; 8] {
    let mut ret = [true; 8];
    for i in 0..8 {
        let mid_data = data;
        if (mid_data << i) >> 7 == 0 {
            ret[i] = false;
        }
    }
    ret
}


/// From U8 to BigInt256, in little endian as four U64
pub fn from_u8_to_big_int256(data : &[u8]) 
    -> [u64; 4] {
    if data.len() != 32 {
        panic!("panic at from_u8_to_big_int256: length of input does not match the rule");
    }
    
    let mut ret = [0u64; 4];
    ret[0] = from_u8_to_u64(&data[0..8]);
    ret[1] = from_u8_to_u64(&data[8..16]);
    ret[2] = from_u8_to_u64(&data[16..24]);
    ret[3] = from_u8_to_u64(&data[24..32]);

    ret
}

/// From U8 to U64, using little endian
pub fn from_u8_to_u64(data : &[u8])
    -> u64 {
    if data.len() != 8 {
        panic!("panic at from_u8_to_u64, length of input does not match the rule.\n The length of input is {:?}", data.len());
    }
    
    let mut ret = 0u64;
    for i in 0..8 {
        ret += (data[7 - i] as u64) << (8 * (7 - i));
    }
    ret
}