use crate::utils::*;
use crate::MerkleMountainRange;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction;
use borsh::{BorshDeserialize, BorshSerialize};
pub const SHARD_SIZE: usize = 8;
use crate::error::ErrorCode;
use crate::id;

//The pool fee covers nullifier storage
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
    pub nullifier_shard: AccountInfo<'info>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,

    ///CHECK: This can be used by different functions
    dummy0_account: AccountInfo<'info>,
    ///CHECK: This can be used by different functions
    dummy1_account: AccountInfo<'info>,
}

#[account]
pub struct BitShard {
    /// how many bits of the nullifier we’ve consumed so far
    pub prefix_len: u8,
    /// those bits, left‐aligned in this 32‐byte array
    pub prefix: [u8; PREFIX_LENGTH],
    /// sorted list of nullifier hashes
    pub nullifiers: Vec<[u8; 32]>,
}
impl BitShard{
    pub fn discriminator()->[u8;8]{
        [0,0,0,0,0,0,0,1]
    }
}

pub fn withdraw_variable_shard_nullifier(
    ctx: Context<WithdrawVariableShard>,
    mode: u8,
    proof: [u8; 256],
    public_inputs: [u8; 136],
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let public_inputs_slice = public_inputs.as_slice();

    let (secret_be, null_be, root_be, maybe_new_leaf) = match mode {
        0 => {
            //withdraw only
            let (val_be, null_be, root_be) = verify_withdraw_proof(&proof, public_inputs_slice)
                .map_err(|_| ErrorCode::InvalidProof)?;
            (val_be, null_be, root_be, None)
        }
        1 => {
            //Withdraw and add a leaf
            let (val_be, null_be, new_leaf, root_be) =
                verify_withdraw_and_deposit_proof(&proof, public_inputs_slice)
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

    let shard_account = &ctx.accounts.nullifier_shard;
    //We take the data from the shard account

    // new: skip the 8-byte Anchor discriminator,
    // then deserialize just the struct portion and ignore the trailing zeros.
    // 1) Deserialize in its own scope so the Ref is dropped immediately
    let mut nullifier_shard = {
        let data = shard_account.data.borrow();
        let mut rdr: &[u8] = &data[8..];
        msg!("Length of data: {}", rdr.len());
        BitShard::deserialize(&mut rdr).map_err(|_| ErrorCode::InvalidNullifierList)?
    };
    let prefix = nullifier_shard.prefix.as_slice();
    let prefix_length = nullifier_shard.prefix_len;
    let prefix_bits = get_prefix_bits(prefix, prefix_length as usize);
    let nullifier_prefix_bits = get_prefix_bits(null_be.as_slice(), prefix_length as usize);

    require!(
        nullifier_prefix_bits == prefix_bits,
        ErrorCode::InvalidNullifierBits
    );

    let shard_key = derive_shard_pda_key(pool.identifier, &prefix_bits, prefix_length);
    require!(
        &shard_key.0 == shard_account.key,
        ErrorCode::InvalidShardSelection
    );

    //Add code for progressive account resizing
    if nullifier_shard.nullifiers.len() < SHARD_SIZE {
        // find insert position
        let pos = match nullifier_shard.nullifiers.binary_search(&null_be) {
            Ok(_) => return err!(ErrorCode::NullifierAlreadyUsed),
            Err(p) => p,
        };

        // insert into the Vec
        nullifier_shard.nullifiers.insert(pos, null_be);

        // grow account if needed
        let shard_ai = ctx.accounts.nullifier_shard.to_account_info();
        maybe_grow_shard_account(&shard_ai, &nullifier_shard, &ctx.accounts.pool.to_account_info(), shard_key.1)?;

        // serialize back into account
        nullifier_shard
            .serialize(&mut &mut shard_ai.data.borrow_mut()[..])
            .map_err(|_| ErrorCode::InvalidNullifierList)?;
    } else {
        let child0_ai = &ctx.accounts.dummy0_account;
        let child1_ai = &ctx.accounts.dummy1_account;
        // full → split & insert new
        split_shard_and_insert(
            &ctx.accounts.nullifier_shard,
            /* shard0_ai: */ &child0_ai,
            /* shard1_ai: */ &child1_ai,
            &null_be,
            &pool.identifier,
            &ctx.accounts.user,
            &ctx.accounts.system_program,
            ctx.program_id,
        )?;
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
    shard_ai: &AccountInfo<'info>,   // the shard PDA
    shard:    &BitShard,             // in‐memory struct
    pool_ai:  &AccountInfo<'info>,   // the pool PDA
    pool_bump: u8,                   // you need the pool bump to sign
) -> Result<()> {
    // figure out serialized size…
    let data     = shard.try_to_vec().map_err(|_| ErrorCode::InvalidNullifierList)?;
    let new_len  = 8 + data.len();       
    let info     = shard_ai.clone();

    if new_len > info.data_len() {
        // re‐alloc the shard account buffer
        info.realloc(new_len, false)?;

        // compute how much extra rent we need
        let rent     = Rent::get()?;
        let required = rent.minimum_balance(new_len);
        let current  = info.lamports();
        let delta    = required.saturating_sub(current);

        if delta > 0 {
            // build a system transfer from pool PDA → shard PDA
            let ix = system_instruction::transfer(
                &pool_ai.key(),
                &info.key(),
                delta,
            );
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
    let mut bs0 = BitShard {
        prefix_len: shard.prefix_len + 1,
        prefix: shard.prefix,
        nullifiers: lower.to_vec(),
    };
    let mut bs1 = BitShard {
        prefix_len: shard.prefix_len + 1,
        prefix: shard.prefix,
        nullifiers: upper.to_vec(),
    };
    // set the new bit in bs1.prefix
    {
        let idx = shard.prefix_len as usize;
        let mask = 1 << (7 - (idx % 8));
        bs1.prefix[idx / 8] |= mask;
    }

    // 4) create / realloc child0
    let space = 8 + 1 + 8 + 4 + 32 * SHARD_SIZE;
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
    child0_ai.data.borrow_mut()[..8].copy_from_slice(&BitShard::discriminator());
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
    bs1.serialize(&mut &mut child1_ai.data.borrow_mut()[..])?;

    // 6) insert into correct child
    let bit_byte = (new_nullifier[new_len / 8] >> (7 - (new_len % 8))) & 1;
    let target_ai = if bit_byte == 0 { child0_ai } else { child1_ai };
    // let mut child: BitShard = BitShard::try_from_slice(&target_ai.data.borrow())?;

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

    // shard1: prefix_len=1, prefix bit=1
    let s1 = &mut ctx.accounts.shard1;
    s1.prefix_len = 1;
    let mut p1 = [0u8; PREFIX_LENGTH];
    p1[0] = 0b1000_0000; // set the high bit
    s1.prefix = p1;
    s1.nullifiers = Vec::new();

    Ok(())
}


// pub fn maybe_grow_shard_account<'info>(
//     shard_ai: &AccountInfo<'info>,
//     shard: &BitShard,
//     authority: &Signer<'info>,
// ) -> Result<()> {
//     // 1) serialize shard to bytes
//     let data = shard
//         .try_to_vec()
//         .map_err(|_| ErrorCode::InvalidNullifierList)?;
//     let new_len = 8  // Anchor discriminator
//                 + data.len();

//     // 2) if we need more space, realloc
//     let info = shard_ai.clone();
//     if new_len > info.data_len() {
//         // grow, no zero‐copy here so gap is zeroed
//         info.realloc(new_len, false)?;
//         // we assume the authority has signed and can pay (caller ensures this)
//     }
//     let rent = Rent::get()?;
//     let required = rent.minimum_balance(new_len);
//     let current = shard_ai.lamports();
//     if current < required {
//         let delta = required - current;
//         invoke(
//             &system_instruction::transfer(&payer.key(), &shard_ai.key(), delta),
//             &[payer.clone(), shard_ai.clone(), system_program.clone()],
//         )?;
//     }
//     Ok(())
// }
