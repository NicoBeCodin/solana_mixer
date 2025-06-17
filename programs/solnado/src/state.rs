use crate::utils::get_default_root_depth;
use crate::{
    utils::next_power_of_two_batch, DEFAULT_LEAF, MIN_PDA_SIZE, SMALL_TREE_BATCH_DEPTH,
    TARGET_DEPTH_LARGE, TARGET_DEPTH_LARGE_ARRAY,
};
use anchor_lang::{prelude::*, solana_program::log::sol_log_compute_units};
use solana_poseidon::{hashv, Endianness, Parameters};
pub const SHARD_SIZE: usize = 8;

#[derive(Accounts)]
#[instruction(identifier: [u8;16])]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = authority,
        // We'll allocate enough space for the Pool struct.
        space = 8 + MerkleMountainRange::MAX_SIZE,
        seeds = [b"pool_merkle".as_ref(), &identifier],
        bump
    )]
    pub pool: Account<'info, MerkleMountainRange>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(identifier: [u8;16])]
pub struct InitializeVariablePool<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + MerkleMountainRange::MAX_SIZE,
        seeds = [b"variable_pool".as_ref(), &identifier],
        bump
    )]
    pub pool: Account<'info, MerkleMountainRange>,

    ///CHECK: leaves_indexer (called only when subbatch is full)
    // #[account(mut)]
    #[account(mut)]
    pub leaves_indexer: AccountInfo<'info>,

    ///CHECK : subtree_indexer called when a 2^16 tree is
    #[account(mut)]
    pub subtree_indexer: AccountInfo<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitializeTreasury<'info> {
    #[account(
        init,
        payer = payer,
        space = 8,
        seeds = [b"treasury"],
        bump
    )]
    pub treasury: Account<'info, TreasuryAccount>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}
#[account]
pub struct TreasuryAccount {} // Dummy, for space allocation

#[derive(Accounts)]
pub struct WithdrawFromTreasury<'info> {
    #[account(mut, seeds = [b"treasury"], bump)]
    ///CHECK: Treasury account
    pub treasury: Account<'info, TreasuryAccount>,

    #[account(mut)]
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct DepositVariable<'info> {
    #[account(
        mut,
        seeds = [b"variable_pool".as_ref(), &pool.identifier],
        bump
    )]
    pub pool: Account<'info, MerkleMountainRange>,

    #[account(mut)]
    pub depositor: Signer<'info>,

    ///CHECK : SYSVAR for instructions
    pub instruction_account: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
    // ///CHECK: leaves_indexer (called only when subbatch is full)
    // #[account(mut)]
    // pub leaves_indexer: AccountInfo<'info>,

    // ///CHECK : subtree_indexer called when a 2^16 tree is
    // #[account(mut)]
    // pub subtree_indexer: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct CombineDeposit<'info> {
    #[account(
        mut,
        seeds = [b"variable_pool".as_ref(), &pool.identifier],
        bump
    )]
    pub pool: Account<'info, MerkleMountainRange>,

    #[account(mut)]
    pub user: Signer<'info>,

    /// SYSVAR_INSTRUCTIONS must be passed to read the Memo
    ///CHECK:
    pub instruction_account: AccountInfo<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: first nullifier PDA (always required)
    #[account(mut)]
    pub nullifier1: AccountInfo<'info>,

    /// CHECK: second nullifier PDA (only for modes 0 & 2) or coule be leaves_indexer in some case;
    #[account(mut)]
    pub nullifier2_or_else: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(nullifier_hash: [u8;32])]
