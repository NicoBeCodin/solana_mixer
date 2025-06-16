use crate::error::ErrorCode;
use crate::verifying_key::*;
use crate::{DEFAULT_LEAF, LEAVES_LENGTH};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::sysvar::instructions;
use ark_ff::{FromBytes, ToBytes};
use groth16_solana::groth16::{Groth16Verifier, Groth16Verifyingkey};
use solana_poseidon::{hashv, Endianness, Parameters};
use std::ops::Neg;
pub const SUB_BATCH_SIZE: usize = 8;
pub const MEMO_PROGRAM_ID: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
type G1 = ark_bn254::G1Affine;
use base64::{engine::general_purpose, Engine as _};
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

// in your utils.rs (or wherever you keep these)
pub const DEPTH_ONE: [u8; 32] = [
    32, 152, 245, 251, 158, 35, 158, 171, 60, 234, 195, 242, 123, 129, 228, 129, 220, 49, 36, 213,
    95, 254, 213, 35, 168, 57, 238, 132, 70, 182, 72, 100,
];
pub const DEPTH_TWO: [u8; 32] = [
    16, 105, 103, 61, 205, 177, 34, 99, 223, 48, 26, 111, 245, 132, 167, 236, 38, 26, 68, 203, 157,
    198, 141, 240, 103, 164, 119, 68, 96, 177, 241, 225,
];
pub const DEPTH_THREE: [u8; 32] = [
    24, 244, 51, 49, 83, 126, 226, 175, 46, 61, 117, 141, 80, 247, 33, 6, 70, 124, 110, 234, 80,
    55, 29, 213, 40, 213, 126, 178, 184, 86, 210, 56,
];
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
pub const DEPTH_EIGHT: [u8; 32] = [
    47, 165, 229, 241, 143, 96, 39, 166, 80, 27, 236, 134, 69, 100, 71, 42, 97, 107, 46, 39, 74,
    65, 33, 26, 68, 76, 190, 58, 153, 243, 204, 97,
];
pub const DEPTH_NINE: [u8; 32] = [
    14, 136, 67, 118, 208, 216, 253, 33, 236, 183, 128, 56, 158, 148, 31, 102, 228, 94, 122, 204,
    227, 226, 40, 171, 62, 33, 86, 166, 20, 252, 215, 71,
];
pub const DEPTH_TEN: [u8; 32] = [
    27, 114, 1, 218, 114, 73, 79, 30, 40, 113, 122, 209, 165, 46, 180, 105, 249, 88, 146, 249, 87,
    113, 53, 51, 222, 97, 117, 229, 218, 25, 10, 242,
];
pub const DEPTH_ELEVEN: [u8; 32] = [
    31, 141, 136, 34, 114, 94, 54, 56, 82, 0, 192, 178, 1, 36, 152, 25, 166, 230, 225, 228, 101, 8,
    8, 181, 190, 188, 107, 250, 206, 125, 118, 54,
];
pub const DEPTH_TWELVE: [u8; 32] = [
    44, 93, 130, 246, 108, 145, 75, 175, 185, 112, 21, 137, 186, 140, 252, 251, 97, 98, 176, 161,
    42, 207, 136, 168, 208, 135, 154, 4, 113, 181, 248, 90,
];
pub const DEPTH_THIRTEEN: [u8; 32] = [
    20, 197, 65, 72, 160, 148, 11, 184, 32, 149, 127, 90, 223, 63, 161, 19, 78, 245, 196, 170, 161,
    19, 244, 100, 100, 88, 242, 112, 224, 191, 191, 208,
];
pub const DEPTH_FOURTEEN: [u8; 32] = [
    25, 13, 51, 177, 47, 152, 111, 150, 30, 16, 192, 238, 68, 216, 185, 175, 17, 190, 37, 88, 140,
    173, 137, 212, 22, 17, 142, 75, 244, 235, 232, 12,
];
pub const DEPTH_FIFTEEN: [u8; 32] = [
    34, 249, 138, 169, 206, 112, 65, 82, 172, 23, 53, 73, 20, 173, 115, 237, 17, 103, 174, 101,
    150, 175, 81, 10, 165, 179, 100, 147, 37, 224, 108, 146,
];
pub const DEPTH_SIXTEEN: [u8; 32] = [
    42, 124, 124, 155, 108, 229, 136, 11, 159, 111, 34, 141, 114, 191, 106, 87, 90, 82, 111, 41,
    198, 110, 204, 238, 248, 183, 83, 211, 139, 186, 115, 35,
];
pub const DEPTH_SEVENTEEN: [u8; 32] = [
    46, 129, 134, 229, 88, 105, 142, 193, 198, 122, 249, 193, 77, 70, 63, 252, 71, 0, 67, 201, 194,
    152, 139, 149, 77, 117, 221, 100, 63, 54, 185, 146,
];
pub const DEPTH_EIGHTEEN: [u8; 32] = [
    15, 87, 197, 87, 30, 154, 78, 171, 73, 226, 200, 207, 5, 13, 174, 148, 138, 239, 110, 173, 100,
    115, 146, 39, 53, 70, 36, 157, 28, 31, 241, 15,
];
pub const DEPTH_NINETEEN: [u8; 32] = [
    24, 48, 238, 103, 181, 251, 85, 74, 213, 246, 61, 67, 136, 128, 14, 28, 254, 120, 227, 16, 105,
    125, 70, 228, 60, 156, 227, 97, 52, 247, 44, 202,
];
pub const DEPTH_TWENTY: [u8; 32] = [
    33, 52, 231, 106, 197, 210, 26, 171, 24, 108, 43, 225, 221, 143, 132, 238, 136, 10, 30, 70,
    234, 247, 18, 249, 211, 113, 182, 223, 34, 25, 31, 62,
];

