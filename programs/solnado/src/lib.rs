use anchor_lang::prelude::*;
pub mod error;
pub mod state;
pub mod utils;
pub mod verifying_key;
use crate::error::ErrorCode;
use crate::state::*;
use crate::utils::*;
use anchor_lang::solana_program::{
    program::{invoke, invoke_signed},
    system_instruction,
    sysvar::instructions,
};
use base64::{engine::general_purpose, Engine as _};
pub const DEFAULT_LEAF: [u8; 32] = [0u8; 32];
pub const TREE_DEPTH: u8 = 4;
pub const LEAVES_LENGTH: usize = 16;
pub const NULLIFIER_LIST_LENGTH: usize = 16;
pub const DEFAULT_LEAF_HASH: [u8; 32] = [
    42, 9, 169, 253, 147, 197, 144, 194, 107, 145, 239, 251, 178, 73, 159, 7, 232, 247, 170, 18,
    226, 180, 148, 10, 58, 237, 36, 17, 203, 101, 225, 28,
]; //solana poseidon
const MIN_PDA_SIZE: usize = 1;
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const FIXED_DEPOSIT_AMOUNT: u64 = ((LAMPORTS_PER_SOL as f64) * 0.001) as u64; // 10_000_000 Low for testing purposes
const PROGRAM_FEE: u64 = 1_000_000; //0.001 SOL FEE PER WITHDRAWAL

const TARGET_DEPTH: usize = 20; //This means pools are capped to 256 leaves
                                // declare_id!("Ag36R1MUAHhyAYB96aR3JAScLqE6YFNau81iCcf2Y6RC");
                                // declare_id!("EKadvTET2vdCkurkYFu69v2iXdsAwHs3rQPj8XL5AUin");
                                // declare_id!("URAeHt7FHf56ioY2XJNXbSx5Y3FbvQ9zeLGRpY1RiMD");
declare_id!("FyAuPyboHtdnnqbcAhTXjKwXRNqxiWYK4Xwvc5Gtw8Ln");

// const ADMIN_KEY: Pubkey = pubkey!("EJZQiTeikeg8zgU7YgRfwZCxc9GdhTsYR3fQrXv3uK9V");
const ADMIN_KEY: Pubkey = pubkey!("BSpEVXMrA3C1myPSUmT8hQSecrvJaUin8vnQTfzGGf17");

#[program]
pub mod solnado {

    use num_bigint::BigInt;
    use std::ops::Deref;

    use super::*;
    use crate::error::ErrorCode;

    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        identifier: [u8; 16],
        deposit_amount: u64,
        creator_fee: u64,
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        pool.identifier = identifier;
        let pool_string = std::str::from_utf8(&identifier)
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
        require!(
            deposit_amount >= 10_000_000,
            ErrorCode::InvalidDepositAmount
        );

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

    pub fn initialize_variable_pool(
        ctx: Context<InitializeVariablePool>,
        identifier: [u8; 16],
    ) -> Result<()> {
        msg!("Initializing a variable pool, will be used for variable amounts deposits");

        let pool = &mut ctx.accounts.pool;

        pool.identifier = identifier;
        let pool_string = std::str::from_utf8(&identifier)
            .unwrap_or("Invalid utf_8")
            .trim_end_matches(char::from(0));
        pool.batch_leaves = default_leaves();
        pool.merkle_root_batch = get_root(&pool.batch_leaves);
        pool.batch_number = 0;
        pool.depth = [0; 16];
        pool.number_of_peaks = 0;
        pool.peaks = [DEFAULT_LEAF; 16];
        pool.max_leaves = (2_u32).pow(TARGET_DEPTH as u32) as u32;
        pool.min_deposit_amount = 10_000_000;
        // At least a 0.01 SOL DEPOSIT per action of use

        msg!(
            "Pool initialized by signer: {}\n
        Pool name: {}\n
        Pool max leaves: {}\n",
            ctx.accounts.authority.key(),
            pool_string,
            pool.max_leaves
        );

        msg!(
            "Variable pool initialized with {:?} as root",
            pool.merkle_root_batch
        );
        msg!("This should correspond to {:?}", get_default_root_depth(4));

        Ok(())
    }

