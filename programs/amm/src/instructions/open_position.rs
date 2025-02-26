use crate::error::ErrorCode;
use crate::libraries::liquidity_math;
use crate::libraries::tick_math;
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Token};
use anchor_spl::token_2022::{self, spl_token_2022::instruction::AuthorityType};
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};
use std::cell::RefMut;
#[cfg(feature = "enable-log")]
use std::convert::identity;
use std::ops::Deref;

#[derive(Accounts)]
#[instruction(tick_lower_index: i32, tick_upper_index: i32,tick_array_lower_start_index:i32,tick_array_upper_start_index:i32)]
pub struct OpenPosition<'info> {
    /// Pays to mint the position
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Receives the position NFT
    pub position_nft_owner: UncheckedAccount<'info>,

    /// Unique token mint address
    #[account(
        init,
        mint::decimals = 0,
        mint::authority = pool_state.key(),
        payer = payer,
        mint::token_program = token_program,
    )]
    pub position_nft_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Token account where position NFT will be minted
    /// This account created in the contract by cpi to avoid large stack variables
    #[account(
        init,
        associated_token::mint = position_nft_mint,
        associated_token::authority = position_nft_owner,
        payer = payer,
        token::token_program = token_program,
    )]
    pub position_nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// To store metaplex metadata
    /// CHECK: Safety check performed inside function body
    // #[account(mut)]
    // pub metadata_account: UncheckedAccount<'info>,

    /// Add liquidity for this pool
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Store the information of market marking in range
    #[account(
        init_if_needed,
        seeds = [
            POSITION_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_lower_index.to_be_bytes(),
            &tick_upper_index.to_be_bytes(),
        ],
        bump,
        payer = payer,
        space = ProtocolPositionState::LEN
    )]
    pub protocol_position: AccountLoader<'info, ProtocolPositionState>,

    /// CHECK: Account to mark the lower tick as initialized
    #[account(
        mut,
        seeds = [
            TICK_ARRAY_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_array_lower_start_index.to_be_bytes(),
        ],
        bump,
    )]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK:Account to store data for the position's upper tick
    #[account(
        mut,
        seeds = [
            TICK_ARRAY_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_array_upper_start_index.to_be_bytes(),
        ],
        bump,
    )]
    pub tick_array_upper: UncheckedAccount<'info>,

    /// personal position state
    #[account(
        init,
        seeds = [POSITION_SEED.as_bytes(), position_nft_mint.key().as_ref()],
        bump,
        payer = payer,
        space = PersonalPositionState::LEN
    )]
    pub personal_position: AccountLoader<'info, PersonalPositionState>,

    /// The token_0 account deposit token to the pool
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub token_account_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The token_1 account deposit token to the pool
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub token_account_1: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.load()?.token_vault_0
    )]
    pub token_vault_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.load()?.token_vault_1
    )]
    pub token_vault_1: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Sysvar for token mint and ATA creation
    pub rent: Sysvar<'info, Rent>,

    /// Program to create the position manager state account
    pub system_program: Program<'info, System>,

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,

    //pub metadata_program: Program<'info, Metadata>,
    // remaining account
    // #[account(
    //     seeds = [
    //         POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
    //         pool_state.key().as_ref(),
    //     ],
    //     bump
    // )]
    // pub tick_array_bitmap: AccountLoader<'info, TickArrayBitmapExtension>,
}

#[derive(Accounts)]
#[instruction(tick_lower_index: i32, tick_upper_index: i32,tick_array_lower_start_index:i32,tick_array_upper_start_index:i32)]
pub struct OpenPositionV2<'info> {
    /// Pays to mint the position
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Receives the position NFT
    pub position_nft_owner: UncheckedAccount<'info>,

    /// Unique token mint address
    #[account(
        init,
        mint::decimals = 0,
        mint::authority = pool_state.key(),
        payer = payer,
        mint::token_program = token_program,
    )]
    pub position_nft_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Token account where position NFT will be minted
    /// This account created in the contract by cpi to avoid large stack variables
    #[account(
        init,
        associated_token::mint = position_nft_mint,
        associated_token::authority = position_nft_owner,
        payer = payer,
        token::token_program = token_program,
    )]
    pub position_nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// To store metaplex metadata
    /// CHECK: Safety check performed inside function body
    //#[account(mut)]
    //pub metadata_account: UncheckedAccount<'info>,

    /// Add liquidity for this pool
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Store the information of market marking in range
    #[account(
        init_if_needed,
        seeds = [
            POSITION_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_lower_index.to_be_bytes(),
            &tick_upper_index.to_be_bytes(),
        ],
        bump,
        payer = payer,
        space = ProtocolPositionState::LEN
    )]
    pub protocol_position: AccountLoader<'info, ProtocolPositionState>,

    /// CHECK: Account to mark the lower tick as initialized
    #[account(
        mut,
        seeds = [
            TICK_ARRAY_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_array_lower_start_index.to_be_bytes(),
        ],
        bump,
    )]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK:Account to store data for the position's upper tick
    #[account(
        mut,
        seeds = [
            TICK_ARRAY_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_array_upper_start_index.to_be_bytes(),
        ],
        bump,
    )]
    pub tick_array_upper: UncheckedAccount<'info>,

    /// personal position state
    #[account(
        init,
        seeds = [POSITION_SEED.as_bytes(), position_nft_mint.key().as_ref()],
        bump,
        payer = payer,
        space = PersonalPositionState::LEN
    )]
    pub personal_position: AccountLoader<'info, PersonalPositionState>,

    /// The token_0 account deposit token to the pool
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub token_account_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The token_1 account deposit token to the pool
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub token_account_1: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.load()?.token_vault_0
    )]
    pub token_vault_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.load()?.token_vault_1
    )]
    pub token_vault_1: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Sysvar for token mint and ATA creation
    pub rent: Sysvar<'info, Rent>,

    /// Program to create the position manager state account
    pub system_program: Program<'info, System>,

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// Program to create NFT metadata
    /// CHECK: Metadata program address constraint applied
    //pub metadata_program: Program<'info, Metadata>,
    /// Program to create mint account and mint tokens
    pub token_program_2022: Program<'info, Token2022>,
    /// The mint of token vault 0
    #[account(
        address = token_vault_0.mint
    )]
    pub vault_0_mint: Box<InterfaceAccount<'info, Mint>>,
    /// The mint of token vault 1
    #[account(
        address = token_vault_1.mint
    )]
    pub vault_1_mint: Box<InterfaceAccount<'info, Mint>>,
    // remaining account
    // #[account(
    //     seeds = [
    //         POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
    //         pool_state.key().as_ref(),
    //     ],
    //     bump
    // )]
    // pub tick_array_bitmap: AccountLoader<'info, TickArrayBitmapExtension>,
}

