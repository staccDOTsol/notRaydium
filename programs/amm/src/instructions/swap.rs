use crate::error::ErrorCode;
use crate::libraries::{
    big_num::U128, fixed_point_64, full_math::MulDiv, liquidity_math, swap_math, tick_math,
};
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::TokenAccount;
use std::cell::RefMut;
use std::collections::VecDeque;
#[cfg(feature = "enable-log")]
use std::convert::identity;
use std::ops::{Deref, Neg};

#[derive(Accounts)]
pub struct SwapSingle<'info> {
    /// The user performing the swap
    pub payer: Signer<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: AccountLoader<'info, AmmConfig>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The user token account for input token
    #[account(
        mut,
        token::token_program = token_program,
    )]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The user token account for output token
    #[account(
        mut,
        token::token_program = token_program,
    )]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for input token
    #[account(
        mut,
        token::token_program = token_program,
    )]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for output token
    #[account(
        mut,
        token::token_program = token_program,
    )]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, ObservationState>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    #[account(mut, constraint = tick_array.load()?.pool_id == pool_state.key())]
    pub tick_array: AccountLoader<'info, TickArrayState>,
}

pub struct SwapAccounts<'b, 'info> {
    /// The user performing the swap
    pub signer: Signer<'info>,

    /// The user token account for input token
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The user token account for output token
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for input token
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for output token
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    /// The factory state to read protocol fees
    pub amm_config: &'b AccountLoader<'info, AmmConfig>,

    /// The program account of the pool in which the swap will be performed
    pub pool_state: &'b mut AccountLoader<'info, PoolState>,

    /// The tick_array account of current or next initialized
    pub tick_array_state: &'b mut AccountLoader<'info, TickArrayState>,

    /// The program account for the oracle observation
    pub observation_state: &'b mut AccountLoader<'info, ObservationState>,
}

// the top level state of the swap, the results of which are recorded in storage at the end
#[derive(Debug)]
pub struct SwapState {
    // the amount remaining to be swapped in/out of the input/output asset
    pub amount_specified_remaining: u64,
    // the amount already swapped out/in of the output/input asset
    pub amount_calculated: u64,
    // current sqrt(price)
    pub sqrt_price_x64: u128,
    // the tick associated with the current price
    pub tick: i32,
    // the global fee growth of the input token
    pub fee_growth_global_x64: u128,
    // the global fee of the input token
    pub fee_amount: u64,
    // amount of input token paid as protocol fee
    pub protocol_fee: u64,
    // amount of input token paid as fund fee
    pub fund_fee: u64,
    // the current liquidity in range
    pub liquidity: u128,
}

#[derive(Default)]
struct StepComputations {
    // the price at the beginning of the step
    sqrt_price_start_x64: u128,
    // the next tick to swap to from the current tick in the swap direction
    tick_next: i32,
    // whether tick_next is initialized or not
    initialized: bool,
    // sqrt(price) for the next tick (1/0)
    sqrt_price_next_x64: u128,
    // how much is being swapped in in this step
    amount_in: u64,
    // how much is being swapped out
    amount_out: u64,
    // how much fee is being paid in
    fee_amount: u64,
}

