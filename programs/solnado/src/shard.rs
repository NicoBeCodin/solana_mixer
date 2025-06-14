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
    pub prefix: [u8; 8],
    /// sorted list of nullifier hashes
    pub nullifiers: Vec<[u8; 32]>,
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
    
    let shard_account =  &ctx.accounts.nullifier_shard;
    //We take the data from the shard account
    let mut nullifier_shard = BitShard::try_from_slice(&shard_account.data.borrow_mut())?;
    let prefix = nullifier_shard.prefix.as_slice();
    let prefix_length = nullifier_shard.prefix_len;
    let prefix_bits = get_prefix_bits(prefix, prefix_length as usize);
    let nullifier_prefix_bits = get_prefix_bits(null_be.as_slice(), prefix_length as usize);

    require!(nullifier_prefix_bits == prefix_bits, ErrorCode::InvalidNullifierBits);

    let shard_key= derive_shard_pda_key("nullfier_shard",pool.identifier, &prefix_bits, prefix_length as usize, &id());
    require!(&shard_key.0==shard_account.key, ErrorCode::InvalidShardSelection);
    
    //Add code for progressive account resizing 
    if nullifier_shard.nullifiers.len() < SHARD_SIZE {
        // find insert position
        let pos = match nullifier_shard.nullifiers.binary_search(&null_be) {
            Ok(_)  => return err!(ErrorCode::NullifierAlreadyUsed),
            Err(p) => p,
        };

        // insert into the Vec
        nullifier_shard.nullifiers.insert(pos, null_be);

        // grow account if needed
        let shard_ai = ctx.accounts.nullifier_shard.to_account_info();
        maybe_grow_shard_account(&shard_ai, &nullifier_shard, &ctx.accounts.user)?;

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
            null_be, 
            pool.key(), 
            &ctx.accounts.user, 
            &ctx.accounts.system_program, 
            ctx.program_id
        )?;
    }

    let net_amount = amount.checked_sub(POOL_FEE).unwrap();

    **ctx
            .accounts
            .pool
            .to_account_info()
            .try_borrow_mut_lamports()? -= amount;

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

pub fn maybe_grow_shard_account<'info>(
    shard_ai: &AccountInfo<'info>,
    shard:    &BitShard,
    authority: &Signer<'info>,
) -> Result<()> {
    // 1) serialize shard to bytes
    let data = shard.try_to_vec().map_err(|_| ErrorCode::InvalidNullifierList)?;
    let new_len = 8  // Anchor discriminator
                + data.len();

    // 2) if we need more space, realloc
    let info = shard_ai.clone();
    if new_len > info.data_len() {
        // grow, no zero‐copy here so gap is zeroed
        info.realloc(new_len, false)?;
        // we assume the authority has signed and can pay (caller ensures this)
    }
    Ok(())
}

pub fn get_prefix_bits(data: &[u8], n: usize) -> Vec<u8> {
    let out_len = (n + 7) / 8;
    let mut out = vec![0u8; out_len];
    for i in 0..n {
        // which bit of data?
        let byte = data[i / 8];
        let bit  = (byte >> (7 - (i % 8))) & 1;
        // set in out
        out[i / 8] |= bit << (7 - (i % 8));
    }
    out
}

pub fn derive_shard_pda_key(
    seed_str:    &str,
    pool_id: [u8;16],
    prefix_bits: &[u8],
    prefix_len:  usize,
    program_id:  &Pubkey,
) -> (Pubkey, u8) {
    // how many bytes we need from prefix_bits?
    let byte_len = (prefix_len + 7) / 8;
    let prefix_seed = &prefix_bits[..byte_len];

    Pubkey::find_program_address(
        &[
            seed_str.as_bytes(), &pool_id,
            prefix_seed,
        ],
        program_id,
    )
}

