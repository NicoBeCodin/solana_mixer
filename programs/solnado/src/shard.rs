use crate::error::ErrorCode;
use crate::id;
use crate::utils::*;
use crate::MerkleMountainRange;
use crate::TARGET_DEPTH_LARGE;
use crate::{BATCHES_PER_SMALL_TREE, LEAVES_LENGTH};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::log::sol_log_compute_units;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction;
use borsh::{BorshDeserialize, BorshSerialize};
//The pool fee covers nullifier storage

pub const SHARD_SIZE: usize = 8;
pub const SHARD_SPLITTING_THRESHOLD : usize = 512;
pub const POOL_FEE: u64 = 300_000;
pub const PREFIX_LENGTH: usize = 8;
pub const ON_BEHALF_FEE: u64 = 50_000;

//Fix the borrow mut data (makes program panic)
#[derive(Accounts)]
#[instruction(nullifier_hash: [u8;32])]
pub struct WithdrawVariableShard<'info> {
    /// Our on‐chain pool state
    #[account(
        mut,
        seeds = [ b"variable_pool", pool.identifier.as_ref() ],
        bump,           // assumes you store `bump: u8` in your pool struct
      )]
    pub pool: Account<'info, MerkleMountainRange>,

    ///CHECK :The nullifier shard
    #[account(mut)]
    pub nullifier_shard: AccountInfo<'info>,

    #[account(mut)]
    pub user: Signer<'info>,

    ///CHECK: SYSVAR_INSTRUCTIONS must be passed to read the Memo
    pub instruction_account: AccountInfo<'info>,

    pub system_program: Program<'info, System>,

    ///CHECK: This can be used by different functions
    #[account(mut)]
    pub dummy0_account: AccountInfo<'info>,
    ///CHECK: This can be used by different functions
    #[account(mut)]
    pub dummy1_account: AccountInfo<'info>,
}

//For combine deposit where we nullify only 1 leaf
#[derive(Accounts)]
pub struct CombineDepositShardSingle<'info> {
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

    /// CHECK: This is always present
    #[account(mut)]
    pub nullifier_shard: Account<'info, BitShard>,

    ///CHECK: This can be used by different functions
    #[account(mut)]
    pub dummy0_account: AccountInfo<'info>,
    ///CHECK: This can be used by different functions
    #[account(mut)]
    pub dummy1_account: AccountInfo<'info>,
}

