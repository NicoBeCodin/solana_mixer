use crate::error::ErrorCode;
use crate::{LEAVES_LENGTH,  DEFAULT_LEAF};
use crate::ErrorCode::*;
use anchor_lang::prelude::*;
use ark_ff::{  FromBytes, ToBytes };
use crate::DEFAULT_LEAF_HASH;
use solana_poseidon::{ hashv, Parameters, Endianness };

use groth16_solana::groth16::{ Groth16Verifier, Groth16Verifyingkey };
use crate::verifying_key::*;
use std::ops::Neg;
type G1 = ark_bn254::G1Affine;


pub type LeavesArray = [[u8; 32]; 16];
//Other default hashes can be added to avoid calculation
pub const DEPTH_FOUR: [u8;32] = [7, 249, 216, 55, 203, 23, 176, 211, 99, 32, 255, 233, 59, 165, 35, 69, 241, 183, 40, 87, 26, 86, 130, 101, 202, 172, 151, 85, 157, 188, 149, 42];
pub const DEPTH_FIVE: [u8;32] = [43, 148, 207, 94, 135, 70, 179, 245, 201, 99, 31, 76, 93, 243, 41, 7, 166, 153, 197, 140, 148, 178, 173, 77, 123, 92, 236, 22, 57, 24, 63, 85];
pub const DEPTH_SIX: [u8;32] = [45, 238, 147, 197, 166, 102, 69, 150, 70, 234, 125, 34, 204, 169, 225, 188, 254, 215, 30, 105, 81, 185, 83, 97, 29, 17, 221, 163, 46, 160, 157, 120];
pub const DEPTH_SEVEN: [u8;32] = [7, 130, 149, 229, 162, 43, 132, 233, 130, 207, 96, 30, 182, 57, 89, 123, 139, 5, 21, 168, 140, 181, 172, 127, 168, 164, 170, 190, 60, 135, 52, 157];

pub fn get_root(leaves: &LeavesArray) -> [u8; 32] {
    let mut nodes = leaves.to_vec();

    msg!("Calculating root");
    // Ensure the number of leaves is a power of two
    if (nodes.len() & (nodes.len() - 1)) != 0 {
        panic!("Number of leaves must be a power of two");
    }

    while nodes.len() > 1 {
        let mut next_level = Vec::with_capacity(nodes.len() / 2);
        for i in (0..nodes.len()).step_by(2) {
            let left = nodes[i];
            let right = nodes[i + 1];
            let parent_hash: [u8; 32] = hashv(
                Parameters::Bn254X5,
                Endianness::BigEndian,
                &[&left, &right]
            )
                .unwrap()
                .to_bytes();
            next_level.push(parent_hash);
        }
        nodes = next_level;
    }
    nodes[0]
}

pub fn print_non_default_leaves(leaves: &LeavesArray) {
    for (index, leaf) in leaves.iter().enumerate() {
        if leaf != &DEFAULT_LEAF_HASH {
            msg!("Leaf {} has hash: {:?}", index, leaf);
        }
    }
}
fn change_endianness(bytes: &[u8]) -> Vec<u8> {
    let mut vec = Vec::new();
    for b in bytes.chunks(32) {
        for byte in b.iter().rev() {
            vec.push(*byte);
        }
    }
    vec
}

pub fn default_leaves()->LeavesArray{
    let default_leaves_array: LeavesArray = [DEFAULT_LEAF; LEAVES_LENGTH];
    default_leaves_array
}   

//to calculate the depth depending on batch size
pub fn next_power_of_two_batch(n: usize)-> usize{
    
    for i  in 1..99 {
        if n*16 <= 2_usize.pow(i){
            return i as usize;
        }
    }
    return 99;
}

pub fn root_depth(depth: usize) -> [u8; 32] {
    let mut parent_hash = DEFAULT_LEAF.clone();
    
    // Ensure the number of leaves is a power of two
    let mut i = 0;
    while i<depth{
        parent_hash = hashv(
            Parameters::Bn254X5,
            Endianness::BigEndian,
            &[&parent_hash, &parent_hash]
        )
            .unwrap()
            .to_bytes();
        i+=1;        
    }
    parent_hash
}


pub fn get_default_root_depth(depth: usize) -> [u8; 32] {
    
    let hash = match depth {
        4 => DEPTH_FOUR,
        5=>DEPTH_FIVE,
        6=>DEPTH_SIX,
        7=>DEPTH_SEVEN,
        _ => root_depth(depth),
    };
    hash
}


pub fn verify_proof(proof: &[u8; 256], public_inputs: &[u8]) -> Result<bool> {
    // Ensure public inputs are a multiple of 32 bytes
    if public_inputs.len() % 32 != 0 {
        msg!("Invalid public inputs length");
        return Err(ErrorCode::InvalidArgument.into());
    }
    let public_input_root: [u8; 32] = public_inputs[32..64]
        .try_into()
        .expect("Failed public_input_root parsing");
    let public_input_nullifier: [u8; 32] = public_inputs[0..32]
        .try_into()
        .expect("Failed public_input_nullifier parsing");

    
    let public_inputs_array: &[[u8; 32]; 2] = &[public_input_nullifier,public_input_root];
    let vk: Groth16Verifyingkey = VERIFYINGKEY;
    let proof_a: G1 = <G1 as FromBytes>
        ::read(&*[&change_endianness(&proof[0..64])[..], &[0u8][..]].concat())
        .unwrap();
    let mut proof_a_neg = [0u8; 65];
    <G1 as ToBytes>::write(&proof_a.neg(), &mut proof_a_neg[..]).unwrap();
    let proof_a = change_endianness(&proof_a_neg[..64])
        .try_into()
        .unwrap();
    let proof_b = proof[64..192].try_into().unwrap();
    let proof_c = proof[192..256].try_into().unwrap();
    let mut verifier = Groth16Verifier::new(
        &proof_a,
        &proof_b,
        &proof_c,
        public_inputs_array,
        &vk,
    ).unwrap();
    let res = verifier.verify().unwrap();
    Ok(res)
}