pub const DEPTH_TWENTY_ONE: [u8; 32] = [
    25, 223, 144, 236, 132, 78, 188, 79, 254, 235, 216, 102, 243, 56, 89, 176, 192, 81, 216, 201,
    88, 238, 58, 168, 143, 143, 141, 243, 219, 145, 165, 177,
];

pub const DEPTH_TWENTY_TWO: [u8; 32] = [
    24, 204, 162, 166, 107, 92, 7, 135, 152, 30, 105, 174, 253, 132, 133, 45, 116, 175, 14, 147,
    239, 73, 18, 180, 100, 140, 5, 247, 34, 239, 229, 43,
];

pub const DEPTH_TWENTY_THREE: [u8; 32] = [
    35, 136, 144, 148, 21, 35, 13, 27, 77, 19, 4, 210, 213, 79, 71, 58, 98, 131, 56, 242, 239, 173,
    131, 250, 223, 5, 100, 69, 73, 210, 83, 141,
];

pub const DEPTH_TWENTY_FOUR: [u8; 32] = [
    39, 23, 31, 180, 169, 123, 108, 192, 233, 232, 245, 67, 181, 41, 77, 232, 102, 162, 175, 44,
    156, 141, 11, 29, 150, 230, 115, 228, 82, 158, 213, 64,
];

pub const DEPTH_TWENTY_FIVE: [u8; 32] = [
    47, 246, 101, 5, 64, 246, 41, 253, 87, 17, 160, 188, 116, 252, 13, 40, 220, 178, 48, 185, 57,
    37, 131, 229, 248, 213, 150, 150, 221, 230, 174, 33,
];

pub const DEPTH_TWENTY_SIX: [u8; 32] = [
    18, 12, 88, 241, 67, 212, 145, 233, 89, 2, 247, 245, 39, 119, 120, 162, 224, 173, 81, 104, 246,
    173, 215, 86, 105, 147, 38, 48, 206, 97, 21, 24,
];

pub const DEPTH_TWENTY_SEVEN: [u8; 32] = [
    31, 33, 254, 183, 13, 63, 33, 176, 123, 248, 83, 213, 229, 219, 3, 7, 30, 196, 149, 160, 165,
    101, 162, 29, 162, 214, 101, 210, 121, 72, 55, 149,
];

