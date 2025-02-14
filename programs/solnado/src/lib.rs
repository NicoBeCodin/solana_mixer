use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::{hash, Hash};


declare_id!("Ag36R1MUAHhyAYB96aR3JAScLqE6YFNau81iCcf2Y6RC");



const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
/// Our fixed deposit of 0.1 SOL.
const FIXED_DEPOSIT_AMOUNT: u64 = (LAMPORTS_PER_SOL as f64 * 0.1) as u64; // 100_000_000

#[program]
pub mod solnado {
    use super::*;

    /// Initialize the Pool account that will store:
    /// - 8 leaves (the Merkle tree is of size 8).
    /// - The current merkle_root after all deposits.
    /// - A used_nullifiers list to prevent double-withdraw.
    pub fn initialize_pool(ctx: Context<InitializePool>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.merkle_root = [0u8; 32];
        pool.next_index = 0;
        pool.used_nullifiers = vec![];
        for i in 0..8 {
            pool.leaves[i] = [0u8; 32];
        }
        msg!("Pool initialized. Ready for deposits.");
        Ok(())
    }

    /// Deposit exactly 0.1 SOL and provide a new leaf (hash of nullifier+secret).
    /// We store the leaf in `pool.leaves[next_index]`, increment next_index,
    /// and recalc the Merkle root.
    pub fn deposit(ctx: Context<Deposit>, leaf: [u8; 32]) -> Result<()> {
        let pool_next_index = ctx.accounts.pool.next_index;
        let depositor = &ctx.accounts.depositor;

        // 1) Ensure we haven't filled all 8 leaves
        require!(pool_next_index < 8, MixerError::TreeIsFull);

        // 2) Transfer 0.1 SOL from the depositor to the Pool account
        let deposit_amount = FIXED_DEPOSIT_AMOUNT;
        **depositor.to_account_info().try_borrow_mut_lamports()? -= deposit_amount;
        **ctx.accounts.pool.to_account_info().try_borrow_mut_lamports()? += deposit_amount;

        // 3) Insert the leaf
        let pool = &mut ctx.accounts.pool;
        let idx = pool.next_index as usize;
        pool.leaves[idx] = leaf;
        pool.next_index += 1;

        // 4) Recompute the merkle_root from all 8 leaves (or up to next_index)
        pool.merkle_root = compute_merkle_root(&pool.leaves[..pool.next_index as usize]);

        // Log
        msg!("Deposit successful! Inserted leaf at index {}. New root: {:?}", idx, pool.merkle_root);
        Ok(())
    }

    /// Withdraw 0.1 SOL by providing:
    /// - A "ZK proof" (placeholder here)
    /// - A nullifier
    ///
    /// We "verify" the proof (fake) and check the nullifier isn't used.
    /// If valid, we transfer 0.1 SOL to the caller.
    pub fn withdraw(
        ctx: Context<Withdraw>,
        _zk_proof: Vec<u8>,     // in real code, you'd pass proof data
        nullifier: [u8; 32],
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let recipient = &ctx.accounts.recipient;

        // 1) Fake ZK proof verification
        //    In a real system, you'd call a cryptographic verifier here.
        let proof_is_valid = verify_zk_proof_placeholder(&_zk_proof);
        require!(proof_is_valid, MixerError::InvalidProof);

        // 2) Check nullifier is unused
        let used_list = &pool.used_nullifiers;
        require!(!used_list.contains(&nullifier), MixerError::NullifierAlreadyUsed);

        // 3) Mark it as used
        pool.used_nullifiers.push(nullifier);

        // 4) Transfer 0.1 SOL to the recipient
        let withdraw_amount = FIXED_DEPOSIT_AMOUNT;
        **ctx.accounts.pool.to_account_info().try_borrow_mut_lamports()? -= withdraw_amount;
        **recipient.to_account_info().try_borrow_mut_lamports()? += withdraw_amount;

        msg!("Withdrawal successful! Nullifier: {:?}", nullifier);
        Ok(())
    }
}

//
// Accounts
//

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = authority,
        // We'll allocate enough space for the Pool struct.
        space = 8 + Pool::MAX_SIZE
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
    pub leaves: [[u8; 32]; 8],

    /// Next free index in `leaves`
    pub next_index: u8,

    /// Set of used nullifiers to prevent double-withdraw
    pub used_nullifiers: Vec<[u8; 32]>,
}

impl Pool {
    /// For a small struct, you can over-allocate a bit. 
    ///  - merkle_root: 32 bytes
    ///  - leaves: 8 * 32 = 256 bytes
    ///  - next_index: 1 byte
    ///  - used_nullifiers (Vec<[u8;32]>): We can give it some buffer, e.g. up to 8 or more.
    pub const MAX_SIZE: usize = 32 + (8 * 32) + 1 + (8 * 32) + 100;
}

//
// Errors
//
#[error_code]
pub enum MixerError {
    #[msg("The merkle tree is already full (8 leaves).")]
    TreeIsFull,

    #[msg("The ZK proof is invalid.")]
    InvalidProof,

    #[msg("Nullifier already used.")]
    NullifierAlreadyUsed,
}

//
// A naive merkle root for demonstration (2-layer approach for 8 leaves).
// In reality, you'd do a standard binary tree approach with repeated hashing
// or a more advanced circuit-friendly hashing. Here we do a linear chain just to illustrate.
//
fn compute_merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    // Very naive: just hash all leaves together in sequence
    // A real solution would build a full binary tree, but we keep it short here.
    if leaves.is_empty() {
        return [0u8; 32];
    }
    let mut acc = leaves[0];
    for i in 1..leaves.len() {
        let combined = [acc.as_ref(), leaves[i].as_ref()].concat();
        acc = hash(&combined).to_bytes();
    }
    // The final "acc" is our naive root
    acc
}

//
// Placeholder ZK proof checker
// In a real production mixer, you'd use a SNARK or STARK verifier
// to validate that the user truly has a leaf in the tree.
//
fn verify_zk_proof_placeholder(_proof: &[u8]) -> bool {
    // For now, we just pretend it's always valid.
    true
}