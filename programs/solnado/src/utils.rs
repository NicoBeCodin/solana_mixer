use crate::error::ErrorCode;
use crate::ErrorCode::*;
use anchor_lang::prelude::*;
use ark_ff::{ BigInteger, PrimeField, FromBytes, ToBytes };
use ark_bn254::{ Bn254, Fr as Bn254Fr,  };
use ark_serialize::{ CanonicalDeserialize, CanonicalSerialize };
use crate::state::*;
use crate::{ LEAVES_LENGTH, TREE_DEPTH, DEFAULT_LEAF, DEFAULT_LEAF_HASH };
use arkworks_setups::common::setup_params;
use solana_poseidon::{ hash, hashv, Parameters, Endianness };
use arkworks_setups::Curve;
use arkworks_native_gadgets::poseidon::{ FieldHasher, Poseidon };
use groth16_solana::groth16::{ Groth16Verifier, Groth16Verifyingkey };
use crate::parse_vk::*;
// use ark_bn254;
// use ark_serialize::C
use std::ops::Neg;
type G1 = ark_bn254::G1Affine;
type G2 = ark_bn254::G2Affine;

type LeavesArray = [[u8; 32]; 16];

pub fn get_root(leaves: &[[u8; 32]; 16]) -> [u8; 32] {
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

pub fn default_leaves() -> Result<LeavesArray> {
    let curve = Curve::Bn254;
    let params3 = setup_params(curve, 5, 3);
    let poseidon = Poseidon::<Bn254Fr>::new(params3);
    let default_leaf_f = Bn254Fr::from_be_bytes_mod_order(&DEFAULT_LEAF);
    let default_hash = poseidon.hash(&[default_leaf_f]).unwrap();
    let default_leaf_hash: [u8; 32] = default_hash.0
        .to_bytes_be()
        .try_into()
        .expect("Failed converting hash");
    msg!("Default leaf hash: {:?}", default_leaf_hash);

    if default_leaf_hash != DEFAULT_LEAF_HASH {
        // msg!("Wrong hash calculation!");
        return Err(ErrorCode::InvalidHash.into());
    }

    let merkle_leaves: [[u8; 32]; 16] = [default_leaf_hash; LEAVES_LENGTH];

    Ok(merkle_leaves)
}

pub fn print_non_default_leaves(leaves: &[[u8; 32]; 16]) {
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
        return Err(ErrorCode::InvalidArgument.into());
    }
    let public_input_one: [u8; 32] = public_inputs[0..32]
        .try_into()
        .expect("Failed public_input_one parsing");
    let public_inputs_array: &[[u8; 32]; 1] = &[public_input_one];

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