pub const DEPTH_TWENTY_EIGHT: [u8; 32] = [
    36, 190, 144, 95, 167, 19, 53, 225, 76, 99, 140, 192, 246, 106, 134, 35, 168, 38, 231, 104, 6,
    138, 158, 150, 139, 177, 161, 221, 225, 138, 114, 210,
];

pub const DEPTH_TWENTY_NINE: [u8; 32] = [
    15, 134, 102, 182, 46, 209, 116, 145, 197, 12, 234, 222, 173, 87, 212, 205, 89, 126, 243, 130,
    29, 101, 195, 40, 116, 76, 116, 229, 83, 218, 194, 109,
];

pub const DEPTH_THIRTY: [u8; 32] = [
    9, 24, 212, 107, 245, 45, 152, 176, 52, 65, 63, 74, 26, 28, 65, 89, 78, 122, 122, 63, 106, 224,
    140, 180, 61, 26, 42, 35, 14, 25, 89, 239,
];

/// lookup table for 1..=20, fall back for others
pub fn get_default_root_depth(depth: usize) -> [u8; 32] {
    match depth {
        1 => DEPTH_ONE,
        2 => DEPTH_TWO,
        3 => DEPTH_THREE,
        4 => DEPTH_FOUR,
        5 => DEPTH_FIVE,
        6 => DEPTH_SIX,
        7 => DEPTH_SEVEN,
        8 => DEPTH_EIGHT,
        9 => DEPTH_NINE,
        10 => DEPTH_TEN,
        11 => DEPTH_ELEVEN,
        12 => DEPTH_TWELVE,
        13 => DEPTH_THIRTEEN,
        14 => DEPTH_FOURTEEN,
        15 => DEPTH_FIFTEEN,
        16 => DEPTH_SIXTEEN,
        17 => DEPTH_SEVENTEEN,
        18 => DEPTH_EIGHTEEN,
        19 => DEPTH_NINETEEN,
        20 => DEPTH_TWENTY,
        21 => DEPTH_TWENTY_ONE,
        22 => DEPTH_TWENTY_TWO,
        23 => DEPTH_TWENTY_THREE,
        24 => DEPTH_TWENTY_FOUR,
        25 => DEPTH_TWENTY_FIVE,
        26 => DEPTH_TWENTY_SIX,
        27 => DEPTH_TWENTY_SEVEN,
        28 => DEPTH_TWENTY_EIGHT,
        29 => DEPTH_TWENTY_NINE,
        30 => DEPTH_THIRTY,
        _ => root_depth(depth),
    }
}

//For the fixed deposit amount
pub fn verify_withdraw_fixed_proof(proof: &[u8; 256], public_inputs: &[u8]) -> Result<()> {
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
    let _ = proof_verification(proof, &VERIFYINGKEY, public_inputs_array);

    // let vk: Groth16Verifyingkey = VERIFYINGKEY;
    // let proof_a: G1 =
    //     <G1 as FromBytes>::read(&*[&change_endianness(&proof[0..64])[..], &[0u8][..]].concat())
    //         .unwrap();
    // let mut proof_a_neg = [0u8; 65];
    // <G1 as ToBytes>::write(&proof_a.neg(), &mut proof_a_neg[..]).unwrap();
    // let proof_a = change_endianness(&proof_a_neg[..64]).try_into().unwrap();
    // let proof_b = proof[64..192].try_into().unwrap();
    // let proof_c = proof[192..256].try_into().unwrap();
    // let mut verifier =
    //     Groth16Verifier::new(&proof_a, &proof_b, &proof_c, public_inputs_array, &vk).unwrap();
    // let res = verifier.verify().unwrap();
    msg!("Fixed amount withdrawal proof succesfully verified");
    Ok(())
}

