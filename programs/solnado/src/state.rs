use anchor_lang::prelude::*;
use crate::{ utils::next_power_of_two_batch, DEFAULT_LEAF, LEAVES_LENGTH, TARGET_DEPTH };
use solana_poseidon::{ Parameters, hashv, Endianness };
use crate::utils::get_default_root_depth;

#[derive(Accounts)]
#[instruction(identifier: [u8;16])]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = authority,
        // We'll allocate enough space for the Pool struct.
        space = 8 + Pool::MAX_SIZE,
        seeds = [b"pool_merkle".as_ref(), &identifier],
        bump
    )]
    pub pool: Account<'info, Pool>,

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
        space = 8 + VariablePool::MAX_SIZE,
        seeds = [b"variable_pool".as_ref(), &identifier],
        bump
    )]
    pub pool: Account<'info, VariablePool>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}



#[derive(Accounts)]
#[instruction()]
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
    #[account(mut)]
    pub pool: Account<'info, VariablePool>,

    #[account(mut)]
    pub depositor: Signer<'info>,

    ///CHECK : SYSVAR for instructions
    pub instruction_account: AccountInfo<'info>,
    pub system_program: Program<'info, System>,

}

#[derive(Accounts)]
pub struct CombineDeposit<'info> {
    #[account(mut)]
    pub pool: Account<'info, VariablePool>,

    ///CHECK: Nullifier #1 PDA
    #[account(mut)]
    pub nullifier1_account: AccountInfo<'info>,

    ///CHECK: Nullifier #2 PDA
    #[account(mut)]
    pub nullifier2_account: AccountInfo<'info>,

    #[account(mut)]
    pub user: Signer<'info>,

    /// SYSVAR_INSTRUCTIONS must be passed to read the Memo
    ///CHECK:
    pub instruction_account: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WithdrawVariable<'info> {
    /// Our on‐chain pool state
    #[account(
        mut,
        seeds = [ b"variable_pool", pool.identifier.as_ref() ],
        bump,           // assumes you store `bump: u8` in your pool struct
      )]
    pub pool: Account<'info, VariablePool>,

    /// The user who is withdrawing
    #[account(mut)]
    pub user: Signer<'info>,

    /// A PDA whose seed is exactly the 32‐byte nullifier hash.
    /// We create it here (with size=0) to mark “this nullifier was spent.”
    /// CHECK: we're only using this for lamport‐zero and PDA‐existence checks.
    #[account(mut)]
    pub nullifier_account: AccountInfo<'info>,

    /// System program (for create_account + transfer)
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,

    #[account(mut)]
    pub depositor: Signer<'info>,

    ///CHECK : SYSVAR for instructions
    pub instruction_account: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}



#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,

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

//
// Pool Account Data
//
#[account]
pub struct Pool {
    /// Current merkle root after all deposits
    pub merkle_root: [u8; 32], //8 + 32 + 512 + 16 + 32 + 8 + 8 + 32 + 8 + 512 + 16 + 1 + 4

    /// Leaves array of size 8
    pub leaves: [[u8; 32]; 16],

    // Set of used nullifiers to prevent double-withdraw
    pub identifier: [u8;16],
    pub creator: Pubkey,
    pub creator_fee: u64, 
    pub deposit_amount: u64,
    //This will serve to reconstruct the tree
    pub whole_tree_root: [u8; 32],
    pub batch_number: u64,
    pub peaks: [[u8; 32]; 16], //Peaks to build merkle tree without storing everything
    pub depth: [u8; 16], //With each peak with associate a depth
    pub number_of_peaks: u8,
    pub max_leaves: u32, //MAX number of leaves in a pool
}

#[account]
pub struct VariablePool {
    /// Current merkle root after all deposits
    pub merkle_root_batch: [u8; 32],
    /// Leaves array of size 16
    pub batch_leaves: [[u8; 32]; 16],
    // Set of used nullifiers to prevent double-withdraw
    pub identifier: [u8;16],
    //No creator fee
    //Minimùum deposit amount
    pub min_deposit_amount: u64,
    //This will serve to reconstruct the tree
    pub whole_tree_root: [u8; 32],
    pub batch_number: u64,
    pub peaks: [[u8; 32]; 16], //Peaks to build merkle tree without storing everything
    pub depth: [u8; 16], //With each peak with associate a depth
    pub number_of_peaks: u8,
    pub max_leaves: u32, //MAX number of leaves in a pool
}


impl VariablePool {
    pub const MAX_SIZE: usize =  32 + 512 + 16 + 32 + 8 + 8 + 32 + 8 + 512 + 16 + 1 + 4 +  100;

