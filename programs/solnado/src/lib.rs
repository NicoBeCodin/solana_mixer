use anchor_lang::prelude::*;
pub mod state;
pub mod error;
pub mod utils;
pub mod parse_vk;
use crate::error::ErrorCode;
use crate::state::*;
use crate::utils::*;
// use arkworks_setups::common::{verify_unchecked_raw, setup_params};
use solana_poseidon::{hash, Parameters, Endianness};
use anchor_lang::solana_program::{program::invoke, system_instruction};
use ark_bn254::{ Bn254, Fr as Bn254Fr,  };




pub const DEFAULT_LEAF: [u8; 32] = [0u8; 32];
pub const TREE_DEPTH: u8 = 4;
pub const LEAVES_LENGTH: usize = 16;
//This is with the poseidon crate in the arkworks gadgets
pub const DEFAULT_LEAF_HASH_ARK: [u8;32] = [8, 161, 41, 109, 101, 167, 49, 116, 213, 91, 115, 123, 167, 152, 224, 78, 131, 165, 84, 91, 173, 165, 76, 27, 43, 226, 248, 228, 250, 1, 154, 130]; 
pub const DEFAULT_LEAF_HASH: [u8;32] =[42, 9, 169, 253, 147, 197, 144, 194, 107, 145, 239, 251, 178, 73, 159, 7, 232, 247, 170, 18, 226, 180, 148, 10, 58, 237, 36, 17, 203, 101, 225, 28]; //solana poseidon
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const FIXED_DEPOSIT_AMOUNT: u64 = (LAMPORTS_PER_SOL as f64 * 0.1) as u64; // 100_000_000

declare_id!("Ag36R1MUAHhyAYB96aR3JAScLqE6YFNau81iCcf2Y6RC");

/// Our fixed deposit of 0.1 SOL.



#[program]
pub mod solnado {

    use ark_bn254::Bn254;
    use ark_ff::PrimeField;
    use solana_program::program::invoke_signed;

    use crate::error::ErrorCode;

    use super::*;

    pub fn initialize_pool(ctx: Context<InitializePool>,) -> Result<()> {
        //intialize a 16 long vector (2**TREE_DEPTH) composed of the hash of DEFAULT_LEAF within the Bn254 curve
        //Add a way to get the merkle root of this tree  
        let default_hash = hash(Parameters::Bn254X5, Endianness::BigEndian, &DEFAULT_LEAF).unwrap().to_bytes();
        msg!("default hash  (solana poseidon library){:?}", default_hash);
        let pool =&mut ctx.accounts.pool;
        let leaves = [default_hash;16];
        // default_leaves().unwrap();
        msg!("default leaves activated");
        pool.leaves = leaves;
        msg!("default leaves implemented");
        pool.merkle_root = get_root(&leaves);
        // pool.used_nullifiers = used_nullifiers;
        msg!("Pool initialized with {:?} as root", pool.merkle_root);
        Ok(())
    }   

    
    pub fn deposit(ctx: Context<Deposit>, leaf_hash: [u8; 32]) -> Result<()> {
        
        let pool_leaves= &ctx.accounts.pool.leaves;
        let mut start_index = 0;
        //find the first non default leaf
        while pool_leaves[start_index] != DEFAULT_LEAF_HASH && start_index<LEAVES_LENGTH{
            start_index= start_index+1;
        }
        if start_index == LEAVES_LENGTH{
            msg!("Tree is already full, can't deposit funds");
            return Err(ErrorCode::TreeIsFull.into());
        }

        let transfer_instruction = system_instruction::transfer(
            &ctx.accounts.depositor.key(),
            &ctx.accounts.pool.key(),
            FIXED_DEPOSIT_AMOUNT,
        );

        let _ = invoke(&transfer_instruction, 
            &[
            ctx.accounts.depositor.to_account_info(),
            ctx.accounts.pool.to_account_info(),
        ]);

        let pool = &mut ctx.accounts.pool;
        pool.leaves[start_index] = leaf_hash;
        print_non_default_leaves(&pool.leaves);
        
        msg!("Leaf {:?} added at index {}",leaf_hash, start_index);
        pool.merkle_root= get_root(&pool.leaves);
        msg!("New root is {:?}" ,pool.merkle_root);
        Ok(())
        
    }



    pub fn withdraw(
        ctx: Context<Withdraw>,
        proof: [u8;256],     // Real proof
        public_inputs: [u8;32],
        // nullifier: [u8; 32],   // Nullifier to prevent double-spend not implemented for now
    ) -> Result<()> {
        
        let pool = &mut ctx.accounts.pool;
        let withdrawer = &mut ctx.accounts.withdrawer;
        let system_program = &ctx.accounts.system_program;

        if proof.len() != 256 {
            msg!("Invalid proof length!");
            return Err(ErrorCode::InvalidArgument.into());
        }
        let current_root = &pool.merkle_root;
        let root_as_bigint = Bn254Fr::from_be_bytes_mod_order(current_root);
        msg!("Current root as bigInt: {:?}", current_root);
        msg!("Public input root: {:?}", public_inputs);
        msg!("Verifying proof...");
        //Nullifier logic before checking the proof
        let res = verify_proof(&proof, &public_inputs).map_err(|_e| ErrorCode::InvalidProof)?;

        if !res{
            msg!("Proof result is false!");
            return Err(ErrorCode::InvalidArgument.into());
        }
        msg!("Proof verified successfuly");
        let amount = 100_000_000; // 0.1 SOL

        // Create the system program transfer instruction
        let transfer_instruction = system_instruction::transfer(
            &pool.key(),        // Sender (pool account)
            &withdrawer.key(),  // Receiver (withdrawer)
            amount,             // Amount to transfer
        );

        //PDA needs seeds
    
        // Invoke the system program transfer (signed because of PDA)
        invoke(
            &transfer_instruction,
            &[
                pool.to_account_info(),
                withdrawer.to_account_info(),
                system_program.to_account_info(),
            ],
        )?;
        ("Succesfully transfered 0.1 SOL to withdrawer!");

        Ok(())

    }

}