pub struct WithdrawVariable<'info> {
    /// Our on‐chain pool state
    #[account(
        mut,
        seeds = [ b"variable_pool", pool.identifier.as_ref() ],
        bump,           // assumes you store `bump: u8` in your pool struct
      )]
    pub pool: Account<'info, MerkleMountainRange>,

    ///CHECK :The user who is withdrawing
    // // #[account(
    // //     init_if_needed,
    // //     payer = user,
    // //     seeds = [nullifier_hash.as_ref()],
    // //     bump,
    // //     space = 0
    //   )]
    pub nullifier_account: AccountInfo<'info>,

    #[account(mut)]
    pub user: Signer<'info>,

    // /// A PDA whose seed is exactly the 32‐byte nullifier hash.
    // /// We create it here (with size=0) to mark “this nullifier was spent.”
    // /// CHECK: we're only using this for lamport‐zero and PDA‐existence checks.
    /// System program (for create_account + transfer)
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WithdrawOnBehalf<'info> {
    /// The variable‐pool PDA
    #[account(
        mut,
        seeds = [b"variable_pool", pool.identifier.as_ref()],
        bump,
    )]
    pub pool: Account<'info, MerkleMountainRange>,

    ///A PDA whose seed is exactly the 32-byte nullifier hash.
    ///CHECK: we mark it “used” here.
    #[account(mut)]
    pub nullifier_account: AccountInfo<'info>,

    ///CHECK: The beneficiary of the withdrawal
    #[account(mut)]
    pub withdrawer: AccountInfo<'info>,

    /// The transaction fee‐payer (must sign)
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(
        mut,
        seeds = [ b"merkle_pool", pool.identifier.as_ref() ],
        bump,           // assumes you store `bump: u8` in your pool struct
      )]
    pub pool: Account<'info, MerkleMountainRange>,

    #[account(mut)]
    pub depositor: Signer<'info>,

    ///CHECK : SYSVAR for instructions
    pub instruction_account: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(
        mut,
        seeds = [ b"variable_pool", pool.identifier.as_ref() ],
        bump,           // assumes you store `bump: u8` in your pool struct
      )]
    pub pool: Account<'info, MerkleMountainRange>,

    ///CHECK: Will serve to check if nullifier has already been used
    #[account(mut)]
    pub nullifier_account: AccountInfo<'info>,

    ///CHECK : The creator of the pool gets part of the fee
    #[account(mut)]
    pub pool_creator: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [b"treasury"],
        bump
    )]
    pub treasury: Account<'info, TreasuryAccount>,

    #[account(mut)]
    pub withdrawer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// Gonna delete this
#[derive(Accounts)]
pub struct AdminTransfer<'info> {
    /// The pool PDA from which funds will be transferred.
    /// (This must be derived as: seeds = [b"pool_merkle", identifier.to_le_bytes()], bump)
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

//True size mountain range

#[account]
pub struct MerkleMountainRange {
    /// Current merkle root after all deposits
    pub merkle_root_batch: [u8; 32],
    /// Leaves array of size 16
    pub batch_leaves: [[u8; 32]; 16],
    // Set of used nullifiers to prevent double-withdraw
    pub identifier: [u8; 16],
    //Minimum deposit amount or fixed deposit amount for fixed size pool
    pub min_deposit_amount: u64,
    //This will serve to reconstruct the tree
    pub whole_tree_root: [u8; 32],
    //another variable that will serve to index the small trees
    pub last_small_tree_root: [u8; 32],
    pub batch_number: u64,
    pub peaks: [[u8; 32]; TARGET_DEPTH_LARGE_ARRAY], //Peaks to build merkle tree without storing everything
    pub depth: [u8; TARGET_DEPTH_LARGE_ARRAY],       //With each peak we associate a depth
    pub number_of_peaks: u8,                         //Max number of peaks is
    pub max_leaves: u64,                             //MAX number of leaves in a pool
                                                     //Creator is optional
                                                     // pub creator: Pubkey,
                                                     // //Creator fee is optional
                                                     // pub creator_fee: u64,
                                                     //I'll add fields for the nullifier manager
}

impl MerkleMountainRange {
    pub const MAX_SIZE: usize = 32 + 512 + 16 + 8 + 32 + 32 + 26 * 32 + 26 + 1 + 8 + 32 + 8 + 100;

    pub fn find_first_match(&self) -> usize {
        for (index, element) in self.batch_leaves.into_iter().enumerate() {
            if element == DEFAULT_LEAF {
                return index;
            }
        }
        99
    }
    pub fn get_number_of_peaks(&self) -> usize {
        let mut i: usize = 0;
        while self.peaks[i] != DEFAULT_LEAF {
            i += 1;
        }
        return i;
    }

    pub fn update_peaks(&mut self, new_batch: [u8; 32]) {
        let mut peak_hashes = self.peaks;
        let mut peak_depths = self.depth;
        let mut count = self.number_of_peaks;
        msg!("peaks before update: {:?}", &peak_hashes[..count as usize]);
        msg!("depth before update: {:?}", &peak_depths[..count as usize]);
        msg!("number of peaks before update: {}", count);

        // New batch has default depth 4 (since 16 leaves = 2^4).
        let new_peak_hash = new_batch;
        let new_peak_depth: u8 = 4;

        // Append the new batch.
        if count < TARGET_DEPTH_LARGE_ARRAY as u8 {
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
            //In this case we have a new small tree root
            if merged_depth == SMALL_TREE_BATCH_DEPTH as u8 {
                self.last_small_tree_root = merged_hash;
            }
            peak_hashes[(count - 2) as usize] = merged_hash;
            peak_depths[(count - 2) as usize] = merged_depth;
            count -= 1;
        }

        // Clear unused entries so that the arrays only reflect the active peaks.
        for i in count as usize..16 {
            peak_hashes[i] = [0u8; 32]; // or DEFAULT_LEAF if defined
            peak_depths[i] = 0;
        }

        self.peaks = peak_hashes;
        self.depth = peak_depths;
        self.number_of_peaks = count;
        msg!(
            "peaks after update: {:?}",
            &self.peaks[..self.number_of_peaks as usize]
        );
        msg!(
            "depth after update: {:?}",
            &self.depth[..self.number_of_peaks as usize]
        );
        msg!("number of peaks after update: {}", self.number_of_peaks);
    }

