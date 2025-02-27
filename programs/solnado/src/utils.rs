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


pub fn verify_proof(proof: &[u8; 256], public_inputs: &[u8]) -> Result<bool> {
    // Ensure public inputs are a multiple of 32 bytes
    if public_inputs.len() % 32 != 0 {
        msg!("Invalid public inputs length");
        return Err(crate::ErrorCode::InvalidArgument.into());
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