pub fn open_position_v1<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, OpenPosition<'info>>,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    tick_array_lower_start_index: i32,
    tick_array_upper_start_index: i32,
    base_flag: Option<bool>,
    direction: PositionDirection
) -> Result<()> {
    open_position(
        &ctx.accounts.payer,
        &ctx.accounts.position_nft_owner,
        &ctx.accounts.position_nft_mint,
        &ctx.accounts.position_nft_account,
        //&ctx.accounts.metadata_account,
        &ctx.accounts.pool_state,
        &ctx.accounts.tick_array_lower,
        &ctx.accounts.tick_array_upper,
        &mut ctx.accounts.protocol_position,
        &mut ctx.accounts.personal_position,
        &ctx.accounts.token_account_0,
        &ctx.accounts.token_account_1,
        &ctx.accounts.token_vault_0,
        &ctx.accounts.token_vault_1,
        &ctx.accounts.system_program,
        &ctx.accounts.token_program,
        &ctx.accounts.associated_token_program,
      //  &ctx.accounts.metadata_program,
        None,
        None,
        None,
        &ctx.remaining_accounts,
        ctx.bumps.protocol_position,
        ctx.bumps.personal_position,
        liquidity,
        amount_0_max,
        amount_1_max,
        tick_lower_index,
        tick_upper_index,
        tick_array_lower_start_index,
        tick_array_upper_start_index,
        base_flag,
        direction
    )
}

pub fn open_position_v2<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, OpenPositionV2<'info>>,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    tick_array_lower_start_index: i32,
    tick_array_upper_start_index: i32,
    base_flag: Option<bool>,
    direction: PositionDirection
) -> Result<()> {
    open_position(
        &ctx.accounts.payer,
        &ctx.accounts.position_nft_owner,
        &ctx.accounts.position_nft_mint,
        &ctx.accounts.position_nft_account,
       // &ctx.accounts.metadata_account,
        &ctx.accounts.pool_state,
        &ctx.accounts.tick_array_lower,
        &ctx.accounts.tick_array_upper,
        &mut ctx.accounts.protocol_position,
        &mut ctx.accounts.personal_position,
        &ctx.accounts.token_account_0,
        &ctx.accounts.token_account_1,
        &ctx.accounts.token_vault_0,
        &ctx.accounts.token_vault_1,
        &ctx.accounts.system_program,
        &ctx.accounts.token_program,
        &ctx.accounts.associated_token_program,
       // &ctx.accounts.metadata_program,
        Some(ctx.accounts.token_program_2022.clone()),
        Some(ctx.accounts.vault_0_mint.clone()),
        Some(ctx.accounts.vault_1_mint.clone()),
        &ctx.remaining_accounts,
        ctx.bumps.protocol_position,
        ctx.bumps.personal_position,
        liquidity,
        amount_0_max,
        amount_1_max,
        tick_lower_index,
        tick_upper_index,
        tick_array_lower_start_index,
        tick_array_upper_start_index,
        base_flag,
        direction
    )
}

