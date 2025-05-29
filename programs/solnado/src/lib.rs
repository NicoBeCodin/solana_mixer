use anchor_lang::prelude::*;
pub mod error;
pub mod state;
pub mod utils;
pub mod verifying_key;
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
];
const MIN_PDA_SIZE: usize = 1;
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const FIXED_DEPOSIT_AMOUNT: u64 = ((LAMPORTS_PER_SOL as f64) * 0.001) as u64; // 10_000_000 Low for testing purposes
const PROGRAM_FEE: u64 = 1_000_000; //0.001 SOL FEE PER WITHDRAWAL
const TARGET_DEPTH: usize = 20;

declare_id!("FyAuPyboHtdnnqbcAhTXjKwXRNqxiWYK4Xwvc5Gtw8Ln");
const TARGET_DEPTH_LARGE: usize = 28;
const BATCHES_PER_SMALL_TREE: u64 = 4096; //Corresponds to 2^16 leaves --> about 9 rpc calls
const SMALL_TREE_BATCH_DEPTH: usize = 16; //This 64 000 leaves
                                          // const ADMIN_KEY: Pubkey = pubkey!("EJZQiTeikeg8zgU7YgRfwZCxc9GdhTsYR3fQrXv3uK9V");
const ADMIN_KEY: Pubkey = pubkey!("BSpEVXMrA3C1myPSUmT8hQSecrvJaUin8vnQTfzGGf17");
const ON_BEHALF_FEE: u64 = 10_000;

//The subtreeIndexer when called should be called with the LeavesIndexer too
//Updating the small batch root isn't important as long as we don't post the batch.
#[program]
pub mod solnado {
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
        pool.batch_leaves = default_leaves();
        pool.merkle_root_batch = get_root(&pool.batch_leaves);
        pool.batch_number = 0;
        pool.depth = [0; TARGET_DEPTH_LARGE];
        pool.number_of_peaks = 0;
        pool.peaks = [DEFAULT_LEAF; TARGET_DEPTH_LARGE];
        pool.max_leaves = (2_u64).pow((TARGET_DEPTH_LARGE + 4) as u32); //Because batches
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

        pool.min_deposit_amount = deposit_amount;

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

        msg!("Pool initialized with {:?} as root", pool.merkle_root_batch);

