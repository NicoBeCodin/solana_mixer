use anchor_lang::prelude::*;

//
// Errors
//
#[error_code]
pub enum ErrorCode {
    #[msg("The merkle tree is already full (8 leaves).")]
    TreeIsFull,

    #[msg("The ZK proof is invalid.")]
    InvalidProof,

    #[msg("Nullifier already used.")]
    NullifierAlreadyUsed,

    #[msg("Invalid inputs")]
    InvalidInputs,

    #[msg("Invalid hash")]
    InvalidHash,

    #[msg("Invalid argument")]
    InvalidArgument
}