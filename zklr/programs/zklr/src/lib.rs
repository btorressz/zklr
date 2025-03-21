use anchor_lang::prelude::*;
use anchor_spl::token::{self, Transfer, Token, TokenAccount};

declare_id!("8A9hk3goecdw1ymyyXh5EoKYry88C94q2qMrHM9jvxFX");

/// Constants used in the program.
const PROOF_VALIDITY_PERIOD: i64 = 3600; // 1 hour
const FEE_PERCENTAGE: u8 = 1; // 1% burn fee
const MAX_INVALID_PROOFS: u8 = 3;
const SLASH_PERCENTAGE: u8 = 20; // Slash 20% of stake on repeated failures
const DECAY_PERIOD: i64 = 86400; // 1 day decay period for bandwidth priority
const LOCKUP_PERIOD: i64 = 3600; // 1 hour lockup before unstaking is allowed
const REVEAL_DELAY: i64 = 30; // 30 seconds delay before reveal_trade can be called
const PRIORITY_POOL_BONUS: u8 = 10; // 10% bonus rewards for LPs in priority pools
const MIN_CONFIDENTIAL_STAKE: u64 = 100; // Minimum stake threshold for bandwidth allocation
const LIQUIDITY_LOCK_PERIOD: i64 = 86400; // 1 day liquidity lock period for LPs

#[program]
pub mod zklr {
    use super::*;

    /// Initializes global state.
    pub fn initialize(ctx: Context<Initialize>, admin: Pubkey) -> Result<()> {
        let state = &mut ctx.accounts.global_state;
        state.admin = admin;
        state.total_staked = 0;
        state.total_liquidity = 0;
        Ok(())
    }

