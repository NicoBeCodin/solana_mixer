use crate::error::ErrorCode;
use crate::id;
use crate::utils::*;
use crate::MerkleMountainRange;
use crate::{BATCHES_PER_SMALL_TREE, LEAVES_LENGTH};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction;
use borsh::{BorshDeserialize, BorshSerialize};
//The pool fee covers nullifier storage
pub const SHARD_SIZE: usize = 8;
pub const POOL_FEE: u64 = 300_000;
pub const PREFIX_LENGTH: usize = 8;

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
    pub nullifier_shard: Account<'info, BitShard>,

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

    let deep_root = pool.get_deep_root();
    let mut pi = public_inputs;

    let offset = 96;

    // overwrite that slice with our on-chain root
    pi[offset..offset + 32].copy_from_slice(&deep_root);

    //Unpack the nullifier
    let (n, leaf1, leaf2, r) =
        verify_one_null_two_leaves(&proof, &pi).map_err(|_| ErrorCode::InvalidProof)?;

    let shard = &mut ctx.accounts.nullifier_shard;
    //We take the data from the shard account
    process_one_nullifier(
        &ctx.accounts.pool,
        &shard.to_account_info(),
        shard,
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
    pub pool: Account<'info, MerkleMountainRange>,

    #[account(mut)]
    pub user: Signer<'info>,

    ///CHECK: SYSVAR_INSTRUCTIONS must be passed to read the Memo
    pub instruction_account: AccountInfo<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: This is always present
    #[account(mut)]
    pub nullifier_shard1: Account<'info, BitShard>,

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
    // /CHECK: For the splitting of the second shard
    // #[account(mut)]
    // pub dummy20_account: AccountInfo<'info>,
    // ///CHECK: For the splitting of the second shard
    // #[account(mut)]
    // pub dummy21_account: AccountInfo<'info>,
}

