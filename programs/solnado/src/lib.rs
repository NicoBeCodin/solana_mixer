use anchor_lang::prelude::*;
pub mod state;
pub mod error;
pub mod utils;
pub mod verifying_key;
use crate::error::ErrorCode;
use crate::state::*;
use crate::utils::*;
use anchor_lang::solana_program::{ program::{ invoke, invoke_signed }, system_instruction };
use std::str::FromStr;
use base64::{ engine::general_purpose, Engine as _ };
pub const DEFAULT_LEAF: [u8; 32] = [0u8; 32];
pub const TREE_DEPTH: u8 = 4;
pub const LEAVES_LENGTH: usize = 16;
pub const NULLIFIER_LIST_LENGTH: usize = 16;
pub const DEFAULT_LEAF_HASH: [u8; 32] = [
    42, 9, 169, 253, 147, 197, 144, 194, 107, 145, 239, 251, 178, 73, 159, 7, 232, 247, 170, 18,
    226, 180, 148, 10, 58, 237, 36, 17, 203, 101, 225, 28,
]; //solana poseidon

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const FIXED_DEPOSIT_AMOUNT: u64 = ((LAMPORTS_PER_SOL as f64) * 0.001) as u64; // 10_000_000 Low for testing purposes
const PROGRAM_FEE: u64 = 1_000_000; //0.001 SOL FEE PER WITHDRAWAL

const TARGET_DEPTH: usize = 8; //This means pools are capped to 256 leaves
// declare_id!("Ag36R1MUAHhyAYB96aR3JAScLqE6YFNau81iCcf2Y6RC");
// declare_id!("EKadvTET2vdCkurkYFu69v2iXdsAwHs3rQPj8XL5AUin");
declare_id!("URAeHt7FHf56ioY2XJNXbSx5Y3FbvQ9zeLGRpY1RiMD");

const ADMIN_KEY: Pubkey = pubkey!("EJZQiTeikeg8zgU7YgRfwZCxc9GdhTsYR3fQrXv3uK9V");

/// Our fixed deposit of 0.1 SOL.

#[program]
pub mod solnado {
    use solana_program::sysvar::instructions;

    use crate::error::ErrorCode;
    use super::*;

    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        identifier: [u8; 16],
        deposit_amount: u64,
        creator_fee: u64
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        pool.identifier = identifier;
        let pool_string = std::str
            ::from_utf8(&identifier)
            .unwrap_or("Invalid utf_8")
            .trim_end_matches(char::from(0));
        pool.leaves = default_leaves();
        pool.merkle_root = get_root(&pool.leaves);
        pool.batch_number = 0;
        pool.depth = [0; 16];
        pool.number_of_peaks = 0;
        pool.peaks = [DEFAULT_LEAF; 16];
        pool.max_leaves = (2_u32).pow(TARGET_DEPTH as u32) as u32;
        pool.creator = ctx.accounts.authority.key();
        //Creator fee should be capped
        require!(
            creator_fee <= deposit_amount.checked_div(10).unwrap(),
            ErrorCode::CreatorFeeTooHigh
        );
        pool.creator_fee = creator_fee;
        // At least a 0.01 SOL DEPOSIT
        require!(deposit_amount >= 10_000_000, ErrorCode::InvalidDepositAmount);

        pool.deposit_amount = deposit_amount;

        msg!(
            "Pool initialized by signer: {}\n
        Pool name: {}\n
        Deposit amount: {}\n
        Creator_fee: {}\n",
            ctx.accounts.authority.key(),
            pool_string,
            deposit_amount,
            creator_fee
        );

        msg!("Pool initialized with {:?} as root", pool.merkle_root);
        msg!("This should correspond to {:?}", get_default_root_depth(4));