pub fn swap_internal<'b, 'info>(
    amm_config: &RefMut<'_, AmmConfig>,
    pool_state: &mut RefMut<PoolState>,
    tick_array_states: &mut VecDeque<RefMut<TickArrayState>>,
    observation_state: &mut RefMut<ObservationState>,
    tickarray_bitmap_extension: &Option<TickArrayBitmapExtension>,
    amount_specified: u64,
    sqrt_price_limit_x64: u128,
    zero_for_one: bool,
    is_base_input: bool,
    block_timestamp: u32,
) -> Result<(u64, u64)> {
    require!(amount_specified != 0, ErrorCode::InvaildSwapAmountSpecified);
    if !pool_state.get_status_by_bit(PoolStatusBitIndex::Swap) {
        return err!(ErrorCode::NotApproved);
    }
    require!(
        if zero_for_one {
            sqrt_price_limit_x64 < pool_state.sqrt_price_x64
                && sqrt_price_limit_x64 > tick_math::MIN_SQRT_PRICE_X64
        } else {
            sqrt_price_limit_x64 > pool_state.sqrt_price_x64
                && sqrt_price_limit_x64 < tick_math::MAX_SQRT_PRICE_X64
        },
        ErrorCode::SqrtPriceLimitOverflow
    );

    let liquidity_start = pool_state.liquidity;

    let updated_reward_infos = pool_state.update_reward_infos(block_timestamp as u64)?;

    let mut state = SwapState {
        amount_specified_remaining: amount_specified,
        amount_calculated: 0,
        sqrt_price_x64: pool_state.sqrt_price_x64,
        tick: pool_state.tick_current,
        fee_growth_global_x64: if zero_for_one {
            pool_state.fee_growth_global_0_x64
        } else {
            pool_state.fee_growth_global_1_x64
        },
        fee_amount: 0,
        protocol_fee: 0,
        fund_fee: 0,
        liquidity: liquidity_start,
    };

    // check observation account is owned by the pool
    require_keys_eq!(observation_state.pool_id, pool_state.key());

    let (mut is_match_pool_current_tick_array, first_vaild_tick_array_start_index) =
        pool_state.get_first_initialized_tick_array(&tickarray_bitmap_extension, zero_for_one)?;
    let mut current_vaild_tick_array_start_index = first_vaild_tick_array_start_index;

    let mut tick_array_current = tick_array_states.pop_front().unwrap();
    // find the first active tick array account
    for _ in 0..tick_array_states.len() {
        if tick_array_current.start_tick_index == current_vaild_tick_array_start_index {
            break;
        }
        tick_array_current = tick_array_states
            .pop_front()
            .ok_or(ErrorCode::NotEnoughTickArrayAccount)?;
    }
    // check the first tick_array account is owned by the pool
    require_keys_eq!(tick_array_current.pool_id, pool_state.key());
    // check first tick array account is correct
    require_eq!(
        tick_array_current.start_tick_index,
        current_vaild_tick_array_start_index,
        ErrorCode::InvalidFirstTickArrayAccount
    );

    // continue swapping as long as we haven't used the entire input/output and haven't
    // reached the price limit
    while state.amount_specified_remaining != 0 && state.sqrt_price_x64 != sqrt_price_limit_x64 {
       /*#[cfg(feature = "enable-log")]
        msg!(
            "while begin, is_base_input:{},fee_growth_global_x32:{}, state_sqrt_price_x64:{}, state_tick:{},state_liquidity:{},state.protocol_fee:{}, protocol_fee_rate:{}",
            is_base_input,
            state.fee_growth_global_x64,
            state.sqrt_price_x64,
            state.tick,
            state.liquidity,
            state.protocol_fee,
            amm_config.protocol_fee_rate
        ); */ 
        // Save these three pieces of information for PriceChangeEvent
        // let tick_before = state.tick;
        // let sqrt_price_x64_before = state.sqrt_price_x64;
        // let liquidity_before = state.liquidity;

        let mut step = StepComputations::default();
        step.sqrt_price_start_x64 = state.sqrt_price_x64;

        let mut next_initialized_tick = if let Some(tick_state) = tick_array_current
            .next_initialized_tick(state.tick, pool_state.tick_spacing, zero_for_one)?
        {
            Box::new(*tick_state)
        } else {
            if !is_match_pool_current_tick_array {
                is_match_pool_current_tick_array = true;
                Box::new(*tick_array_current.first_initialized_tick(zero_for_one)?)
            } else {
                Box::new(TickState::default())
            }
        };
        #[cfg(feature = "enable-log")]
        msg!(
            "next_initialized_tick, status:{}, tick_index:{}, tick_array_current:{}",
            next_initialized_tick.is_initialized(),
            identity(next_initialized_tick.tick),
            tick_array_current.key().to_string(),
        );
        if !next_initialized_tick.is_initialized() {
            let next_initialized_tickarray_index = pool_state
                .next_initialized_tick_array_start_index(
                    &tickarray_bitmap_extension,
                    current_vaild_tick_array_start_index,
                    zero_for_one,
                )?;
            if next_initialized_tickarray_index.is_none() {
                return err!(ErrorCode::LiquidityInsufficient);
            }

            while tick_array_current.start_tick_index != next_initialized_tickarray_index.unwrap() {
                tick_array_current = tick_array_states
                    .pop_front()
                    .ok_or(ErrorCode::NotEnoughTickArrayAccount)?;
                // check the tick_array account is owned by the pool
                require_keys_eq!(tick_array_current.pool_id, pool_state.key());
            }
            current_vaild_tick_array_start_index = next_initialized_tickarray_index.unwrap();

            let first_initialized_tick = tick_array_current.first_initialized_tick(zero_for_one)?;
            next_initialized_tick = Box::new(*first_initialized_tick);
        }
        step.tick_next = next_initialized_tick.tick;
        step.initialized = next_initialized_tick.is_initialized();

        if step.tick_next < tick_math::MIN_TICK {
            step.tick_next = tick_math::MIN_TICK;
        } else if step.tick_next > tick_math::MAX_TICK {
            step.tick_next = tick_math::MAX_TICK;
        }
        step.sqrt_price_next_x64 = tick_math::get_sqrt_price_at_tick(step.tick_next)?;

        let target_price = if (zero_for_one && step.sqrt_price_next_x64 < sqrt_price_limit_x64)
            || (!zero_for_one && step.sqrt_price_next_x64 > sqrt_price_limit_x64)
        {
            sqrt_price_limit_x64
        } else {
            step.sqrt_price_next_x64
        };

        if zero_for_one {
            require_gte!(state.tick, step.tick_next);
            require_gte!(step.sqrt_price_start_x64, step.sqrt_price_next_x64);
            require_gte!(step.sqrt_price_start_x64, target_price);
        } else {
            require_gt!(step.tick_next, state.tick);
            require_gte!(step.sqrt_price_next_x64, step.sqrt_price_start_x64);
            require_gte!(target_price, step.sqrt_price_start_x64);
        }
        #[cfg(feature = "enable-log")]
        msg!(
            "sqrt_price_current_x64:{}, sqrt_price_target:{}, liquidity:{}, amount_remaining:{}",
            step.sqrt_price_start_x64,
            target_price,
            state.liquidity,
            state.amount_specified_remaining
        );
        let swap_step = swap_math::compute_swap_step(
            step.sqrt_price_start_x64,
            target_price,
            state.liquidity,
            state.amount_specified_remaining,
            amm_config.trade_fee_rate,
            is_base_input,
            zero_for_one,
        );
        #[cfg(feature = "enable-log")]
        msg!("{:#?}", swap_step);
        if zero_for_one {
            require_gte!(swap_step.sqrt_price_next_x64, target_price);
        } else {
            require_gte!(target_price, swap_step.sqrt_price_next_x64);
        }
        state.sqrt_price_x64 = swap_step.sqrt_price_next_x64;
        step.amount_in = swap_step.amount_in;
        step.amount_out = swap_step.amount_out;
        step.fee_amount = swap_step.fee_amount;

        if is_base_input {
            state.amount_specified_remaining = state
                .amount_specified_remaining
                .checked_sub(step.amount_in + step.fee_amount)
                .unwrap();
            state.amount_calculated = state
                .amount_calculated
                .checked_add(step.amount_out)
                .unwrap();
        } else {
            state.amount_specified_remaining = state
                .amount_specified_remaining
                .checked_sub(step.amount_out)
                .unwrap();
            state.amount_calculated = state
                .amount_calculated
                .checked_add(step.amount_in + step.fee_amount)
                .unwrap();
        }

        let step_fee_amount = step.fee_amount;
        // if the protocol fee is on, calculate how much is owed, decrement fee_amount, and increment protocol_fee
        if amm_config.protocol_fee_rate > 0 {
            let delta = U128::from(step_fee_amount)
                .checked_mul(amm_config.protocol_fee_rate.into())
                .unwrap()
                .checked_div(FEE_RATE_DENOMINATOR_VALUE.into())
                .unwrap()
                .as_u64();
            step.fee_amount = step.fee_amount.checked_sub(delta).unwrap();
            state.protocol_fee = state.protocol_fee.checked_add(delta).unwrap();
        }
        // if the fund fee is on, calculate how much is owed, decrement fee_amount, and increment fund_fee
        if amm_config.fund_fee_rate > 0 {
            let delta = U128::from(step_fee_amount)
                .checked_mul(amm_config.fund_fee_rate.into())
                .unwrap()
                .checked_div(FEE_RATE_DENOMINATOR_VALUE.into())
                .unwrap()
                .as_u64();
            step.fee_amount = step.fee_amount.checked_sub(delta).unwrap();
            state.fund_fee = state.fund_fee.checked_add(delta).unwrap();
        }

        // update global fee tracker
        if state.liquidity > 0 {
            let fee_growth_global_x64_delta = U128::from(step.fee_amount)
                .mul_div_floor(U128::from(fixed_point_64::Q64), U128::from(state.liquidity))
                .unwrap()
                .as_u128();

            state.fee_growth_global_x64 = state
                .fee_growth_global_x64
                .checked_add(fee_growth_global_x64_delta)
                .unwrap();
            state.fee_amount = state.fee_amount.checked_add(step.fee_amount).unwrap();
            #[cfg(feature = "enable-log")]
            msg!(
                "fee_growth_global_x64_delta:{}, state.fee_growth_global_x64:{}, state.liquidity:{}, step.fee_amount:{}, state.fee_amount:{}",
                fee_growth_global_x64_delta,
                state.fee_growth_global_x64, state.liquidity, step.fee_amount, state.fee_amount
            );
        }
        // shift tick if we reached the next price
        if state.sqrt_price_x64 == step.sqrt_price_next_x64 {
            // if the tick is initialized, run the tick transition
            if step.initialized {
                #[cfg(feature = "enable-log")]
                msg!("loading next tick {}", step.tick_next);

                let mut liquidity_net = next_initialized_tick.cross(
                    if zero_for_one {
                        state.fee_growth_global_x64
                    } else {
                        pool_state.fee_growth_global_0_x64
                    },
                    if zero_for_one {
                        pool_state.fee_growth_global_1_x64
                    } else {
                        state.fee_growth_global_x64
                    },
                    &updated_reward_infos,
                );
                // update tick_state to tick_array account
                tick_array_current.update_tick_state(
                    next_initialized_tick.tick,
                    pool_state.tick_spacing.into(),
                    *next_initialized_tick,
                )?;

                if zero_for_one {
                    liquidity_net = liquidity_net.neg();
                }
                state.liquidity = liquidity_math::add_delta(state.liquidity, liquidity_net)?;
            }

            state.tick = if zero_for_one {
                step.tick_next - 1
            } else {
                step.tick_next
            };
        } else if state.sqrt_price_x64 != step.sqrt_price_start_x64 {
            // recompute unless we're on a lower tick boundary (i.e. already transitioned ticks), and haven't moved
            // if only a small amount of quantity is traded, the input may be consumed by fees, resulting in no price change. If state.sqrt_price_x64, i.e., the latest price in the pool, is used to recalculate the tick, some errors may occur.
            // for example, if zero_for_one, and the price falls exactly on an initialized tick t after the first trade, then at this point, pool.sqrtPriceX64 = get_sqrt_price_at_tick(t), while pool.tick = t-1. if the input quantity of the
            // second trade is very small and the pool price does not change after the transaction, if the tick is recalculated, pool.tick will be equal to t, which is incorrect.
            state.tick = tick_math::get_tick_at_sqrt_price(state.sqrt_price_x64)?;
        }

       /*#[cfg(feature = "enable-log")]
        msg!(
            "end, is_base_input:{},step_amount_in:{}, step_amount_out:{}, step_fee_amount:{},fee_growth_global_x32:{}, state_sqrt_price_x64:{}, state_tick:{}, state_liquidity:{},state.protocol_fee:{}, protocol_fee_rate:{}, state.fund_fee:{}, fund_fee_rate:{}",
            is_base_input,
            step.amount_in,
            step.amount_out,
            step.fee_amount,
            state.fee_growth_global_x64,
            state.sqrt_price_x64,
            state.tick,
            state.liquidity,
            state.protocol_fee,
            amm_config.protocol_fee_rate,
            state.fund_fee,
            amm_config.fund_fee_rate,
        ); */ 
        // emit!(PriceChangeEvent {
        //     pool_state: pool_state.key(),
        //     tick_before,
        //     tick_after: state.tick,
        //     sqrt_price_x64_before,
        //     sqrt_price_x64_after: state.sqrt_price_x64,
        //     liquidity_before,
        //     liquidity_after: state.liquidity,
        //     zero_for_one,
        // });
    }
    // update tick
    if state.tick != pool_state.tick_current {
        pool_state.tick_current = state.tick;
    }
    // update the previous price to the observation
    let next_observation_index = observation_state
        .update_check(
            block_timestamp,
            pool_state.sqrt_price_x64,
            pool_state.observation_index,
            pool_state.observation_update_duration.into(),
        )
        .unwrap();
    match next_observation_index {
        Option::Some(index) => pool_state.observation_index = index,
        Option::None => {}
    }
    pool_state.sqrt_price_x64 = state.sqrt_price_x64;

    if liquidity_start != state.liquidity {
        pool_state.liquidity = state.liquidity;
    }

    let (amount_0, amount_1) = if zero_for_one == is_base_input {
        (
            amount_specified
                .checked_sub(state.amount_specified_remaining)
                .unwrap(),
            state.amount_calculated,
        )
    } else {
        (
            state.amount_calculated,
            amount_specified
                .checked_sub(state.amount_specified_remaining)
                .unwrap(),
        )
    };

    if zero_for_one {
        pool_state.fee_growth_global_0_x64 = state.fee_growth_global_x64;
        pool_state.total_fees_token_0 = pool_state
            .total_fees_token_0
            .checked_add(state.fee_amount)
            .unwrap();

        if state.protocol_fee > 0 {
            pool_state.protocol_fees_token_0 = pool_state
                .protocol_fees_token_0
                .checked_add(state.protocol_fee)
                .unwrap();
        }
        if state.fund_fee > 0 {
            pool_state.fund_fees_token_0 = pool_state
                .fund_fees_token_0
                .checked_add(state.fund_fee)
                .unwrap();
        }
        pool_state.swap_in_amount_token_0 = pool_state
            .swap_in_amount_token_0
            .checked_add(u128::from(amount_0))
            .unwrap();
        pool_state.swap_out_amount_token_1 = pool_state
            .swap_out_amount_token_1
            .checked_add(u128::from(amount_1))
            .unwrap();
    } else {
        pool_state.fee_growth_global_1_x64 = state.fee_growth_global_x64;
        pool_state.total_fees_token_1 = pool_state
            .total_fees_token_1
            .checked_add(state.fee_amount)
            .unwrap();

        if state.protocol_fee > 0 {
            pool_state.protocol_fees_token_1 = pool_state
                .protocol_fees_token_1
                .checked_add(state.protocol_fee)
                .unwrap();
        }
        if state.fund_fee > 0 {
            pool_state.fund_fees_token_1 = pool_state
                .fund_fees_token_1
                .checked_add(state.fund_fee)
                .unwrap();
        }
        pool_state.swap_in_amount_token_1 = pool_state
            .swap_in_amount_token_1
            .checked_add(u128::from(amount_1))
            .unwrap();
        pool_state.swap_out_amount_token_0 = pool_state
            .swap_out_amount_token_0
            .checked_add(u128::from(amount_0))
            .unwrap();
    }

    Ok((amount_0, amount_1))
}