pub fn open_position<'a, 'b, 'c: 'info, 'info>(
    payer: &'b Signer<'info>,
    position_nft_owner: &'b UncheckedAccount<'info>,
    position_nft_mint: &'b Box<InterfaceAccount<'info, Mint>>,
    position_nft_account: &'b Box<InterfaceAccount<'info, TokenAccount>>,
    //metadata_account: &'b UncheckedAccount<'info>,
    pool_state_loader: &'b AccountLoader<'info, PoolState>,
    tick_array_lower_loader: &'b UncheckedAccount<'info>,
    tick_array_upper_loader: &'b UncheckedAccount<'info>,
    protocol_position: &'b mut AccountLoader<'info, ProtocolPositionState>,
    personal_position: &'b mut AccountLoader<'info, PersonalPositionState>,
    token_account_0: &'b Box<InterfaceAccount<'info, TokenAccount>>,
    token_account_1: &'b Box<InterfaceAccount<'info, TokenAccount>>,
    token_vault_0: &'b Box<InterfaceAccount<'info, TokenAccount>>,
    token_vault_1: &'b Box<InterfaceAccount<'info, TokenAccount>>,
    system_program: &'b Program<'info, System>,
    token_program: &'b Program<'info, Token>,
    _associated_token_program: &'b Program<'info, AssociatedToken>,
    //metadata_program: &'b Program<'info, Metadata>,
    token_program_2022: Option<Program<'info, Token2022>>,
    vault_0_mint: Option<Box<InterfaceAccount<'info, Mint>>>,
    vault_1_mint: Option<Box<InterfaceAccount<'info, Mint>>>,

    remaining_accounts: &'c [AccountInfo<'info>],
    protocol_position_bump: u8,
    personal_position_bump: u8,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    tick_array_lower_start_index: i32,
    tick_array_upper_start_index: i32,
    base_flag: Option<bool>,
    direction: PositionDirection
) -> Result<()> {
    let mut liquidity = liquidity;
    {
        let pool_state = &mut pool_state_loader.load_mut()?;
        if !pool_state.get_status_by_bit(PoolStatusBitIndex::OpenPositionOrIncreaseLiquidity) {
            return err!(ErrorCode::NotApproved);
        }
        check_ticks_order(tick_lower_index, tick_upper_index)?;
        check_tick_array_start_index(
            tick_array_lower_start_index,
            tick_lower_index,
            pool_state.tick_spacing,
        )?;
        check_tick_array_start_index(
            tick_array_upper_start_index,
            tick_upper_index,
            pool_state.tick_spacing,
        )?;

        // Why not use anchor's `init-if-needed` to create?
        // Beacuse `tick_array_lower` and `tick_array_upper` can be the same account, anchor can initialze tick_array_lower but it causes a crash when anchor to initialze the `tick_array_upper`,
        // the problem is variable scope, tick_array_lower_loader not exit to save the discriminator while build tick_array_upper_loader.
        let tick_array_lower_loader = TickArrayState::get_or_create_tick_array(
            payer.to_account_info(),
            tick_array_lower_loader.to_account_info(),
            system_program.to_account_info(),
            &pool_state_loader,
            tick_array_lower_start_index,
            pool_state.tick_spacing,
        )?;

        let tick_array_upper_loader =
            if tick_array_lower_start_index == tick_array_upper_start_index {
                AccountLoad::<TickArrayState>::try_from(&tick_array_upper_loader.to_account_info())?
            } else {
                TickArrayState::get_or_create_tick_array(
                    payer.to_account_info(),
                    tick_array_upper_loader.to_account_info(),
                    system_program.to_account_info(),
                    &pool_state_loader,
                    tick_array_upper_start_index,
                    pool_state.tick_spacing,
                )?
            };
        // check if protocol position is initilized
        if protocol_position.load()?.pool_id == Pubkey::default() {
            {
           let protocol_position = &mut protocol_position.load_init()?;

            protocol_position.bump = protocol_position_bump;
            protocol_position.pool_id = pool_state_loader.key();
            protocol_position.tick_lower_index = tick_lower_index;
            protocol_position.tick_upper_index = tick_upper_index;
            tick_array_lower_loader
                .load_mut()?
                .get_tick_state_mut(tick_lower_index, pool_state.tick_spacing)?
                .tick = tick_lower_index;
            tick_array_upper_loader
                .load_mut()?
                .get_tick_state_mut(tick_upper_index, pool_state.tick_spacing)?
                .tick = tick_upper_index;
        }
        }
        let protocol_position = &mut protocol_position.load_mut()?;

        let use_tickarray_bitmap_extension = pool_state.is_overflow_default_tickarray_bitmap(vec![
            tick_array_lower_start_index,
            tick_array_upper_start_index,
        ]);

        let (amount_0, amount_1, amount_0_transfer_fee, amount_1_transfer_fee) = add_liquidity(
            payer,
            token_account_0,
            token_account_1,
            token_vault_0,
            token_vault_1,
            &tick_array_lower_loader,
            &tick_array_upper_loader,
            protocol_position,
            token_program_2022,
            token_program,
            vault_0_mint,
            vault_1_mint,
            if use_tickarray_bitmap_extension {
                require_keys_eq!(
                    remaining_accounts[0].key(),
                    TickArrayBitmapExtension::key(pool_state_loader.key())
                );
                Some(&remaining_accounts[0])
            } else {
                None
            },
            pool_state,
            &mut liquidity,
            amount_0_max,
            amount_1_max,
            tick_lower_index,
            tick_upper_index,
            base_flag,
        )?;

        let personal_position = &mut personal_position.load_init()?;
        personal_position.bump = personal_position_bump;
        personal_position.nft_mint = position_nft_mint.key();
        personal_position.pool_id = pool_state_loader.key();
        personal_position.tick_lower_index = tick_lower_index;
        personal_position.tick_upper_index = tick_upper_index;

        personal_position.fee_growth_inside_0_last_x64 =
            protocol_position.fee_growth_inside_0_last_x64;
        personal_position.fee_growth_inside_1_last_x64 =
            protocol_position.fee_growth_inside_1_last_x64;
        personal_position.direction = direction;
        // update rewards, must update before update liquidity

        for i in 0..REWARD_NUM {
            personal_position.reward_infos[i].entry_price = pool_state.sqrt_price_x64;
            personal_position.reward_infos[i].current_price = pool_state.sqrt_price_x64;
        }
        personal_position.update_rewards(protocol_position.reward_growth_inside, false)?;
        personal_position.liquidity = liquidity;

        emit!(CreatePersonalPositionEvent {
            pool_state: pool_state_loader.key(),
            minter: payer.key(),
            nft_owner: position_nft_owner.key(),
            tick_lower_index: tick_lower_index,
            tick_upper_index: tick_upper_index,
            liquidity: liquidity,
            deposit_amount_0: amount_0,
            deposit_amount_1: amount_1,
            deposit_amount_0_transfer_fee: amount_0_transfer_fee,
            deposit_amount_1_transfer_fee: amount_1_transfer_fee
        });
    }
    create_nft_with_metadata(
        pool_state_loader,
        position_nft_mint,
        position_nft_account,
        //metadata_account,
        //metadata_program,
        token_program,
    )?;

    Ok(())
}

