use anchor_lang::prelude::*;
use ark_groth16::{prepare_verifying_key, Groth16, Proof, ProvingKey, VerifyingKey, PreparedVerifyingKey,  };
use ark_bn254::{Bn254, Fr as Bn254Fr};
use ark_ec::PairingEngine;
use ark_snark::SNARK;
use solana_poseidon::{Parameters, Endianness, hash, hashv};
use crate::state::*;
use crate::{LEAVES_LENGTH, TREE_DEPTH, DEFAULT_LEAF};
use ark_serialize::CanonicalDeserialize;



impl Pool {
    pub fn get_root(&self)->[u8;32]{

        let start_index = (1 << TREE_DEPTH) - 1;
        let mut merkle_leaves = self.leaves;
        
        for i in (0..start_index).rev() {
            let left_child = merkle_leaves[2 * i + 1];
            let right_child = merkle_leaves[2 * i + 2];
            let mut concat_left_right: [u8;64] = [0;64];
            concat_left_right[..32].copy_from_slice(&left_child);
            concat_left_right[32..].copy_from_slice(&right_child);
            // left_child.concat(&right_child);
            merkle_leaves[i] = hash(Parameters::Bn254X5, Endianness::BigEndian, &concat_left_right).unwrap().to_bytes();
        }

        merkle_leaves[0]

    }
    pub fn default_leaves(&self)->[[u8; 32]; 16]{
        let default_leaf_hash = hash(Parameters::Bn254X5, Endianness::BigEndian, &DEFAULT_LEAF).unwrap();
        let merkle_leaves:[[u8;32];16] = [default_leaf_hash.to_bytes(); LEAVES_LENGTH];

        merkle_leaves

    }
}

pub fn get_root(leaves: &[[u8; 32]; 16]) -> [u8; 32] {
    let mut nodes = leaves.to_vec();

    // Ensure the number of leaves is a power of two
    if nodes.len() & (nodes.len() - 1) != 0 {
        panic!("Number of leaves must be a power of two");
    }

    while nodes.len() > 1 {
        let mut next_level = Vec::with_capacity(nodes.len() / 2);
        for i in (0..nodes.len()).step_by(2) {
            let left = nodes[i];
            let right = nodes[i + 1];
            // msg!("left: {:?}, right {:?} i: {}", left, right, i);

            let parent_hash = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&left, &right])
                .expect("Hashing failed")
                .to_bytes();
            next_level.push(parent_hash);
        }
        nodes = next_level;
    }

    nodes[0]
}

pub fn default_leaves()->[[u8; 32]; 16]{
    let default_leaf_hash = hash(Parameters::Bn254X5, Endianness::BigEndian, &DEFAULT_LEAF).unwrap();
    let merkle_leaves:[[u8;32];16] = [default_leaf_hash.to_bytes(); LEAVES_LENGTH];

    merkle_leaves

}

pub fn print_non_default_leaves(leaves: &[[u8; 32]; 16]){
    let default_hash = hash(Parameters::Bn254X5, Endianness::BigEndian, &DEFAULT_LEAF).unwrap().to_bytes();
    for (index, leaf) in leaves.iter().enumerate() {
        if leaf != &default_hash{
            msg!("Leaf {} has hash: {:?}",index, leaf);
        }
    }
}

pub fn test_poseidon_hash() {


    // Define the secret and nullifier as byte arrays
    let secret = b"secret1";
    let secret_hash = hash(Parameters::Bn254X5, Endianness::BigEndian, secret).unwrap().to_bytes();
    // let secret_hash = hashv(Parameters::Bn254X5, Endianness::BigEndian, secret);
    let nullifier = b"nullifier1";
    let nullifier_hash = hash(Parameters::Bn254X5, Endianness::BigEndian, nullifier).unwrap().to_bytes();
    
    msg!("secret_hash: {:?} / nullifier_hash: {:?}", secret_hash, nullifier_hash);
    // Concatenate secret and nullifier
    let mut concatenated = Vec::new();
    concatenated.extend_from_slice(secret);
    concatenated.extend_from_slice(nullifier);

    // Compute the Poseidon hash
    let poseidon_hash = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&concatenated])
        .expect("Hashing failed")
        .to_bytes();

        let poseidon_hash_other = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&secret_hash, &nullifier_hash])
        .expect("Hashing failed")
        .to_bytes();
    // Print the resulting hash
    msg!("Poseidon hash: {:?}", poseidon_hash);
    msg!("Poseidon hash other: {:?}", poseidon_hash_other);
}

pub fn verify<E: PairingEngine>(
	public_inputs: &[E::Fr],
	vk_bytes: &[u8],
	proof: &[u8],
) -> bool {
	let vk = VerifyingKey::<E>::deserialize(vk_bytes).unwrap();
	let proof = Proof::<E>::deserialize(proof).unwrap();
	let res = Groth16::<E>::verify(&vk, public_inputs, &proof).unwrap();
    
    res
}