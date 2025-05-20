use crate::error::ErrorCode;
use crate::ErrorCode::*;
use crate::DEFAULT_LEAF_HASH;
use crate::{DEFAULT_LEAF, LEAVES_LENGTH};
use anchor_lang::prelude::*;
use ark_ff::{FromBytes, ToBytes};
use solana_poseidon::{hashv, Endianness, Parameters};
use anchor_lang::solana_program::{sysvar::instructions};
use crate::verifying_key::*;
use groth16_solana::groth16::{Groth16Verifier, Groth16Verifyingkey};
use std::ops::Neg;
pub const SUB_BATCH_SIZE: usize = 8;
type G1 = ark_bn254::G1Affine;
use base64::{engine::general_purpose, Engine as _};
pub type LeavesArray = [[u8; 32]; 16];
//Other default hashes can be added to avoid calculation
pub const DEPTH_FOUR: [u8; 32] = [
    7, 249, 216, 55, 203, 23, 176, 211, 99, 32, 255, 233, 59, 165, 35, 69, 241, 183, 40, 87, 26,
    86, 130, 101, 202, 172, 151, 85, 157, 188, 149, 42,
];
pub const DEPTH_FIVE: [u8; 32] = [
    43, 148, 207, 94, 135, 70, 179, 245, 201, 99, 31, 76, 93, 243, 41, 7, 166, 153, 197, 140, 148,
    178, 173, 77, 123, 92, 236, 22, 57, 24, 63, 85,
];
pub const DEPTH_SIX: [u8; 32] = [
    45, 238, 147, 197, 166, 102, 69, 150, 70, 234, 125, 34, 204, 169, 225, 188, 254, 215, 30, 105,
    81, 185, 83, 97, 29, 17, 221, 163, 46, 160, 157, 120,
];
pub const DEPTH_SEVEN: [u8; 32] = [
    7, 130, 149, 229, 162, 43, 132, 233, 130, 207, 96, 30, 182, 57, 89, 123, 139, 5, 21, 168, 140,
    181, 172, 127, 168, 164, 170, 190, 60, 135, 52, 157,
];

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
            let parent_hash: [u8; 32] =
                hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&left, &right])
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

pub fn default_leaves() -> LeavesArray {
    let default_leaves_array: LeavesArray = [DEFAULT_LEAF; LEAVES_LENGTH];
    default_leaves_array
}

//to calculate the depth depending on batch size
pub fn next_power_of_two_batch(n: usize) -> usize {
    for i in 1..99 {
        if n * 16 <= 2_usize.pow(i) {
            return i as usize;
        }
    }
    return 99;
}

pub fn root_depth(depth: usize) -> [u8; 32] {
    let mut parent_hash = DEFAULT_LEAF.clone();

    // Ensure the number of leaves is a power of two
    let mut i = 0;
    while i < depth {
        parent_hash = hashv(
            Parameters::Bn254X5,
            Endianness::BigEndian,
            &[&parent_hash, &parent_hash],
        )
        .unwrap()
        .to_bytes();
        i += 1;
        // msg!("Depth i: {}, hash : {:?}", i, parent_hash);
    }
    parent_hash
}

pub fn get_default_root_depth(depth: usize) -> [u8; 32] {
    let hash = match depth {
        4 => DEPTH_FOUR,
        5 => DEPTH_FIVE,
        6 => DEPTH_SIX,
        7 => DEPTH_SEVEN,
        _ => root_depth(depth),
    };
    hash
}

//For the fixed deposit amount
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

    let public_inputs_array: &[[u8; 32]; 2] = &[public_input_nullifier, public_input_root];
    let vk: Groth16Verifyingkey = VERIFYINGKEY;
    let proof_a: G1 =
        <G1 as FromBytes>::read(&*[&change_endianness(&proof[0..64])[..], &[0u8][..]].concat())
            .unwrap();
    let mut proof_a_neg = [0u8; 65];
    <G1 as ToBytes>::write(&proof_a.neg(), &mut proof_a_neg[..]).unwrap();
    let proof_a = change_endianness(&proof_a_neg[..64]).try_into().unwrap();
    let proof_b = proof[64..192].try_into().unwrap();
    let proof_c = proof[192..256].try_into().unwrap();
    let mut verifier =
        Groth16Verifier::new(&proof_a, &proof_b, &proof_c, public_inputs_array, &vk).unwrap();
    let res = verifier.verify().unwrap();
    Ok(res)
}