//Corresponds to mode 1
pub fn combine_deposit_shard_single_nullifier<'info>(
    ctx: Context<CombineDepositShardSingle>,
    proof: [u8; 256],
    public_inputs: [u8; 128],
) -> Result<()> {
    let pool = &ctx.accounts.pool;
    let sysvar = &ctx.accounts.instruction_account;

    //Unpack the nullifier
    let (n, leaf1, leaf2, r) =
        verify_one_null_two_leaves(&proof, &public_inputs).map_err(|_| ErrorCode::InvalidProof)?;

    let temp_batch = get_root(&pool.batch_leaves);
    // sol_log_compute_units();
    msg!("Temp batch root: {:?}", temp_batch);
    let temp_root = pool.update_peaks_temp(temp_batch);
    sol_log_compute_units();
    msg!("Temp root: {:?}", temp_root);
    sol_log_compute_units();
    // 2) Check the root against our on‐chain deepened root
    require!(
        pool.deepen_temp(temp_root, TARGET_DEPTH_LARGE) == r,
        ErrorCode::InvalidPublicInputRoot
    );

    let shard = &mut ctx.accounts.nullifier_shard;
    //We take the data from the shard account
    process_one_nullifier_ai(
        &ctx.accounts.pool,
        ctx.bumps.pool,
        &mut shard.to_account_info(),
        &ctx.accounts.dummy0_account,
        &ctx.accounts.dummy1_account,
        n,
        &ctx.accounts.user.to_account_info(),
        &ctx.accounts.system_program,
        ctx.program_id,
    )?;

    let pool = &mut ctx.accounts.pool;

    for leaf in [leaf1, leaf2].iter() {
        let idx = pool.find_first_match();
        pool.batch_leaves[idx] = *leaf;
        pool.merkle_root_batch = get_root(&pool.batch_leaves);

        // a) first sub‐batch boundary?
        if idx + 1 == SUB_BATCH_SIZE {
            let expected_idxr = Pubkey::find_program_address(
                &[b"leaves_indexer", &pool.identifier],
                ctx.program_id,
            )
            .0;
            let indexer_account_key = ctx.remaining_accounts[0].key;

            require!(
                expected_idxr == *indexer_account_key,
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

            let indexer_account_key = ctx.remaining_accounts[0].key;

            require!(
                expected_idxr == *indexer_account_key,
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
            pool.batch_leaves = default_leaves();
            pool.merkle_root_batch = get_root(&pool.batch_leaves);
        }
        // c) small‐tree boundary? Post the subtree root of the previuous subtree when first depositing
        else if pool.batch_number % BATCHES_PER_SMALL_TREE == 0 && idx == 0 {
            let expected_subtree_indexer = Pubkey::find_program_address(
                &[b"subtree_indexer", &pool.identifier],
                ctx.program_id,
            )
            .0;
            let expected_leaves_indexer = Pubkey::find_program_address(
                &[b"leaves_indexer", &pool.identifier],
                ctx.program_id,
            )
            .0;

            let leaves_indexer_key = ctx.remaining_accounts[0].key;
            let subtree_indexer_key = ctx.remaining_accounts[1].key;

            require!(
                expected_leaves_indexer == *leaves_indexer_key,
                ErrorCode::InvalidIndexerAccount
            );
            require!(
                expected_subtree_indexer == *subtree_indexer_key,
                ErrorCode::InvalidIndexerAccount
            );
            enforce_small_tree_memo(&sysvar, pool.batch_number - 1, pool.last_small_tree_root)?;
        }
    }

    // Collect pool fee for nullifier processing (moved to end to avoid borrowing conflicts)
    let transfer_ix = anchor_lang::solana_program::system_instruction::transfer(
        &ctx.accounts.user.key(),
        &ctx.accounts.pool.key(),
        POOL_FEE,
    );
    
    anchor_lang::solana_program::program::invoke(
        &transfer_ix,
        &[
            ctx.accounts.user.to_account_info(),
            ctx.accounts.pool.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
    )?;
    
    msg!("Collected {} lamports as pool fee for nullifier processing", POOL_FEE);

    Ok(())
}

#[derive(Accounts)]
pub struct CombineDepositShardDouble<'info> {
    #[account(
        mut,
        seeds = [b"variable_pool".as_ref(), &pool.identifier],
        bump
    )]
    pub pool: Account<'info, MerkleMountainRange>,

    #[account(mut)]
    pub user: Signer<'info>,

    ///CHECK: SYSVAR_INSTRUCTIONS must be passed to read the Memo
    pub instruction_account: AccountInfo<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: This is always present
    #[account(mut)]
    pub nullifier_shard1: Box<Account<'info, BitShard>>,

    ///CHECK: For the splitting of the first shard
    #[account(mut)]
    pub dummy10_account: AccountInfo<'info>,
    ///CHECK: For the splitting of the first shard
    #[account(mut)]
    pub dummy11_account: AccountInfo<'info>,

    ///CHECK : Second nullfier shard, might be useless if both nullifers are supposed to go on same shard
    #[account(mut)]
    pub nullifier_shard2: Box<Account<'info, BitShard>>,
    //this is a temporary adjustement, needs to be solved to allow for the edge case of two shards being full at the same time and being split
    //Currently this blows up the stack size
    /// CHECK: For the splitting of the second shard
    #[account(mut)]
    pub dummy20_account: AccountInfo<'info>,
    ///CHECK: For the splitting of the second shard
    #[account(mut)]
    pub dummy21_account: AccountInfo<'info>,
}