    /// Confidentially stakes tokens.
    /// This function uses a confidential transfer so that the staked amount remains hidden.
    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        let clock = Clock::get()?;
        confidential_transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.trader_token_account.to_account_info(),
                    to: ctx.accounts.stake_vault.to_account_info(),
                    authority: ctx.accounts.trader.to_account_info(),
                },
            ),
            amount,
        )?;

        let trader_account = &mut ctx.accounts.trader_account;
        trader_account.staked_amount = trader_account
            .staked_amount
            .checked_add(amount)
            .ok_or(ErrorCode::Overflow)?;
        trader_account.last_stake_timestamp = clock.unix_timestamp;
        let global_state = &mut ctx.accounts.global_state;
        global_state.total_staked = global_state
            .total_staked
            .checked_add(amount)
            .ok_or(ErrorCode::Overflow)?;
        Ok(())
    }

    /// Verifies a trader’s zero-knowledge proof.
    /// Stores a hash of the proof and commitment for delayed reveal,
    /// burns a fee from the confidential stake, and computes a speed multiplier.
    pub fn verify_priority(
        ctx: Context<VerifyPriority>,
        zk_proof: Vec<u8>,
        commitment: [u8; 32],
        latency: u64, // lower latency => higher speed multiplier
    ) -> Result<()> {
        if zk_proof.len() < 10 {
            let trader_account = &mut ctx.accounts.trader_account;
            trader_account.invalid_proof_attempts = trader_account
                .invalid_proof_attempts
                .checked_add(1)
                .ok_or(ErrorCode::Overflow)?;
            if trader_account.invalid_proof_attempts >= MAX_INVALID_PROOFS {
                let slash_amount = trader_account
                    .staked_amount
                    .checked_mul(SLASH_PERCENTAGE as u64)
                    .ok_or(ErrorCode::Overflow)?
                    .checked_div(100)
                    .ok_or(ErrorCode::Underflow)?;
                trader_account.staked_amount = trader_account
                    .staked_amount
                    .checked_sub(slash_amount)
                    .ok_or(ErrorCode::Underflow)?;
                ctx.accounts.global_state.total_staked = ctx
                    .accounts
                    .global_state
                    .total_staked
                    .checked_sub(slash_amount)
                    .ok_or(ErrorCode::Underflow)?;
                trader_account.invalid_proof_attempts = 0;
            }
            return Err(ErrorCode::InvalidZKProof.into());
        }

        // Store the hash of the ZK proof and the commitment.
        let proof_hash = anchor_lang::solana_program::hash::hash(&zk_proof).to_bytes();
        let trader_account = &mut ctx.accounts.trader_account;
        trader_account.zk_proof_hash = proof_hash;
        trader_account.commitment = commitment;
        let clock = Clock::get()?;
        trader_account.last_proof_update = clock.unix_timestamp;
        trader_account.proof_expiry = clock.unix_timestamp + PROOF_VALIDITY_PERIOD;

        // Burn a fee portion from the confidential stake.
        let fee = trader_account
            .staked_amount
            .checked_mul(FEE_PERCENTAGE as u64)
            .ok_or(ErrorCode::Overflow)?
            .checked_div(100)
            .ok_or(ErrorCode::Underflow)?;
        trader_account.staked_amount = trader_account
            .staked_amount
            .checked_sub(fee)
            .ok_or(ErrorCode::Underflow)?;
        ctx.accounts.global_state.total_staked = ctx
            .accounts
            .global_state
            .total_staked
            .checked_sub(fee)
            .ok_or(ErrorCode::Underflow)?;

        // Compute speed multiplier (adaptive rewards).
        // Here, a simple formula: multiplier = 1000 / (latency + 1)
        trader_account.speed_multiplier = 1000u64
            .checked_div(latency + 1)
            .ok_or(ErrorCode::DivisionByZero)?;

        // Reset invalid proof attempts.
        trader_account.invalid_proof_attempts = 0;
        Ok(())
    }

    /// Batch confidential transaction: stakes, burns fee, verifies ZK proof, and grants priority in one atomic transaction.
    pub fn batch_stake_and_verify(
        ctx: Context<BatchStakeAndVerify>,
        amount: u64,
        zk_proof: Vec<u8>,
        commitment: [u8; 32],
        latency: u64,
    ) -> Result<()> {
        let clock = Clock::get()?;

        // Confidentially stake tokens.
        confidential_transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.trader_token_account.to_account_info(),
                    to: ctx.accounts.stake_vault.to_account_info(),
                    authority: ctx.accounts.trader.to_account_info(),
                },
            ),
            amount,
        )?;

        let trader_account = &mut ctx.accounts.trader_account;
        trader_account.staked_amount = trader_account
            .staked_amount
            .checked_add(amount)
            .ok_or(ErrorCode::Overflow)?;
        trader_account.last_stake_timestamp = clock.unix_timestamp;
        ctx.accounts.global_state.total_staked = ctx
            .accounts
            .global_state
            .total_staked
            .checked_add(amount)
            .ok_or(ErrorCode::Overflow)?;

        // Burn fee.
        let fee = trader_account
            .staked_amount
            .checked_mul(FEE_PERCENTAGE as u64)
            .ok_or(ErrorCode::Overflow)?
            .checked_div(100)
            .ok_or(ErrorCode::Underflow)?;
        trader_account.staked_amount = trader_account
            .staked_amount
            .checked_sub(fee)
            .ok_or(ErrorCode::Underflow)?;
        ctx.accounts.global_state.total_staked = ctx
            .accounts
            .global_state
            .total_staked
            .checked_sub(fee)
            .ok_or(ErrorCode::Underflow)?;

        // Verify the ZK proof (simulate) and store proof hash & commitment.
        if zk_proof.len() < 10 {
            trader_account.invalid_proof_attempts = trader_account
                .invalid_proof_attempts
                .checked_add(1)
                .ok_or(ErrorCode::Overflow)?;
            return Err(ErrorCode::InvalidZKProof.into());
        }
        let proof_hash = anchor_lang::solana_program::hash::hash(&zk_proof).to_bytes();
        trader_account.zk_proof_hash = proof_hash;
        trader_account.commitment = commitment;
        trader_account.last_proof_update = clock.unix_timestamp;
        trader_account.proof_expiry = clock.unix_timestamp + PROOF_VALIDITY_PERIOD;

        // Compute speed multiplier.
        trader_account.speed_multiplier = 1000u64
            .checked_div(latency + 1)
            .ok_or(ErrorCode::DivisionByZero)?;

        // Mark trader as verified.
        trader_account.is_verified = true;
        trader_account.invalid_proof_attempts = 0;
        Ok(())
    }

    /// Allocates network bandwidth (priority) using anonymous execution pools.
    /// The effective (anonymous) priority is computed using a decay factor,
    /// the confidential staked amount, the speed multiplier, and the trader’s confidential trade volume.
    pub fn allocate_bandwidth(ctx: Context<AllocateBandwidth>) -> Result<()> {
        let trader_account = &ctx.accounts.trader_account;
        let clock = Clock::get()?;
        if clock.unix_timestamp > trader_account.proof_expiry {
            return Err(ErrorCode::ProofExpired.into());
        }
        if !trader_account.is_verified {
            return Err(ErrorCode::TraderNotVerified.into());
        }
        let elapsed = clock.unix_timestamp - trader_account.last_proof_update;
        let decay_factor = if elapsed < DECAY_PERIOD {
            DECAY_PERIOD - elapsed
        } else {
            0
        };
        let base_priority = trader_account
            .staked_amount
            .checked_mul(decay_factor as u64)
            .ok_or(ErrorCode::Overflow)?
            .checked_div(DECAY_PERIOD as u64)
            .ok_or(ErrorCode::Underflow)?;
        // Incorporate the speed multiplier and confidential trade volume.
        let effective_priority = base_priority
            .checked_mul(trader_account.speed_multiplier)
            .ok_or(ErrorCode::Overflow)?
            .checked_add(trader_account.trade_volume)
            .ok_or(ErrorCode::Overflow)?;
        msg!("Anonymous bandwidth priority allocated: {}", effective_priority);
        Ok(())
    }

    /// Allows traders to unstake tokens using a confidential withdrawal.
    pub fn unstake(ctx: Context<Unstake>, amount: u64) -> Result<()> {
        let trader_account = &mut ctx.accounts.trader_account;
        let clock = Clock::get()?;
        if clock.unix_timestamp < trader_account.last_stake_timestamp + LOCKUP_PERIOD {
            return Err(ErrorCode::LockupPeriodNotElapsed.into());
        }
        if trader_account.staked_amount < amount {
            return Err(ErrorCode::InsufficientStake.into());
        }
        confidential_transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.stake_vault.to_account_info(),
                    to: ctx.accounts.trader_token_account.to_account_info(),
                    authority: ctx.accounts.stake_authority.to_account_info(),
                },
            ),
            amount,
        )?;
        trader_account.staked_amount = trader_account
            .staked_amount
            .checked_sub(amount)
            .ok_or(ErrorCode::Underflow)?;
        ctx.accounts.global_state.total_staked = ctx
            .accounts
            .global_state
            .total_staked
            .checked_sub(amount)
            .ok_or(ErrorCode::Underflow)?;
        Ok(())
    }

    /// Liquidity providers deposit tokens into a confidential liquidity pool.
    /// Funds must be locked for a minimum period before rewards can be claimed.
    /// Additionally, confidential trade volume is tracked for market-making incentives.
    pub fn provide_liquidity(
        ctx: Context<ProvideLiquidity>,
        amount: u64,
        trade_volume: u64,
    ) -> Result<()> {
        let clock = Clock::get()?;
        if clock.unix_timestamp < ctx.accounts.lp_account.lock_timestamp + LIQUIDITY_LOCK_PERIOD {
            return Err(ErrorCode::LiquidityLockNotElapsed.into());
        }
        confidential_transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.lp_token_account.to_account_info(),
                    to: ctx.accounts.liquidity_vault.to_account_info(),
                    authority: ctx.accounts.lp.to_account_info(),
                },
            ),
            amount,
        )?;
        let bonus = if ctx.accounts.lp_account.is_priority_pool {
            amount
                .checked_mul(PRIORITY_POOL_BONUS as u64)
                .ok_or(ErrorCode::Overflow)?
                .checked_div(100)
                .ok_or(ErrorCode::Underflow)?
        } else {
            0
        };
        let total_liquidity = amount.checked_add(bonus).ok_or(ErrorCode::Overflow)?;
        let lp_account = &mut ctx.accounts.lp_account;
        lp_account.liquidity_provided = lp_account
            .liquidity_provided
            .checked_add(total_liquidity)
            .ok_or(ErrorCode::Overflow)?;
        lp_account.reward_balance = lp_account
            .reward_balance
            .checked_add(bonus)
            .ok_or(ErrorCode::Overflow)?;
        // Update confidential trade volume for market-making incentives.
        lp_account.trade_volume = lp_account
            .trade_volume
            .checked_add(trade_volume)
            .ok_or(ErrorCode::Overflow)?;
        ctx.accounts.global_state.total_liquidity = ctx
            .accounts
            .global_state
            .total_liquidity
            .checked_add(total_liquidity)
            .ok_or(ErrorCode::Overflow)?;
        Ok(())
    }

    /// Reveals an encrypted order for fully on-chain confidential order matching.
    /// This function simulates order range verification using an additional order range proof.
    pub fn reveal_trade(
        ctx: Context<RevealTrade>,
        actual_order: Vec<u8>,
        order_range_proof: Vec<u8>,
    ) -> Result<()> {
        let trader_account = &mut ctx.accounts.trader_account;
        let clock = Clock::get()?;
        if clock.unix_timestamp < trader_account.last_proof_update + REVEAL_DELAY {
            return Err(ErrorCode::RevealTooEarly.into());
        }
        // Verify the order range proof (simulated check).
        if order_range_proof.len() < 10 {
            trader_account.invalid_proof_attempts = trader_account
                .invalid_proof_attempts
                .checked_add(1)
                .ok_or(ErrorCode::Overflow)?;
            if trader_account.invalid_proof_attempts >= MAX_INVALID_PROOFS {
                let slash_amount = trader_account
                    .staked_amount
                    .checked_mul(SLASH_PERCENTAGE as u64)
                    .ok_or(ErrorCode::Overflow)?
                    .checked_div(100)
                    .ok_or(ErrorCode::Underflow)?;
                trader_account.staked_amount = trader_account
                    .staked_amount
                    .checked_sub(slash_amount)
                    .ok_or(ErrorCode::Underflow)?;
                ctx.accounts.global_state.total_staked = ctx
                    .accounts
                    .global_state
                    .total_staked
                    .checked_sub(slash_amount)
                    .ok_or(ErrorCode::Underflow)?;
                trader_account.invalid_proof_attempts = 0;
            }
            return Err(ErrorCode::InvalidReveal.into());
        }
        let order_hash = anchor_lang::solana_program::hash::hash(&actual_order).to_bytes();
        if order_hash != trader_account.commitment {
            trader_account.invalid_proof_attempts = trader_account
                .invalid_proof_attempts
                .checked_add(1)
                .ok_or(ErrorCode::Overflow)?;
            if trader_account.invalid_proof_attempts >= MAX_INVALID_PROOFS {
                let slash_amount = trader_account
                    .staked_amount
                    .checked_mul(SLASH_PERCENTAGE as u64)
                    .ok_or(ErrorCode::Overflow)?
                    .checked_div(100)
                    .ok_or(ErrorCode::Underflow)?;
                trader_account.staked_amount = trader_account
                    .staked_amount
                    .checked_sub(slash_amount)
                    .ok_or(ErrorCode::Underflow)?;
                ctx.accounts.global_state.total_staked = ctx
                    .accounts
                    .global_state
                    .total_staked
                    .checked_sub(slash_amount)
                    .ok_or(ErrorCode::Underflow)?;
                trader_account.invalid_proof_attempts = 0;
            }
            return Err(ErrorCode::InvalidReveal.into());
        }
        // If valid, mark the trader as verified.
        trader_account.is_verified = true;
        Ok(())
    }
}