//For variable deposit amount
pub fn verify_deposit_proof(
    proof: &[u8; 256],
    public_inputs: &[u8],
) -> Result<([u8; 32], [u8; 32], [u8; 32])> {
    // must be exactly three 32‐byte words
    if public_inputs.len() != 96 {
        msg!("Invalid public inputs length: {}", public_inputs.len());
        return Err(ErrorCode::InvalidArgument.into());
    }

    // unpack the three outputs
    let sum_be: &[u8; 32] = &public_inputs[0..32]
        .try_into()
        .expect("Error converting type");
    let leaf1: &[u8; 32] = &public_inputs[32..64]
        .try_into()
        .expect("Error converting type");
    let leaf2: &[u8; 32] = &public_inputs[64..96]
        .try_into()
        .expect("Error converting type");

    let inputs_arr: &[[u8; 32]; 3] = &[sum_be.clone(), leaf1.clone(), leaf2.clone()];

    let proof_a: G1 =
        <G1 as FromBytes>::read(&*[&change_endianness(&proof[0..64])[..], &[0u8][..]].concat())
            .unwrap();
    let mut proof_a_neg = [0u8; 65];
    <G1 as ToBytes>::write(&proof_a.neg(), &mut proof_a_neg[..]).unwrap();
    let proof_a = change_endianness(&proof_a_neg[..64]).try_into().unwrap();
    let proof_b = proof[64..192].try_into().unwrap();
    let proof_c = proof[192..256].try_into().unwrap();

    // Verify the proof
    let mut v = Groth16Verifier::new(
        &proof_a,
        &proof_b,
        &proof_c,
        &inputs_arr,
        &VERIFYINGKEY_VAR as &Groth16Verifyingkey,
    )
    .map_err(|_| ErrorCode::InvalidProof)?;
    // run it
    let good = v.verify().map_err(|_| ErrorCode::InvalidProof)?;
    require!(good, ErrorCode::InvalidProof);
    msg!("Proof succesfully verified");

    Ok((*sum_be, *leaf1, *leaf2))

}



pub fn verify_combine_proof(
    proof: &[u8; 256],
    public_inputs: &[u8],
) -> Result<([u8; 32], [u8; 32], [u8; 32], [u8; 32])> {
    // Expect exactly 4 × 32 bytes
    if public_inputs.len() != 128 {
        msg!("Invalid public inputs length: {}", public_inputs.len());
        return Err(ErrorCode::InvalidArgument.into());
    }

    // Slice out each 32-byte word
    let null1: &[u8; 32] = public_inputs[0..32]
        .try_into()
        .expect("slice with correct length");
    let null2: &[u8; 32] = public_inputs[32..64]
        .try_into()
        .expect("slice with correct length");
    let new_leaf: &[u8; 32] = public_inputs[64..96]
        .try_into()
        .expect("slice with correct length");
    let root: &[u8; 32] = public_inputs[96..128]
        .try_into()
        .expect("slice with correct length");

    // Build the fixed‐size array reference for the verifier
    let inputs_arr: &[[u8; 32]; 4] =
        &[ *null1, *null2, *new_leaf, *root ];

    // Deserialize πA with endianness fix
    let proof_a: G1 = <G1 as FromBytes>::read(
        &*[&change_endianness(&proof[0..64])[..], &[0u8][..]].concat()
    ).unwrap();
    let mut proof_a_neg = [0u8; 65];
    <G1 as ToBytes>::write(&proof_a.neg(), &mut proof_a_neg[..]).unwrap();
    let proof_a = change_endianness(&proof_a_neg[..64])
        .try_into()
        .unwrap();

    // πB and πC come directly
    let proof_b: [u8; 128] = proof[64..192].try_into().unwrap();
    let proof_c: [u8; 64]  = proof[192..256].try_into().unwrap();

    // Run the verifier
    let mut v = Groth16Verifier::new(
        &proof_a,
        &proof_b,
        &proof_c,
        inputs_arr,
        &COMBINE_VERIFYINGKEY as &Groth16Verifyingkey,
    )
    .map_err(|_| ErrorCode::InvalidProof)?;

    let ok = v.verify().map_err(|_| ErrorCode::InvalidProof)?;
    require!(ok, ErrorCode::InvalidProof);
    msg!("Combine proof successfully verified");

    // Return the four public outputs
    Ok((*null1, *null2, *new_leaf, *root))
}