pub fn combine_deposit_shard_double_nullifier<'info>(
    ctx: Context<CombineDepositShardDouble>,
    same_shard: u8,
    proof: [u8; 256],
    public_inputs: [u8; 128],
) -> Result<()> {
    let pool = &ctx.accounts.pool;
    let sysvar = &ctx.accounts.instruction_account;


    // --- two nullifiers → one leaf (old behavior) ---
    let (n1, n2, leaf, r) =
        verify_combine_proof(&proof, &public_inputs).map_err(|_| ErrorCode::InvalidProof)?;

    let temp_batch = get_root(&pool.batch_leaves);
    // sol_log_compute_units();
    msg!("Temp batch root: {:?}", temp_batch);
    let temp_root = pool.update_peaks_temp(temp_batch);
    sol_log_compute_units();
    msg!("Temp root: {:?}", temp_root);
    sol_log_compute_units();
    // 2) Check the root against our on‐chain deepened root
    require!(
        pool.deepen_temp(temp_root, TARGET_DEPTH_LARGE) == r,
        ErrorCode::InvalidPublicInputRoot
    );

    let shard1 = &mut ctx.accounts.nullifier_shard1;
    process_one_nullifier_ai(
        &ctx.accounts.pool,
        ctx.bumps.pool,
        &mut shard1.to_account_info(),
        &ctx.accounts.dummy10_account,
        &ctx.accounts.dummy11_account,
        n1,
        &ctx.accounts.user.to_account_info(),
        &ctx.accounts.system_program,
        ctx.program_id,
    )?;

    

    if same_shard == 1 {
        process_one_nullifier_ai(
            &ctx.accounts.pool,
            ctx.bumps.pool,
            &mut shard1.to_account_info(),
            &ctx.accounts.dummy10_account,
            &ctx.accounts.dummy11_account,
            n2,
            &ctx.accounts.user.to_account_info(),
            &ctx.accounts.system_program,
            ctx.program_id,
        )?;
    } else {
        let shard2 = &mut ctx.accounts.nullifier_shard2;
        
        process_one_nullifier_ai(
            &ctx.accounts.pool,
            ctx.bumps.pool,
            &mut shard2.to_account_info(),
            &ctx.accounts.dummy20_account.to_account_info(),
            &ctx.accounts.dummy21_account.to_account_info(),
            n2,
            &ctx.accounts.user.to_account_info(),
            &ctx.accounts.system_program,
            ctx.program_id,
        )?;
    }

    let idx = pool.find_first_match() as usize;
    let pool = &mut ctx.accounts.pool;
    pool.batch_leaves[idx] = leaf;
    pool.merkle_root_batch = get_root(&pool.batch_leaves);

    // a) first sub‐batch boundary?
    if idx + 1 == SUB_BATCH_SIZE {
        let expected_idxr =
            Pubkey::find_program_address(&[b"leaves_indexer", pool.identifier.as_ref()], ctx.program_id).0;

        let indexer_account_key = ctx.remaining_accounts[0].key;

        require!(
            expected_idxr == *indexer_account_key,
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
        let expected_idxr =
            Pubkey::find_program_address(&[b"leaves_indexer", pool.identifier.as_ref()], ctx.program_id).0;
        let indexer_account_key = ctx.remaining_accounts[0].key;

        require!(
            expected_idxr == *indexer_account_key,
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

        pool.batch_leaves = default_leaves();
        pool.merkle_root_batch = get_root(&pool.batch_leaves);
    }
    // c) small‐tree boundary? Post the subtree root of the previuous subtree when first depositing
    else if pool.batch_number % BATCHES_PER_SMALL_TREE == 0 && idx == 0 {
        let expected_st_idxr =
            Pubkey::find_program_address(&[b"subtree_indexer", pool.identifier.as_ref()], ctx.program_id).0;
        let expected_indexer =
            Pubkey::find_program_address(&[b"leaves_indexer", pool.identifier.as_ref()], ctx.program_id).0;
        let (indexer_account_key, subtree_indxr) =
            (ctx.remaining_accounts[0].key, ctx.remaining_accounts[1].key);

        require!(
            expected_indexer == *indexer_account_key,
            ErrorCode::InvalidIndexerAccount
        );
        require!(
            expected_st_idxr == *subtree_indxr,
            ErrorCode::InvalidIndexerAccount
        );
        enforce_small_tree_memo(&sysvar, pool.batch_number - 1, pool.last_small_tree_root)?;
    }

    // Collect pool fees for nullifier processing (moved to end to avoid borrowing conflicts)
    let fee_amount = if same_shard == 1 { POOL_FEE * 2 } else { POOL_FEE * 2 };
    
    let transfer_ix = anchor_lang::solana_program::system_instruction::transfer(
        &ctx.accounts.user.key(),
        &ctx.accounts.pool.key(),
        fee_amount,
    );
    
    anchor_lang::solana_program::program::invoke(
        &transfer_ix,
        &[
            ctx.accounts.user.to_account_info(),
            ctx.accounts.pool.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
    )?;
    
    msg!("Collected {} lamports as pool fee for {} nullifier processing", fee_amount, if same_shard == 1 { "2 (same shard)" } else { "2 (different shards)" });

    Ok(())
}

pub fn check_prefix(null: &[u8; 32], shard_prefix: &[u8], shard_prefix_length: u8) -> Result<()> {
    msg!("Checking prefix - length: {}, nullifier: {:?}", shard_prefix_length, null);
    msg!("Shard prefix: {:?}", &shard_prefix[..shard_prefix_length as usize]);
    
    for i in 0..shard_prefix_length as usize {
        let over: u8 = if null[i] > 127 { 1 } else { 0 };
        msg!("Position {}: nullifier[{}] = {}, over = {}, shard_prefix[{}] = {}", 
             i, i, null[i], over, i, shard_prefix[i]);
        if over != shard_prefix[i] {
            return err!(ErrorCode::InvalidNullifierBits);
        }
    }

    Ok(())
}

fn process_one_nullifier_ai<'info>(
    pool: &Account<'info, MerkleMountainRange>,
    pool_bump: u8,
    shard_ai: &mut AccountInfo<'info>,
    child0_ai: &AccountInfo<'info>,
    child1_ai: &AccountInfo<'info>,
    null_be: [u8; 32],
    user_ai: &AccountInfo<'info>,
    system_program: &Program<'info, System>,
    program_id: &Pubkey,
) -> Result<()> {
    // 1) Slurp the account bytes into a local Vec and then drop the borrow immediately

    let raw_body = {
        let data = shard_ai.data.borrow();
        let v = data[8..].to_vec();
        drop(data);
        v
    };

    // now no active borrow on shard_ai.data

    // 2) Deserialize
    let mut shard = BitShard::deserialize(&mut raw_body.as_slice())
        .map_err(|_| ErrorCode::InvalidNullifierList)?;

    //Check that the crrect shard is being used
    check_prefix(&null_be, &shard.prefix, shard.prefix_len)?;

    // 4) PDA check
    let (expected_pda, _) = derive_shard_pda_key(pool.identifier, &shard.prefix, shard.prefix_len);
    require!(
        &expected_pda == shard_ai.key,
        ErrorCode::InvalidShardSelection
    );

    // 5) Insert or split
    if shard.nullifiers.len() < SHARD_SPLITTING_THRESHOLD {
        // insert into in‐memory struct
        let pos = match shard.nullifiers.binary_search(&null_be) {
            Ok(_) => return err!(ErrorCode::NullifierAlreadyUsed),
            Err(p) => p,
        };
        shard.nullifiers.insert(pos, null_be);
        shard.count += 1;

        // maybe grow the account
        let info = shard_ai.to_account_info();
        maybe_grow_shard_account(&info, &shard, &pool.to_account_info())?;
        // now *one* mutable borrow to write back
        {
            let mut d = shard_ai.data.borrow_mut();
            let buf = shard.try_to_vec()?;
            d[8..8 + buf.len()].copy_from_slice(&buf);
        }
        
    } else {
        msg!("Splitting shard");
        split_shard_and_insert(
            shard_ai,
            child0_ai,
            child1_ai,
            &pool.to_account_info(),
            pool_bump,
            &null_be,
            &pool.identifier,
            user_ai,
            system_program,
            program_id,
        )?;
    }
    msg!("End of process_nullifier_ai ");

    Ok(())
}

#[account]
pub struct BitShard {
    /// how many bits of the nullifier we’ve consumed so far
    pub prefix_len: u8,
    /// those bits, left‐aligned in this 32‐byte array
    pub prefix: [u8; PREFIX_LENGTH],

    pub count: u32,
    /// sorted list of nullifier hashes
    pub nullifiers: Vec<[u8; 32]>,
}

pub fn withdraw_variable_shard_nullifier(
    ctx: Context<WithdrawVariableShard>,
    mode: u8,
    proof: [u8; 256],
    public_inputs: [u8; 136],
) -> Result<()> {
    let public_inputs_slice = public_inputs.as_slice();
    let sysvar = &ctx.accounts.instruction_account;

    let (secret_be, null_be, root_be, new_leaf) = match mode {
        0 => {
            //withdraw only
            let (val_be, null_be, root_be) = verify_withdraw_proof(&proof, public_inputs_slice)
                .map_err(|_| ErrorCode::InvalidProof)?;
            (val_be, null_be, root_be, None)
        }
        1 => {
            //Withdraw and add a leaf
            let (val_be, null_be, root_be, new_leaf) =
                verify_withdraw_and_add_proof(&proof, public_inputs_slice)
                    .map_err(|_| ErrorCode::InvalidProof)?;
            (val_be, null_be, root_be, Some(new_leaf))
        }
        _ => return Err(ErrorCode::InvalidArgument.into()),
    };

    let amount = u64::from_be_bytes(secret_be);
    msg!("Amount: {}", amount);
    let pool = &ctx.accounts.pool;

    // sol_log_compute_units();
    // msg!("Temp batch: {:?}", &pool.batch_leaves);
    let temp_batch = get_root(&pool.batch_leaves);
    // sol_log_compute_units();
    msg!("Temp batch root: {:?}", temp_batch);
    let temp_root = pool.update_peaks_temp(temp_batch);
    sol_log_compute_units();
    msg!("Temp root: {:?}", temp_root);
    sol_log_compute_units();
    // 2) Check the root against our on‐chain deepened root
    require!(
        pool.deepen_temp(temp_root, TARGET_DEPTH_LARGE) == root_be,
        ErrorCode::InvalidPublicInputRoot
    );
    // let shard = &mut ctx.accounts.nullifier_shard;
    sol_log_compute_units();
    process_one_nullifier_ai(
        &ctx.accounts.pool,
        ctx.bumps.pool,
        &mut ctx.accounts.nullifier_shard.to_account_info(),
        &ctx.accounts.dummy0_account,
        &ctx.accounts.dummy1_account,
        null_be,
        &ctx.accounts.user.to_account_info(),
        &ctx.accounts.system_program,
        &id(),
    )?;

    //Add to the batch
    if mode == 1 {
        let idx = pool.find_first_match() as usize;
        let pool = &mut ctx.accounts.pool;
        pool.batch_leaves[idx] = new_leaf.unwrap();
        pool.merkle_root_batch = get_root(&pool.batch_leaves);

        // a) first sub‐batch boundary?
        if idx + 1 == SUB_BATCH_SIZE {
            let expected_idxr =
                Pubkey::find_program_address(&[b"leaves_indexer", pool.identifier.as_ref()], ctx.program_id).0;

            let indexer_account_key = ctx.remaining_accounts[0].key;

            require!(
                expected_idxr == *indexer_account_key,
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
            let expected_idxr =
                Pubkey::find_program_address(&[b"leaves_indexer", pool.identifier.as_ref()], ctx.program_id).0;
            let indexer_account_key = ctx.remaining_accounts[0].key;

            require!(
                expected_idxr == *indexer_account_key,
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
        else if pool.batch_number % BATCHES_PER_SMALL_TREE == 0 && idx == 0 {
            let expected_st_idxr =
                Pubkey::find_program_address(&[b"subtree_indexer", pool.identifier.as_ref()], ctx.program_id).0;
            let expected_indexer =
                Pubkey::find_program_address(&[b"leaves_indexer", pool.identifier.as_ref()], ctx.program_id).0;
            let (indexer_account_key, subtree_indxr) =
                (ctx.remaining_accounts[0].key, ctx.remaining_accounts[1].key);

            require!(
                expected_indexer == *indexer_account_key,
                ErrorCode::InvalidIndexerAccount
            );
            require!(
                expected_st_idxr == *subtree_indxr,
                ErrorCode::InvalidIndexerAccount
            );
            enforce_small_tree_memo(&sysvar, pool.batch_number - 1, pool.last_small_tree_root)?;
        }
    }

    let net_amount = amount.checked_sub(POOL_FEE).unwrap();

    **ctx
        .accounts
        .pool
        .to_account_info()
        .try_borrow_mut_lamports()? -= net_amount;

    **ctx
        .accounts
        .user
        .to_account_info()
        .try_borrow_mut_lamports()? += net_amount;

    msg!(
        "Withdrew {} total; {} for nullifier storage, {} net to user",
        amount,
        POOL_FEE,
        net_amount
    );
    Ok(())
}

pub fn derive_shard_pda_key(pool_id: [u8; 16], prefix_bits: &[u8], prefix_len: u8) -> (Pubkey, u8) {
    // how many bytes we need from prefix_bits?

    Pubkey::find_program_address(
        &[
            b"nullifier_shard",
            &pool_id,
            &[prefix_len as u8],
            &prefix_bits[..prefix_len as usize],
        ],
        &id(),
    )
}

//This will be useful when the shard accounts need to grow, not useful at the moment
pub fn maybe_grow_shard_account<'info>(
    shard_ai: &AccountInfo<'info>, // the shard PDA
    shard: &BitShard,              // in‐memory struct
    pool_ai: &AccountInfo<'info>,  // the pool PDA
                                   // you need the pool bump to sign
) -> Result<()> {
    // figure out serialized size…

    let data = shard
        .try_to_vec()
        .map_err(|_| ErrorCode::InvalidNullifierList)?;
    let new_len = 8 + 32 + data.len();
    msg!("Data length: {}", new_len);
    let info = shard_ai.clone();

    if new_len > info.data_len() {
        // re‐alloc the shard account buffer
        info.realloc(new_len, false)?;
        msg!("Growing shard account!");
        // compute how much extra rent we need
        let rent = Rent::get()?;
        let required = rent.minimum_balance(new_len);
        let current = info.lamports();
        let delta = required.saturating_sub(current);

        if delta > 0 {
            **pool_ai.try_borrow_mut_lamports()? -= delta;
            **info.try_borrow_mut_lamports()? += delta;
            msg!("Manually moved {} lamports from pool → shard", delta);
        }
    }

    Ok(())
}
pub fn split_shard_and_insert<'info>(
    old_ai: &AccountInfo<'info>,
    child0_ai: &AccountInfo<'info>,
    child1_ai: &AccountInfo<'info>,
    pool_ai: &AccountInfo<'info>,
    pool_bump: u8,
    new_nullifier: &[u8; 32],
    pool_id: &[u8; 16],
    authority: &AccountInfo<'info>,
    system_program: &Program<'info, System>,
    program_id: &Pubkey,
) -> Result<()> {
    // 1) Load & deserialize the old shard
    let shard = {
        let data = old_ai.data.borrow();
        let mut rdr: &[u8] = &data[8..];
        BitShard::deserialize(&mut rdr).map_err(|_| ErrorCode::InvalidNullifierList)?
    };
    // must be full
    assert_eq!(shard.nullifiers.len(), SHARD_SIZE);

    // 2) New prefix length and split buffer for left/right
    let new_prefix_len = shard.prefix_len + 1;
    let mut left = Vec::with_capacity(SHARD_SIZE / 2);
    let mut right = Vec::with_capacity(SHARD_SIZE / 2);

    // check_prefix(nf, &shard.prefix, shard.prefix_len)?;
    // for each existing nullifier, test its next bit
    for nf in shard.nullifiers.iter() {
        // this will err if the prefix bits don't match at all
        // now check the *next* bit (i.e. at index = old prefix_len)
        let bit = if nf[shard.prefix_len as usize] > 127 {
            1
        } else {
            0
        };
        if bit == 0 {
            left.push(*nf);
        } else {
            right.push(*nf);
        }
    }
    // sanity
    assert_eq!(left.len() + right.len(), SHARD_SIZE);

    // 3) build the two new BitShard structs
    let bs0 = BitShard {
        prefix_len: new_prefix_len,
        prefix: shard.prefix,
        nullifiers: left,
        count: (SHARD_SIZE / 2) as u32,
    };
    let mut bs1 = BitShard {
        prefix_len: new_prefix_len,
        prefix: shard.prefix,
        nullifiers: right,
        count: (SHARD_SIZE / 2) as u32,
    };
    // flip the new bit in bs1.prefix
    {
        bs1.prefix[(bs1.prefix_len - 1) as usize] = 1;
    }

    // helper to create or realloc & write a child shard
    let write_child = |child_ai: &AccountInfo<'info>, bs: &BitShard| -> Result<()> {
        let body = bs
            .try_to_vec()
            .map_err(|_| ErrorCode::InvalidNullifierList)?;
        let space = 8 + body.len();
        let rent = Rent::get()?;
        let min_bal = rent.minimum_balance(space);

        // derive and check PDA
        let (pda, _) = derive_shard_pda_key(
            *pool_id,
            &bs.prefix[..new_prefix_len as usize],
            new_prefix_len,
        );
        require!(pda == child_ai.key(), ErrorCode::InvalidShardSelection);

        // create if needed
        if child_ai.lamports() < min_bal {
            // transfer lamports out of the pool PDA into the child PDA:
            let ix = system_instruction::create_account(
                &pool_ai.key(), // **pool** pays
                &pda,           // child PDA
                min_bal,        // funding amount
                space as u64,   // bytes of data
                program_id,     // owned by your program
            );
            // now invoke with the pool PDA signing:
            invoke_signed(
                &ix,
                &[
                    pool_ai.clone(),  // must be writable & signer
                    child_ai.clone(), // child PDA
                    system_program.to_account_info(),
                ],
                &[&[
                    b"variable_pool", // your seed tag
                    pool_id,          // the 16-byte identifier
                    &[pool_bump],     // bump
                ]],
            )?;
        }

        // write discriminator + body
        let mut d = child_ai.data.borrow_mut();
        d[..8].copy_from_slice(&BitShard::DISCRIMINATOR);
        d[8..8 + body.len()].copy_from_slice(&body);
        Ok(())
    };

    // 4) write both children
    write_child(child0_ai, &bs0)?;
    write_child(child1_ai, &bs1)?;

    // 5) insert the NEW nullifier into the correct child
    let next_bit = if new_nullifier[shard.prefix_len as usize] > 127 {
        1
    } else {
        0
    };
    let target = if next_bit == 0 { child0_ai } else { child1_ai };
    {
        let mut buf = target.data.borrow_mut();
        let mut rdr: &[u8] = &buf[8..];
        let mut child: BitShard =
            BitShard::deserialize(&mut rdr).map_err(|_| ErrorCode::InvalidNullifierList)?;
        match child.nullifiers.binary_search(new_nullifier) {
            Ok(_) => return err!(ErrorCode::NullifierAlreadyUsed),
            Err(pos) => child.nullifiers.insert(pos, *new_nullifier),
        }
        // write back updated list
        let body = child
            .try_to_vec()
            .map_err(|_| ErrorCode::InvalidNullifierList)?;
        buf[..8].copy_from_slice(&BitShard::DISCRIMINATOR);
        buf[8..8 + body.len()].copy_from_slice(&body);
    }

    // 6) close (zero‐realloc + reclaim) the old shard
    {
        // shrink to zero
        old_ai.realloc(0, false)?;
        let bal = old_ai.lamports();
        if bal > 0 {
            **pool_ai.try_borrow_mut_lamports()? += bal;
            **old_ai.try_borrow_mut_lamports()? = 0;
            msg!("Closed old shard PDA, reclaimed {} lamports", bal);
        }
    }

    Ok(())
}

pub const SHARD_SPACE: usize = 8 + 1 + 32 + 4 + 32 * SHARD_SIZE;

#[derive(Accounts)]
pub struct InitializeNullifierShards<'info> {
    /// Your pool PDA (so we can seed the shards off it)
    #[account(
        mut,
        seeds = [ b"variable_pool", &pool.identifier ],
        bump
    )]
    pub pool: Account<'info, MerkleMountainRange>,

    //init_if_needed to reiinit shards
    /// shard for bit=0 at prefix_len=1
    #[account(
        init,
        payer = authority,
        space = SHARD_SPACE,
        seeds = [ b"nullifier_shard", pool.identifier.as_ref(), &[1_u8], &[0_u8] ],
        bump
    )]
    pub shard0: Account<'info, BitShard>,

    /// shard for bit=1 at prefix_len=1
    #[account(
        init,
        payer = authority,
        space = SHARD_SPACE,
        seeds = [ b"nullifier_shard", pool.identifier.as_ref(), &[1_u8], &[1_u8] ],
        bump
    )]
    pub shard1: Account<'info, BitShard>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_nullifier_shards(ctx: Context<InitializeNullifierShards>) -> Result<()> {
    // shard0: prefix_len=1, prefix bit=0
    let s0 = &mut ctx.accounts.shard0;
    s0.prefix_len = 1;
    s0.prefix = [0u8; PREFIX_LENGTH]; // all bits zero
    s0.nullifiers = Vec::new();
    s0.count = 0;

    // shard1: prefix_len=1, prefix bit=1
    let s1 = &mut ctx.accounts.shard1;
    s1.prefix_len = 1;
    let mut p1 = [0u8; PREFIX_LENGTH];
    p1[0] = 1; // set the high bit
    s1.prefix = p1;
    s1.nullifiers = Vec::new();
    s1.count = 0;

    Ok(())
}