/// Add liquidity to an initialized pool
pub fn add_liquidity<'b, 'c: 'info, 'info>(
    payer: &'b Signer<'info>,
    token_account_0: &'b Box<InterfaceAccount<'info, TokenAccount>>,
    token_account_1: &'b Box<InterfaceAccount<'info, TokenAccount>>,
    token_vault_0: &'b Box<InterfaceAccount<'info, TokenAccount>>,
    token_vault_1: &'b Box<InterfaceAccount<'info, TokenAccount>>,
    tick_array_lower_loader: &'b AccountLoad<'info, TickArrayState>,
    tick_array_upper_loader: &'b AccountLoad<'info, TickArrayState>,
    protocol_position: &mut RefMut<ProtocolPositionState>,
    token_program_2022: Option<Program<'info, Token2022>>,
    token_program: &'b Program<'info, Token>,
    vault_0_mint: Option<Box<InterfaceAccount<'info, Mint>>>,
    vault_1_mint: Option<Box<InterfaceAccount<'info, Mint>>>,
    tick_array_bitmap_extension: Option<&'c AccountInfo<'info>>,
    pool_state: &mut RefMut<PoolState>,
    liquidity: &mut u128,
    amount_0_max: u64,
    amount_1_max: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    base_flag: Option<bool>,
) -> Result<(u64, u64, u64, u64)> {
    if *liquidity == 0 {
        if base_flag.is_none() {
            // when establishing a new position , liquidity allows for further additions
            return Ok((0, 0, 0, 0));
        }
        if base_flag.unwrap() {
            // must deduct transfer fee before calculate liquidity
            // because only v2 instruction support token_2022, vault_0_mint must be exist
            let amount_0_transfer_fee =
                get_transfer_fee(vault_0_mint.clone().unwrap(), amount_0_max).unwrap();
            *liquidity = liquidity_math::get_liquidity_from_single_amount_0(
                pool_state.sqrt_price_x64,
                tick_math::get_sqrt_price_at_tick(tick_lower_index)?,
                tick_math::get_sqrt_price_at_tick(tick_upper_index)?,
                amount_0_max.checked_sub(amount_0_transfer_fee).unwrap(),
            );
            #[cfg(feature = "enable-log")]
            msg!(
                "liquidity: {}, amount_0_max:{}, amount_0_transfer_fee:{}",
                *liquidity,
                amount_0_max,
                amount_0_transfer_fee
            );
        } else {
            // must deduct transfer fee before calculate liquidity
            // because only v2 instruction support token_2022, vault_1_mint must be exist
            let amount_1_transfer_fee =
                get_transfer_fee(vault_1_mint.clone().unwrap(), amount_1_max).unwrap();
            *liquidity = liquidity_math::get_liquidity_from_single_amount_1(
                pool_state.sqrt_price_x64,
                tick_math::get_sqrt_price_at_tick(tick_lower_index)?,
                tick_math::get_sqrt_price_at_tick(tick_upper_index)?,
                amount_1_max.checked_sub(amount_1_transfer_fee).unwrap(),
            );
            #[cfg(feature = "enable-log")]
            msg!(
                "liquidity: {}, amount_1_max:{}, amount_1_transfer_fee:{}",
                *liquidity,
                amount_1_max,
                amount_1_transfer_fee
            );
        }
    }
    assert!(*liquidity > 0);
    let liquidity_before = pool_state.liquidity;
    require_keys_eq!(tick_array_lower_loader.load()?.pool_id, pool_state.key());
    require_keys_eq!(tick_array_upper_loader.load()?.pool_id, pool_state.key());

    // get tick_state
    let mut tick_lower_state = *tick_array_lower_loader
        .load_mut()?
        .get_tick_state_mut(tick_lower_index, pool_state.tick_spacing)?;
    let mut tick_upper_state = *tick_array_upper_loader
        .load_mut()?
        .get_tick_state_mut(tick_upper_index, pool_state.tick_spacing)?;
    if tick_lower_state.tick == 0 {
        tick_lower_state.tick = tick_lower_index;
    }
    if tick_upper_state.tick == 0 {
        tick_upper_state.tick = tick_upper_index;
    }
    let clock = Clock::get()?;
    let (amount_0, amount_1, flip_tick_lower, flip_tick_upper) = modify_position(
        i128::try_from(*liquidity).unwrap(),
        pool_state,
        protocol_position,
        &mut tick_lower_state,
        &mut tick_upper_state,
        clock.unix_timestamp as u64,
    )?;

    // update tick_state
    tick_array_lower_loader.load_mut()?.update_tick_state(
        tick_lower_index,
        pool_state.tick_spacing,
        tick_lower_state,
    )?;
    tick_array_upper_loader.load_mut()?.update_tick_state(
        tick_upper_index,
        pool_state.tick_spacing,
        tick_upper_state,
    )?;

    if flip_tick_lower {
        let mut tick_array_lower = tick_array_lower_loader.load_mut()?;
        let before_init_tick_count = tick_array_lower.initialized_tick_count;
        tick_array_lower.update_initialized_tick_count(true)?;

        if before_init_tick_count == 0 {
            pool_state.flip_tick_array_bit(
                tick_array_bitmap_extension,
                tick_array_lower.start_tick_index,
            )?;
        }
    }
    if flip_tick_upper {
        let mut tick_array_upper = tick_array_upper_loader.load_mut()?;
        let before_init_tick_count = tick_array_upper.initialized_tick_count;
        tick_array_upper.update_initialized_tick_count(true)?;

        if before_init_tick_count == 0 {
            pool_state.flip_tick_array_bit(
                tick_array_bitmap_extension,
                tick_array_upper.start_tick_index,
            )?;
        }
    }
    require!(
        amount_0 > 0 || amount_1 > 0,
        ErrorCode::ForbidBothZeroForSupplyLiquidity
    );

    let mut amount_0_transfer_fee = 0;
    let mut amount_1_transfer_fee = 0;
    if vault_0_mint.is_some() {
        amount_0_transfer_fee =
            get_transfer_inverse_fee(vault_0_mint.clone().unwrap(), amount_0).unwrap();
    };
    if vault_1_mint.is_some() {
        amount_1_transfer_fee =
            get_transfer_inverse_fee(vault_1_mint.clone().unwrap(), amount_1).unwrap();
    }
    emit!(LiquidityCalculateEvent {
        pool_liquidity: liquidity_before,
        pool_sqrt_price_x64: pool_state.sqrt_price_x64,
        pool_tick: pool_state.tick_current,
        calc_amount_0: amount_0,
        calc_amount_1: amount_1,
        trade_fee_owed_0: 0,
        trade_fee_owed_1: 0,
        transfer_fee_0: amount_0_transfer_fee,
        transfer_fee_1: amount_1_transfer_fee,
    });
    #[cfg(feature = "enable-log")]
    msg!(
        "amount_0: {}, amount_0_transfer_fee: {}, amount_1: {}, amount_1_transfer_fee: {}",
        amount_0,
        amount_0_transfer_fee,
        amount_1,
        amount_1_transfer_fee
    );
    if amount_0_max != 0 {
        require_gte!(
            amount_0_max,
            amount_0 + amount_0_transfer_fee,
            ErrorCode::PriceSlippageCheck
        );
    }
    if amount_1_max != 0 {
        require_gte!(
            amount_1_max,
            amount_1 + amount_1_transfer_fee,
            ErrorCode::PriceSlippageCheck
        );
    }
    let mut token_2022_program_opt: Option<AccountInfo> = None;
    if token_program_2022.is_some() {
        token_2022_program_opt = Some(token_program_2022.clone().unwrap().to_account_info());
    }
    transfer_from_user_to_pool_vault(
        payer,
        token_account_0,
        token_vault_0,
        vault_0_mint,
        &token_program,
        token_2022_program_opt.clone(),
        amount_0 + amount_0_transfer_fee,
    )?;

    transfer_from_user_to_pool_vault(
        payer,
        token_account_1,
        token_vault_1,
        vault_1_mint,
        &token_program,
        token_2022_program_opt.clone(),
        amount_1 + amount_1_transfer_fee,
    )?;
    emit!(LiquidityChangeEvent {
        pool_state: pool_state.key(),
        tick: pool_state.tick_current,
        tick_lower: tick_lower_index,
        tick_upper: tick_upper_index,
        liquidity_before: liquidity_before,
        liquidity_after: pool_state.liquidity,
    });
    Ok((
        amount_0,
        amount_1,
        amount_0_transfer_fee,
        amount_1_transfer_fee,
    ))
}

