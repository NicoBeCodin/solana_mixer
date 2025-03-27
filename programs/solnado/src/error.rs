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

    #[msg("Invalid nullifier list")]
    InvalidNullifierList,

    #[msg("Invalid nullifier account")]
    InvalidNullifierAccount,

    #[msg("Inputed wrong program id")]
    WrongProgramId,

    #[msg("Invalid store batch account")]
    InvalidStoreBatchAccount,

    #[msg("Invalid pool account")]
    InvalidPoolAccount,

    #[msg("Invalid User batch Number input")]
    InvalidUserBatchNumber,

    #[msg("Invalid user leaves batch")]
    InvalidUserLeaves,

    #[msg("Invalid Indexing error")]
    InvalidIndexing,

    #[msg("Deposit amount too low")]
    InvalidDepositAmount,

    #[msg("Creator fee shouldn't be more than 1/10th of deposit amount")]
    CreatorFeeTooHigh,

    #[msg("This action is only executable by ADMIN")]
    UnauthorizedAction,

    #[msg("Invalid memo length")]
    InvalidMemoLength,

    #[msg("Invalid memo utf8")]
    InvalidMemoUtf8,
    
    #[msg("Missing memo instruction")]
    MissingMemoInstruction,

    #[msg("Invalid memo base 64")]
    InvalidMemoBase64,

    #[msg("Failed to parse batch")]
    FailedToParseBatch,

    #[msg("Invalid pool creator")]
    InvalidPoolCreator,
    

}