#[derive(Accounts)]
pub struct ResetNullifierShards<'info> {
    #[account(
        mut,
        seeds = [ b"variable_pool", pool.identifier.as_ref() ],
        bump
    )]
    pub pool: Account<'info, MerkleMountainRange>,

    /// these must already exist and have the correct discriminator
    #[account(mut)]
    pub shard0: Account<'info, BitShard>,
    #[account(mut)]
    pub shard1: Account<'info, BitShard>,

    pub authority: Signer<'info>,
}

pub fn reset_nullifier_shards(ctx: Context<ResetNullifierShards>) -> Result<()> {
    // zero out shard0
    let s0 = &mut ctx.accounts.shard0;
    s0.prefix_len = 1;
    s0.prefix = [0u8; PREFIX_LENGTH];
    s0.count = 0;
    s0.nullifiers.clear();

    // zero out shard1
    let s1 = &mut ctx.accounts.shard1;
    s1.prefix_len = 1;
    let mut p1 = [0u8; PREFIX_LENGTH];
    p1[0] = 1;
    s1.prefix = p1;
    s1.count = 0;
    s1.nullifiers.clear();

    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawOnBehalfShard<'info> {
    /// The variable‐pool PDA
    #[account(
        mut,
        seeds = [b"variable_pool", pool.identifier.as_ref()],
        bump,
    )]
    pub pool: Account<'info, MerkleMountainRange>,

    ///CHECK: The nullifier shard
    #[account(mut)]
    pub nullifier_shard: AccountInfo<'info>,

    ///CHECK: The beneficiary of the withdrawal
    #[account(mut)]
    pub withdrawer: AccountInfo<'info>,

    /// The transaction fee‐payer (must sign)
    pub payer: Signer<'info>,

    ///CHECK: SYSVAR_INSTRUCTIONS must be passed to read the Memo
    pub instruction_account: AccountInfo<'info>,

    pub system_program: Program<'info, System>,

    ///CHECK: This can be used by different functions
    #[account(mut)]
    pub dummy0_account: AccountInfo<'info>,
    ///CHECK: This can be used by different functions
    #[account(mut)]
    pub dummy1_account: AccountInfo<'info>,
}

