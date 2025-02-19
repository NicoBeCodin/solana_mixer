use anchor_lang::prelude::*;
use solana_poseidon::{Parameters, Endianness, hash};
use crate::TREE_DEPTH;
use crate::LEAVES_LENGTH;


#[derive(Accounts)]
#[instruction(identifier: u64)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = authority,
        // We'll allocate enough space for the Pool struct.
        space = 8 + Pool::MAX_SIZE,
        seeds = [b"pool_merkle".as_ref(), identifier.to_le_bytes().as_ref()], 
        bump
    )]
    pub pool: Account<'info, Pool>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,

    #[account(mut)]
    pub depositor: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,

    #[account(mut)]
    pub recipient: Signer<'info>,

    pub system_program: Program<'info, System>,
}

//
// Pool Account (PDA or normal) Data
//
#[account]
pub struct Pool {
    /// Current merkle root after all deposits
    pub merkle_root: [u8; 32],

    /// Leaves array of size 8
    pub leaves: [[u8; 32]; 16],

    /// Set of used nullifiers to prevent double-withdraw
    pub used_nullifiers:  [[u8; 32]; 16],
    }

impl Pool {
    /// For a small struct, you can over-allocate a bit. 
    ///  - merkle_root: 32 bytes
    ///  - leaves: 8 * 32 = 256 bytes
    ///  - next_index: 1 byte
    ///  - used_nullifiers (Vec<[u8;32]>): We can give it some buffer, e.g. up to 8 or more.
    pub const MAX_SIZE: usize = 64 + (16 * 32) + 1 + (16 * 32) + 100;
}