/// Unpacks & verifies a single‐leaf Merkle‐inclusion proof for withdrawal.
/// Expects `public_inputs = secret_be || nullifier_hash_be || root_be`, each 32 bytes.
pub fn verify_withdraw_proof(
    proof: &[u8;256],
    public_inputs: &[u8;96],
) -> Result<([u8;32], [u8;32], [u8;32])> {
    // 1) chop into three 32-byte slices
    // let mut secret_be         = [0u8;32];
    // let mut nullifier_hash_be = [0u8;32];
    // let mut root_be           = [0u8;32];
    // secret_be.copy_from_slice(&public_inputs[ 0.. 32]);
    // nullifier_hash_be.copy_from_slice(&public_inputs[32.. 64]);
    // root_be.copy_from_slice(&public_inputs[64.. 96]);

    // // 2) build the array of public signals
    // let inputs: &[[u8;32];3] = &[
    //     secret_be,
    //     nullifier_hash_be,
    //     root_be,
    // ];

    
    // Slice out each 32-byte word
    let secret: &[u8; 32] = public_inputs[0..32]
        .try_into()
        .expect("slice with correct length");
    let nullifier: &[u8; 32] = public_inputs[32..64]
        .try_into()
        .expect("slice with correct length");
    let root: &[u8; 32] = public_inputs[64..96]
        .try_into()
        .expect("slice with correct length");


        let inputs_arr: &[[u8; 32]; 3] =
        &[ *secret, *nullifier, *root ];

    // Deserialize πA with endianness fix
    let proof_a: G1 = <G1 as FromBytes>::read(
        &*[&change_endianness(&proof[0..64])[..], &[0u8][..]].concat()
    ).unwrap();
    let mut proof_a_neg = [0u8; 65];
    <G1 as ToBytes>::write(&proof_a.neg(), &mut proof_a_neg[..]).unwrap();
    let proof_a: [u8;64] = change_endianness(&proof_a_neg[..64])
        .try_into()
        .unwrap();

    // πB and πC come directly
    let proof_b: [u8; 128] = proof[64..192].try_into().unwrap();
    let proof_c: [u8; 64]  = proof[192..256].try_into().unwrap();

    let mut v = Groth16Verifier::new(
        &proof_a,
        &proof_b,
        &proof_c,
        inputs_arr,
        &WITHDRAW_VAR_VK as &Groth16Verifyingkey,
    )
    .map_err(|_| ErrorCode::InvalidProof)?;

    let ok = v.verify().map_err(|_| ErrorCode::InvalidProof)?;
    require!(ok, ErrorCode::InvalidProof);
    msg!("Combine proof successfully verified");

    Ok((*secret, *nullifier, *root))
}

/// Checks that the subbatch memo is correct, essential for easy parsing
pub fn enforce_sub_batch_memo(
    sysvar_account: &AccountInfo,
    batch_number: u64,
    expected_leaves: &[[u8;32]],
) -> Result<()> {
    // Load the first instruction (must be Memo)
    let memo_ix =   instructions::load_instruction_at_checked(0, sysvar_account)?;
    let memo_program_id = anchor_lang::solana_program::pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
        
    require!(
        memo_ix.program_id == memo_program_id,
        ErrorCode::MissingMemoInstruction
    );

    // Decode base64 payload
    let memo_str = std::str::from_utf8(&memo_ix.data)
    .map_err(|_| ErrorCode::InvalidMemoUtf8)?;
    msg!("Translating from utf8 {}", memo_str);
    let memo_bytes = general_purpose::STANDARD.decode(memo_str)
    .map_err(|_| ErrorCode::InvalidMemoBase64)?;
    msg!("Getting memo bytes {:?}", memo_bytes);

    // Must be exactly 8-byte batch number + N*32 bytes
    let expected_len = 8 + expected_leaves.len() * 32;
    require!(memo_bytes.len() == expected_len, ErrorCode::InvalidMemoLength);

    // Check batch number (big-endian u64)
    let user_batch = u64::from_be_bytes(memo_bytes[0..8].try_into().unwrap());
    require!(user_batch == batch_number, ErrorCode::InvalidUserBatchNumber);

    // Verify each leaf
    for (i, leaf) in expected_leaves.iter().enumerate() {
        let start = 8 + i * 32;
        let slice: [u8;32] = memo_bytes[start..start+32].try_into().unwrap();
        require!(&slice == leaf, ErrorCode::InvalidUserLeaves);
    }
    Ok(())
}

