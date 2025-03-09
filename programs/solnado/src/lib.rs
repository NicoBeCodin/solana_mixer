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

pub const DEFAULT_LEAF: [u8; 32] = [0u8; 32];
pub const TREE_DEPTH: u8 = 4;
pub const LEAVES_LENGTH: usize = 16;
pub const NULLIFIER_LIST_LENGTH: usize = 16;
pub const DEFAULT_LEAF_HASH: [u8; 32] = [
    42, 9, 169, 253, 147, 197, 144, 194, 107, 145, 239, 251, 178, 73, 159, 7, 232, 247, 170, 18,
    226, 180, 148, 10, 58, 237, 36, 17, 203, 101, 225, 28,
]; //solana poseidon
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const FIXED_DEPOSIT_AMOUNT: u64 = ((LAMPORTS_PER_SOL as f64) * 0.01) as u64; // 10_000_000 Low for testing purposes

declare_id!("Ag36R1MUAHhyAYB96aR3JAScLqE6YFNau81iCcf2Y6RC");

/// Our fixed deposit of 0.1 SOL.

#[program]
pub mod solnado {
    use crate::error::ErrorCode;

    use super::*;

    pub fn initialize_pool(ctx: Context<InitializePool>, identifier: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        pool.identifier = identifier;
        pool.leaves = default_leaves();
        pool.merkle_root = get_root(&pool.leaves);
        pool.batch_number = 0;
        pool.depth = [0; 16];
        pool.number_of_peaks = 0;
        pool.peaks = [DEFAULT_LEAF; 16];
        msg!("Pool initialized with {:?} as root", pool.merkle_root);
        msg!("This should correspond to {:?}", get_default_root_depth(4));
        Ok(())
    }

    pub fn deposit(ctx: Context<Deposit>, leaf_hash: [u8; 32]) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let free = pool.find_first_match();