pub fn modify_position(
    liquidity_delta: i128,
    pool_state: &mut RefMut<PoolState>,
    protocol_position_state: &mut ProtocolPositionState,
    tick_lower_state: &mut TickState,
    tick_upper_state: &mut TickState,
    timestamp: u64,
) -> Result<(u64, u64, bool, bool)> {
    let (flip_tick_lower, flip_tick_upper) = update_position(
        liquidity_delta,
        pool_state,
        protocol_position_state,
        tick_lower_state,
        tick_upper_state,
        timestamp,
    )?;
    let mut amount_0 = 0;
    let mut amount_1 = 0;

    if liquidity_delta != 0 {
        (amount_0, amount_1) = liquidity_math::get_delta_amounts_signed(
            pool_state.tick_current,
            pool_state.sqrt_price_x64,
            tick_lower_state.tick,
            tick_upper_state.tick,
            liquidity_delta,
        )?;
        if pool_state.tick_current >= tick_lower_state.tick
            && pool_state.tick_current < tick_upper_state.tick
        {
            pool_state.liquidity =
                liquidity_math::add_delta(pool_state.liquidity, liquidity_delta)?;
        }
    }

    Ok((amount_0, amount_1, flip_tick_lower, flip_tick_upper))
}

/// Updates a position with the given liquidity delta and tick
pub fn update_position(
    liquidity_delta: i128,
    pool_state: &mut RefMut<PoolState>,
    protocol_position_state: &mut ProtocolPositionState,
    tick_lower_state: &mut TickState,
    tick_upper_state: &mut TickState,
    timestamp: u64,
) -> Result<(bool, bool)> {
    let updated_reward_infos = pool_state.update_reward_infos(timestamp)?;

    let mut flipped_lower = false;
    let mut flipped_upper = false;

    // update the ticks if liquidity delta is non-zero
    if liquidity_delta != 0 {
        // Update tick state and find if tick is flipped
        flipped_lower = tick_lower_state.update(
            pool_state.tick_current,
            liquidity_delta,
            pool_state.fee_growth_global_0_x64,
            pool_state.fee_growth_global_1_x64,
            false,
            &updated_reward_infos,
        )?;
        flipped_upper = tick_upper_state.update(
            pool_state.tick_current,
            liquidity_delta,
            pool_state.fee_growth_global_0_x64,
            pool_state.fee_growth_global_1_x64,
            true,
            &updated_reward_infos,
        )?;
        #[cfg(feature = "enable-log")]
        msg!(
            "tick_upper.reward_growths_outside_x64:{:?}, tick_lower.reward_growths_outside_x64:{:?}",
            identity(tick_upper_state.reward_growths_outside_x64),
            identity(tick_lower_state.reward_growths_outside_x64)
        );
    }

    // Update fees
    let (fee_growth_inside_0_x64, fee_growth_inside_1_x64) = tick_array::get_fee_growth_inside(
        tick_lower_state.deref(),
        tick_upper_state.deref(),
        pool_state.tick_current,
        pool_state.fee_growth_global_0_x64,
        pool_state.fee_growth_global_1_x64,
    );

    // Update reward outside if needed
    let reward_growths_inside = tick_array::get_reward_growths_inside(
        tick_lower_state.deref(),
        tick_upper_state.deref(),
        pool_state.tick_current,
        &updated_reward_infos,
    );

    protocol_position_state.update(
        tick_lower_state.tick,
        tick_upper_state.tick,
        liquidity_delta,
        fee_growth_inside_0_x64,
        fee_growth_inside_1_x64,
        reward_growths_inside,
    )?;
    if liquidity_delta < 0 {
        if flipped_lower {
            tick_lower_state.clear();
        }
        if flipped_upper {
            tick_upper_state.clear();
        }
    }
    Ok((flipped_lower, flipped_upper))
}