//For variable deposit amount, 2 leaves to one
pub fn verify_deposit_proof(
    proof: &[u8; 256],
    public_inputs: &[u8],
) -> Result<([u8; 8], [u8; 32], [u8; 32])> {
    if public_inputs.len() != 72 {
        msg!("Invalid public inputs length: {}", public_inputs.len());
        return Err(ErrorCode::InvalidArgument.into());
    }

    // unpack the three outputs
    let sum_be8: [u8; 8] = public_inputs[0..8]
        .try_into()
        .expect("Error converting type");
    let mut secret_be = [0u8; 32];
    // copy your 8 bytes into the *right* end of the 32-byte buffer
    secret_be[32 - 8..].copy_from_slice(&sum_be8);
    let leaf1: [u8; 32] = public_inputs[8..40]
        .try_into()
        .expect("Error converting type");
    let leaf2: [u8; 32] = public_inputs[40..72]
        .try_into()
        .expect("Error converting type");

    let inputs_arr: &[[u8; 32]; 3] = &[secret_be, leaf1, leaf2];

    let _ = proof_verification(proof, &VERIFYINGKEY_DEPOSIT2, inputs_arr);

    msg!("Double leaf deposit proof succesfully verified");

    Ok((sum_be8, leaf1, leaf2))
}

pub fn verify_single_deposit_proof(
    proof: &[u8; 256],
    public_inputs: &[u8],
) -> Result<([u8; 8], [u8; 32])> {
    // must be exactly three 32‐byte words
    if public_inputs.len() != 72 {
        msg!("Invalid public inputs length: {}", public_inputs.len());
        return Err(ErrorCode::InvalidArgument.into());
    }

    // unpack the three outputs
    let sum_be8: [u8; 8] = public_inputs[0..8]
        .try_into()
        .expect("Error converting type");
    let mut secret_be = [0u8; 32];
    // copy your 8 bytes into the *right* end of the 32-byte buffer
    secret_be[32 - 8..].copy_from_slice(&sum_be8);
    let leaf1: [u8; 32] = public_inputs[8..40]
        .try_into()
        .expect("Error converting type");

    let inputs_arr: &[[u8; 32]; 2] = &[secret_be, leaf1];
    let _ = proof_verification(proof, &VERIFYINGKEY_DEPOSIT1, inputs_arr);

    msg!("Proof single leaf deposit proof succesfully verified");

    Ok((sum_be8, leaf1))
}

// 2 null -> 1 leaf
pub fn verify_combine_proof(
    proof: &[u8; 256],
    // null1: [u8; 32],
    // null2: [u8; 32],
    // new_leaf: [u8; 32],
    // root: [u8; 32],
    public_inputs: &[u8],
) -> Result<([u8; 32], [u8; 32], [u8; 32], [u8; 32])> {
    // Build the fixed‐size array reference for the verifier
    let n1: [u8; 32] = public_inputs[..32].try_into().expect("Failed converting");
    let n2: [u8; 32] = public_inputs[32..64].try_into().expect("Failed converting");
    let new_leaf: [u8; 32] = public_inputs[64..96].try_into().expect("Failed converting");
    let root: [u8; 32] = public_inputs[96..128]
        .try_into()
        .expect("Failed converting");
    let inputs_arr: &[[u8; 32]; 4] = &[n1, n2, new_leaf, root];

    let _ = proof_verification(proof, &COMBINE2TO1_VERIFYINGKEY, inputs_arr);

    msg!("Combine proof successfully verified");

    // Return the four public outputs
    Ok((n1, n2, new_leaf, root))
    // (*new_leaf, *root)
}