pub fn combine_deposit_shard_double_nullifier<'info>(
    ctx: Context<CombineDepositShardDouble>,
    same_shard: u8,
    proof: [u8; 256],
    public_inputs: [u8; 128],
) -> Result<()> {
    let pool = &ctx.accounts.pool;
    let sysvar = &ctx.accounts.instruction_account;

    let deep_root = pool.get_deep_root();
    let mut pi = public_inputs;
    let offset = 96;
    // overwrite that slice with our on-chain root
    pi[offset..offset + 32].copy_from_slice(&deep_root);

    // --- two nullifiers → one leaf (old behavior) ---
    let (n1, n2, leaf, r) =
        verify_combine_proof(&proof, &pi).map_err(|_| ErrorCode::InvalidProof)?;

    let shard1 = &mut ctx.accounts.nullifier_shard1;
    process_one_nullifier(
        &ctx.accounts.pool,
        &shard1.to_account_info(),
        shard1,
        &ctx.accounts.dummy10_account,
        &ctx.accounts.dummy11_account,
        n1,
        &ctx.accounts.user.to_account_info(),
        &ctx.accounts.system_program,
        ctx.program_id,
    )?;

    if same_shard == 1 {
        process_one_nullifier(
            &ctx.accounts.pool,
            &shard1.to_account_info(),
            shard1,
            &ctx.accounts.dummy10_account,
            &ctx.accounts.dummy11_account,
            n2,
            &ctx.accounts.user.to_account_info(),
            &ctx.accounts.system_program,
            ctx.program_id,
        )?;
    } else {
        process_one_nullifier_ai(
            &ctx.accounts.pool,
            &ctx.accounts.nullifier_shard2.to_account_info(),
            &ctx.accounts.dummy10_account,
            &ctx.accounts.dummy11_account,
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
            Pubkey::find_program_address(&[b"leaves_indexer", &pool.identifier], ctx.program_id).0;

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
            Pubkey::find_program_address(&[b"leaves_indexer", &pool.identifier], ctx.program_id).0;
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
            Pubkey::find_program_address(&[b"subtree_indexer", &pool.identifier], ctx.program_id).0;
        let expected_indexer =
            Pubkey::find_program_address(&[b"leaves_indexer", &pool.identifier], ctx.program_id).0;
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
    Ok(())
}

fn process_one_nullifier<'info>(
    pool: &Account<'info, MerkleMountainRange>,
    shard_ai: &AccountInfo<'info>,
    shard: &mut BitShard,
    child0_ai: &AccountInfo<'info>,
    child1_ai: &AccountInfo<'info>,
    null_be: [u8; 32],
    user_ai: &AccountInfo<'info>,
    system_program: &Program<'info, System>,
    program_id: &Pubkey,
) -> Result<()> {
    msg!("shard ai key: {} ", shard_ai.key);
    msg!("shard ai data: {:?}", shard_ai.data);
    msg!("shard data length: {}", shard_ai.data_len());

    // 2) Prefix check (no Vecs)
    for bit in 0..shard.prefix_len {
        let byte = (bit / 8) as usize;
        let shift = 7 - (bit % 8);
        let p = (shard.prefix[byte] >> shift) & 1;
        let n = (null_be[byte] >> shift) & 1;
        require!(p == n, ErrorCode::InvalidNullifierBits);
    }

    // 3) Derive & check PDA
    let (expected_pda, bump) =
        derive_shard_pda_key(pool.identifier, &shard.prefix, shard.prefix_len);
    require!(
        &expected_pda == shard_ai.key,
        ErrorCode::InvalidShardSelection
    );

    // 4) Insert or split
    if shard.nullifiers.len() < SHARD_SIZE {
        let pos = match shard.nullifiers.binary_search(&null_be) {
            Ok(_) => return err!(ErrorCode::NullifierAlreadyUsed),
            Err(p) => p,
        };
        shard.nullifiers.insert(pos, null_be);
        shard.count += 1;

        let info = shard_ai.to_account_info();
        maybe_grow_shard_account(&info, &shard, &pool.to_account_info(), bump)?;
    } else {
        msg!("splitting shard");
        split_shard_and_insert(
            shard_ai,
            child0_ai,
            child1_ai,
            &null_be,
            &pool.identifier,
            user_ai,
            system_program,
            program_id,
        )?;
    }

    Ok(())
}

//Only deserialize if needed
fn process_one_nullifier_ai<'info>(
    pool: &Account<'info, MerkleMountainRange>,
    shard_ai: &AccountInfo<'info>,
    child0_ai: &AccountInfo<'info>,
    child1_ai: &AccountInfo<'info>,
    null_be: [u8; 32],
    user_ai: &AccountInfo<'info>,
    system_program: &Program<'info, System>,
    program_id: &Pubkey,
) -> Result<()> {
    msg!("shard ai key: {} ", shard_ai.key);
    msg!("shard ai data: {:?}", shard_ai.data);
    msg!("shard data length: {}", shard_ai.data_len());

    // 1) Pull off and decode the zero‐copy discriminator
    let mut shard = {
        let data = shard_ai.data.borrow();
        let mut rdr: &[u8] = &data[8..];

        BitShard::deserialize(&mut rdr).map_err(|_| ErrorCode::InvalidNullifierList)?
    };

    // 2) Prefix check (no Vecs)
    for bit in 0..shard.prefix_len {
        let byte = (bit / 8) as usize;
        let shift = 7 - (bit % 8);
        let p = (shard.prefix[byte] >> shift) & 1;
        let n = (null_be[byte] >> shift) & 1;
        require!(p == n, ErrorCode::InvalidNullifierBits);
    }

    // 3) Derive & check PDA
    let (expected_pda, bump) =
        derive_shard_pda_key(pool.identifier, &shard.prefix, shard.prefix_len);
    require!(
        &expected_pda == shard_ai.key,
        ErrorCode::InvalidShardSelection
    );

    // 4) Insert or split
    if shard.nullifiers.len() < SHARD_SIZE {
        let pos = match shard.nullifiers.binary_search(&null_be) {
            Ok(_) => return err!(ErrorCode::NullifierAlreadyUsed),
            Err(p) => p,
        };
        shard.nullifiers.insert(pos, null_be);
        shard.count += 1;

        let info = shard_ai.to_account_info();
        maybe_grow_shard_account(&info, &shard, &pool.to_account_info(), bump)?;
        let mut d = shard_ai.data.borrow_mut();
        let buf = shard
            .try_to_vec()
            .map_err(|_| ErrorCode::InvalidNullifierList)?;
        d[8..8 + buf.len()].copy_from_slice(&buf);
    } else {
        msg!("splitting shard");
        split_shard_and_insert(
            shard_ai,
            child0_ai,
            child1_ai,
            &null_be,
            &pool.identifier,
            user_ai,
            system_program,
            program_id,
        )?;
    }

    Ok(())
}