//const METADATA_URI: &str =
//    "https://cloudflare-ipfs.com/ipfs/QmbzJafuKY3B4t25eq9zdKZMgXiMeW4jHLzf6KE6ZmHWn1/02.json";

fn create_nft_with_metadata<'info>(
    pool_state_loader: &AccountLoader<'info, PoolState>,
    position_nft_mint: &Box<InterfaceAccount<'info, Mint>>,
    position_nft_account: &Box<InterfaceAccount<'info, TokenAccount>>,
    //metadata_account: &UncheckedAccount<'info>,
    //metadata_program: &Program<'info, Metadata>,
    token_program: &Program<'info, Token>,
) -> Result<()> {
    let pool_state = pool_state_loader.load()?;
    let seeds = pool_state.seeds();
    // Mint the NFT
    token::mint_to(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            token::MintTo {
                mint: position_nft_mint.to_account_info(),
                to: position_nft_account.to_account_info(),
                authority: pool_state_loader.to_account_info(),
            },
            &[&seeds],
        ),
        1,
    )?;
    // Disable minting
    token_2022::set_authority(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            token_2022::SetAuthority {
                current_authority: pool_state_loader.to_account_info(),
                account_or_mint: position_nft_mint.to_account_info(),
            },
            &[&seeds],
        ),
        AuthorityType::MintTokens,
        None,
    )?;
    Ok(())
}

