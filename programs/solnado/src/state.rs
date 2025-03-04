use anchor_lang::prelude::*;
use crate::error::ErrorCode;
use crate::utils::LeavesArray;
use crate::{DEFAULT_LEAF, DEFAULT_LEAF_HASH, LEAVES_LENGTH, NULLIFIER_LIST_LENGTH};



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

    #[account(
        init,
        payer = authority,
        // We'll allocate enough space for the Pool struct.
        space = 8 + NullifierList::MAX_SIZE,
        seeds = [b"nullifier".as_ref(), identifier.to_le_bytes().as_ref()], 
        bump
    )]
    pub nullifier_list: Account<'info, NullifierList>,
    
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

    ///CHECK: Will serve to check if nullifier has already been used
    #[account(mut)]
    pub nullifier_account: AccountInfo<'info>,

    #[account(mut)]
    pub withdrawer: Signer<'info>,

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
    pub leaves: [[u8;32];16],

    // Set of used nullifiers to prevent double-withdraw
    pub identifier: u64,
    }

impl Pool {
    /// For a small struct, you can over-allocate a bit. 
    ///  - merkle_root: 32 bytes
    ///  - leaves: 16 * 32 = 512 bytes
    pub const MAX_SIZE: usize = 32 + (16 * 32) + 8 +  100;
    pub fn get_free_leaf(&self)->Result<usize>{
        let mut start_index = 0;
        //find the first non default leaf
        while self.leaves[start_index] != DEFAULT_LEAF_HASH && start_index < LEAVES_LENGTH {
            start_index += 1;
        }
        if start_index == LEAVES_LENGTH {
            msg!("Tree is already full, can't deposit funds");
            return Err(ErrorCode::TreeIsFull.into());
        }

        Ok(start_index)
    }
}
#[account]
pub struct NullifierList {
    pub nullifier_list: [[u8;32]; 16],
    pub identifier: u64,
}

impl NullifierList{
    pub const MAX_SIZE: usize = 32*16 + 8;
    pub fn get_free_nullifier(&self)->Result<usize>{
        let mut start_index: usize = 0;
        while self.nullifier_list[start_index] != DEFAULT_LEAF && start_index < NULLIFIER_LIST_LENGTH{
            start_index +=1;
        }
        if start_index == NULLIFIER_LIST_LENGTH {
            msg!("Nullifier list is full, can't add nullifier");
            return Err(ErrorCode::NullifierListIsFull.into());
        }
        Ok(start_index)
    }
}