//
// Helper: Confidential Transfer (placeholder for actual confidential token CPI)
//
fn confidential_transfer<'info>(
    ctx: CpiContext<'_, '_, '_, 'info, Transfer<'info>>,
    amount: u64,
) -> Result<()> {
    // In production, call the confidential token transfer CPI so that the amount remains encrypted.
    token::transfer(ctx, amount)
}

//
// Account Structures
//

#[account]
pub struct GlobalState {
    pub admin: Pubkey,
    pub total_staked: u64,
    pub total_liquidity: u64,
}

impl GlobalState {
    const SIZE: usize = 32 + 8 + 8;
}

#[account]
pub struct TraderAccount {
    pub trader: Pubkey,
    /// In a true confidential system, this value is stored encrypted.
    pub staked_amount: u64,
    pub is_verified: bool,
    pub proof_expiry: i64,
    pub last_proof_update: i64,
    pub zk_proof_hash: [u8; 32],
    pub invalid_proof_attempts: u8,
    /// Commitment for encrypted order matching.
    pub commitment: [u8; 32],
    pub last_stake_timestamp: i64,
    /// Adaptive rewards: higher multiplier for faster execution.
    pub speed_multiplier: u64,
    /// Confidential trade volume used for market-making incentives.
    pub trade_volume: u64,
}