#[cfg(test)]
mod modify_position_test {
    use super::modify_position;
    use crate::error::ErrorCode;
    use crate::libraries::tick_math;
    use crate::states::oracle::block_timestamp_mock;
    use crate::states::pool_test::build_pool;
    use crate::states::protocol_position::*;
    use crate::states::tick_array_test::build_tick;

    #[test]
    fn liquidity_delta_zero_empty_liquidity_not_allowed_test() {
        let pool_state_ref = build_pool(1, 10, 1000, 10000);
        let pool_state = &mut pool_state_ref.borrow_mut();
        let tick_lower_state = &mut build_tick(1, 10, 10).take();
        let tick_upper_state = &mut build_tick(2, 10, -10).take();

        let result = modify_position(
            0,
            pool_state,
            &mut ProtocolPositionState::default(),
            tick_lower_state,
            tick_upper_state,
            block_timestamp_mock(),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ErrorCode::InvaildLiquidity.into());
    }

    #[test]
    fn init_position_in_range_test() {
        let liquidity = 10000;
        let tick_current = 1;
        let pool_state_ref = build_pool(
            tick_current,
            10,
            tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
            liquidity,
        );
        let pool_state = &mut pool_state_ref.borrow_mut();

        let tick_lower_index = 0;
        let tick_upper_index = 2;
        let tick_lower_state = &mut build_tick(tick_lower_index, 0, 0).take();
        let tick_upper_state = &mut build_tick(tick_upper_index, 0, 0).take();

        let liquidity_delta = 10000;
        let protocol_position = &mut ProtocolPositionState::default();
        let (amount_0_int, amount_1_int, flip_tick_lower, flip_tick_upper) = modify_position(
            liquidity_delta,
            pool_state,
            protocol_position,
            tick_lower_state,
            tick_upper_state,
            block_timestamp_mock(),
        )
        .unwrap();
        assert!(amount_0_int != 0);
        assert!(amount_1_int != 0);
        assert_eq!(flip_tick_lower, true);
        assert_eq!(flip_tick_upper, true);

        // check pool active liquidity
        let new_liquidity = pool_state.liquidity;
        assert_eq!(new_liquidity, liquidity + (liquidity_delta as u128));

        // check tick state
        assert!(tick_lower_state.is_initialized());
        assert!(tick_lower_state.liquidity_gross == 10000);
        assert!(tick_upper_state.liquidity_gross == 10000);

        assert!(tick_lower_state.liquidity_net == 10000);
        assert!(tick_upper_state.liquidity_net == -10000);

        assert!(tick_lower_state.fee_growth_outside_0_x64 == pool_state.fee_growth_global_0_x64);
        assert!(tick_lower_state.fee_growth_outside_1_x64 == pool_state.fee_growth_global_1_x64);
        assert!(tick_upper_state.fee_growth_outside_0_x64 == 0);
        assert!(tick_upper_state.fee_growth_outside_1_x64 == 0);

        // check protocol position
        let fee_growth_inside_0_last_x64 = pool_state.fee_growth_global_0_x64
            - tick_lower_state.fee_growth_outside_0_x64
            - tick_upper_state.fee_growth_outside_0_x64;
        let fee_growth_inside_1_last_x64 = pool_state.fee_growth_global_1_x64
            - tick_lower_state.fee_growth_outside_1_x64
            - tick_upper_state.fee_growth_outside_1_x64;
        assert!(protocol_position.fee_growth_inside_0_last_x64 == fee_growth_inside_0_last_x64);
        assert!(protocol_position.fee_growth_inside_1_last_x64 == fee_growth_inside_1_last_x64);
        assert!(protocol_position.token_fees_owed_0 == 0);
        assert!(protocol_position.token_fees_owed_1 == 0);
        assert!(protocol_position.tick_lower_index == tick_lower_index);
        assert!(protocol_position.tick_upper_index == tick_upper_index);

        // check protocol position state
    }

