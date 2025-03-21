# zklr

# ZKLR: Zero-Knowledge Liquidity and Rewards Protocol

ZKLR is a Solana-based decentralized protocol that leverages zero-knowledge proofs (ZKPs) and confidential transactions to enable secure staking, liquidity provision, and bandwidth allocation. The protocol ensures privacy and fairness while incentivizing participants through adaptive rewards and penalties. It is designed for high-frequency traders (HFTs), institutional market makers, and liquidity providers who demand confidentiality and low latency in trade execution.

## Features

- **Confidential Staking:** Stake tokens using confidential transfers to keep the staked amount private.
- **Zero-Knowledge Proof Verification:** Verify ZK-SNARK proofs to grant bandwidth priority and compute adaptive rewards without exposing sensitive data.
- **Liquidity Provision:** Deposit tokens into confidential liquidity pools with enforced lockup periods and bonus rewards for priority pools.
- **Bandwidth Allocation:** Allocate network bandwidth based on a confidential stake, speed multiplier, and trade volume, ensuring fair and efficient execution.
- **Adaptive Rewards:** Dynamically compute rewards based on execution latency, stake amount, and trade volume.
- **Penalties for Invalid Proofs:** Automatically slash a trader’s stake for repeated invalid zero-knowledge proof attempts.
- **Batch Confidential Transactions:** Aggregate staking, fee burning, and proof verification into a single atomic transaction to reduce gas costs.
- **Anonymous Execution Pools:** Enable priority matching using ZK proofs without revealing trader identities.
- **Confidential Order Matching:** Support encrypted order submissions to keep order sizes and other sensitive details private during execution.

  
## Program Instructions

### 1. `initialize`
Initializes the global state of the protocol.

- **Parameters:**
  - `admin`: The public key of the admin account.
- **Accounts:**
  - `global_state`: The global state account to be initialized.
  - `admin`: The admin account (payer).
  - `system_program`: The Solana system program.

### 2. `stake`
Confidentially stakes tokens.

- **Parameters:**
  - `amount`: The amount of tokens to stake.
- **Accounts:**
  - `trader`: The trader's signer account.
  - `trader_token_account`: The trader's token account.
  - `stake_vault`: The vault where staked tokens are stored.
  - `trader_account`: The trader's protocol account.
  - `global_state`: The global state account.
  - `token_program`: The token program.

### 3. `verify_priority`
Verifies a trader's zero-knowledge proof and grants bandwidth priority.

- **Parameters:**
  - `zk_proof`: The zero-knowledge proof data.
  - `commitment`: The commitment hash for encrypted order matching.
  - `latency`: The measured latency (lower latency results in a higher speed multiplier).
- **Accounts:**
  - `trader`: The trader's signer account.
  - `trader_account`: The trader's protocol account.
  - `global_state`: The global state account.

### 4. `batch_stake_and_verify`
Performs staking, burns a fee, verifies the ZK-SNARK proof, and grants priority in a single atomic transaction.

- **Parameters:**
  - `amount`: The amount of tokens to stake.
  - `zk_proof`: The zero-knowledge proof data.
  - `commitment`: The commitment hash for encrypted order matching.
  - `latency`: The latency value for adaptive rewards.
- **Accounts:**
  - Same as for `stake` and `verify_priority`.

### 5. `allocate_bandwidth`
Allocates network bandwidth (execution priority) based on the confidential stake, speed multiplier, and trade volume.  
This function uses only the stored proof hash along with an adaptive calculation to ensure anonymity.

- **Accounts:**
  - `trader`: The trader's signer account.
  - `trader_account`: The trader's protocol account.

### 6. `unstake`
Allows traders to withdraw staked tokens using a confidential withdrawal mechanism after a mandatory lockup period.

- **Parameters:**
  - `amount`: The amount of tokens to unstake.
- **Accounts:**
  - `trader`: The trader's signer account.
  - `trader_token_account`: The trader's token account.
  - `stake_vault`: The vault holding staked tokens.
  - `stake_authority`: The PDA authority for the stake vault.
  - `trader_account`: The trader's protocol account.
  - `global_state`: The global state account.
  - `token_program`: The token program.