    pub fn update_peaks_temp(&self, new_batch: [u8; 32]) -> [u8; 32] {
        let mut peak_hashes = self.peaks;
        let mut peak_depths = self.depth;
        let mut count = self.number_of_peaks;
        // msg!("peaks before update: {:?}", &peak_hashes[..count as usize]);
        // msg!("depth before update: {:?}", &peak_depths[..count as usize]);
        // msg!("number of peaks before update: {}", count);

        // New batch has default depth 4 (since 16 leaves = 2^4).
        let new_peak_hash = new_batch;
        let new_peak_depth: u8 = 4;

        // Append the new batch.
        if count < TARGET_DEPTH_LARGE_ARRAY as u8 {
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
            //In this case we have a new small tree root

            peak_hashes[(count - 2) as usize] = merged_hash;
            peak_depths[(count - 2) as usize] = merged_depth;
            count -= 1;
        }

        // Clear unused entries so that the arrays only reflect the active peaks.
        for i in count as usize..16 {
            peak_hashes[i] = [0u8; 32]; // or DEFAULT_LEAF if defined
            peak_depths[i] = 0;
        }

        let temp_root = self.compute_root_from_peaks_temp(count, peak_depths, peak_hashes);
        // self.peaks = peak_hashes;
        // self.depth = peak_depths;
        // self.number_of_peaks = count;
        // msg!("peaks after update: {:?}", &self.peaks[..self.number_of_peaks as usize]);
        // msg!("depth after update: {:?}", &self.depth[..self.number_of_peaks as usize]);
        // msg!("number of peaks after update: {}", self.number_of_peaks);
        temp_root
    }

    // Helper function to merge two nodes with potentially different depths.
    pub fn merge_nodes(a: ([u8; 32], u8), b: ([u8; 32], u8)) -> ([u8; 32], u8) {
        let (mut a_node, mut a_depth) = a;
        let (mut b_node, mut b_depth) = b;

        // "Lift" the shallower node until both depths match.
        while a_depth < b_depth {
            let default = get_default_root_depth(a_depth as usize);
            a_node = hashv(
                Parameters::Bn254X5,
                Endianness::BigEndian,
                &[&a_node, &default],
            )
            .unwrap()
            .to_bytes();
            a_depth += 1;
        }
        while b_depth < a_depth {
            let default = get_default_root_depth(b_depth as usize);
            b_node = hashv(
                Parameters::Bn254X5,
                Endianness::BigEndian,
                &[&b_node, &default],
            )
            .unwrap()
            .to_bytes();
            b_depth += 1;
        }

        // Now both nodes are at the same depth. Merge them.
        let merged = hashv(
            Parameters::Bn254X5,
            Endianness::BigEndian,
            &[&a_node, &b_node],
        )
        .unwrap()
        .to_bytes();
        (merged, a_depth + 1)
    }

    pub fn compute_root_from_peaks(&self) -> [u8; 32] {
        // Create a vector of (node, depth) pairs from the stored peaks.
        let mut nodes: Vec<([u8; 32], u8)> = (0..self.number_of_peaks as usize)
            .map(|i| (self.peaks[i], self.depth[i]))
            .collect();

        // Merge pairwise until we have one node.
        while nodes.len() > 1 {
            let mut next_level: Vec<([u8; 32], u8)> = Vec::with_capacity((nodes.len() + 1) / 2);
            let mut i = 0;
            while i < nodes.len() {
                if i + 1 < nodes.len() {
                    // Merge two adjacent nodes (taking care of depth differences)
                    let merged = Self::merge_nodes(nodes[i], nodes[i + 1]);
                    next_level.push(merged);
                } else {
                    // If there's an odd node out, merge it with the default node for its depth.
                    let (node, depth) = nodes[i];
                    let default = get_default_root_depth(depth as usize);
                    let merged = hashv(
                        Parameters::Bn254X5,
                        Endianness::BigEndian,
                        &[&node, &default],
                    )
                    .unwrap()
                    .to_bytes();
                    next_level.push((merged, depth + 1));
                }
                i += 2;
            }
            nodes = next_level;
        }
        // Return the computed root.
        nodes[0].0
    }

