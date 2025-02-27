use anchor_lang::prelude::*;
pub mod state;
pub mod error;
pub mod utils;
pub mod verifying_key;
use crate::error::ErrorCode;
use crate::state::*;
use crate::utils::*;
use anchor_lang::solana_program::{ program::invoke, system_instruction };
use solana_program::program::invoke_signed;


pub const DEFAULT_LEAF: [u8; 32] = [0u8; 32];
pub const TREE_DEPTH: u8 = 4;
pub const LEAVES_LENGTH: usize = 16;
pub const NULLIFIER_LIST_LENGTH: usize = 16;
pub const DEFAULT_LEAF_HASH: [u8; 32] = [
    42, 9, 169, 253, 147, 197, 144, 194, 107, 145, 239, 251, 178, 73, 159, 7, 232, 247, 170, 18,
    226, 180, 148, 10, 58, 237, 36, 17, 203, 101, 225, 28,
]; //solana poseidon
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const FIXED_DEPOSIT_AMOUNT: u64 = ((LAMPORTS_PER_SOL as f64) * 0.1) as u64; // 100_000_000

declare_id!("Ag36R1MUAHhyAYB96aR3JAScLqE6YFNau81iCcf2Y6RC");

/// Our fixed deposit of 0.1 SOL.

#[program]
pub mod solnado {

    use super::*;

    pub fn initialize_pool(ctx: Context<InitializePool>, identifier: u64) -> Result<()> {
        let nullifier_list = &mut ctx.accounts.nullifier_list;
        let pool = &mut ctx.accounts.pool;
        nullifier_list.nullifier_list = [[0; 32]; 16];
        let leaves = [DEFAULT_LEAF_HASH; 16];
        pool.identifier = identifier;
        pool.leaves = leaves;
        pool.merkle_root = get_root(&leaves);
        msg!("Pool initialized with {:?} as root", pool.merkle_root);
        msg!("nullifier list initialized");
        Ok(())
    }

    pub fn deposit(ctx: Context<Deposit>, leaf_hash: [u8; 32]) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let start_index = pool.get_free_leaf().unwrap();

        let transfer_instruction = system_instruction::transfer(
            &ctx.accounts.depositor.key(),
            &ctx.accounts.pool.key(),
            FIXED_DEPOSIT_AMOUNT
        );

        let _ = invoke(
            &transfer_instruction,
            &[ctx.accounts.depositor.to_account_info(), ctx.accounts.pool.to_account_info()]
        );
        msg!("Transfered 0.1 SOL to pool");
        let pool = &mut ctx.accounts.pool;
        pool.leaves[start_index] = leaf_hash;
        print_non_default_leaves(&pool.leaves);

        msg!("Leaf {:?} added at index {}", leaf_hash, start_index);
        pool.merkle_root = get_root(&pool.leaves);
        msg!("New root is {:?}", pool.merkle_root);
        Ok(())
    }

    pub fn withdraw(
        ctx: Context<Withdraw>,
        proof: [u8; 256], // Real proof (a,b and c)
        public_inputs: [u8; 64] //root & nullifier hash
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let nullifier_list = &mut ctx.accounts.nullifier_list;
        let withdrawer = &mut ctx.accounts.withdrawer;
        let system_program = &ctx.accounts.system_program;

        if proof.len() != 256 {
            msg!("Invalid proof length!");
            return Err(ErrorCode::InvalidArgument.into());
        }

        //It's the opposite, the first 32 are the nullifier and the rest the root
        let public_input_root: [u8; 32] = public_inputs[32..64]
            .try_into()
            .expect("Failed converting the public_input root");
        let nullifier_hash = public_inputs[0..32]
            .try_into()
            .expect("Failed converting nullifier to hash");
        if pool.merkle_root != public_input_root {
            msg!("Public input root: {:?}", public_input_root);
            msg!("Tree root is {:?}", pool.merkle_root);
            return Err(ErrorCode::InvalidPublicInputRoot.into())
        }

        if nullifier_list.nullifier_list.contains(nullifier_hash) {
            return Err(ErrorCode::NullifierAlreadyUsed.into());
        }

        let current_root = &pool.merkle_root;
        // let root_as_bigint = Bn254Fr::from_be_bytes_mod_order(current_root);
        msg!("Current root : {:?}", current_root);
        msg!("Public input root: {:?}", public_input_root);
        msg!("Submitted nullifier hash: {:?}", nullifier_hash);
        msg!("Verifying proof...");
        //Nullifier logic before checking the proof
        let res = verify_proof(&proof, &public_inputs).map_err(|_e| ErrorCode::InvalidProof)?;

        if !res {
            msg!("Proof result is false!");
            return Err(ErrorCode::InvalidArgument.into());
        }
        msg!("Proof verified successfuly");

        //Add nullifier hash to nullifier list
        let index = nullifier_list.get_free_nullifier()?;
        nullifier_list.nullifier_list[index] = *nullifier_hash;
        msg!("Added {:?} to nullifier list", nullifier_hash);

        let amount = FIXED_DEPOSIT_AMOUNT; // 0.1 SOL

        // Create the system program transfer instruction
        let transfer_instruction = system_instruction::transfer(
            &pool.key(), // Sender (pool account)
            &withdrawer.key(), // Receiver (withdrawer)
            amount // Amount to transfer
        );
        
        let (_, bump) = Pubkey::find_program_address(&[b"pool_merkle", &pool.identifier.to_le_bytes()], &id());
        msg!("derived bump: {}", bump);
        let seeds: &[&[u8]] = &[
            b"pool_merkle",
            &pool.identifier.to_le_bytes(),
            &[bump]
        ];
        //PDA needs seeds

        // Invoke the system program transfer (signed because of PDA)
        invoke_signed(
            &transfer_instruction,
            &[
                pool.to_account_info().clone(),
                withdrawer.to_account_info(),
                system_program.to_account_info(),
            ],
            &[seeds]
        )?;
        "Succesfully transfered 0.1 SOL to withdrawer!";

        Ok(())
    }
}