        Ok(())
    }

    //maybe put the leaves indexer Pubkey in the struct to not have to derive the ekey everytime
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
        pool.depth = [0; TARGET_DEPTH_LARGE];
        pool.number_of_peaks = 0;
        pool.peaks = [DEFAULT_LEAF; TARGET_DEPTH_LARGE];
        pool.max_leaves = (2_u64).pow(TARGET_DEPTH_LARGE as u32);
        pool.min_deposit_amount = 5_000_000;
        // At least a 0.005 SOL DEPOSIT per action of use

        msg!(
            "Pool initialized by signer: {}\n
        Pool name: {}\n
        Pool max leaves: {}\n",
            ctx.accounts.authority.key(),
            pool_string,
            pool.max_leaves
        );

        //We create two PDAs, leaves_indexer and subtree_indexer

        let (pda1, bump1) = Pubkey::find_program_address(
            &[b"leaves_indexer".as_ref(), &pool.identifier],
            ctx.program_id,
        );
        require!(
            pda1 == *ctx.accounts.leaves_indexer.key,
            ErrorCode::InvalidNullifierAccount
        );
        require!(
            ctx.accounts.leaves_indexer.lamports() == 0,
            ErrorCode::NullifierAlreadyUsed
        );
        invoke_signed(
            &system_instruction::create_account(
                &ctx.accounts.authority.key(),
                &pda1,
                Rent::get()?.minimum_balance(MIN_PDA_SIZE),
                MIN_PDA_SIZE as u64,
                ctx.program_id,
            ),
            &[
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.leaves_indexer.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[&[b"leaves_indexer".as_ref(), &pool.identifier, &[bump1]]],
        )?;

        let (pda2, bump2) = Pubkey::find_program_address(
            &[b"subtree_indexer".as_ref(), &pool.identifier],
            ctx.program_id,
        );
        require!(
            pda2 == *ctx.accounts.subtree_indexer.key,
            ErrorCode::InvalidNullifierAccount
        );
        require!(
            ctx.accounts.subtree_indexer.lamports() == 0,
            ErrorCode::NullifierAlreadyUsed
        );
        invoke_signed(
            &system_instruction::create_account(
                &ctx.accounts.authority.key(),
                &pda2,
                Rent::get()?.minimum_balance(MIN_PDA_SIZE),
                MIN_PDA_SIZE as u64,
                ctx.program_id,
            ),
            &[
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.subtree_indexer.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[&[b"subtree_indexer".as_ref(), &pool.identifier, &[bump2]]],
        )?;

        msg!(
            "Variable pool initialized with {:?} as root",
            pool.merkle_root_batch
        );

        Ok(())
    }

    pub fn deposit_variable(
        ctx: Context<DepositVariable>,
        proof: [u8; 256],
        public_inputs: [u8; 72],
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let depositor = ctx.accounts.depositor.to_account_info();
        let pool_ai = pool.to_account_info();
        let sysvar_ai = &ctx.accounts.instruction_account;

        // 1️⃣ Decode & verify the proof, depending on how many outputs we got
        //    – 96 bytes means sum||leaf1||leaf2 (two-leaf proof)
        //    – 64 bytes means sum||leaf1        (one-leaf proof)
        let null_leaf2: [u8; 32] = public_inputs[40..72].try_into().expect("Failed converting");

        let (deposit_sum, leaves) = match null_leaf2 == DEFAULT_LEAF {
            false => {
                // two-leaf proof
                let (sum_be, leaf1, leaf2) = verify_deposit_proof(&proof, &public_inputs)
                    .map_err(|_| ErrorCode::InvalidProof)?;
                let sum = u64::from_be_bytes(sum_be.try_into().unwrap());
                msg!("Leaf1: {:?}, leaf2: {:?}, sum: {}", leaf1, leaf2, sum);
                (sum, vec![leaf1, leaf2])
            }
            true => {
                // single-leaf proof
                let (sum_be, leaf1) = verify_single_deposit_proof(&proof, &public_inputs)
                    .map_err(|_| ErrorCode::InvalidProof)?;
                let sum = u64::from_be_bytes(sum_be.try_into().unwrap());
                msg!("Leaf1: {:?}, sum {}", leaf1, sum);
                (sum, vec![leaf1])
            }
            _ => return Err(ErrorCode::InvalidArgument.into()),
        };

        // 2) Transfer lamports
        invoke(
            &system_instruction::transfer(
                &ctx.accounts.depositor.key(),
                &pool_ai.key(),
                deposit_sum,
            ),
            &[
                depositor.clone(),
                pool_ai.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        let mut idx = pool.find_first_match() as usize;
        require!(idx < LEAVES_LENGTH, ErrorCode::InvalidIndexing);

        for (_, leaf) in leaves.into_iter().enumerate() {
            // 1) insert
            pool.batch_leaves[idx] = leaf;
            // 2) update root
            pool.merkle_root_batch = get_root(&pool.batch_leaves);
            msg!("Leaf that was inserted: {:?}", leaf);

            // 3) did we just cross the 8‐leaf mark?
            if idx + 1 == SUB_BATCH_SIZE {
                //Make sure that the leaves indexer is included
                let (expected_leaves_indexer, _bump) = Pubkey::find_program_address(
                    &[b"leaves_indexer", &pool.identifier],
                    ctx.program_id,
                );
                require!(
                    expected_leaves_indexer == *ctx.remaining_accounts[0].key,
                    ErrorCode::InvalidIndexerAccount
                );
                msg!("Enforcing first sub‐batch memo");
                enforce_sub_batch_memo(
                    sysvar_ai,
                    pool.batch_number,
                    &pool.batch_leaves[..SUB_BATCH_SIZE],
                )?;
            }
            // 4) did we just fill up all 16 slots?
            if idx + 1 == LEAVES_LENGTH {
                //Make sure that the leaves indexer is included
                let (expected_leaves_indexer, _bump) = Pubkey::find_program_address(
                    &[b"leaves_indexer", &pool.identifier],
                    ctx.program_id,
                );
                require!(
                    expected_leaves_indexer == *ctx.remaining_accounts[0].key,
                    ErrorCode::InvalidIndexerAccount
                );
                msg!("Enforcing second sub‐batch memo");

                enforce_sub_batch_memo(
                    sysvar_ai,
                    pool.batch_number,
                    &pool.batch_leaves[SUB_BATCH_SIZE..LEAVES_LENGTH],
                )?;
                // rollover into peaks, bump batch_number, reset leaves
                let batch_root = pool.merkle_root_batch;

                pool.update_peaks(batch_root);
                pool.batch_number = pool.batch_number.checked_add(1).unwrap();
                pool.whole_tree_root = pool.compute_root_from_peaks();

                pool.batch_leaves = default_leaves();
                pool.merkle_root_batch = get_root(&pool.batch_leaves);
                // after rollover, the *next* leaves go at slot 0
                idx = 0;
                continue;
            } else if idx == 0
                && (pool.batch_number % BATCHES_PER_SMALL_TREE == 0)
                && pool.batch_number != 0
            {
                //Make sure that the correct subtree indexer is included
                let (expected_subtree_indexer, _bump) = Pubkey::find_program_address(
                    &[b"subtree_indexer", &pool.identifier],
                    ctx.program_id,
                );
                require!(
                    expected_subtree_indexer == *ctx.remaining_accounts[1].key,
                    ErrorCode::InvalidIndexerAccount
                );
                let expected_idxr = Pubkey::find_program_address(
                    &[b"leaves_indexer", &pool.identifier],
                    ctx.program_id,
                )
                .0;
                require!(
                    expected_idxr == *ctx.remaining_accounts[0].key,
                    ErrorCode::InvalidIndexerAccount
                );

                //in this case, ensure the user posts the correct small tree root
                enforce_small_tree_memo(
                    sysvar_ai,
                    pool.batch_number - 1,
                    pool.last_small_tree_root,
                )?;
            }

            idx += 1;
        }
        Ok(())
    }

    pub fn combine_deposit<'info>(
        ctx: Context<CombineDeposit>,
        mode: u8,
        proof: [u8; 256],
        public_inputs: Vec<u8>,
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let sysvar = &ctx.accounts.instruction_account;
        let user_ai = ctx.accounts.user.to_account_info();
        let sysprog = ctx.accounts.system_program.to_account_info();

        //
        // 1) Decode & verify, unpack nullifiers and new leaf( s ) + root
        //
        // (you’ll implement these three functions to match your circom)
        let (nulls, new_leaves, root) = match mode {
            0 => {
                // --- two nullifiers → one leaf (old behavior) ---
                let (n1, n2, leaf, r) = verify_combine_proof(&proof, &public_inputs)
                    .map_err(|_| ErrorCode::InvalidProof)?;
                (vec![n1, n2], vec![leaf], r)
            }
            1 => {
                // --- one nullifier → two leaves ---
                // dummy placeholder: user to implement
                let (n, leaf1, leaf2, r) = verify_one_null_two_leaves(&proof, &public_inputs)
                    .map_err(|_| ErrorCode::InvalidProof)?;
                (vec![n], vec![leaf1, leaf2], r)
            }
            2 => {
                // --- two nullifiers → two leaves ---
                // dummy placeholder: user to implement
                let (n1, n2, leaf1, leaf2, r) = verify_two_null_two_leaves(&proof, &public_inputs)
                    .map_err(|_| ErrorCode::InvalidProof)?;
                (vec![n1, n2], vec![leaf1, leaf2], r)
            }
            _ => return Err(ErrorCode::InvalidArgument.into()),
        };

        for (i, n) in nulls.iter().enumerate() {
            let (expected, bump) = Pubkey::find_program_address(&[n], ctx.program_id);
            // let acct = *ctx.remaining_accounts[i].key;
            let acct_info = match i {
                0 => ctx.accounts.nullifier1.to_account_info(),
                _ => ctx.accounts.nullifier2_or_else.to_account_info(),
            };

            require!(
                expected == acct_info.key(),
                ErrorCode::InvalidNullifierAccount
            );
            require!(
                ctx.remaining_accounts[i].lamports() == 0,
                ErrorCode::NullifierAlreadyUsed
            );

            invoke_signed(
                &system_instruction::create_account(
                    &user_ai.key(),
                    &expected,
                    Rent::get()?.minimum_balance(MIN_PDA_SIZE),
                    MIN_PDA_SIZE as u64,
                    ctx.program_id,
                ),
                &[user_ai.clone(), acct_info, sysprog.clone()],
                &[&[n.as_ref(), &[bump]]],
            )?;
        }

        // for (i, n) in nulls.iter().enumerate() {
        //     let (expected, bump) = Pubkey::find_program_address(&[n], ctx.program_id);
        //     let acct = match i {
        //       0 => ctx.accounts.nullifier1.to_account_info(),
        //       1 => ctx.accounts
        //                .nullifier2
        //                .as_ref()
        //                .ok_or(ErrorCode::InvalidNullifierAccount)?
        //                .to_account_info(),
        //       _ => unreachable!(),
        //     };
        //     require!(expected == *acct.key, ErrorCode::InvalidNullifierAccount);
        //     require!(acct.lamports() == 0,    ErrorCode::NullifierAlreadyUsed);

        //     invoke_signed(
        //       &system_instruction::create_account(
        //         &user_ai.key(),
        //         &expected,
        //         Rent::get()?.minimum_balance(MIN_PDA_SIZE),
        //         MIN_PDA_SIZE as u64,
        //         ctx.program_id,
        //       ),
        //       &[ user_ai.clone(), acct.clone(), sysprog.clone() ],
        //       &[&[n.as_ref(), &[bump]]],
        //     )?;
        //   }

        //
        // 4) Insert new_leaf(s) into the batch, same sub-batch/memo logic
        //
        let mut idx = pool.find_first_match() as usize;
        let acct_index: usize = match mode {
            0 | 2 => 1,
            _ => 0,
        };

        require!(idx < LEAVES_LENGTH, ErrorCode::InvalidIndexing);

        for leaf in new_leaves {
            pool.batch_leaves[idx] = leaf;
            pool.merkle_root_batch = get_root(&pool.batch_leaves);

            // a) first sub‐batch boundary?
            if idx + 1 == SUB_BATCH_SIZE {
                let expected_idxr = Pubkey::find_program_address(
                    &[b"leaves_indexer", &pool.identifier],
                    ctx.program_id,
                )
                .0;
                require!(
                    expected_idxr == *ctx.remaining_accounts[0].key,
                    ErrorCode::InvalidIndexerAccount
                );
                enforce_sub_batch_memo(
                    &sysvar,
                    pool.batch_number,
                    &pool.batch_leaves[..SUB_BATCH_SIZE],
                )?;
            }
            // b) second boundary → rollover
            else if idx + 1 == LEAVES_LENGTH {
                let expected_idxr = Pubkey::find_program_address(
                    &[b"leaves_indexer", &pool.identifier],
                    ctx.program_id,
                )
                .0;
                let indxr_acct_key = match acct_index {
                    0 => ctx.accounts.nullifier2_or_else.key,
                    _ => ctx.remaining_accounts[1].key,
                };

                require!(
                    expected_idxr == *indxr_acct_key,
                    ErrorCode::InvalidIndexerAccount
                );
                enforce_sub_batch_memo(
                    &sysvar,
                    pool.batch_number,
                    &pool.batch_leaves[SUB_BATCH_SIZE..LEAVES_LENGTH],
                )?;

                // rollover:
                let batch_root = pool.merkle_root_batch;
                pool.update_peaks(batch_root);
                pool.batch_number = pool.batch_number.checked_add(1).unwrap();
                pool.whole_tree_root = pool.compute_root_from_peaks();
            }
            
            // c) small‐tree boundary? Post the subtree root of the previuous subtree when first depositing
            else if pool.batch_number % BATCHES_PER_SMALL_TREE == 0 && idx==0 {
                let expected_st_idxr = Pubkey::find_program_address(
                    &[b"subtree_indexer", &pool.identifier],
                    ctx.program_id,
                )
                .0;
                let expected_idxr = Pubkey::find_program_address(
                    &[b"leaves_indexer", &pool.identifier],
                    ctx.program_id,
                )
                .0;
                let (indxr_acct_key, subtree_indxr) = match acct_index {
                0 => (ctx.accounts.nullifier2_or_else.key, ctx.remaining_accounts[1].key),
                _ => (ctx.remaining_accounts[1].key,ctx.remaining_accounts[2].key)
                };
                require!(
                    expected_idxr == *indxr_acct_key,
                    ErrorCode::InvalidIndexerAccount
                );
                require!(
                    expected_st_idxr == *subtree_indxr,
                    ErrorCode::InvalidIndexerAccount
                );
                enforce_small_tree_memo(&sysvar, pool.batch_number - 1, pool.last_small_tree_root)?;
            }
            idx+=1;   
        }
        Ok(())
    }

    pub fn withdraw_on_behalf(
        ctx: Context<WithdrawOnBehalf>,
        proof: [u8; 256],
        public_inputs: Vec<u8>, // nullifier||amount||root||withdrawer_pubkey
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        //
        // 1) Decode & verify, unpack secret_be, null_be, root_be, withdrawer_bytes
        //
        let (secret_be, null_be, withdrawer_bytes, root_be) =
            verify_withdraw_on_behalf(&proof, public_inputs.as_slice())
                .map_err(|_| ErrorCode::InvalidProof)?;

        // reconstruct withdrawer pubkey and check it matches the passed-in account
        let withdrawer_key = ctx.accounts.withdrawer.key.to_bytes();

        let expected_withdrawer = pubkey!(&withdrawer_bytes);
        require!(
            withdrawer_key == withdrawer_bytes,
            ErrorCode::InvalidWithdrawerKey
        );

        // also ensure the withdrawer is not the same as the fee‐payer
        require!(
            expected_withdrawer != &ctx.accounts.payer.key().to_bytes(),
            ErrorCode::InvalidArgument
        );

        // 2) Check the Merkle root
        require!(
            pool.compare_to_deep(root_be),
            ErrorCode::InvalidPublicInputRoot
        );

        // 3) Compute amount and rent
        let amount = u64::from_be_bytes(secret_be);
        let rent = Rent::get()?;
        let rent_lamports = rent.minimum_balance(MIN_PDA_SIZE);
        let net_amount = amount
            .checked_sub(rent_lamports + ON_BEHALF_FEE)
            .ok_or(ErrorCode::InvalidArgument)?;

        // 4) Nullifier PDA must match and be unused
        let (null_pda, bump) = Pubkey::find_program_address(&[&null_be], ctx.program_id);
        require!(
            null_pda == *ctx.accounts.nullifier_account.key,
            ErrorCode::InvalidNullifierAccount
        );
        require!(
            ctx.accounts.nullifier_account.lamports() == 0,
            ErrorCode::NullifierAlreadyUsed
        );

        // 5) Create the nullifier account (size=0) to mark it used
        let create_ix = system_instruction::create_account(
            &ctx.accounts.payer.key(),
            &null_pda,
            rent_lamports,
            MIN_PDA_SIZE as u64,
            ctx.program_id,
        );
        invoke_signed(
            &create_ix,
            &[
                ctx.accounts.payer.to_account_info(),
                ctx.accounts.nullifier_account.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[&[&null_be, &[bump]]],
        )?;

        // 6) Move lamports: pool → withdrawer + payer + nullifier
        **pool.to_account_info().try_borrow_mut_lamports()? -= amount;
        **ctx.accounts.withdrawer.try_borrow_mut_lamports()? += net_amount;
        **ctx.accounts.payer.try_borrow_mut_lamports()? += ON_BEHALF_FEE;
        **ctx.accounts.nullifier_account.try_borrow_mut_lamports()? += rent_lamports;

        Ok(())
    }

    pub fn withdraw_variable(
        ctx: Context<WithdrawVariable>,
        proof: [u8; 256],
        public_inputs: Vec<u8>,
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let public_inputs_slice = public_inputs.as_slice();

        let (secret_be, null_be, root_be, maybe_new_leaf) = match public_inputs.len() {
            96 => {
                // ❌ 96 bytes → 3 public signals → withdraw‐only
                let (secret_be, null_be, root_be) =
                    verify_withdraw_proof(&proof, public_inputs_slice)
                        .map_err(|_| ErrorCode::InvalidProof)?;
                (secret_be, null_be, root_be, None)
            }
            128 => {
                // ✅ 128 bytes → 4 public signals → withdraw + deposit
                let (secret_be, null_be, new_leaf, root_be) =
                    verify_withdraw_and_deposit_proof(&proof, public_inputs_slice)
                        .map_err(|_| ErrorCode::InvalidProof)?;
                (secret_be, null_be, root_be, Some(new_leaf))
            }
            _ => return Err(ErrorCode::InvalidArgument.into()),
        };

        // // 1) Verify ZK proof, unpack [ secret_be, nullifier_hash_be, root_be ]
        // let (secret_be, nullifier_hash_be, root_be) =
        //     verify_withdraw_proof(&proof, secret, nullifier_hash, public_root).map_err(|_| ErrorCode::InvalidProof)?;
        // // the “secret” is really the amount in BE bytes
        // let amount = u64::from_be_bytes(secret_be[24..32].try_into().unwrap());
        let amount = u64::from_be_bytes(secret_be);

        // 2) Check the root against our on‐chain deepened root
        require!(
            pool.compare_to_deep(root_be),
            ErrorCode::InvalidPublicInputRoot
        );

        // 3) Derive & check the nullifier PDA is unused
        let (null_pda, bump) = Pubkey::find_program_address(&[&null_be], ctx.program_id);
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

        //Nullifier PDA is in the anchor constraints

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
                pool.min_deposit_amount,
            );

            let _ = invoke(
                &transfer_instruction,
                &[ctx.accounts.depositor.to_account_info(), pool_info],
            );
            let start_index = free;
            msg!("Transfered {} lamports to pool", FIXED_DEPOSIT_AMOUNT);
            // let pool = &mut ctx.accounts.pool;
            pool.batch_leaves[start_index] = leaf_hash;

            msg!("Leaf {:?} \nadded at index {}", leaf_hash, start_index);
            pool.merkle_root_batch = get_root(&pool.batch_leaves);

            if &user_leaves != &pool.batch_leaves {
                msg!("Leaves mismatch!");
                return Err(ErrorCode::InvalidUserLeaves.into());
            }

            msg!("New root of temporary pool is {:?}", pool.merkle_root_batch);
            if free == 15 {
                //After adding the leaf we create a new temporary pool
                msg!(
                    "Temporary pool is now at max capacity, storing the hash and creating a new one"
                );
                //After adding the leaf, we need to create a new pool
                let new_batch = pool.merkle_root_batch;
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
                pool.batch_leaves = default_leaves();
                pool.merkle_root_batch = get_root(&pool.batch_leaves);
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
        // This is now done with an anchor constraint
        // let (nullifier_pda, bump) =
        //     Pubkey::find_program_address(&[nullifier_hash.as_ref()], ctx.program_id);
        // if &nullifier_pda != nullifier_account.key {
        //     msg!("The provided nullifier account and nullifier derived pda do not match.");
        //     return Err(ErrorCode::InvalidNullifierAccount.into());
        // }

        let nullifier_account = &ctx.accounts.nullifier_account;
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
        // let create_ix = system_instruction::create_account(
        //     &ctx.accounts.withdrawer.key(), // payer
        //     &nullifier_pda,                 // new account address
        //     rent_lamports,
        //     MIN_PDA_SIZE as u64,
        //     ctx.program_id, // owner: our program
        // );
        // let seeds = &[nullifier_hash.as_ref(), &[bump]];
        // invoke_signed(
        //     &create_ix,
        //     &[
        //         ctx.accounts.withdrawer.to_account_info(),
        //         nullifier_account.clone(),
        //         ctx.accounts.system_program.to_account_info(),
        //     ],
        //     &[seeds],
        // )?;

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
        verify_withdraw_fixed_proof(&proof, &public_inputs)
            .map_err(|_e| ErrorCode::InvalidProof)?;

        let withdraw_pool_amount = pool.min_deposit_amount;

        let withdrawer_amount = pool
            .min_deposit_amount
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

    // use anchor_spl::token::{self, Token, TokenAccount, Mint, Transfer};

    //Spl token integration causes shit ton of dependencies issues
    // pub fn deposit_variable_token(
    //     ctx: Context<DepositVariableToken>,
    //     proof: [u8; 256],
    //     public_inputs: [u8;72],
    // ) -> Result<()> {

    //     let pool     = &mut ctx.accounts.pool;
    //     let sysvar_ai = &ctx.accounts.instruction_account;

    //     // 1️⃣ Decode & verify the proof, depending on how many outputs we got
    //     //    – 96 bytes means sum||leaf1||leaf2 (two-leaf proof)
    //     //    – 64 bytes means sum||leaf1        (one-leaf proof)
    //     let null_leaf2: [u8;32] = public_inputs[40..72].try_into().expect("Failed converting");

    //     //Add to the public inputs a check that the mint.key corresponds to a valid public input
    //     let (deposit_sum, leaves) = match null_leaf2 ==DEFAULT_LEAF {
    //         false => {
    //             // two-leaf proof
    //             let (sum_be, leaf1, leaf2) =
    //                 verify_deposit_proof(&proof, &public_inputs)
    //                     .map_err(|_| ErrorCode::InvalidProof)?;
    //             let sum = u64::from_be_bytes(sum_be.try_into().unwrap());
    //             (sum, vec![leaf1, leaf2])
    //         }
    //         true => {
    //             // single-leaf proof
    //             let (sum_be, leaf1) =
    //                 verify_single_deposit_proof(&proof, &public_inputs)
    //                     .map_err(|_| ErrorCode::InvalidProof)?;
    //             let sum = u64::from_be_bytes(sum_be.try_into().unwrap());
    //             (sum, vec![leaf1])
    //         }
    //         _ => return Err(ErrorCode::InvalidArgument.into()),
    //     };

    //     let cpi = CpiContext::new(
    //         ctx.accounts.token_program.to_account_info(),
    //         token::Transfer {
    //             from: ctx.accounts.depositor_ata.to_account_info(),
    //             to:   ctx.accounts.pool_ata.to_account_info(),
    //             authority: ctx.accounts.depositor.to_account_info(),
    //         },
    //     );
    //     token::transfer(cpi, deposit_sum)?;

    //     let mut idx = pool.find_first_match() as usize;
    //     require!(idx < LEAVES_LENGTH, ErrorCode::InvalidIndexing);

    //     for (_, leaf) in leaves.into_iter().enumerate() {
    //         // 1) insert
    //         pool.batch_leaves[idx] = leaf;
    //         // 2) update root
    //         pool.merkle_root_batch = get_root(&pool.batch_leaves);

    //         // 3) did we just cross the 8‐leaf mark?
    //         if idx + 1 == SUB_BATCH_SIZE {
    //             //Make sure that the leaves indexer is included
    //             let (expected_leaves_indexer, _bump) =
    //             Pubkey::find_program_address(&[b"leaves_indexer", &pool.identifier], ctx.program_id);
    //             require!(
    //                 expected_leaves_indexer == *ctx.remaining_accounts[0].key,
    //                 ErrorCode::InvalidIndexerAccount
    //             );
    //             msg!("Enforcing first sub‐batch memo");
    //             enforce_sub_batch_memo(
    //                 sysvar_ai,
    //                 pool.batch_number,
    //                 &pool.batch_leaves[..SUB_BATCH_SIZE],
    //             )?;
    //         }
    //         // 4) did we just fill up all 16 slots?
    //         if idx + 1 == LEAVES_LENGTH {
    //             //Make sure that the leaves indexer is included
    //             let (expected_leaves_indexer, _bump) =
    //             Pubkey::find_program_address(&[b"leaves_indexer", &pool.identifier], ctx.program_id);
    //         require!(
    //             expected_leaves_indexer == *ctx.remaining_accounts[0].key,
    //             ErrorCode::InvalidIndexerAccount
    //         );
    //             msg!("Enforcing second sub‐batch memo");

    //             enforce_sub_batch_memo(
    //                 sysvar_ai,
    //                 pool.batch_number,
    //                 &pool.batch_leaves[SUB_BATCH_SIZE..LEAVES_LENGTH],
    //             )?;
    //             // rollover into peaks, bump batch_number, reset leaves
    //             let batch_root = pool.merkle_root_batch;

    //             pool.update_peaks(batch_root);
    //             pool.batch_number = pool.batch_number.checked_add(1).unwrap();
    //             pool.whole_tree_root = pool.compute_root_from_peaks();

    //             pool.batch_leaves = default_leaves();
    //             pool.merkle_root_batch = get_root(&pool.batch_leaves);
    //             // after rollover, the *next* leaves go at slot 0
    //             idx = 0;
    //             continue;
    //         }
    //         else if idx==0 && (pool.batch_number % BATCHES_PER_SMALL_TREE == 0) && pool.batch_number!=0{
    //             //Make sure that the correct subtree indexer is included
    //             let (expected_subtree_indexer, _bump) =
    //             Pubkey::find_program_address(&[b"subtree_indexer", &pool.identifier], ctx.program_id);
    //         require!(
    //             expected_subtree_indexer== *ctx.remaining_accounts[1].key,
    //             ErrorCode::InvalidIndexerAccount
    //         );
    //             //in this case, ensure the user posts the correct small tree root
    //             enforce_small_tree_memo(sysvar_ai, pool.batch_number-1, pool.last_small_tree_root)?;
    //         }
    //         idx += 1;
    //     }
    //     Ok(())
    // }
}