    pub fn deposit_variable(
        ctx: Context<DepositVariable>,
        proof: [u8; 256],
        public_inputs: [u8; 96],
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let depositor_ai = ctx.accounts.depositor.to_account_info();
        let pool_ai = pool.to_account_info();
        let sysvar_ai = &ctx.accounts.instruction_account;

        // 1) Verify ZK proof and unpack
        let (sum_be, leaf1, leaf2) = verify_deposit_proof(&proof, &public_inputs)?;
        let deposit_sum = u64::from_be_bytes(sum_be[24..32].try_into().unwrap());
        msg!("🔐 Proof ok — depositing {} lamports", deposit_sum);

        // 2) Transfer lamports
        invoke(
            &system_instruction::transfer(
                &ctx.accounts.depositor.key(),
                &pool_ai.key(),
                deposit_sum,
            ),
            &[
                depositor_ai.clone(),
                pool_ai.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        let leaves = [leaf1, leaf2];
        let mut idx = pool.find_first_match() as usize;
        require!(idx < LEAVES_LENGTH, ErrorCode::InvalidIndexing);

        for (i, leaf) in leaves.into_iter().enumerate() {
            // 1) insert
            pool.batch_leaves[idx] = leaf;
            // 2) update root
            pool.merkle_root_batch = get_root(&pool.batch_leaves);

            // 3) did we just cross the 8‐leaf mark?
            if idx + 1 == SUB_BATCH_SIZE {
                msg!("Enforcing first sub‐batch memo");
                enforce_sub_batch_memo(
                    sysvar_ai,
                    pool.batch_number,
                    &pool.batch_leaves[..SUB_BATCH_SIZE],
                )?;
            }
            // 4) did we just fill up all 16 slots?
            if idx + 1 == LEAVES_LENGTH {
                msg!("Enforcing second sub‐batch memo");
                enforce_sub_batch_memo(
                    sysvar_ai,
                    pool.batch_number,
                    &pool.batch_leaves[SUB_BATCH_SIZE..LEAVES_LENGTH],
                )?;
                // rollover into peaks, bump batch_number, reset leaves
                let batch_root = pool.merkle_root_batch;
                let batch_root_bigint = BigInt::from_signed_bytes_be(&batch_root);
                msg!(
                    "Updating with batch root: {:?} , bigint: {:?}",
                    batch_root,
                    batch_root_bigint
                );
                for i in 0..LEAVES_LENGTH {
                    let leaf = pool.batch_leaves[i];
                    let leaf_bigint = BigInt::from_signed_bytes_be(&leaf);
                    msg!("Leaf {}, {:?}, bigint: {:?}", i, leaf, leaf_bigint);
                }
                pool.update_peaks(batch_root);
                pool.batch_number = pool.batch_number.checked_add(1).unwrap();
                pool.whole_tree_root = pool.compute_root_from_peaks();
                let whole_tree_root_bigint = BigInt::from_signed_bytes_be(&pool.whole_tree_root);

                let depth = next_power_of_two_batch(pool.batch_number as usize);
                let deep_root = pool.deepen(depth, TARGET_DEPTH);
                let deep_root_bigint = num_bigint::BigInt::from_signed_bytes_be(&deep_root);
                msg!("Whole tree root: {:?}, as bigint: {:?}, depth: {}, deep root: {:?}, deep_root_bigint, {:?}", pool.whole_tree_root, whole_tree_root_bigint, depth, deep_root, deep_root_bigint);

                pool.batch_leaves = default_leaves();
                pool.merkle_root_batch = get_root(&pool.batch_leaves);
                // after rollover, the *next* leaves go at slot 0
                idx = 0;
                continue;
            }

            idx += 1;
        }

        Ok(())
    }

    // pub fn deposit_variable(
    //     ctx: Context<DepositVariable>,
    //     proof: [u8; 256],
    //     public_inputs: [u8; 96],
    // ) -> Result<()> {
    //     let pool = &mut ctx.accounts.pool;
    //     let depositor_ai = ctx.accounts.depositor.to_account_info();
    //     let pool_ai = pool.to_account_info();
    //     let sysvar_ai = &ctx.accounts.instruction_account;

    //     // 1) Verify ZK proof and unpack
    //     let user_root: [u8;32] = public_inputs[64..96].try_into().expect("Failed converting type");
    //     require!(pool.compare_to_deep(user_root), ErrorCode::InvalidPublicInputRoot);
    //     let (sum_be, leaf1, leaf2) = verify_deposit_proof(&proof, &public_inputs)?;
    //     let deposit_sum = u64::from_be_bytes(sum_be[24..32].try_into().unwrap());
    //     msg!("🔐 Proof ok — depositing {} lamports", deposit_sum);

    //     // 2) Transfer lamports
    //     invoke(
    //         &system_instruction::transfer(
    //             &ctx.accounts.depositor.key(),
    //             &pool_ai.key(),
    //             deposit_sum,
    //         ),
    //         &[
    //             depositor_ai.clone(),
    //             pool_ai.clone(),
    //             ctx.accounts.system_program.to_account_info(),
    //         ],
    //     )?;

    //     //All of this is assuming you can only deposit with 2 leaves
    //     // 3) Insert leaves in 8-leaf windows
    //     let free = pool.find_first_match() as usize;
    //     // --- Case A: both fit into current sub-batch without crossing ---
    //     if free + 2 < SUB_BATCH_SIZE {
    //         pool.batch_leaves[free] = leaf1;
    //         pool.batch_leaves[free + 1] = leaf2;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);
    //         return Ok(());
    //     } else if free + 2 == SUB_BATCH_SIZE {
    //         //Case if 2 leaves are added to have 8 leaves

    //         pool.batch_leaves[free] = leaf1;
    //         pool.batch_leaves[free + 1] = leaf2;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);
    //         msg!("Checking that memo is correct...");
    //         enforce_sub_batch_memo(
    //             sysvar_ai,
    //             pool.batch_number,
    //             &pool.batch_leaves[0..SUB_BATCH_SIZE],
    //         )?;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);
    //         return Ok(());
    //     }
    //     else if free + 1 < SUB_BATCH_SIZE {
    //         // --- Case B: crossing first 8-leaf boundary (free < 8 < free+2) ---

    //         // 1) insert first leaf
    //         pool.batch_leaves[free] = leaf1;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);

    //         // 2) require memo over leaves[0..8]
    //         msg!("Checking that memo is correct...");
    //         enforce_sub_batch_memo(
    //             sysvar_ai,
    //             pool.batch_number,
    //             &pool.batch_leaves[0..SUB_BATCH_SIZE],
    //         )?;

    //         // 3) now insert second leaf at index 8
    //         pool.batch_leaves[SUB_BATCH_SIZE] = leaf2;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);
    //         return Ok(());
    //     }
    //     // --- Case C: both fit entirely in second sub-batch (8 ≤ free and free+2 ≤ 16) ---
    //     if free + 2 < LEAVES_LENGTH as usize {
    //         pool.batch_leaves[free] = leaf1;
    //         pool.batch_leaves[free + 1] = leaf2;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);
    //         return Ok(());
    //     }

    //     // --- Case D: final rollover (filling to ≥16) ---
    //     // only possible if free == 15 (one slot) or free == 14 (two slots)
    //     if free == LEAVES_LENGTH - 1 {
    //         // insert first leaf at slot 15
    //         pool.batch_leaves[free] = leaf1;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);

    //         // require memo over second sub-batch (leaves[8..16])
    //         msg!("Checking that memo is correct...");
    //         enforce_sub_batch_memo(
    //             sysvar_ai,
    //             pool.batch_number,
    //             &pool.batch_leaves[SUB_BATCH_SIZE..LEAVES_LENGTH],
    //         )?;

    //         // rollover full 16-leaf batch exactly as your original code
    //         let batch_root = pool.merkle_root_batch;
    //         pool.update_peaks(batch_root);
    //         pool.batch_number = pool.batch_number.checked_add(1).unwrap();
    //         pool.whole_tree_root = pool.compute_root_from_peaks();
    //         let depth = crate::utils::next_power_of_two_batch(pool.batch_number as usize);
    //         let deepened_root = pool.deepen(depth, crate::TARGET_DEPTH);
    //         msg!("Deepened root {:?}", deepened_root);

    //         // start fresh and insert second leaf
    //         pool.batch_leaves = default_leaves();
    //         pool.batch_leaves[0] = leaf2;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);

    //         Ok(())

    //     } else {
    //         // free == 14: exactly two slots left in second sub-batch
    //         pool.batch_leaves[free] = leaf1;
    //         pool.batch_leaves[free + 1] = leaf2;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);

    //         // require memo over second sub-batch
    //         msg!("Checking that memo is correct...");
    //         enforce_sub_batch_memo(
    //             sysvar_ai,
    //             pool.batch_number,
    //             &pool.batch_leaves[SUB_BATCH_SIZE..LEAVES_LENGTH],
    //         )?;

    //         // rollover
    //         let batch_root = pool.merkle_root_batch;
    //         pool.update_peaks(batch_root);
    //         pool.batch_number = pool.batch_number.checked_add(1).unwrap();
    //         pool.whole_tree_root = pool.compute_root_from_peaks();
    //         let depth = crate::utils::next_power_of_two_batch(pool.batch_number as usize);
    //         let deepened_root = pool.deepen(depth, crate::TARGET_DEPTH);
    //         msg!("Deepened root {:?}", deepened_root);

    //         // leave the new batch empty
    //         pool.batch_leaves = default_leaves();
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);
    //         return Ok(());
    //     }
    // }

    pub fn combine_deposit(
        ctx: Context<CombineDeposit>,
        proof: [u8; 256],
        public_inputs: [u8; 128],
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let sysvar_ai = &ctx.accounts.instruction_account;

        // … proof‐verify, nullifier PDAs …
        let (null1, null2, new_leaf, root) =
            verify_combine_proof(&proof, &public_inputs).map_err(|_| ErrorCode::InvalidProof)?;
        require!(
            pool.compare_to_deep(root),
            ErrorCode::InvalidPublicInputRoot
        );

        // 2) Nullifier #1 PDA
        let (pda1, bump1) = Pubkey::find_program_address(&[&null1], ctx.program_id);
        require!(
            pda1 == *ctx.accounts.nullifier1_account.key,
            ErrorCode::InvalidNullifierAccount
        );
        require!(
            ctx.accounts.nullifier1_account.lamports() == 0,
            ErrorCode::NullifierAlreadyUsed
        );
        invoke_signed(
            &system_instruction::create_account(
                &ctx.accounts.user.key(),
                &pda1,
                Rent::get()?.minimum_balance(MIN_PDA_SIZE),
                MIN_PDA_SIZE as u64,
                ctx.program_id,
            ),
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.nullifier1_account.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[&[&null1, &[bump1]]],
        )?;

        // 3) Nullifier #2 PDA
        let (pda2, bump2) = Pubkey::find_program_address(&[&null2], ctx.program_id);
        require!(
            pda2 == *ctx.accounts.nullifier2_account.key,
            ErrorCode::InvalidNullifierAccount
        );
        require!(
            ctx.accounts.nullifier2_account.lamports() == 0,
            ErrorCode::NullifierAlreadyUsed
        );
        invoke_signed(
            &system_instruction::create_account(
                &ctx.accounts.user.key(),
                &pda2,
                Rent::get()?.minimum_balance(MIN_PDA_SIZE),
                MIN_PDA_SIZE as u64,
                ctx.program_id,
            ),
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.nullifier2_account.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[&[&null2, &[bump2]]],
        )?;

        let mut idx = pool.find_first_match() as usize;
        require!(idx < LEAVES_LENGTH, ErrorCode::InvalidIndexing);

        // Insert exactly one leaf
        pool.batch_leaves[idx] = new_leaf;
        pool.merkle_root_batch = get_root(&pool.batch_leaves);

        // If we just hit slot 7 or 15, enforce a memo
        if idx + 1 == SUB_BATCH_SIZE {
            msg!("Enforcing first sub‐batch memo");
            enforce_sub_batch_memo(
                sysvar_ai,
                pool.batch_number,
                &pool.batch_leaves[..SUB_BATCH_SIZE],
            )?;
        } else if idx + 1 == LEAVES_LENGTH {
            msg!("Enforcing second sub‐batch memo");
            enforce_sub_batch_memo(
                sysvar_ai,
                pool.batch_number,
                &pool.batch_leaves[SUB_BATCH_SIZE..LEAVES_LENGTH],
            )?;
            // then rollover exactly as before
            let batch_root = pool.merkle_root_batch;
            pool.update_peaks(batch_root);
            pool.batch_number = pool.batch_number.checked_add(1).unwrap();
            pool.whole_tree_root = pool.compute_root_from_peaks();
            let depth = next_power_of_two_batch(pool.batch_number as usize);
            let _deep = pool.deepen(depth, TARGET_DEPTH);
            pool.batch_leaves = default_leaves();
            pool.merkle_root_batch = get_root(&pool.batch_leaves);
        }

        Ok(())
    }

    pub fn withdraw_variable(
        ctx: Context<WithdrawVariable>,
        proof: [u8; 256],
        public_inputs: [u8; 96],
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        // 1) Verify ZK proof, unpack [ secret_be, nullifier_hash_be, root_be ]
        let (secret_be, nullifier_hash_be, root_be) =
            verify_withdraw_proof(&proof, &public_inputs).map_err(|_| ErrorCode::InvalidProof)?;
        // the “secret” is really the amount in BE bytes
        let amount = u64::from_be_bytes(secret_be[24..32].try_into().unwrap());

        // 2) Check the root against our on‐chain deepened root
        require!(
            pool.compare_to_deep(root_be),
            ErrorCode::InvalidPublicInputRoot
        );

        // 3) Derive & check the nullifier PDA is unused
        let (null_pda, bump) = Pubkey::find_program_address(&[&nullifier_hash_be], ctx.program_id);
        require!(
            null_pda == *ctx.accounts.nullifier_account.key,
            ErrorCode::InvalidNullifierAccount
        );
        require!(
            ctx.accounts.nullifier_account.lamports() == 0,
            ErrorCode::NullifierAlreadyUsed
        );

        // 4) Compute rent & net withdrawal
        let rent = Rent::get()?;
        let rent_lamports = rent.minimum_balance(MIN_PDA_SIZE);
        let net_amount = amount.checked_sub(rent_lamports).unwrap();
        msg!("Rent to store nullifier PDA: {}", rent_lamports);

        // 5) Create the nullifier‐marker account (payer = user)
        let create_ix = system_instruction::create_account(
            &ctx.accounts.user.key(),
            &null_pda,
            rent_lamports,
            MIN_PDA_SIZE as u64,
            ctx.program_id,
        );
        let signer_seeds = &[&nullifier_hash_be[..], &[bump]];
        invoke_signed(
            &create_ix,
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.nullifier_account.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[signer_seeds],
        )?;
        msg!("Nullifier PDA created; {} lamports reserved", rent_lamports);

        // 6) Move lamports from pool → user + nullifier PDA
        //    (Because `pool` is declared with `seeds`/`bump`, Anchor will
        //     automatically pass its PDA seeds so it can sign this CPI.)
        **ctx
            .accounts
            .pool
            .to_account_info()
            .try_borrow_mut_lamports()? -= amount;
        **ctx.accounts.nullifier_account.try_borrow_mut_lamports()? += rent_lamports;
        **ctx
            .accounts
            .user
            .to_account_info()
            .try_borrow_mut_lamports()? += net_amount;

        msg!(
            "Withdrew {} total; {} for nullifier PDA, {} net to user",
            amount,
            rent_lamports,
            net_amount
        );

        Ok(())
    }

    // pub fn withdraw_variable(
    //     ctx: Context<WithdrawVariable>,
    //     proof: [u8; 256],
    //     public_inputs: [u8; 96],
    // ) -> Result<()> {
    //     let pool = &mut ctx.accounts.pool;

    //     // 1) Verify the ZK proof + unpack the three public signals
    //     //    (we’ll write this helper below)
    //     let (secret_be, nullifier_hash_be, root_be) =
    //         verify_withdraw_proof(&proof, &public_inputs).map_err(|_| ErrorCode::InvalidProof)?;
    //     let amount = u64::from_be_bytes(secret_be[24..32].try_into().unwrap());

    //     // 2) Check that the root the user provided matches our on‐chain root
    //     require!(
    //         pool.compare_to_deep(root_be),
    //         ErrorCode::InvalidPublicInputRoot
    //     );

    //     // 3) Derive the “nullifier PDA” and make sure it’s unused
    //     let (null_pda, bump) = Pubkey::find_program_address(&[&nullifier_hash_be], ctx.program_id);
    //     require!(
    //         null_pda == *ctx.accounts.nullifier_account.key,
    //         ErrorCode::InvalidNullifierAccount
    //     );
    //     require!(
    //         ctx.accounts.nullifier_account.lamports() == 0,
    //         ErrorCode::NullifierAlreadyUsed
    //     );

    //     // 4) Create the nullifier‐marker account so nobody else can spend it again
    //     let rent = Rent::get()?;
    //     let rent_lamports = rent.minimum_balance(MIN_PDA_SIZE);
    //     let withdrawal_amount = amount.checked_sub(rent_lamports).unwrap();
    //     msg!("Rent lamports: {}", rent_lamports);

    //     // invoke_signed(
    //     //     &system_instruction::create_account(
    //     //         &ctx.accounts.user.key(),
    //     //         &null_pda,
    //     //         rent_lamports,
    //     //         MIN_PDA_SIZE as u64,
    //     //         ctx.program_id,
    //     //     ),
    //     //     &[
    //     //         ctx.accounts.user.to_account_info(),
    //     //         ctx.accounts.nullifier_account.clone(),
    //     //         ctx.accounts.system_program.to_account_info(),
    //     //     ],
    //     //     &[&[&nullifier_hash_be, &[bump]]],
    //     // )?;

    //     // **ctx.accounts.user.try_borrow_mut_lamports()? -= rent_lamports;
    //     // **ctx.accounts.nullifier_account.try_borrow_mut_lamports()? += rent_lamports;

    //     // ctx.accounts.nullifier_account.realloc(MIN_PDA_SIZE, false)?;
    //     // ctx.accounts.nullifier_account.assign(ctx.program_id);

    //     // **ctx.accounts.pool.to_account_info().try_borrow_mut_lamports()? -= withdrawal_amount;
    //     // **ctx.accounts.user.try_borrow_mut_lamports()? += withdrawal_amount;

    //     // 1) create the nullifier marker account, funded by the **pool PDA**:

    //     let seeds: [&[u8]; 2] = [b"variable_pool".as_ref(), &pool.identifier];
    //     let identifier_bytes = &pool.identifier;
    //     let (pool_pda, pool_bump) =
    //         Pubkey::find_program_address(&[b"variable_pool", identifier_bytes], ctx.program_id);
    //     require!(
    //         pool_pda == pool.to_account_info().key(),
    //         ErrorCode::InvalidPDA
    //     );
    //     let signer_seeds = &[
    //         b"variable_pool".as_ref(),
    //         pool.identifier.as_ref(),
    //         &[bump],
    //     ];

    //     invoke_signed(
    //         &system_instruction::create_account(
    //             &pool.key(),
    //             &null_pda,
    //             rent_lamports,
    //             MIN_PDA_SIZE as u64,
    //             ctx.program_id,
    //         ),
    //         &[
    //             pool.to_account_info(),
    //             ctx.accounts.nullifier_account.clone(),
    //             ctx.accounts.system_program.to_account_info(),
    //         ],
    //         &[signer_seeds],
    //     )?;

    //     invoke_signed(
    //         &system_instruction::transfer(&pool.key(), &ctx.accounts.user.key(), withdrawal_amount),
    //         &[
    //             pool.to_account_info(),
    //             ctx.accounts.user.to_account_info(),
    //             ctx.accounts.system_program.to_account_info(),
    //         ],
    //         &[signer_seeds],
    //     )?;

    //     msg!("Withdrew {} (minus {} rent) to user", amount, rent_lamports);

    //     msg!(
    //         "Withdrew {} from pool, {} for the nullifier PDA and {} goes back to the user",
    //         amount,
    //         rent_lamports,
    //         withdrawal_amount
    //     );

    //     Ok(())
    // }

    //Proof inclusion of two leaves, add 1 leaf to tree
    // pub fn combine_deposit(
    //     ctx: Context<CombineDeposit>,
    //     proof: [u8; 256],
    //     public_inputs: [u8; 128],
    // ) -> Result<()> {
    //     let pool = &mut ctx.accounts.pool;

    //     // 1) Verify and unpack proof
    //     let (null1, null2, new_leaf, root) =
    //     verify_combine_proof(&proof, &public_inputs).map_err(|_| ErrorCode::InvalidProof)?;
    //     require!(pool.compare_to_deep(root), ErrorCode::InvalidPublicInputRoot);

    //     // 2) Nullifier #1 PDA
    //     let (pda1, bump1) = Pubkey::find_program_address(&[&null1], ctx.program_id);
    //     require!(
    //         pda1 == *ctx.accounts.nullifier1_account.key,
    //         ErrorCode::InvalidNullifierAccount
    //     );
    //     require!(
    //         ctx.accounts.nullifier1_account.lamports() == 0,
    //         ErrorCode::NullifierAlreadyUsed
    //     );
    //     invoke_signed(
    //         &system_instruction::create_account(
    //             &ctx.accounts.user.key(),
    //             &pda1,
    //             Rent::get()?.minimum_balance(2),
    //             8,
    //             ctx.program_id,
    //         ),
    //         &[
    //             ctx.accounts.user.to_account_info(),
    //             ctx.accounts.nullifier1_account.clone(),
    //             ctx.accounts.system_program.to_account_info(),
    //         ],
    //         &[&[&null1, &[bump1]]],
    //     )?;

    //     // 3) Nullifier #2 PDA
    //     let (pda2, bump2) = Pubkey::find_program_address(&[&null2], ctx.program_id);
    //     require!(
    //         pda2 == *ctx.accounts.nullifier2_account.key,
    //         ErrorCode::InvalidNullifierAccount
    //     );
    //     require!(
    //         ctx.accounts.nullifier2_account.lamports() == 0,
    //         ErrorCode::NullifierAlreadyUsed
    //     );
    //     invoke_signed(
    //         &system_instruction::create_account(
    //             &ctx.accounts.user.key(),
    //             &pda2,
    //             Rent::get()?.minimum_balance(2),
    //             8,
    //             ctx.program_id,
    //         ),
    //         &[
    //             ctx.accounts.user.to_account_info(),
    //             ctx.accounts.nullifier2_account.clone(),
    //             ctx.accounts.system_program.to_account_info(),
    //         ],
    //         &[&[&null2, &[bump2]]],
    //     )?;

    //     // 4) Determine where the new leaf would land
    //     let free = pool.find_first_match() as usize;
    //     require!(free < LEAVES_LENGTH, ErrorCode::InvalidIndexing);

    //     // 5) If crossing the first 8-leaf boundary:
    //     if free == 7 {
    //         pool.batch_leaves[free] = new_leaf;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);

    //         msg!("Enforcing first sub-batch memo");
    //         enforce_sub_batch_memo(
    //             &ctx.accounts.instruction_account,
    //             pool.batch_number,
    //             &pool.batch_leaves[0..SUB_BATCH_SIZE],
    //         )?;
    //     }
    //     // 6) If crossing the second (16-leaf) boundary:
    //     else if free == 15 {
    //         pool.batch_leaves[free] = new_leaf;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);
    //         msg!("Enforcing second sub-batch memo");
    //         enforce_sub_batch_memo(
    //             &ctx.accounts.instruction_account,
    //             pool.batch_number,
    //             &pool.batch_leaves[SUB_BATCH_SIZE..LEAVES_LENGTH],
    //         )?;

    //         pool.batch_leaves[free] = new_leaf;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);

    //         // rollover
    //         let batch_root = pool.merkle_root_batch;
    //         pool.update_peaks(batch_root);
    //         pool.batch_number = pool.batch_number.checked_add(1).unwrap();
    //         pool.whole_tree_root = pool.compute_root_from_peaks();
    //         let depth = crate::utils::next_power_of_two_batch(pool.batch_number as usize);
    //         let _ = pool.deepen(depth, crate::TARGET_DEPTH);

    //         // leave the new batch empty
    //         pool.batch_leaves = default_leaves();
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);
    //     } else {
    //         pool.batch_leaves[free] = new_leaf;
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);
    //     }

    //     Ok(())
    // }

    pub fn deposit(
        ctx: Context<Deposit>,
        leaf_hash: [u8; 32], //leaves_info: [u8; 520]
    ) -> Result<()> {
        let pool_info = ctx.accounts.pool.to_account_info();
        let pool_batch = ctx.accounts.pool.batch_number.clone();
        let pool = &mut ctx.accounts.pool;

        let free = pool.find_first_match();
        let sysvar_account = &ctx.accounts.instruction_account;

        // Load the full instruction list (memo should be at index 0 based on your tx order)
        let maybe_memo_ix = instructions::load_instruction_at_checked(0, sysvar_account)?;

        // Verify that it's the Memo program
        let memo_program_id = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
        if maybe_memo_ix.program_id != memo_program_id {
            msg!("First instruction is not a Memo, skipping decoding");
            return Err(ErrorCode::MissingMemoInstruction.into());
        }

        // Memo data is a UTF-8 base64 string
        let memo_base64 =
            std::str::from_utf8(&maybe_memo_ix.data).map_err(|_| ErrorCode::InvalidMemoUtf8)?;

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
            memo_bytes[0..8]
                .try_into()
                .map_err(|_| ErrorCode::FailedToParseBatch)?,
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
                pool.deposit_amount,
            );

            let _ = invoke(
                &transfer_instruction,
                &[ctx.accounts.depositor.to_account_info(), pool_info],
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
                msg!(
                    "Computed deep root with target depth: {} \n{:?}",
                    TARGET_DEPTH,
                    deep_root
                );

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
        require_keys_eq!(
            ctx.accounts.payer.key(),
            ADMIN_KEY,
            ErrorCode::UnauthorizedAction
        );
        Ok(())
    }

    pub fn withdraw_from_treasury(ctx: Context<WithdrawFromTreasury>, amount: u64) -> Result<()> {
        // Check authority
        require_keys_eq!(
            ctx.accounts.authority.key(),
            ADMIN_KEY,
            ErrorCode::UnauthorizedAction
        );

        // Transfer lamports from PDA to authority
        **ctx
            .accounts
            .treasury
            .to_account_info()
            .try_borrow_mut_lamports()? -= amount;
        **ctx
            .accounts
            .authority
            .to_account_info()
            .try_borrow_mut_lamports()? += amount;

        Ok(())
    }

    pub fn withdraw(
        ctx: Context<Withdraw>,
        proof: [u8; 256],        // Real proof (a,b and c)
        public_inputs: [u8; 64], //root & nullifier hash
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
        let (nullifier_pda, bump) =
            Pubkey::find_program_address(&[nullifier_hash.as_ref()], ctx.program_id);
        let nullifier_account = &ctx.accounts.nullifier_account;
        if &nullifier_pda != nullifier_account.key {
            msg!("The provided nullifier account and nullifier derived pda do not match.");
            return Err(ErrorCode::InvalidNullifierAccount.into());
        }

        if nullifier_account.lamports() != 0 {
            msg!("The nullifier account balance is not zero, it has already been initialized");
            return Err(ErrorCode::NullifierAlreadyUsed.into());
        }
        require!(
            ctx.accounts.pool_creator.key() == pool.creator,
            ErrorCode::InvalidPoolCreator
        );

        // Otherwise, create the account.
        // (Assume a minimal account size of 8 bytes; adjust as needed.)
        let rent = Rent::get()?;

        let rent_lamports = rent.minimum_balance(MIN_PDA_SIZE);
        let create_ix = system_instruction::create_account(
            &ctx.accounts.withdrawer.key(), // payer
            &nullifier_pda,                 // new account address
            rent_lamports,
            MIN_PDA_SIZE as u64,
            ctx.program_id, // owner: our program
        );
        let seeds = &[nullifier_hash.as_ref(), &[bump]];
        invoke_signed(
            &create_ix,
            &[
                ctx.accounts.withdrawer.to_account_info(),
                nullifier_account.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[seeds],
        )?;
        msg!("Creating nullifier PDA costs {} lamports", rent_lamports);

        **ctx.accounts.withdrawer.try_borrow_mut_lamports()? -= rent_lamports;
        **ctx.accounts.nullifier_account.try_borrow_mut_lamports()? += rent_lamports;

        // Mark the PDA as rent-exempt by allocating space & assigning ownership
        ctx.accounts
            .nullifier_account
            .realloc(MIN_PDA_SIZE, false)?;
        ctx.accounts.nullifier_account.assign(ctx.program_id);

        let depth = next_power_of_two_batch(pool.batch_number as usize);
        msg!("Current depth: {}", depth);

        //This allows to deepen the tree to match a certain size
        let deepen_root = pool.deepen(depth, TARGET_DEPTH);
        if deepen_root != public_input_root {
            msg!("Deepened root isn't same as public_input_root");
            msg!(
                "Deepen root {:?}\n public_input_root {:?}",
                deepen_root,
                public_input_root
            );
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

        let withdrawer_amount = pool
            .deposit_amount
            .checked_sub(pool.creator_fee)
            .unwrap()
            .checked_sub(PROGRAM_FEE)
            .unwrap();
        let creator_amount = pool.creator_fee;

        **ctx
            .accounts
            .pool
            .to_account_info()
            .try_borrow_mut_lamports()? -= withdraw_pool_amount;

        **ctx
            .accounts
            .treasury
            .to_account_info()
            .try_borrow_mut_lamports()? += PROGRAM_FEE;
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