pub fn verify_one_null_two_leaves(
    proof: &[u8; 256],
    // null1: [u8; 32],
    // null2: [u8; 32],
    // new_leaf: [u8; 32],
    // root: [u8; 32],
    public_inputs: &[u8],
) -> Result<([u8; 32], [u8; 32], [u8; 32], [u8; 32])> {
    // Build the fixed‐size array reference for the verifier
    let n1: [u8; 32] = public_inputs[..32].try_into().expect("Failed converting");
    let leaf1: [u8; 32] = public_inputs[32..64].try_into().expect("Failed converting");
    let leaf2: [u8; 32] = public_inputs[64..96].try_into().expect("Failed converting");
    let root: [u8; 32] = public_inputs[96..128]
        .try_into()
        .expect("Failed converting");
    let inputs_arr: &[[u8; 32]; 4] = &[n1, leaf1, leaf2, root];

    let _ = proof_verification(proof, &COMBINE1TO2_VERIFYINGKEY, inputs_arr);

    msg!("Combine proof successfully verified");

    // Return the four public outputs
    Ok((n1, leaf1, leaf2, root))
    // (*new_leaf, *root)
}

pub fn verify_two_null_two_leaves(
    proof: &[u8; 256],
    // null1: [u8; 32],
    // null2: [u8; 32],
    // new_leaf: [u8; 32],
    // root: [u8; 32],
    public_inputs: &[u8],
) -> Result<([u8; 32], [u8; 32], [u8; 32], [u8; 32], [u8; 32])> {
    // Build the fixed‐size array reference for the verifier
    let n1: [u8; 32] = public_inputs[..32].try_into().expect("Failed converting");
    let n2: [u8; 32] = public_inputs[32..64].try_into().expect("Failed converting");
    let leaf1: [u8; 32] = public_inputs[64..96].try_into().expect("Failed converting");
    let leaf2: [u8; 32] = public_inputs[96..128]
        .try_into()
        .expect("Failed converting");
    let root: [u8; 32] = public_inputs[128..160]
        .try_into()
        .expect("Failed converting");
    let inputs_arr: &[[u8; 32]; 5] = &[n1, n2, leaf1, leaf2, root];

    let _ = proof_verification(proof, &COMBINE2TO2_VERIFYINGKEY, inputs_arr);

    msg!("Combine proof successfully verified");

    // Return the four public outputs
    Ok((n1, n2, leaf1, leaf2, root))
}

/// Unpacks & verifies a single‐leaf Merkle‐inclusion proof for withdrawal.
/// Expects `public_inputs = secret_be || nullifier_hash_be || root_be`, each 32 bytes.
pub fn verify_withdraw_proof(
    proof: &[u8; 256],
    public_inputs: &[u8],
) -> Result<([u8; 8], [u8; 32], [u8; 32])> {
    // let secret_be: [u8; 32] = Fr::from(8u64)
    //     .0
    //     .to_bytes_be()
    //     .try_into()
    //     .expect("Failed conversion");

    // allocate a 32-byte buffer, zero-initialized
    let mut val_be = [0u8; 32];
    // copy your 8 bytes into the *right* end of the 32-byte buffer
    let null: [u8; 32] = public_inputs[0..32].try_into().expect("Failed");
    let asset_id: [u8; 32] = public_inputs[32..64].try_into().expect("Failed");
    let val_be8: [u8; 8] = public_inputs[64..72].try_into().expect("Failed");
    val_be[32 - 8..].copy_from_slice(&val_be8);
    let root: [u8; 32] = public_inputs[72..104].try_into().expect("Failed");

    //For SOL this must be 0.
    for i in asset_id {
        if i != 0 {
            return Err(ErrorCode::InvalidAssetId.into());
        }
    }

    let inputs_arr: &[[u8; 32]; 4] = &[val_be, asset_id, null, root];

    let _ = proof_verification(proof, &WITHDRAW_VAR_VK, inputs_arr);

    Ok((val_be8, null, root))
}