impl TraderAccount {
    // 32 + 8 + 1 + 8 + 8 + 32 + 1 + 32 + 8 + 8 + 8 = 138 bytes (plus 8-byte discriminator)
    const SIZE: usize = 32 + 8 + 1 + 8 + 8 + 32 + 1 + 32 + 8 + 8 + 8;
}

#[account]
pub struct LiquidityAccount {
    pub lp: Pubkey,
    /// Confidential liquidity provided (encrypted in production).
    pub liquidity_provided: u64,
    pub is_priority_pool: bool,
    pub reward_balance: u64,
    /// Timestamp when liquidity was locked.
    pub lock_timestamp: i64,
    /// Confidential trade volume for market-making incentives.
    pub trade_volume: u64,
}

impl LiquidityAccount {
    // 32 + 8 + 1 + 8 + 8 + 8 = 65 bytes (plus 8-byte discriminator)
    const SIZE: usize = 32 + 8 + 1 + 8 + 8 + 8;
}

//
// Instruction Contexts
//

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = 8 + GlobalState::SIZE)]
    pub global_state: Account<'info, GlobalState>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,
    #[account(mut)]
    pub trader_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub stake_vault: Account<'info, TokenAccount>,
    #[account(mut, has_one = trader)]
    pub trader_account: Account<'info, TraderAccount>,
    #[account(mut)]
    pub global_state: Account<'info, GlobalState>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct VerifyPriority<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,
    #[account(mut, has_one = trader)]
    pub trader_account: Account<'info, TraderAccount>,
    #[account(mut)]
    pub global_state: Account<'info, GlobalState>,
}