    pub fn compute_root_from_peaks_temp(
        &self,
        number_of_peaks: u8,
        depth: [u8; 26],
        peaks: [[u8; 32]; 26],
    ) -> [u8; 32] {
        // Create a vector of (node, depth) pairs from the stored peaks.
        let mut nodes: Vec<([u8; 32], u8)> = (0..number_of_peaks as usize)
            .map(|i| (peaks[i], depth[i]))
            .collect();

        // Merge pairwise until we have one node.
        while nodes.len() > 1 {
            let mut next_level: Vec<([u8; 32], u8)> = Vec::with_capacity((nodes.len() + 1) / 2);
            let mut i = 0;
            while i < nodes.len() {
                if i + 1 < nodes.len() {
                    // Merge two adjacent nodes (taking care of depth differences)
                    let merged = Self::merge_nodes(nodes[i], nodes[i + 1]);
                    next_level.push(merged);
                } else {
                    // If there's an odd node out, merge it with the default node for its depth.
                    let (node, depth) = nodes[i];
                    let default = get_default_root_depth(depth as usize);
                    let merged = hashv(
                        Parameters::Bn254X5,
                        Endianness::BigEndian,
                        &[&node, &default],
                    )
                    .unwrap()
                    .to_bytes();
                    next_level.push((merged, depth + 1));
                }
                i += 2;
            }
            nodes = next_level;
        }
        // Return the computed root.
        nodes[0].0
    }

    //this method allows
    pub fn deepen(&self, current_depth: usize, wanted_depth: usize) -> [u8; 32] {
        let mut default_hash = get_default_root_depth(current_depth);
        let mut hashed = hashv(
            Parameters::Bn254X5,
            Endianness::BigEndian,
            &[&self.whole_tree_root, &default_hash],
        )
        .unwrap()
        .to_bytes();
        let range = current_depth + 1..wanted_depth;
        for x in range {
            default_hash = get_default_root_depth(x);
            hashed = hashv(
                Parameters::Bn254X5,
                Endianness::BigEndian,
                &[&hashed, &default_hash],
            )
            .unwrap()
            .to_bytes();
        }
        hashed
    }

    pub fn deepen_temp(&self,temp_root: [u8;32], wanted_depth: usize) -> [u8; 32] {
        let current_depth = next_power_of_two_batch(self.batch_number as usize + 1);
        let mut default_hash = get_default_root_depth(current_depth);
        let mut hashed = hashv(
            Parameters::Bn254X5,
            Endianness::BigEndian,
            &[&temp_root, &default_hash],
        )
        .unwrap()
        .to_bytes();
        let range = current_depth + 1..wanted_depth;
        for x in range {
            default_hash = get_default_root_depth(x);
            hashed = hashv(
                Parameters::Bn254X5,
                Endianness::BigEndian,
                &[&hashed, &default_hash],
            )
            .unwrap()
            .to_bytes();
        }
        hashed
    }



    pub fn compare_to_deep(&self, user_root: [u8; 32]) -> bool {
        let current_depth = next_power_of_two_batch(self.batch_number as usize);
        let deep_root = self.deepen(current_depth, TARGET_DEPTH_LARGE);

        if !(user_root == deep_root) {
            msg!("user root: {:?} \n deep_root: {:?}", user_root, deep_root);
            return false;
        }
        true
    }
    pub fn get_deep_root(&self) -> [u8; 32] {
        let current_depth = next_power_of_two_batch(self.batch_number as usize);
        self.deepen(current_depth, TARGET_DEPTH_LARGE)
    }


}

// use anchor_spl::token::{self, Token, TokenAccount, Mint, Transfer};

//SPL token causes a shit ton of dependencies issues
// #[derive(Accounts)]
// pub struct DepositVariableToken<'info> {
//     // your existing pool PDA:
//     #[account(mut, seeds = [b"variable_pool", &pool.identifier], bump)]
//     pub pool: Account<'info, MerkleMountainRange>,

//     // payer of the deposit:
//     #[account(mut)]
//     pub depositor: Signer<'info>,

//     /// SYSVAR_INSTRUCTIONS for reading the memo
//     ///CHECK:
//     pub instruction_account: AccountInfo<'info>,

//     // the depositor’s associated token account:
//     #[account(mut,
//         associated_token::mint = asset_mint,
//         associated_token::authority = depositor
//     )]
//     pub depositor_ata: Account<'info, TokenAccount>,

//     // the pool’s associated token account, must be created ahead of time:
//     #[account(mut,
//         associated_token::mint = asset_mint,
//         associated_token::authority = pool
//     )]
//     pub pool_ata: Account<'info, TokenAccount>,

//     // which mint we’re depositing:
//     pub asset_mint: Account<'info, Mint>,

//     // the CPI programs:
//     pub token_program: Program<'info, Token>,
//     pub associated_token_program: Program<'info, AssociatedToken>,

//     pub system_program: Program<'info, System>,
//     pub rent: Sysvar<'info, Rent>,
// }