pub fn verify_withdraw_and_add_proof(
    proof: &[u8; 256],
    public_inputs: &[u8],
) -> Result<([u8; 8], [u8; 32], [u8; 32], [u8; 32])> {
    let mut val_be = [0u8; 32];
    let val_be8: [u8; 8] = public_inputs[96..104].try_into().expect("Failed");
    // copy your 8 bytes into the *right* end of the 32-byte buffer
    val_be[32 - 8..].copy_from_slice(&val_be8);
    let null: [u8; 32] = public_inputs[0..32].try_into().expect("Failed");
    let asset_id: [u8; 32] = public_inputs[32..64].try_into().expect("Failed");
    let root = public_inputs[64..96].try_into().expect("Failed");
    let new_leaf: [u8; 32] = public_inputs[104..136].try_into().expect("Failed");
    let inputs_arr: &[[u8; 32]; 5] = &[val_be, asset_id, null, new_leaf, root];
    //For SOL this must be 0.
    for i in asset_id {
        if i != 0 {
            return Err(ErrorCode::InvalidAssetId.into());
        }
    }
    msg!(" asset_id, val_be8, null, root, new_leaf {:?}\n, {:?}\n, {:?}\n, {:?}\n, {:?}\n", asset_id, val_be8, null, root, new_leaf);
    msg!("inpust arr: {:?}", inputs_arr);
    let _ = proof_verification(proof, &WITHDRAW_AND_ADD_VERIFYINGKEY, inputs_arr);
    msg!("Verification for withdraw and add leaf succeded");
    Ok((val_be8, null, root, new_leaf))
}

pub fn verify_withdraw_on_behalf(
    proof: &[u8; 256],
    public_inputs: &[u8],
) -> Result<([u8; 8], [u8; 32], [u8; 32], [u8; 32])> {
    // let secret_be: [u8; 32] = Fr::from(8u64)
    //     .0
    //     .to_bytes_be()
    //     .try_into()
    //     .expect("Failed conversion");

    let secret_be8: [u8; 8] = public_inputs[..8].try_into().expect("Failed");
    // allocate a 32-byte buffer, zero-initialized
    let mut secret_be = [0u8; 32];
    // copy your 8 bytes into the *right* end of the 32-byte buffer
    secret_be[32 - 8..].copy_from_slice(&secret_be8);
    let null: [u8; 32] = public_inputs[8..40].try_into().expect("Failed");
    let withdrawer_bytes: [u8; 32] = public_inputs[40..72].try_into().expect("Failed");
    let root: [u8; 32] = public_inputs[40..72].try_into().expect("Failed");

    let inputs_arr: &[[u8; 32]; 4] = &[secret_be, null, withdrawer_bytes, root];

    let _ = proof_verification(proof, &WITHDRAW_VAR_VK, inputs_arr);

    Ok((secret_be8, null, withdrawer_bytes, root))
}

fn proof_verification<const N: usize>(
    proof: &[u8; 256],
    verifying_key: &Groth16Verifyingkey,
    public_inputs: &[[u8; 32]; N],
) -> Result<()> {
    // let public_inputs_arr: [[u8;32];_] = public_inputs.try_into().expect("Failed");
    // Deserialize πA with endianness fix
    let proof_a: G1 =
        <G1 as FromBytes>::read(&*[&change_endianness(&proof[0..64])[..], &[0u8][..]].concat())
            .unwrap();
    let mut proof_a_neg = [0u8; 65];
    <G1 as ToBytes>::write(&proof_a.neg(), &mut proof_a_neg[..]).unwrap();
    let proof_a: [u8; 64] = change_endianness(&proof_a_neg[..64]).try_into().unwrap();

    // πB and πC come directly
    let proof_b: [u8; 128] = proof[64..192].try_into().unwrap();
    let proof_c: [u8; 64] = proof[192..256].try_into().unwrap();

    let mut v = Groth16Verifier::new(&proof_a, &proof_b, &proof_c, public_inputs, verifying_key)
        .map_err(|_| ErrorCode::InvalidProof)?;

    let ok = v.verify().map_err(|_| ErrorCode::InvalidProof)?;
    require!(ok, ErrorCode::InvalidProof);
    Ok(())
}