        Ok(())
    }

    pub fn deposit(
        ctx: Context<Deposit>,
        leaf_hash: [u8; 32]
        //leaves_info: [u8; 520]
    ) -> Result<()> {
        let pool_info = ctx.accounts.pool.to_account_info();
        let pool_batch = ctx.accounts.pool.batch_number.clone();
        let pool = &mut ctx.accounts.pool;

        let free = pool.find_first_match();
        let sysvar_account = &ctx.accounts.instruction_account;

        // Load the full instruction list (memo should be at index 0 based on your tx order)
        let maybe_memo_ix = instructions::load_instruction_at_checked(0, sysvar_account)?;

        // msg!("Instruction details of this transaction: {:?}", maybe_memo_ix);

        // Verify that it's the Memo program
        let memo_program_id = Pubkey::from_str(
            "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"
        ).unwrap();
        if maybe_memo_ix.program_id != memo_program_id {
            msg!("First instruction is not a Memo, skipping decoding");
            return Err(ErrorCode::MissingMemoInstruction.into());
        }

        // Memo data is a UTF-8 base64 string
        let memo_base64 = std::str
            ::from_utf8(&maybe_memo_ix.data)
            .map_err(|_| ErrorCode::InvalidMemoUtf8)?;

        let memo_bytes = general_purpose::STANDARD
            .decode(memo_base64)
            .map_err(|_| ErrorCode::InvalidMemoBase64)?;
        // Now you have back the original Buffer.concat([batchNumber, leaves])
        if memo_bytes.len() != 520 {
            msg!("Memo bytes length invalid: got {}", memo_bytes.len());
            return Err(ErrorCode::InvalidMemoLength.into());
        }

        // Extract the batch number and leaves from the memo
        let user_batch_number = u64::from_be_bytes(
            memo_bytes[0..8].try_into().map_err(|_| ErrorCode::FailedToParseBatch)?
        );

        let mut user_leaves: [[u8; 32]; 16] = [[0u8; 32]; 16];
        for i in 0..16 {
            user_leaves[i].copy_from_slice(&memo_bytes[8 + i * 32..8 + (i + 1) * 32]);
        }

        msg!("User batch number from memo: {}", user_batch_number);
        msg!("First leaf in memo: {:?}", user_leaves[0]);

        // OPTIONAL: Compare to current pool state
        if &user_batch_number != &pool_batch {
            return Err(ErrorCode::InvalidUserBatchNumber.into());
        }

        if free <= 15 {
            let transfer_instruction = system_instruction::transfer(
                &ctx.accounts.depositor.key(),
                &*pool_info.key,
                pool.deposit_amount
            );

            let _ = invoke(
                &transfer_instruction,
                &[ctx.accounts.depositor.to_account_info(), pool_info]
            );
            let start_index = free;
            msg!("Transfered {} lamports to pool", FIXED_DEPOSIT_AMOUNT);
            // let pool = &mut ctx.accounts.pool;
            pool.leaves[start_index] = leaf_hash;

            msg!("Leaf {:?} \nadded at index {}", leaf_hash, start_index);
            pool.merkle_root = get_root(&pool.leaves);

            if &user_leaves != &pool.leaves {
                msg!("Leaves mismatch!");
                return Err(ErrorCode::InvalidUserLeaves.into());
            }

            msg!("New root of temporary pool is {:?}", pool.merkle_root);
            if free == 15 {
                //After adding the leaf we create a new temporary pool
                msg!(
                    "Temporary pool is now at max capacity, storing the hash and creating a new one"
                );
                //After adding the leaf, we need to create a new pool
                let new_batch = pool.merkle_root.clone();
                pool.update_peaks(new_batch);
                pool.batch_number += 1;
                msg!("New batch number {}", pool.batch_number);

                let new_root = pool.compute_root_from_peaks();
                let current_depth = next_power_of_two_batch(pool.batch_number as usize);
                pool.whole_tree_root = new_root;
                msg!("New root of the whole tree: {:?}", &pool.whole_tree_root);
                let deep_root = pool.deepen(current_depth, TARGET_DEPTH);
                msg!("Computed deep root with target depth: {} \n{:?}", TARGET_DEPTH, deep_root);

                // Clear the pool leaves
                pool.leaves = default_leaves();
                pool.merkle_root = get_root(&pool.leaves);
            }
            Ok(())
        } else {
            return Err(ErrorCode::InvalidIndexing.into());
        }
    }
    pub fn initialize_treasury(ctx: Context<InitializeTreasury>) -> Result<()> {
        require_keys_eq!(ctx.accounts.payer.key(), ADMIN_KEY, ErrorCode::UnauthorizedAction);
        Ok(())
    }

    pub fn withdraw_from_treasury(ctx: Context<WithdrawFromTreasury>, amount: u64) -> Result<()> {
        // Check authority
        require_keys_eq!(ctx.accounts.authority.key(), ADMIN_KEY, ErrorCode::UnauthorizedAction);

        // Transfer lamports from PDA to authority
        **ctx.accounts.treasury.to_account_info().try_borrow_mut_lamports()? -= amount;
        **ctx.accounts.authority.to_account_info().try_borrow_mut_lamports()? += amount;

        Ok(())
    }

    pub fn withdraw(
        ctx: Context<Withdraw>,
        proof: [u8; 256], // Real proof (a,b and c)
        public_inputs: [u8; 64] //root & nullifier hash
    ) -> Result<()> {
        let pool = &ctx.accounts.pool;
        let system_program = &ctx.accounts.system_program;

        if proof.len() != 256 {
            msg!("Invalid proof length!");
            return Err(ErrorCode::InvalidArgument.into());
        }

        let nullifier_hash: &[u8; 32] = public_inputs[0..32]
            .try_into()
            .expect("Failed converting nullifier to hash");
        let public_input_root: [u8; 32] = public_inputs[32..64]
            .try_into()
            .expect("Failed converting the public_input root");

        //Nullifier pda creation to store nullifier hash
        let (nullifier_pda, bump) = Pubkey::find_program_address(
            &[nullifier_hash.as_ref()],
            ctx.program_id
        );
        let nullifier_account = &ctx.accounts.nullifier_account;
        if &nullifier_pda != nullifier_account.key {
            msg!("The provided nullifier account and nullifier derived pda do not match.");
            return Err(ErrorCode::InvalidNullifierAccount.into());
        }

        if nullifier_account.lamports() != 0 {
            msg!("The nullifier account balance is not zero, it has already been initialized");
            return Err(ErrorCode::NullifierAlreadyUsed.into());
        }
        require!(ctx.accounts.pool_creator.key()==pool.creator, ErrorCode::InvalidPoolCreator);

        // Otherwise, create the account.
        // (Assume a minimal account size of 8 bytes; adjust as needed.)
        let rent = Rent::get()?;
        let space = 8;
        let rent_lamports = rent.minimum_balance(space);
        let create_ix = system_instruction::create_account(
            &ctx.accounts.withdrawer.key(), // payer
            &nullifier_pda, // new account address
            rent_lamports,
            space as u64,
            ctx.program_id // owner: our program
        );
        let seeds = &[nullifier_hash.as_ref(), &[bump]];
        invoke_signed(
            &create_ix,
            &[
                ctx.accounts.withdrawer.to_account_info(),
                nullifier_account.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[seeds]
        )?;
        msg!("Creating nullifier PDA costs {} lamports", rent_lamports);

        **ctx.accounts.withdrawer.try_borrow_mut_lamports()? -= rent_lamports;
        **ctx.accounts.nullifier_account.try_borrow_mut_lamports()? += rent_lamports;

        // Mark the PDA as rent-exempt by allocating space & assigning ownership
        ctx.accounts.nullifier_account.realloc(space, false)?;
        ctx.accounts.nullifier_account.assign(ctx.program_id);

        let depth = next_power_of_two_batch(pool.batch_number as usize);
        msg!("Current depth: {}", depth);

        //This allows to deepen the tree to match a certain size
        let deepen_root = pool.deepen(depth, TARGET_DEPTH);
        if deepen_root != public_input_root {
            msg!("Deepened root isn't same as public_input_root");
            msg!("Deepen root {:?}\n public_input_root {:?}", deepen_root, public_input_root);
            return Err(ErrorCode::InvalidPublicInputRoot.into());
        }

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

        let withdraw_pool_amount = pool.deposit_amount;

        let withdrawer_amount = pool.deposit_amount
            .checked_sub(pool.creator_fee)
            .unwrap()
            .checked_sub(PROGRAM_FEE)
            .unwrap();
        let creator_amount = pool.creator_fee;

        **ctx.accounts.pool.to_account_info().try_borrow_mut_lamports()? -= withdraw_pool_amount;

        **ctx.accounts.treasury.to_account_info().try_borrow_mut_lamports()? += PROGRAM_FEE;
        **ctx.accounts.withdrawer.try_borrow_mut_lamports()? += withdrawer_amount;
        **ctx.accounts.pool_creator.try_borrow_mut_lamports()? += creator_amount;
        msg!(
            "Withdrew {} lamports from pool\nTransfered {} lamports to user\n Transfered {} to pool creator",
            withdraw_pool_amount,
            withdrawer_amount,
            creator_amount,
        );

        Ok(())
    }

    /// Transfers `amount` lamports from the pool PDA to a recipient
    /// This will be deleted in final version
    pub fn admin_transfer(ctx: Context<AdminTransfer>, amount: u64, identifier: u64) -> Result<()> {
        let pool_info = &ctx.accounts.pool;

        let recipient_info = &ctx.accounts.recipient;
        let system_program_info = &ctx.accounts.system_program;

        // Derive the pool PDA from the identifier.
        let seed = identifier.to_le_bytes();
        let seeds: &[&[u8]] = &[b"pool_merkle", &seed];
        let (pda, _) = Pubkey::find_program_address(seeds, ctx.program_id);
        require!(pda == *pool_info.key, ErrorCode::InvalidPoolAccount);

        **pool_info.try_borrow_mut_lamports()? -= amount;
        **recipient_info.try_borrow_mut_lamports()? += amount;
        msg!("Succesfully transfered {} lamports", amount);

        Ok(())
    }
}