/// Performs a single exact input/output swap
/// if is_base_input = true, return vaule is the max_amount_out, otherwise is min_amount_in
pub fn exact_internal<'b, 'c: 'info, 'info>(
    ctx: &mut SwapAccounts<'b, 'info>,
    remaining_accounts: &'c [AccountInfo<'info>],
    amount_specified: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<u64> {
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;
    let amm_config = &mut ctx.amm_config.load_mut()?;
    let amount_0;
    let amount_1;
    let zero_for_one;
    let swap_price_before;

    let input_balance_before = ctx.input_vault.amount;
    let output_balance_before = ctx.output_vault.amount;

    {
        swap_price_before = ctx.pool_state.load()?.sqrt_price_x64;
        let pool_state = &mut ctx.pool_state.load_mut()?;
        zero_for_one = ctx.input_vault.mint == pool_state.token_mint_0;

        require_gt!(block_timestamp, pool_state.open_time);

        require!(
            if zero_for_one {
                ctx.input_vault.key() == pool_state.token_vault_0
                    && ctx.output_vault.key() == pool_state.token_vault_1
            } else {
                ctx.input_vault.key() == pool_state.token_vault_1
                    && ctx.output_vault.key() == pool_state.token_vault_0
            },
            ErrorCode::InvalidInputPoolVault
        );

        let mut tickarray_bitmap_extension = None;
        let tick_array_states = &mut VecDeque::new();
        tick_array_states.push_back(ctx.tick_array_state.load_mut()?);

        let tick_array_bitmap_extension_key = TickArrayBitmapExtension::key(pool_state.key());
        for account_info in remaining_accounts.into_iter() {
            if account_info.key().eq(&tick_array_bitmap_extension_key) {
                tickarray_bitmap_extension = Some(
                    *(AccountLoader::<TickArrayBitmapExtension>::try_from(account_info)?
                        .load()?
                        .deref()),
                );
                continue;
            }
            tick_array_states.push_back(AccountLoad::load_data_mut(account_info)?);
        }

        (amount_0, amount_1) = swap_internal(
            &amm_config,
            pool_state,
            tick_array_states,
            &mut ctx.observation_state.load_mut()?,
            &tickarray_bitmap_extension,
            amount_specified,
            if sqrt_price_limit_x64 == 0 {
                if zero_for_one {
                    tick_math::MIN_SQRT_PRICE_X64 + 1
                } else {
                    tick_math::MAX_SQRT_PRICE_X64 - 1
                }
            } else {
                sqrt_price_limit_x64
            },
            zero_for_one,
            is_base_input,
            oracle::block_timestamp(),
        )?;

        #[cfg(feature = "enable-log")]
        msg!(
            "exact_swap_internal, is_base_input:{}, amount_0: {}, amount_1: {}",
            is_base_input,
            amount_0,
            amount_1
        );
        require!(
            amount_0 != 0 && amount_1 != 0,
            ErrorCode::TooSmallInputOrOutputAmount
        );
    }
    let (token_account_0, token_account_1, vault_0, vault_1) = if zero_for_one {
        (
            ctx.input_token_account.clone(),
            ctx.output_token_account.clone(),
            ctx.input_vault.clone(),
            ctx.output_vault.clone(),
        )
    } else {
        (
            ctx.output_token_account.clone(),
            ctx.input_token_account.clone(),
            ctx.output_vault.clone(),
            ctx.input_vault.clone(),
        )
    };

    if zero_for_one {
        //  x -> y, deposit x token from user to pool vault.
        transfer_from_user_to_pool_vault(
            &ctx.signer,
            &token_account_0,
            &vault_0,
            None,
            &ctx.token_program,
            None,
            amount_0,
        )?;
        if vault_1.amount <= amount_1 {
            // freeze pool, disable all instructions
            ctx.pool_state.load_mut()?.set_status(255);
        }
        // x -> y，transfer y token from pool vault to user.
        transfer_from_pool_vault_to_user(
            &ctx.pool_state,
            &vault_1,
            &token_account_1,
            None,
            &ctx.token_program,
            None,
            amount_1,
        )?;
    } else {
        transfer_from_user_to_pool_vault(
            &ctx.signer,
            &token_account_1,
            &vault_1,
            None,
            &ctx.token_program,
            None,
            amount_1,
        )?;
        if vault_0.amount <= amount_0 {
            // freeze pool, disable all instructions
            ctx.pool_state.load_mut()?.set_status(255);
        }
        transfer_from_pool_vault_to_user(
            &ctx.pool_state,
            &vault_0,
            &token_account_0,
            None,
            &ctx.token_program,
            None,
            amount_0,
        )?;
    }
    ctx.output_vault.reload()?;
    ctx.input_vault.reload()?;

    let pool_state = ctx.pool_state.load()?;
    emit!(SwapEvent {
        pool_state: pool_state.key(),
        sender: ctx.signer.key(),
        token_account_0: token_account_0.key(),
        token_account_1: token_account_1.key(),
        amount_0,
        transfer_fee_0: 0,
        amount_1,
        transfer_fee_1: 0,
        zero_for_one,
        sqrt_price_x64: pool_state.sqrt_price_x64,
        liquidity: pool_state.liquidity,
        tick: pool_state.tick_current
    });
    if zero_for_one {
        require_gt!(swap_price_before, pool_state.sqrt_price_x64);
    } else {
        require_gt!(pool_state.sqrt_price_x64, swap_price_before);
    }

    if is_base_input {
        Ok(output_balance_before
            .checked_sub(ctx.output_vault.amount)
            .unwrap())
    } else {
        Ok(ctx
            .input_vault
            .amount
            .checked_sub(input_balance_before)
            .unwrap())
    }
}

pub fn swap<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SwapSingle<'info>>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<()> {
    let amount = exact_internal(
        &mut SwapAccounts {
            signer: ctx.accounts.payer.clone(),
            amm_config: &ctx.accounts.amm_config,
            input_token_account: ctx.accounts.input_token_account.clone(),
            output_token_account: ctx.accounts.output_token_account.clone(),
            input_vault: ctx.accounts.input_vault.clone(),
            output_vault: ctx.accounts.output_vault.clone(),
            token_program: ctx.accounts.token_program.clone(),
            pool_state: &mut ctx.accounts.pool_state,
            tick_array_state: &mut ctx.accounts.tick_array,
            observation_state: &mut ctx.accounts.observation_state,
        },
        ctx.remaining_accounts,
        amount,
        sqrt_price_limit_x64,
        is_base_input,
    )?;
    if is_base_input {
        require!(
            amount >= other_amount_threshold,
            ErrorCode::TooLittleOutputReceived
        );
    } else {
        require!(
            amount <= other_amount_threshold,
            ErrorCode::TooMuchInputPaid
        );
    }

    Ok(())
}