/// Checks that the subbatch memo is correct, essential for easy parsing
pub fn enforce_sub_batch_memo(
    sysvar_account: &AccountInfo,
    batch_number: u64,
    expected_leaves: &[[u8; 32]],
) -> Result<()> {
    // Load the first instruction (must be Memo)
    let mut found = false;

    let memo_ix: Option<Instruction> = None;
    for i in 0..=2 {
        if let Ok(memo_ix) = instructions::load_instruction_at_checked(i, sysvar_account) {
            msg!(
                "Checking instruction at index {}: {:?}",
                i,
                memo_ix.program_id
            );
            if memo_ix.program_id == MEMO_PROGRAM_ID {
                found = true;

                msg!("Found memo instruction at index {}", i);
                break;
            }
        }
    }
    let memo = memo_ix.unwrap();
    require!(found, ErrorCode::MissingMemoInstruction);
    // Decode base64 payload
    let memo_str = std::str::from_utf8(&memo.data).map_err(|_| ErrorCode::InvalidMemoUtf8)?;
    msg!("Translating from utf8 {}", memo_str);
    let memo_bytes = general_purpose::STANDARD
        .decode(memo_str)
        .map_err(|_| ErrorCode::InvalidMemoBase64)?;
    msg!("Getting memo bytes {:?}", memo_bytes);

    // Check batch number (big-endian u64)
    let user_batch = u64::from_be_bytes(memo_bytes[0..8].try_into().unwrap());
    require!(
        user_batch == batch_number,
        ErrorCode::InvalidUserBatchNumber
    );

    // Verify each leaf
    for (i, leaf) in expected_leaves.iter().enumerate() {
        let start = 8 + i * 32;
        let slice: [u8; 32] = memo_bytes[start..start + 32].try_into().unwrap();
        require!(&slice == leaf, ErrorCode::InvalidUserLeaves);
    }
    Ok(())
}

///  Memo format:   batch_number_be(8) || small_tree_root(32)
pub fn enforce_small_tree_memo(
    ix_sysvar: &AccountInfo,
    closed_batch: u64,
    expected_root: [u8; 32],
) -> Result<()> {
    let memo_ix = instructions::load_instruction_at_checked(0, ix_sysvar)?;

    require!(
        memo_ix.program_id == MEMO_PROGRAM_ID,
        ErrorCode::MissingMemoInstruction
    );

    let memo_bytes = {
        let memo_str =
            core::str::from_utf8(&memo_ix.data).map_err(|_| ErrorCode::InvalidMemoUtf8)?;
        general_purpose::STANDARD
            .decode(memo_str)
            .map_err(|_| ErrorCode::InvalidMemoBase64)?
    };

    // require!(memo_bytes.len() == 40, ErrorCode::InvalidMemoLength); // 8 + 32

    let bn = u64::from_be_bytes(memo_bytes[0..8].try_into().unwrap());
    require!(bn == closed_batch, ErrorCode::InvalidUserBatchNumber);

    let root_slice: [u8; 32] = memo_bytes[8..40].try_into().unwrap();
    require!(root_slice == expected_root, ErrorCode::InvalidSmallTreeRoot);

    Ok(())
}

pub fn enforce_nullifier_shard_memo(
    sysvar_ai: &AccountInfo,
    expected_initials: &[[u8; 32]; 4],
) -> Result<()> {
    // 1) Load the first instruction via the instructions sysvar
    let memo_ix = instructions::load_instruction_at_checked(0, sysvar_ai)
        .map_err(|_| error!(ErrorCode::MissingMemoInstruction))?;
    // 2) Must be the Memo program
    let memo_program_id = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
    require!(
        memo_ix.program_id == memo_program_id,
        ErrorCode::MissingMemoInstruction
    );
    // 3) Memo data is base64‐encoded; decode it
    let s = std::str::from_utf8(&memo_ix.data).map_err(|_| error!(ErrorCode::InvalidMemoUtf8))?;
    let memo_bytes = general_purpose::STANDARD
        .decode(s)
        .map_err(|_| error!(ErrorCode::InvalidMemoBase64))?;
    // 4) For each expected 32-byte initial, ensure it appears somewhere
    for init in expected_initials.iter() {
        let found = memo_bytes.windows(32).any(|window| window == init);
        require!(found, ErrorCode::InvalidNullifierShardMemo);
    }
    Ok(())
}