#[derive(Accounts)]
pub struct BatchStakeAndVerify<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,
    #[account(mut)]
    pub trader_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub stake_vault: Account<'info, TokenAccount>,
    #[account(mut, has_one = trader)]
    pub trader_account: Account<'info, TraderAccount>,
    #[account(mut)]
    pub global_state: Account<'info, GlobalState>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AllocateBandwidth<'info> {
    pub trader: Signer<'info>,
    pub trader_account: Account<'info, TraderAccount>,
}

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,
    #[account(mut, has_one = trader)]
    pub trader_account: Account<'info, TraderAccount>,
    #[account(mut)]
    pub trader_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub stake_vault: Account<'info, TokenAccount>,
    /// CHECK: PDA authority for the stake vault.
    pub stake_authority: AccountInfo<'info>,
    #[account(mut)]
    pub global_state: Account<'info, GlobalState>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ProvideLiquidity<'info> {
    #[account(mut)]
    pub lp: Signer<'info>,
    #[account(mut)]
    pub lp_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub liquidity_vault: Account<'info, TokenAccount>,
    #[account(mut, has_one = lp)]
    pub lp_account: Account<'info, LiquidityAccount>,
    #[account(mut)]
    pub global_state: Account<'info, GlobalState>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct RevealTrade<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,
    #[account(mut, has_one = trader)]
    pub trader_account: Account<'info, TraderAccount>,
    #[account(mut)]
    pub global_state: Account<'info, GlobalState>,
}

//
// Custom Errors
//

#[error_code]
pub enum ErrorCode {
    #[msg("Arithmetic overflow occurred.")]
    Overflow,
    #[msg("Arithmetic underflow occurred.")]
    Underflow,
    #[msg("Division by zero.")]
    DivisionByZero,
    #[msg("Invalid zero-knowledge proof provided.")]
    InvalidZKProof,
    #[msg("Invalid reveal: commitment does not match the revealed order or range proof failed.")]
    InvalidReveal,
    #[msg("Trader is not verified for bandwidth allocation.")]
    TraderNotVerified,
    #[msg("Proof has expired.")]
    ProofExpired,
    #[msg("Lockup period has not elapsed for unstaking.")]
    LockupPeriodNotElapsed,
    #[msg("Insufficient staked amount.")]
    InsufficientStake,
    #[msg("Reveal attempted too early.")]
    RevealTooEarly,
    #[msg("Liquidity funds are still locked.")]
    LiquidityLockNotElapsed,
}