pub fn withdraw_on_behalf_with_shard(
    ctx: Context<WithdrawOnBehalfShard>,
    proof: [u8; 256],
    public_inputs: [u8; 104], // nullifier(32)||amount(8)||root(32)||withdrawer_pubkey(32)
) -> Result<()> {
    let pool = &ctx.accounts.pool;

    //
    // 1) Decode & verify, unpack secret_be, null_be, root_be, withdrawer_bytes
    //
    let (secret_be, null_be, withdrawer_bytes, root_be) =
        verify_withdraw_on_behalf(&proof, &public_inputs)
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
    let temp_batch = get_root(&pool.batch_leaves);
    let temp_root = pool.update_peaks_temp(temp_batch);
    require!(
        pool.deepen_temp(temp_root, TARGET_DEPTH_LARGE) == root_be,
        ErrorCode::InvalidPublicInputRoot
    );

    // 3) Compute amount and fee
    let amount = u64::from_be_bytes(secret_be);
    
    // Validate that the amount is reasonable (not too large)
    const MAX_REASONABLE_AMOUNT: u64 = 1_000_000_000_000; // 1000 SOL in lamports
    msg!("Validating amount: {} <= {}", amount, MAX_REASONABLE_AMOUNT);
    require!(
        amount <= MAX_REASONABLE_AMOUNT,
        ErrorCode::InvalidArgument
    );
    
    let pool_fee = POOL_FEE; // Pool fee for nullifier storage
    let on_behalf_fee = ON_BEHALF_FEE; // Fee for on-behalf withdrawal
    let total_fees = pool_fee + on_behalf_fee;
    msg!("Pool fee: {}, On-behalf fee: {}, Total fees: {}", pool_fee, on_behalf_fee, total_fees);
    let net_amount = amount
        .checked_sub(total_fees)
        .ok_or(ErrorCode::InvalidArgument)?;
    msg!("Net amount after fees: {}", net_amount);

    // Check if pool has sufficient balance
    let pool_lamports = ctx.accounts.pool.to_account_info().lamports();
    msg!("Pool lamports: {}", pool_lamports);
    msg!("Amount: {}", amount);
    require!(
        pool_lamports >= amount,
        ErrorCode::InsufficientFunds
    );

    // 4) Process nullifier using shard mechanism
    process_one_nullifier_ai(
        &ctx.accounts.pool,
        ctx.bumps.pool,
        &mut ctx.accounts.nullifier_shard.to_account_info(),
        &ctx.accounts.dummy0_account,
        &ctx.accounts.dummy1_account,
        null_be,
        &ctx.accounts.payer.to_account_info(),
        &ctx.accounts.system_program,
        &id(),
    )?;

    // 5) Move lamports: pool → withdrawer + payer
    // Subtract only the amount that leaves the pool (net_amount + on_behalf_fee)
    let amount_leaving_pool = net_amount + on_behalf_fee;
    **ctx.accounts.pool.to_account_info().try_borrow_mut_lamports()? -= amount_leaving_pool;
    **ctx.accounts.withdrawer.try_borrow_mut_lamports()? += net_amount;
    **ctx.accounts.payer.try_borrow_mut_lamports()? += on_behalf_fee;
    // Pool fee stays in the pool (already accounted for in amount_leaving_pool calculation)

    msg!(
        "Withdrew {} total; {} for pool fee, {} for on-behalf fee, {} net to withdrawer",
        amount,
        pool_fee,
        on_behalf_fee,
        net_amount
    );

    Ok(())
}