pub fn split_shard_and_insert<'info>(
    old_ai:        &AccountInfo<'info>,
    child0_ai:     &AccountInfo<'info>,
    child1_ai:     &AccountInfo<'info>,
    new_nullifier: [u8;32],
    pool_key:      Pubkey,
    authority:     &AccountInfo<'info>,
    system_program:&Program<'info, System>,
    program_id:    &Pubkey,
) -> Result<()> {
    // 1) load & sort
    let old_data = old_ai.data.borrow().to_vec();
    let mut shard: BitShard = BitShard::try_from_slice(&old_data)?;
    shard.nullifiers.sort_unstable();
    assert!(shard.nullifiers.len() == SHARD_SIZE);

    // 2) split
    let half   = SHARD_SIZE/2;
    let lower  = &shard.nullifiers[..half];
    let upper  = &shard.nullifiers[half..];
    let new_len= shard.prefix_len.checked_add(1).unwrap() as usize;

    // 3) prepare Borsh‐buffers
    let mut bs0 = BitShard {
        prefix_len: shard.prefix_len + 1,
        prefix:     shard.prefix,
        nullifiers: lower.to_vec(),
    };
    let mut bs1 = BitShard {
        prefix_len: shard.prefix_len + 1,
        prefix:     shard.prefix,
        nullifiers: upper.to_vec(),
    };
    // set the new bit in bs1.prefix
    {
        let idx = shard.prefix_len as usize;
        let mask = 1 << (7 - (idx % 8));
        bs1.prefix[idx/8] |= mask;
    }

    // 4) create / realloc child0
    let space = 8 + 1 + 32 + 4 + 32*SHARD_SIZE;
    let lam  = Rent::get()?.minimum_balance(space);
    let seed0= &[b"nullifier_shard", pool_key.as_ref(), &[new_len as u8], &[0_u8]];
    let (pda0, bump0) = Pubkey::find_program_address(seed0, program_id);
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
            &[authority.clone(), child0_ai.clone(), system_program.to_account_info()],
            &[&[seed0[0],seed0[1],seed0[2],seed0[3],&[bump0]]],
        )?;
    }
    // serialize bs0 into child0
    bs0.serialize(&mut &mut child0_ai.data.borrow_mut()[..])?;

    // 5) create / realloc child1
    let seed1= &[b"nullifier_shard", pool_key.as_ref(), &[new_len as u8], &[1_u8]];
    let (pda1, bump1) = Pubkey::find_program_address(seed1, program_id);
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
            &[authority.clone(), child1_ai.clone(), system_program.to_account_info()],
            &[&[seed1[0],seed1[1],seed1[2],seed1[3],&[bump1]]],
        )?;
    }
    bs1.serialize(&mut &mut child1_ai.data.borrow_mut()[..])?;

    // 6) insert into correct child
    let bit_byte = (new_nullifier[new_len/8] >> (7 - (new_len % 8))) & 1;
    let target_ai = if bit_byte==0 { child0_ai } else { child1_ai };
    let mut child: BitShard = BitShard::try_from_slice(&target_ai.data.borrow())?;
    match child.nullifiers.binary_search(&new_nullifier) {
        Ok(_)    => return err!(ErrorCode::NullifierAlreadyUsed),
        Err(pos) => child.nullifiers.insert(pos, new_nullifier),
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
    pub pool:       Account<'info, MerkleMountainRange>,

    /// shard for bit=0 at prefix_len=1
    #[account(
        init,
        payer = authority,
        space = SHARD_SPACE,
        seeds = [ b"nullifier_shard", pool.identifier.as_ref(), &[1_u8], &[0_u8] ],
        bump
    )]
    pub shard0:     Account<'info, BitShard>,

    /// shard for bit=1 at prefix_len=1
    #[account(
        init,
        payer = authority,
        space = SHARD_SPACE,
        seeds = [ b"nullifier_shard", pool.identifier.as_ref(), &[1_u8], &[1_u8] ],
        bump
    )]
    pub shard1:     Account<'info, BitShard>,

    #[account(mut)]
    pub authority:  Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_nullifier_shards(
    ctx: Context<InitializeNullifierShards>
) -> Result<()> {
    // shard0: prefix_len=1, prefix bit=0
    let s0 = &mut ctx.accounts.shard0;
    s0.prefix_len = 1;
    s0.prefix = [0u8; 8];        // all bits zero
    s0.nullifiers = Vec::new();

    // shard1: prefix_len=1, prefix bit=1
    let s1 = &mut ctx.accounts.shard1;
    s1.prefix_len = 1;
    let mut p1 = [0u8; 8];
    p1[0] = 0b1000_0000;           // set the high bit
    s1.prefix = p1;
    s1.nullifiers = Vec::new();

    Ok(())
}