### 7. `provide_liquidity`
Deposits tokens into a confidential liquidity pool. Liquidity providers must lock funds for a minimum period and receive bonus rewards if they are part of a priority pool. This function also tracks confidential trade volume for market-making incentives.

- **Parameters:**
  - `amount`: The amount of tokens to deposit.
  - `trade_volume`: Confidential trade volume for market-making incentives.
- **Accounts:**
  - `lp`: The liquidity provider's signer account.
  - `lp_token_account`: The liquidity provider's token account.
  - `liquidity_vault`: The vault where liquidity is stored.
  - `lp_account`: The liquidity provider's protocol account.
  - `global_state`: The global state account.
  - `token_program`: The token program.

 
 ### 8. `reveal_trade`
Reveals an encrypted order for confidential order matching. This function simulates order range verification using an additional order range proof.

- **Parameters:**
  - `actual_order`: The actual order data.
  - `order_range_proof`: A proof verifying that the order falls within a valid range.
- **Accounts:**
  - `trader`: The trader's signer account.
  - `trader_account`: The trader's protocol account.
  - `global_state`: The global state account.
    
    
## Account Structures

### GlobalState
Stores global protocol state.

- `admin`: The admin's public key.
- `total_staked`: Total staked tokens.
- `total_liquidity`: Total liquidity in the protocol.

### TraderAccount
Stores trader-specific data.

- `trader`: The trader's public key.
- `staked_amount`: Confidential staked amount.
- `is_verified`: Whether the trader is verified.
- `proof_expiry`: Expiry timestamp of the proof.
- `last_proof_update`: Timestamp of the last proof update.
- `zk_proof_hash`: Hash of the zero-knowledge proof.
- `invalid_proof_attempts`: Count of invalid proof attempts.
- `commitment`: Commitment hash for encrypted order matching.
- `last_stake_timestamp`: Timestamp of the last stake.
- `speed_multiplier`: Adaptive rewards multiplier (computed from latency).
- `trade_volume`: Confidential trade volume (used for market-making incentives).

### LiquidityAccount
Stores liquidity provider-specific data.

- `lp`: The liquidity provider's public key.
- `liquidity_provided`: Confidential liquidity amount.
- `is_priority_pool`: Whether the account is in a priority pool.
- `reward_balance`: Rewards balance.
- `lock_timestamp`: Timestamp when liquidity was locked.
- `trade_volume`: Confidential trade volume (used for market-making incentives).

## Error Codes

- **Overflow:** Arithmetic overflow occurred.
- **Underflow:** Arithmetic underflow occurred.
- **DivisionByZero:** Division by zero.
- **InvalidZKProof:** Invalid zero-knowledge proof provided.
- **InvalidReveal:** Invalid reveal or commitment mismatch.
- **TraderNotVerified:** Trader is not verified for bandwidth allocation.
- **ProofExpired:** The provided proof has expired.
- **LockupPeriodNotElapsed:** The required lockup period has not elapsed for unstaking.
- **InsufficientStake:** The staked amount is insufficient.
- **RevealTooEarly:** The trade reveal was attempted too early.
- **LiquidityLockNotElapsed:** Liquidity funds are still locked.

## Constants

- **PROOF_VALIDITY_PERIOD:** 3600 seconds (1 hour).
- **FEE_PERCENTAGE:** 1% burn fee.
- **MAX_INVALID_PROOFS:** 3 invalid proof attempts before slashing.
- **SLASH_PERCENTAGE:** 20% of stake is slashed on repeated failures.
- **DECAY_PERIOD:** 86400 seconds (1 day) for bandwidth priority decay.
- **LOCKUP_PERIOD:** 3600 seconds (1 hour) lockup before unstaking.
- **REVEAL_DELAY:** 30 seconds delay before trade reveal.
- **PRIORITY_POOL_BONUS:** 10% bonus rewards for priority pools.
- **MIN_CONFIDENTIAL_STAKE:** Minimum stake threshold for bandwidth allocation.
- **LIQUIDITY_LOCK_PERIOD:** 86400 seconds (1 day) liquidity lock period.
