use crate::error::ErrorCode;
use crate::id;
use crate::utils::*;
use crate::MerkleMountainRange;
use crate::TARGET_DEPTH_LARGE;
use crate::{BATCHES_PER_SMALL_TREE, LEAVES_LENGTH};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::log::sol_log_compute_units;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::hash::hash;
use anchor_lang::solana_program::system_instruction;
use borsh::{BorshDeserialize, BorshSerialize};
//The pool fee covers nullifier storage

pub const SHARD_SIZE: usize = 8;
pub const SHARD_SPLITTING_THRESHOLD: usize = 64;
pub const POOL_FEE: u64 = 300_000;
pub const PREFIX_LENGTH: usize = 32;
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
    pub nullifier_shard: AccountInfo<'info>,

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
    require!(pool.version == 2, ErrorCode::InvalidPoolAccount);
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

    // Pool fee collection happens BEFORE shard processing
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


    Ok(())
}

#[derive(Accounts)]
pub struct CombineDepositShardDouble<'info> {
    #[account(
        mut,
        seeds = [b"variable_pool".as_ref(), &pool.identifier],
        bump
    )]
    pub pool: Box<Account<'info, MerkleMountainRange>>,

    #[account(mut)]
    pub user: Signer<'info>,

    ///CHECK: SYSVAR_INSTRUCTIONS must be passed to read the Memo
    pub instruction_account: AccountInfo<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: This is always present
    #[account(mut)]
    pub nullifier_shard1: AccountInfo<'info>,

    ///CHECK: For the splitting of the first shard
    #[account(mut)]
    pub dummy10_account: AccountInfo<'info>,
    ///CHECK: For the splitting of the first shard
    #[account(mut)]
    pub dummy11_account: AccountInfo<'info>,

    ///CHECK : Second nullfier shard, might be useless if both nullifers are supposed to go on same shard
    #[account(mut)]
    pub nullifier_shard2: AccountInfo<'info>,
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
    require!(pool.version == 2, ErrorCode::InvalidPoolAccount);
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
    require!(shard_prefix.len() == PREFIX_LENGTH, ErrorCode::InvalidShardSelection);
    let route = hash(null).to_bytes();
    for i in 0..shard_prefix_length as usize {
        if bit_at(&route, i) != bit_at(shard_prefix, i) {
            return err!(ErrorCode::InvalidNullifierBits);
        }
    }

    Ok(())
}

fn bit_at(bytes: &[u8], bit: usize) -> u8 {
    (bytes[bit / 8] >> (7 - bit % 8)) & 1
}

fn set_bit(bytes: &mut [u8; PREFIX_LENGTH], bit: usize, value: u8) {
    let mask = 1u8 << (7 - bit % 8);
    if value == 0 { bytes[bit / 8] &= !mask; } else { bytes[bit / 8] |= mask; }
}

fn process_one_nullifier_ai<'info>(
    pool: &Account<'info, MerkleMountainRange>,
    _pool_bump: u8,
    shard_ai: &mut AccountInfo<'info>,
    _child0_ai: &AccountInfo<'info>,
    _child1_ai: &AccountInfo<'info>,
    null_be: [u8; 32],
    _user_ai: &AccountInfo<'info>,
    _system_program: &Program<'info, System>,
    _program_id: &Pubkey,
) -> Result<()> {
    require!(pool.version == 2, ErrorCode::InvalidPoolAccount);
    // 1) Slurp the account bytes into a local Vec and then drop the borrow immediately

    require!(shard_ai.owner == &id(), ErrorCode::InvalidShardSelection);
    require!(shard_ai.data_len() >= 8, ErrorCode::InvalidShardSelection);
    let raw_body = {
        let data = shard_ai.data.borrow();
        require!(&data[..8] == BitShard::DISCRIMINATOR, ErrorCode::InvalidShardSelection);
        let v = data[8..].to_vec();
        drop(data);
        v
    };

    // now no active borrow on shard_ai.data

    // 2) Deserialize
    let mut shard = BitShard::deserialize(&mut raw_body.as_slice())
        .map_err(|_| ErrorCode::InvalidNullifierList)?;
    require!(shard.nullifiers.len() == shard.count as usize, ErrorCode::InvalidNullifierList);
    require!(shard.nullifiers.len() <= SHARD_SPLITTING_THRESHOLD, ErrorCode::InvalidNullifierList);
    require!(shard.nullifiers.windows(2).all(|p| p[0] < p[1]), ErrorCode::InvalidNullifierList);
    require!(!shard.is_internal, ErrorCode::InvalidShardSelection);

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
        
    } else { return err!(ErrorCode::ShardNeedsSplit); }
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
    pub is_internal: bool,
}

