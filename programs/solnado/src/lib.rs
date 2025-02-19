use anchor_lang::prelude::*;
pub mod state;
pub mod error;
pub mod utils;
use crate::error::ErrorCode;
use crate::state::*;
use crate::utils::*;
use ark_bn254::{Bn254, Fr,  };
use solana_poseidon::{Parameters, Endianness, hash, HASH_BYTES};

pub const DEFAULT_LEAF: [u8; 32] = [0u8; 32];
pub const TREE_DEPTH: u8 = 4;
pub const LEAVES_LENGTH: usize = 16;

declare_id!("Ag36R1MUAHhyAYB96aR3JAScLqE6YFNau81iCcf2Y6RC");



const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
/// Our fixed deposit of 0.1 SOL.
const FIXED_DEPOSIT_AMOUNT: u64 = (LAMPORTS_PER_SOL as f64 * 0.1) as u64; // 100_000_000



#[program]
pub mod solnado {
    
    

    use solana_poseidon::PoseidonHash;

    use crate::error::ErrorCode;

    use super::*;

    pub fn initialize_pool(ctx: Context<InitializePool>,) -> Result<()> {
        //intialize a 16 long vector (2**TREE_DEPTH) composed of the hash of DEFAULT_LEAF within the Bn254 curve
        //Add a way to get the merkle root of this tree  
        let used_nullifiers:  [[u8; 32]; 16] = [[0;32];16];
        
        let pool =&mut ctx.accounts.pool;
        let leaves = default_leaves();
        pool.leaves = leaves;
        pool.merkle_root = get_root(&leaves);
        pool.used_nullifiers = used_nullifiers;
        msg!("Pool initialized with {:?} as root", pool.merkle_root);
        Ok(())
    }

    pub fn deposit(ctx: Context<Deposit>, leaf: [u8; 32]) -> Result<()> {
        // test_poseidon_hash();
        
        let default_hash = hash(Parameters::Bn254X5, Endianness::BigEndian, &DEFAULT_LEAF).unwrap().to_bytes();
        let pool= &mut ctx.accounts.pool;
        let mut start_index = 0;
        //find the first non default leaf
        while pool.leaves[start_index] != default_hash && start_index<LEAVES_LENGTH{
            start_index= start_index+1;
        }
        if start_index == LEAVES_LENGTH{
            return Err(ErrorCode::TreeIsFull.into());
        }
        //A leaf is a hashv(secret || nullifier)
        pool.leaves[start_index] = leaf;
        print_non_default_leaves(&pool.leaves);

        msg!("Leaf {:?} added at index {}",leaf, start_index);
        pool.merkle_root= get_root(&pool.leaves);
        msg!("New root is {:?}" ,pool.merkle_root);
        Ok(())
    }



    pub fn withdraw(
        ctx: Context<Withdraw>,
        zk_proof: Vec<u8>,     // Real proof
        nullifier: [u8; 32],   // Nullifier to prevent double-spend
    ) -> Result<()> {
        unimplemented!()
    
    }

}