        if free <= 15 {
            let start_index = free;
            let transfer_instruction = system_instruction::transfer(
                &ctx.accounts.depositor.key(),
                &ctx.accounts.pool.key(),
                FIXED_DEPOSIT_AMOUNT
            );

            let _ = invoke(
                &transfer_instruction,
                &[ctx.accounts.depositor.to_account_info(), ctx.accounts.pool.to_account_info()]
            );
            msg!("Transfered {} lamports to pool", FIXED_DEPOSIT_AMOUNT);
            let pool = &mut ctx.accounts.pool;
            pool.leaves[start_index] = leaf_hash;

            msg!("Leaf {:?} \nadded at index {}", leaf_hash, start_index);
            pool.merkle_root = get_root(&pool.leaves);
            msg!("New root of temporary pool is {:?}", pool.merkle_root);

            Ok(())
        } else {
            // Pool is full: create batch data and call store_batch_with_ledger via CPI.
            let leaves = &pool.leaves;
            let pool_key = &pool.key();
            let batch_data: Vec<u8> = Vec::new();
            // for leaf in &pool.leaves {
            //     batch_data.extend_from_slice(leaf);
            // }
            let discriminator = [76, 65, 183, 124, 253, 177, 208, 199];
            let identifier: [u8; 8] = pool.identifier.to_le_bytes();
            let batch_number: [u8; 8] = pool.batch_number.to_le_bytes();
            // Prepend the discriminator to the batch data.
            let mut cpi_data = discriminator.to_vec();
            cpi_data.extend_from_slice(&identifier);
            cpi_data.extend_from_slice(&batch_number);
            for leaf in &pool.leaves {
                cpi_data.extend_from_slice(leaf);
            }
            msg!("Discriminator length: {}", discriminator.len()); // Should be 8
            msg!("Identifier length: {}", identifier.len()); // Should be 8
            msg!("Batch number length: {}", batch_number.len()); // Should be 8
            msg!("Batch data length: {}", batch_data.len()); // Should be 512
            msg!("Total expected length: {}", 8 + 8 + 8 + batch_data.len());

            // let batch_array: [[u8;32]; 16] =batch_data.try_into().expect("Failed");
            // cpi_data.extend_from_slice(batch_array);
            let store_batch = &ctx.accounts.store_batch;

            //The recipeint program
            let store_batch_key = Pubkey::from_str(
                "7GHv6NewxZEFDjkUor8Ko3DG9BbMu9UwvHz9ZhgEsoZF"
            ).unwrap();
            if store_batch.key != &store_batch_key {
                return Err(ErrorCode::InvalidStoreBatchAccount.into());
            }
            msg!("cpi_data length: {}", cpi_data.len());
            msg!("cpi_data: {:?}", cpi_data);
            // build_memo(&cpi_data, [pool_sol_key]);
            let store_batch_ix = anchor_lang::solana_program::instruction::Instruction {
                program_id: *store_batch.key,
                accounts: vec![
                    // AccountMeta::new(*ctx.accounts.pool.to_account_info().key, false),
                    AccountMeta::new(*pool_key, false),
                    AccountMeta::new_readonly(ctx.accounts.system_program.key(), false)
                ],
                data: cpi_data, // This is our batch data (16 leaves concatenated)
            };

            // Invoke the CPI (it calls store_batch_with_ledger)
            let pool_id_bytes = &pool.identifier.to_le_bytes();
            let seeds: &[&[u8]] = &[b"pool_merkle".as_ref(), pool_id_bytes.as_ref()];
            let (_, bump) = Pubkey::find_program_address(seeds, &crate::ID);
            let signer_seeds = &[b"pool_merkle".as_ref(), pool_id_bytes.as_ref(), &[bump]];

            anchor_lang::solana_program::program::invoke_signed(
                &store_batch_ix,
                &[pool.to_account_info(), ctx.accounts.system_program.to_account_info()],
                &[signer_seeds]
            )?;
            msg!("Batch transaction created with calldata!");

            // After CPI returns, reset the pool:
            let new_batch = get_root(leaves);
            pool.update_peaks(new_batch);
            pool.batch_number += 1;
            msg!("New batch number {}", pool.batch_number);
            pool.whole_tree_root = pool.compute_root_from_peaks();

            msg!("New root of the whole tree: {:?}", &pool.whole_tree_root);
            // Clear the pool leaves
            pool.leaves = default_leaves();
            // Add the new batch merkle root as the first leaf
            pool.merkle_root = get_root(&pool.leaves);
            Ok(())
        }
    }

    /// Transfers `amount` lamports from the pool PDA to a recipient.
    /// `identifier` is used in deriving the pool PDA.
    pub fn admin_transfer(ctx: Context<AdminTransfer>, amount: u64, identifier: u64) -> Result<()> {
        let pool_info = &ctx.accounts.pool;
        let recipient_info = &ctx.accounts.recipient;
        let system_program_info = &ctx.accounts.system_program;

        // Derive the pool PDA from the identifier.
        let seed = identifier.to_le_bytes();
        let seeds: &[&[u8]] = &[b"pool_merkle", &seed];
        let (pda, bump) = Pubkey::find_program_address(seeds, ctx.program_id);
        require!(pda == *pool_info.key, ErrorCode::InvalidPoolAccount);

        **pool_info.try_borrow_mut_lamports()? -= amount;
        **recipient_info.try_borrow_mut_lamports()? += amount;
        msg!("Succesfully transfered {} lamports", amount);

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

        //It's the opposite, the first 32 are the nullifier and the rest the root
        let nullifier_hash: &[u8; 32] = public_inputs[0..32]
            .try_into()
            .expect("Failed converting nullifier to hash");
        let public_input_root: [u8; 32] = public_inputs[32..64]
            .try_into()
            .expect("Failed converting the public_input root");

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

        // Otherwise, create the account.
        // (Assume a minimal account size of 8 bytes; adjust as needed.)
        let rent = Rent::get()?;
        let space = 8;
        let lamports = rent.minimum_balance(space);
        let create_ix = system_instruction::create_account(
            &ctx.accounts.withdrawer.key(), // payer
            &nullifier_pda, // new account address
            lamports,
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

        if pool.merkle_root != public_input_root {
            msg!("Public input root: {:?}", public_input_root);
            msg!("Tree root is {:?}", pool.merkle_root);
            return Err(ErrorCode::InvalidPublicInputRoot.into());
        }

        let current_root = &pool.merkle_root;
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

        let amount = FIXED_DEPOSIT_AMOUNT; // 0.1 SOL

        **ctx.accounts.pool.to_account_info().try_borrow_mut_lamports()? -= amount;
        **ctx.accounts.withdrawer.try_borrow_mut_lamports()? += amount;
        Ok(())
    }
}