pub fn withdraw_variable_shard_nullifier(
    ctx: Context<WithdrawVariableShard>,
    mode: u8,
    proof: [u8; 256],
    public_inputs: [u8; 200],
) -> Result<()> {
    let public_inputs_slice = public_inputs.as_slice();
    let sysvar = &ctx.accounts.instruction_account;

    let (secret_be, null_be, root_be, new_leaf) = match mode {
        0 => {
            //withdraw only
            let (val_be, null_be, root_be, recipient) = verify_withdraw_proof(&proof, public_inputs_slice)
                .map_err(|_| ErrorCode::InvalidProof)?;
            require!(recipient == ctx.accounts.user.key().to_bytes(), ErrorCode::InvalidWithdrawerKey);
            (val_be, null_be, root_be, None)
        }
        1 => {
            //Withdraw and add a leaf
            let (val_be, null_be, root_be, new_leaf, recipient) =
                verify_withdraw_and_add_proof(&proof, public_inputs_slice)
                    .map_err(|_| ErrorCode::InvalidProof)?;
            require!(recipient == ctx.accounts.user.key().to_bytes(), ErrorCode::InvalidWithdrawerKey);
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

    let net_amount = amount.checked_sub(POOL_FEE).ok_or(ErrorCode::InvalidArgument)?;
    let pool_info = ctx.accounts.pool.to_account_info();
    let rent_floor = Rent::get()?.minimum_balance(pool_info.data_len());
    require!(pool_info.lamports().saturating_sub(net_amount) >= rent_floor, ErrorCode::InsufficientFunds);

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
    let prefix_bytes = (prefix_len as usize).div_ceil(8);
    Pubkey::find_program_address(
        &[
            b"nullifier_shard_v2",
            &pool_id,
            &[prefix_len],
            &prefix_bits[..prefix_bytes],
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
    let remaining_in_chunk = (SHARD_SIZE - shard.nullifiers.len() % SHARD_SIZE) % SHARD_SIZE;
    let new_len = 8 + data.len() + 32 * remaining_in_chunk;
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
fn create_child_shard<'info>(
    pool_id: &[u8; 16],
    shard: &BitShard,
    child_ai: &AccountInfo<'info>,
    payer_ai: &AccountInfo<'info>,
    system_ai: &AccountInfo<'info>,
) -> Result<()> {
    let (expected, bump) = derive_shard_pda_key(*pool_id, &shard.prefix, shard.prefix_len);
    require!(expected == *child_ai.key, ErrorCode::InvalidShardSelection);
    require!(child_ai.lamports() == 0 && child_ai.data_is_empty(), ErrorCode::InvalidShardSelection);
    let body = shard.try_to_vec().map_err(|_| ErrorCode::InvalidNullifierList)?;
    let spare = (SHARD_SIZE - shard.nullifiers.len() % SHARD_SIZE) % SHARD_SIZE;
    let space = 8 + body.len() + spare * 32;
    let rent = Rent::get()?.minimum_balance(space);
    let depth = [shard.prefix_len];
    let bump_seed = [bump];
    let prefix_bytes = (shard.prefix_len as usize).div_ceil(8);
    let signer: &[&[u8]] = &[
        b"nullifier_shard_v2", pool_id, &depth,
        &shard.prefix[..prefix_bytes], &bump_seed,
    ];
    invoke_signed(
        &system_instruction::create_account(payer_ai.key, child_ai.key, rent, space as u64, &id()),
        &[payer_ai.clone(), child_ai.clone(), system_ai.clone()],
        &[signer],
    )?;
    let mut data = child_ai.try_borrow_mut_data()?;
    data[..8].copy_from_slice(&BitShard::DISCRIMINATOR);
    data[8..8 + body.len()].copy_from_slice(&body);
    Ok(())
}

#[derive(Accounts)]
pub struct SplitNullifierShard<'info> {
    #[account(mut, seeds = [b"variable_pool", pool.identifier.as_ref()], bump)]
    pub pool: Account<'info, MerkleMountainRange>,
    #[account(mut)]
    pub shard: Account<'info, BitShard>,
    /// CHECK: PDA and empty-account status are checked before creation.
    #[account(mut)]
    pub child0: UncheckedAccount<'info>,
    /// CHECK: PDA and empty-account status are checked before creation.
    #[account(mut)]
    pub child1: UncheckedAccount<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn split_nullifier_shard(ctx: Context<SplitNullifierShard>) -> Result<()> {
    let pool_id = ctx.accounts.pool.identifier;
    let parent = &mut ctx.accounts.shard;
    require!(!parent.is_internal, ErrorCode::InvalidShardSelection);
    require!(parent.nullifiers.len() >= SHARD_SPLITTING_THRESHOLD - 1 && parent.nullifiers.len() <= SHARD_SPLITTING_THRESHOLD, ErrorCode::InvalidArgument);
    require!(parent.count as usize == parent.nullifiers.len(), ErrorCode::InvalidNullifierList);
    require!(parent.nullifiers.windows(2).all(|p| p[0] < p[1]), ErrorCode::InvalidNullifierList);
    require!(parent.prefix_len < 255, ErrorCode::InvalidArgument);
    let (expected, _) = derive_shard_pda_key(pool_id, &parent.prefix, parent.prefix_len);
    require!(expected == parent.key(), ErrorCode::InvalidShardSelection);
    let depth = parent.prefix_len as usize;
    let mut prefix0 = parent.prefix;
    let mut prefix1 = parent.prefix;
    set_bit(&mut prefix0, depth, 0);
    set_bit(&mut prefix1, depth, 1);
    let mut left = Vec::with_capacity(SHARD_SPLITTING_THRESHOLD);
    let mut right = Vec::with_capacity(SHARD_SPLITTING_THRESHOLD);
    for nullifier in &parent.nullifiers {
        if bit_at(&hash(nullifier).to_bytes(), depth) == 0 { left.push(*nullifier); } else { right.push(*nullifier); }
    }
    let child0 = BitShard {
        prefix_len: parent.prefix_len + 1, prefix: prefix0,
        count: left.len() as u32, nullifiers: left, is_internal: false,
    };
    let child1 = BitShard {
        prefix_len: parent.prefix_len + 1, prefix: prefix1,
        count: right.len() as u32, nullifiers: right, is_internal: false,
    };
    let payer = ctx.accounts.payer.to_account_info();
    let system = ctx.accounts.system_program.to_account_info();
    create_child_shard(&pool_id, &child0, &ctx.accounts.child0.to_account_info(), &payer, &system)?;
    create_child_shard(&pool_id, &child1, &ctx.accounts.child1.to_account_info(), &payer, &system)?;
    parent.nullifiers.clear();
    parent.count = 0;
    parent.is_internal = true;
    let old_info = parent.to_account_info();
    let new_len = 8 + parent.try_to_vec()?.len();
    old_info.realloc(new_len, false)?;
    let min_balance = Rent::get()?.minimum_balance(new_len);
    let refund = old_info.lamports().saturating_sub(min_balance);
    if refund > 0 {
        **old_info.try_borrow_mut_lamports()? -= refund;
        **payer.try_borrow_mut_lamports()? += refund;
    }
    Ok(())
}

pub const SHARD_SPACE: usize = 8 + 1 + PREFIX_LENGTH + 4 + 4 + 32 * SHARD_SIZE + 1;

#[derive(Accounts)]
pub struct InitializeNullifierShards<'info> {
    #[account(mut, seeds = [b"variable_pool", pool.identifier.as_ref()], bump)]
    pub pool: Account<'info, MerkleMountainRange>,
    #[account(
        init,
        payer = authority,
        space = SHARD_SPACE,
        seeds = [b"nullifier_shard_v2", pool.identifier.as_ref(), &[0u8]],
        bump,
    )]
    pub root: Account<'info, BitShard>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn initialize_nullifier_shards(ctx: Context<InitializeNullifierShards>) -> Result<()> {
    let root = &mut ctx.accounts.root;
    root.prefix_len = 0;
    root.prefix = [0u8; PREFIX_LENGTH];
    root.count = 0;
    root.nullifiers = Vec::new();
    root.is_internal = false;
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
    public_inputs: [u8; 136], // nullifier, amount, root, recipient, binding
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
    require!(amount <= MAX_REASONABLE_AMOUNT, ErrorCode::InvalidArgument);
    
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
    require!(pool_lamports >= amount, ErrorCode::InsufficientFunds);

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
    let pool_info = ctx.accounts.pool.to_account_info();
    let rent_floor = Rent::get()?.minimum_balance(pool_info.data_len());
    require!(pool_info.lamports().saturating_sub(amount_leaving_pool) >= rent_floor, ErrorCode::InsufficientFunds);
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