fn check_prefix(prefix: &[u8], null_be: &[u8; 32], len: u8) -> bool {
    for i in 0..len {
        let byte_idx = (i / 8) as usize;
        let shift = 7 - (i % 8);
        let pbit = (prefix[byte_idx] >> shift) & 1;
        let nbit = (null_be[byte_idx] >> shift) & 1;
        if pbit != nbit {
            return false;
        }
    }
    true
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
//For serialization purpose, manualy create the discriminator for the shard
// impl BitShard {
//     pub fn discriminator() -> [u8; 8] {
//         [0, 0, 0, 0, 0, 0, 0, 1]
//     }
// }

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

    let pool = &ctx.accounts.pool;

    // 2) Check the root against our on‐chain deepened root
    require!(
        pool.compare_to_deep(root_be),
        ErrorCode::InvalidPublicInputRoot
    );
    let shard = &mut ctx.accounts.nullifier_shard;

    process_one_nullifier(
        &ctx.accounts.pool,
        &shard.to_account_info(),
        shard,
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
        }
        // c) small‐tree boundary? Post the subtree root of the previuous subtree when first depositing
        else if pool.batch_number % BATCHES_PER_SMALL_TREE == 0 && idx == 0 {
            let expected_st_idxr = Pubkey::find_program_address(
                &[b"subtree_indexer", &pool.identifier],
                ctx.program_id,
            )
            .0;
            let expected_indexer = Pubkey::find_program_address(
                &[b"leaves_indexer", &pool.identifier],
                ctx.program_id,
            )
            .0;
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

pub fn get_prefix_bits(data: &[u8], n: usize) -> Vec<u8> {
    let out_len = (n + 7) / 8;
    let mut out = vec![0u8; out_len];
    for i in 0..n {
        // which bit of data?
        let byte = data[i / 8];
        let bit = (byte >> (7 - (i % 8))) & 1;
        // set in out
        out[i / 8] |= bit << (7 - (i % 8));
    }
    out
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
    pool_bump: u8,                 // you need the pool bump to sign
) -> Result<()> {
    // figure out serialized size…
    let data = shard
        .try_to_vec()
        .map_err(|_| ErrorCode::InvalidNullifierList)?;
    let new_len = 8 + data.len();
    let info = shard_ai.clone();

    if new_len > info.data_len() {
        // re‐alloc the shard account buffer
        info.realloc(new_len, false)?;

        // compute how much extra rent we need
        let rent = Rent::get()?;
        let required = rent.minimum_balance(new_len);
        let current = info.lamports();
        let delta = required.saturating_sub(current);

        if delta > 0 {
            // build a system transfer from pool PDA → shard PDA
            let ix = system_instruction::transfer(&pool_ai.key(), &info.key(), delta);
            // sign with the pool PDA’s seeds
            invoke_signed(
                &ix,
                &[
                    pool_ai.clone(),
                    info.clone(),
                    shard_ai.clone(), // not actually used by sysvar, but safe
                ],
                &[&[
                    b"variable_pool",
                    pool_ai.data.borrow()[..16].as_ref(), // pool.identifier
                    &[pool_bump],
                ]],
            )?;
        }
    }
    Ok(())
}

pub fn split_shard_and_insert<'info>(
    old_ai: &AccountInfo<'info>,
    child0_ai: &AccountInfo<'info>,
    child1_ai: &AccountInfo<'info>,
    new_nullifier: &[u8; 32],
    pool_id: &[u8; 16],
    authority: &AccountInfo<'info>,
    system_program: &Program<'info, System>,
    program_id: &Pubkey,
) -> Result<()> {
    // 1) load & sort
    let data = old_ai.data.borrow();
    let mut rdr: &[u8] = &data[8..];
    msg!("Length of data: {}", rdr.len());
    let mut shard = BitShard::deserialize(&mut rdr).map_err(|_| ErrorCode::InvalidNullifierList)?;
    shard.nullifiers.sort_unstable();
    assert!(shard.nullifiers.len() == SHARD_SIZE);

    // 2) split
    let half = SHARD_SIZE / 2;
    let lower = &shard.nullifiers[..half];
    let upper = &shard.nullifiers[half..];
    let new_len = shard.prefix_len.checked_add(1).unwrap() as usize;

    // 3) prepare Borsh‐buffers
    let bs0 = BitShard {
        prefix_len: shard.prefix_len + 1,
        prefix: shard.prefix,
        nullifiers: lower.to_vec(),
        count: half as u32,
    };
    let mut bs1 = BitShard {
        prefix_len: shard.prefix_len + 1,
        prefix: shard.prefix,
        nullifiers: upper.to_vec(),
        count: half as u32,
    };
    // set the new bit in bs1.prefix
    {
        let idx = shard.prefix_len as usize;
        let mask = 1 << (7 - (idx % 8));
        bs1.prefix[idx / 8] |= mask;
    }

    // 4) create / realloc child0
    let space = 8 + 1 + 8 + 4 + 32 * SHARD_SIZE / 2;
    let lam = Rent::get()?.minimum_balance(space);
    let (pda0, bump0) = derive_shard_pda_key(
        *pool_id,
        &bs0.prefix[..bs0.prefix_len as usize],
        bs0.prefix_len,
    );
    require!(pda0 == child0_ai.key(), ErrorCode::InvalidShardSelection);

    if child0_ai.lamports() < lam {
        invoke_signed(
            &system_instruction::create_account(
                &authority.key(),
                &pda0,
                lam,
                space as u64,
                program_id,
            ),
            &[
                authority.clone(),
                child0_ai.clone(),
                system_program.to_account_info(),
            ],
            &[&[
                b"nullifier_shard".as_ref(),
                pool_id,
                &bs0.prefix[..bs0.prefix_len as usize],
                &[bs0.prefix_len],
            ]],
        )?;
    }
    // serialize bs0 into child0
    child0_ai.data.borrow_mut()[..8].copy_from_slice(&BitShard::DISCRIMINATOR);
    bs0.serialize(&mut &mut child0_ai.data.borrow_mut()[8..])?;
    // bs0.serialize(&mut &mut child0_ai.data.borrow_mut()[..])?;

    // 5) create / realloc child1
    let (pda1, bump1) = derive_shard_pda_key(
        *pool_id,
        &bs1.prefix[..bs1.prefix_len as usize],
        bs1.prefix_len,
    );

    require!(pda1 == child1_ai.key(), ErrorCode::InvalidShardSelection);

    if child1_ai.lamports() < lam {
        invoke_signed(
            &system_instruction::create_account(
                &authority.key(),
                &pda1,
                lam,
                space as u64,
                program_id,
            ),
            &[
                authority.clone(),
                child1_ai.clone(),
                system_program.to_account_info(),
            ],
            &[&[
                b"nullifier_shard".as_ref(),
                pool_id,
                &bs1.prefix[..bs1.prefix_len as usize],
                &[bs1.prefix_len],
            ]],
        )?;
    }
    // bs1.serialize(&mut &mut child1_ai.data.borrow_mut()[..])?;
    child1_ai.data.borrow_mut()[..8].copy_from_slice(&BitShard::DISCRIMINATOR);
    bs1.serialize(&mut &mut child1_ai.data.borrow_mut()[8..])?;

    // 6) insert into correct child
    let bit_byte = (new_nullifier[new_len / 8] >> (7 - (new_len % 8))) & 1;
    let target_ai = if bit_byte == 0 { child0_ai } else { child1_ai };

    let raw = target_ai.data.borrow();
    // skip any discriminator (none for raw create_account, but safe)
    let mut rdr: &[u8] = &raw[0..];
    let mut child: BitShard = BitShard::deserialize(&mut rdr)?;

    match child.nullifiers.binary_search(&new_nullifier) {
        Ok(_) => return err!(ErrorCode::NullifierAlreadyUsed),
        Err(pos) => child.nullifiers.insert(pos, *new_nullifier),
    }
    child.serialize(&mut &mut target_ai.data.borrow_mut()[..])?;

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
        init_if_needed,
        payer = authority,
        space = SHARD_SPACE,
        seeds = [ b"nullifier_shard", pool.identifier.as_ref(), &[1_u8], &[0_u8] ],
        bump
    )]
    pub shard0: Account<'info, BitShard>,

    /// shard for bit=1 at prefix_len=1
    #[account(
        init_if_needed,
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
    p1[0] = 0b1000_0000; // set the high bit
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
    p1[0] = 0b1000_0000;
    s1.prefix = p1;
    s1.count = 0;
    s1.nullifiers.clear();

    Ok(())
}
