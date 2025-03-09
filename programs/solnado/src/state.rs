use anchor_lang::prelude::*;
use crate::error::ErrorCode;
use crate::utils::LeavesArray;
use crate::{DEFAULT_LEAF, DEFAULT_LEAF_HASH, LEAVES_LENGTH, NULLIFIER_LIST_LENGTH};
use solana_poseidon::{Parameters, hashv, Endianness};

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
pub struct AdminTransfer<'info> {
    /// The pool PDA from which funds will be transferred.
    /// (This must be derived as: seeds = [b"pool_merkle", identifier.to_le_bytes()], bump)
    #[account(mut)]
    ///CHECK: Pool pda
    pub pool: AccountInfo<'info>,
    /// The recipient account that will receive the funds.
    #[account(mut)]
    ///CHECK: Recipient
    pub recipient: AccountInfo<'info>,
    /// The admin authority who is allowed to trigger the transfer.
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,

    #[account(mut)]
    pub depositor: Signer<'info>,
    ///CHECK: The program itself
    pub store_batch: AccountInfo<'info>,

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
    //This will serve to reconstruct the tree
    pub whole_tree_root : [u8;32],
    pub batch_number: u64,
    pub peaks: [[u8;32]; 16], //Peaks to build merkle tree without storing everything
    pub depth: [u8; 16], //With each peak with associate a depth
    pub number_of_peaks: u8,
    }

impl Pool {
    pub const MAX_SIZE: usize = 32 + (16 * 32) + 8  +  8 + 32*16 + 8 + 16+ 1+  100;


    pub fn get_free_leaf(&self)->usize{
        let mut start_index = 0;
        //find the first non default leaf
        while self.leaves[start_index] != DEFAULT_LEAF && start_index <= LEAVES_LENGTH-1 {
            start_index += 1;
        }
        if start_index == LEAVES_LENGTH  {
            msg!("Tree is already full, can't deposit funds");
            return 99;
        }

        start_index 
        }

    pub fn find_first_match(&self) -> usize {
            for (index, element) in self.leaves.into_iter().enumerate() {
                if element == DEFAULT_LEAF {
                    return index;
                }
            }
            99
        }
    pub fn get_number_of_peaks(&self)->usize{
        let mut i:usize = 0;
        while self.peaks[i] != DEFAULT_LEAF {
            i+=1;
        }
        return i;
    }
    pub fn update_peaks(&mut self, new_batch: [u8;32]) {
        let mut peak_hashes = self.peaks;
        let mut peak_depths = self.depth;
        let mut count = self.number_of_peaks;
        msg!("peaks before update: {:?}", &peak_hashes[..(count as usize)]);
        msg!("depth before update: {:?}", &peak_depths[..(count as usize)]);
        msg!("number of peaks before update: {}", count);
        
        // New batch has default depth 4 (since 16 leaves = 2^4).
        let new_peak_hash = new_batch;
        let new_peak_depth: u8 = 4;
        
        // Append the new batch.
        if count < 16 {
            peak_hashes[count as usize] = new_peak_hash;
            peak_depths[count as usize] = new_peak_depth;
            count += 1;
        } else {
            panic!("Exceeded maximum peak capacity");
        }
        
        // Merge adjacent peaks while they have the same depth.
        while count >= 2 && peak_depths[(count - 1) as usize] == peak_depths[(count - 2) as usize] {
            let left = peak_hashes[(count - 2) as usize];
            let right = peak_hashes[(count - 1) as usize];
            let merged_hash = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&left, &right])
                .unwrap()
                .to_bytes();
            let merged_depth = peak_depths[(count - 1) as usize] + 1;
            peak_hashes[(count - 2) as usize] = merged_hash;
            peak_depths[(count - 2) as usize] = merged_depth;
            count -= 1;
        }
        
        // Clear unused entries so that the arrays only reflect the active peaks.
        for i in count as usize..16 {
            peak_hashes[i] = [0u8; 32];  // or DEFAULT_LEAF if defined
            peak_depths[i] = 0;
        }
        
        self.peaks = peak_hashes;
        self.depth = peak_depths;
        self.number_of_peaks = count;
        msg!("peaks after update: {:?}", &self.peaks[..(self.number_of_peaks as usize)]);
        msg!("depth after update: {:?}", &self.depth[..(self.number_of_peaks as usize)]);
        msg!("number of peaks after update: {}", self.number_of_peaks);
    }
    
    // pub fn compute_root_from_peaks(&self) -> [u8;32] {
    //     let mut nodes: Vec<[u8;32]> = self.peaks[..(self.number_of_peaks as usize)].to_vec();
    //     while nodes.len() > 1 {
    //         let mut next_level = Vec::with_capacity((nodes.len() + 1) / 2);
    //         let mut i = 0;
    //         while i < nodes.len() {
    //             if i + 1 < nodes.len() {
    //                 let merged = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&nodes[i], &nodes[i+1]])
    //                     .unwrap()
    //                     .to_bytes();
    //                 next_level.push(merged);
    //             } else {
    //                 // No sibling; duplicate the last node.
    //                 let merged = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&nodes[i], &nodes[i]])
    //                     .unwrap()
    //                     .to_bytes();
    //                 next_level.push(merged);
    //             }
    //             i += 2;
    //         }
    //         nodes = next_level;
    //     }
    //     nodes[0]
    // }
    //Padding with default leaves
    pub fn compute_root_from_peaks(&self) -> [u8;32] {
        let mut nodes: Vec<[u8;32]> = self.peaks[..(self.number_of_peaks as usize)].to_vec();
        let mut depth = 4; // Default depth for a batch
        
        while nodes.len() > 1 {
            let mut next_level = Vec::with_capacity((nodes.len() + 1) / 2);
            let mut i = 0;
            
            while i < nodes.len() {
                if i + 1 < nodes.len() {
                    let merged = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&nodes[i], &nodes[i+1]])
                        .unwrap()
                        .to_bytes();
                    next_level.push(merged);
                } else {
                    // No sibling; use default root for this depth instead of duplicating last node
                    let default_root = get_default_root_depth(depth);
                    msg!("i: {}, Getting default root of depth {} : {:?}", i, depth, default_root);
                    let merged = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&nodes[i], &default_root])
                        .unwrap()
                        .to_bytes();
                    next_level.push(merged);
                }
                i += 2;
            }   
            nodes = next_level;
            depth += 1; // Increase depth as we move up the tree
        }
        nodes[0]
    }
}

pub fn get_default_root_depth(depth: usize) -> [u8; 32] {
    let mut parent_hash = DEFAULT_LEAF.clone();
    
    // Ensure the number of leaves is a power of two
    let mut i = 0;
    while i<depth{
        parent_hash = hashv(
            Parameters::Bn254X5,
            Endianness::BigEndian,
            &[&parent_hash, &parent_hash]
        )
            .unwrap()
            .to_bytes();
        i+=1;
        msg!("Depth {} hash {:?}", i, parent_hash);
    }
    parent_hash
}