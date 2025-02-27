use anchor_lang::prelude::*;

//
// Errors
//
#[error_code]
pub enum ErrorCode {
    #[msg("The merkle tree is already full .")]
    TreeIsFull,

    #[msg("The nullifier list is full")]
    NullifierListIsFull,

    #[msg("The ZK proof is invalid.")]
    InvalidProof,

    #[msg("Nullifier already used.")]
    NullifierAlreadyUsed,

    #[msg("Invalid inputs")]
    InvalidInputs,

    #[msg("Invalid hash")]
    InvalidHash,

    #[msg("Invalid argument")]
    InvalidArgument,

    #[msg("Invalid public input root")]
    InvalidPublicInputRoot,

    
}