    #[test]
    fn init_position_in_left_of_current_tick_test() {
        let liquidity = 10000;
        let tick_current = 1;
        let pool_state_ref = build_pool(
            tick_current,
            10,
            tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
            liquidity,
        );
        let pool_state = &mut pool_state_ref.borrow_mut();

        let tick_lower_index = -1;
        let tick_upper_index = 0;
        let tick_lower_state = &mut build_tick(tick_lower_index, 0, 0).take();
        let tick_upper_state = &mut build_tick(tick_upper_index, 0, 0).take();

        let liquidity_delta = 10000;
        let protocol_position = &mut ProtocolPositionState::default();
        let (amount_0_int, amount_1_int, flip_tick_lower, flip_tick_upper) = modify_position(
            liquidity_delta,
            pool_state,
            protocol_position,
            tick_lower_state,
            tick_upper_state,
            block_timestamp_mock(),
        )
        .unwrap();
        assert!(amount_0_int == 0);
        assert!(amount_1_int != 0);
        assert_eq!(flip_tick_lower, true);
        assert_eq!(flip_tick_upper, true);

        // check pool active liquidity
        let new_liquidity = pool_state.liquidity;
        assert_eq!(new_liquidity, liquidity_delta as u128);

        // check tick state
        assert!(tick_lower_state.is_initialized());
        assert!(tick_lower_state.liquidity_gross == 10000);
        assert!(tick_upper_state.liquidity_gross == 10000);

        assert!(tick_lower_state.liquidity_net == 10000);
        assert!(tick_upper_state.liquidity_net == -10000);

        assert!(tick_lower_state.fee_growth_outside_0_x64 == pool_state.fee_growth_global_0_x64);
        assert!(tick_lower_state.fee_growth_outside_1_x64 == pool_state.fee_growth_global_1_x64);
        assert!(tick_upper_state.fee_growth_outside_0_x64 == pool_state.fee_growth_global_0_x64);
        assert!(tick_upper_state.fee_growth_outside_1_x64 == pool_state.fee_growth_global_1_x64);

        // check protocol position
        let fee_growth_inside_0_last_x64 = pool_state.fee_growth_global_0_x64
            - tick_lower_state.fee_growth_outside_0_x64
            - (pool_state.fee_growth_global_0_x64 - tick_upper_state.fee_growth_outside_0_x64);
        let fee_growth_inside_1_last_x64 = pool_state.fee_growth_global_1_x64
            - tick_lower_state.fee_growth_outside_1_x64
            - (pool_state.fee_growth_global_1_x64 - tick_upper_state.fee_growth_outside_1_x64);
        assert!(protocol_position.fee_growth_inside_0_last_x64 == fee_growth_inside_0_last_x64);
        assert!(protocol_position.fee_growth_inside_1_last_x64 == fee_growth_inside_1_last_x64);
        assert!(protocol_position.token_fees_owed_0 == 0);
        assert!(protocol_position.token_fees_owed_1 == 0);
        assert!(protocol_position.tick_lower_index == tick_lower_index);
        assert!(protocol_position.tick_upper_index == tick_upper_index);
    }

    #[test]
    fn init_position_in_right_of_current_tick_test() {
        let liquidity = 10000;
        let tick_current = 1;
        let pool_state_ref = build_pool(
            tick_current,
            10,
            tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
            liquidity,
        );
        let pool_state = &mut pool_state_ref.borrow_mut();

        let tick_lower_index = 2;
        let tick_upper_index = 3;
        let tick_lower_state = &mut build_tick(tick_lower_index, 0, 0).take();
        let tick_upper_state = &mut build_tick(tick_upper_index, 0, 0).take();

        let liquidity_delta = 10000;
        let protocol_position = &mut ProtocolPositionState::default();
        let (amount_0_int, amount_1_int, flip_tick_lower, flip_tick_upper) = modify_position(
            liquidity_delta,
            pool_state,
            protocol_position,
            tick_lower_state,
            tick_upper_state,
            block_timestamp_mock(),
        )
        .unwrap();
        assert!(amount_0_int != 0);
        assert!(amount_1_int == 0);
        assert_eq!(flip_tick_lower, true);
        assert_eq!(flip_tick_upper, true);

        // check pool active liquidity
        let new_liquidity = pool_state.liquidity;
        assert_eq!(new_liquidity, liquidity_delta as u128);

        // check tick state
        assert!(tick_lower_state.is_initialized());
        assert!(tick_lower_state.liquidity_gross == 10000);
        assert!(tick_upper_state.liquidity_gross == 10000);

        assert!(tick_lower_state.liquidity_net == 10000);
        assert!(tick_upper_state.liquidity_net == -10000);

        assert!(tick_lower_state.fee_growth_outside_0_x64 == 0);
        assert!(tick_lower_state.fee_growth_outside_1_x64 == 0);
        assert!(tick_upper_state.fee_growth_outside_0_x64 == 0);
        assert!(tick_upper_state.fee_growth_outside_1_x64 == 0);

        // check protocol position
        let fee_growth_inside_0_last_x64 = pool_state.fee_growth_global_0_x64
            - (pool_state.fee_growth_global_0_x64 - tick_lower_state.fee_growth_outside_0_x64)
            - tick_upper_state.fee_growth_outside_0_x64;
        let fee_growth_inside_1_last_x64 = pool_state.fee_growth_global_1_x64
            - (pool_state.fee_growth_global_1_x64 - tick_lower_state.fee_growth_outside_1_x64)
            - tick_upper_state.fee_growth_outside_1_x64;
        assert!(protocol_position.fee_growth_inside_0_last_x64 == fee_growth_inside_0_last_x64);
        assert!(protocol_position.fee_growth_inside_1_last_x64 == fee_growth_inside_1_last_x64);
        assert!(protocol_position.token_fees_owed_0 == 0);
        assert!(protocol_position.token_fees_owed_1 == 0);
        assert!(protocol_position.tick_lower_index == tick_lower_index);
        assert!(protocol_position.tick_upper_index == tick_upper_index);

        // check protocol position state
    }
}