    pub fn get_free_leaf(&self) -> usize {
        let mut start_index = 0;
        //find the first non default leaf
        while self.batch_leaves[start_index] != DEFAULT_LEAF && start_index <= LEAVES_LENGTH - 1 {
            start_index += 1;
        }
        if start_index == LEAVES_LENGTH {
            msg!("Tree is already full, creating new one");
            //This is for when a new batch is created
            return 99;
        }

        start_index
    }

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
                .unwrap().to_bytes();
                
            let merged_depth = peak_depths[(count - 1) as usize] + 1;
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
        msg!("peaks after update: {:?}", &self.peaks[..self.number_of_peaks as usize]);
        msg!("depth after update: {:?}", &self.depth[..self.number_of_peaks as usize]);
        msg!("number of peaks after update: {}", self.number_of_peaks);
    }


    // Helper function to merge two nodes with potentially different depths.
    pub fn merge_nodes(a: ([u8; 32], u8), b: ([u8; 32], u8)) -> ([u8; 32], u8) {
        let (mut a_node, mut a_depth) = a;
        let (mut b_node, mut b_depth) = b;

        // "Lift" the shallower node until both depths match.
        while a_depth < b_depth {
            let default = get_default_root_depth(a_depth as usize);
            a_node = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&a_node, &default])
                .unwrap()
                .to_bytes();
            a_depth += 1;
        }
        while b_depth < a_depth {
            let default = get_default_root_depth(b_depth as usize);
            b_node = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&b_node, &default])
                .unwrap()
                .to_bytes();
            b_depth += 1;
        }

        // Now both nodes are at the same depth. Merge them.
        let merged = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&a_node, &b_node])
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
                        &[&node, &default]
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
            &[&self.whole_tree_root, &default_hash]
        )
            .unwrap()
            .to_bytes();
        let range = current_depth + 1..wanted_depth;
        for x in range {
            default_hash = get_default_root_depth(x);
            hashed = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&hashed, &default_hash])
                .unwrap()
                .to_bytes();
        }
        hashed
    }
    pub fn compare_to_deep(&self, user_root: [u8;32])->bool{
        let current_depth = next_power_of_two_batch(self.batch_number as usize);
        let deep_root= self.deepen(current_depth, TARGET_DEPTH);
        if !(user_root==deep_root){
            msg!("user root: {:?} \n deep_root: {:?}", user_root, deep_root);
            return false
        }
        return true
    }
}



impl Pool {
    pub const MAX_SIZE: usize =  32 + 512 + 16 + 32 + 8 + 8 + 32 + 8 + 512 + 16 + 1 + 4 +  100;

    pub fn get_free_leaf(&self) -> usize {
        let mut start_index = 0;
        //find the first non default leaf
        while self.leaves[start_index] != DEFAULT_LEAF && start_index <= LEAVES_LENGTH - 1 {
            start_index += 1;
        }
        if start_index == LEAVES_LENGTH {
            msg!("Tree is already full, creating new one");
            //This is for when a new batch is created
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
            peak_hashes[i] = [0u8; 32]; // or DEFAULT_LEAF if defined
            peak_depths[i] = 0;
        }

        self.peaks = peak_hashes;
        self.depth = peak_depths;
        self.number_of_peaks = count;
        msg!("peaks after update: {:?}", &self.peaks[..self.number_of_peaks as usize]);
        msg!("depth after update: {:?}", &self.depth[..self.number_of_peaks as usize]);
        msg!("number of peaks after update: {}", self.number_of_peaks);
    }


    // Helper function to merge two nodes with potentially different depths.
    pub fn merge_nodes(a: ([u8; 32], u8), b: ([u8; 32], u8)) -> ([u8; 32], u8) {
        let (mut a_node, mut a_depth) = a;
        let (mut b_node, mut b_depth) = b;

        // "Lift" the shallower node until both depths match.
        while a_depth < b_depth {
            let default = get_default_root_depth(a_depth as usize);
            a_node = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&a_node, &default])
                .unwrap()
                .to_bytes();
            a_depth += 1;
        }
        while b_depth < a_depth {
            let default = get_default_root_depth(b_depth as usize);
            b_node = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&b_node, &default])
                .unwrap()
                .to_bytes();
            b_depth += 1;
        }

        // Now both nodes are at the same depth. Merge them.
        let merged = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&a_node, &b_node])
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
                        &[&node, &default]
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
            &[&self.whole_tree_root, &default_hash]
        )
            .unwrap()
            .to_bytes();
        let range = current_depth + 1..wanted_depth;
        for x in range {
            default_hash = get_default_root_depth(x);
            hashed = hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&hashed, &default_hash])
                .unwrap()
                .to_bytes();
        }
        hashed
    }
    pub fn compare_to_deep(&self, user_root: [u8;32])->bool{
        let current_depth = next_power_of_two_batch(self.batch_number as usize);
        let deep_root= self.deepen(current_depth, TARGET_DEPTH);
        return user_root==deep_root
    }
}


// Gonna delete this 
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