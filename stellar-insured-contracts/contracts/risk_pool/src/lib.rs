#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};
use stellar_insured_lib::PoolStats;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Token,
    MinStake,
    TotalCapital,
    AvailableCapital,
    ClaimsPaid,
    ProviderCount,
    ProviderStake(Address),
}

// --- Storage helpers (#378: data access abstraction) ---

fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

fn get_token(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Token).unwrap()
}

fn get_total_capital(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::TotalCapital).unwrap_or(0)
}

fn get_available_capital(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::AvailableCapital).unwrap_or(0)
}

fn get_provider_stake(env: &Env, provider: &Address) -> i128 {
    env.storage().persistent().get(&DataKey::ProviderStake(provider.clone())).unwrap_or(0)
}

fn get_provider_count(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::ProviderCount).unwrap_or(0)
}

// --------------------------------------------------------

#[contract]
pub struct RiskPoolContract;

#[contractimpl]
impl RiskPoolContract {
    pub fn initialize(env: Env, admin: Address, token: Address, min_stake: i128) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        // #52: a negative minimum would disable the deposit threshold entirely.
        validate_min_stake(min_stake);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::MinStake, &min_stake);
        env.storage().instance().set(&DataKey::TotalCapital, &0i128);
        env.storage().instance().set(&DataKey::AvailableCapital, &0i128);
        env.storage().instance().set(&DataKey::ClaimsPaid, &0i128);
        env.storage().instance().set(&DataKey::ProviderCount, &0u32);
    }

    pub fn deposit_liquidity(env: Env, provider: Address, amount: i128) {
        provider.require_auth();

        // #52: checked before the min-stake comparison, which cannot catch a
        // negative amount when min_stake is negative or zero.
        validate_liquidity_amount(amount);
        
        let min_stake: i128 = env.storage().instance().get(&DataKey::MinStake)
            .expect("Contract not initialized");

        if amount < min_stake {
            panic!("Amount below minimum stake");
        }

        let token: Address = env.storage().instance().get(&DataKey::Token)
            .unwrap_or_else(|| panic!("Contract not initialized"));
        
        // Transfer tokens from provider to this contract
        let client = soroban_sdk::token::Client::new(&env, &token);
        client.transfer(&provider, &env.current_contract_address(), &amount);

        let current_stake = get_provider_stake(&env, &provider);
        let new_stake = current_stake + amount;
        env.storage().persistent().set(&DataKey::ProviderStake(provider.clone()), &new_stake);

        // If this is a new provider, increment provider count
        if current_stake == 0 {
            let current_provider_count = get_provider_count(&env);
            env.storage().instance().set(&DataKey::ProviderCount, &(current_provider_count + 1));
        }

        let new_total = get_total_capital(&env) + amount;
        let new_available = get_available_capital(&env) + amount;
        env.storage().instance().set(&DataKey::TotalCapital, &new_total);
        env.storage().instance().set(&DataKey::AvailableCapital, &new_available);

        // #412: Enhanced event emission with provider info
        env.events().publish(
            (symbol_short!("pool"), symbol_short!("deposit")),
            (provider, amount, new_stake),
        );
    }

    pub fn withdraw_liquidity(env: Env, provider: Address, amount: i128) {
        provider.require_auth();

        // #52: a negative withdrawal would credit the caller's stake while
        // moving no funds.
        validate_liquidity_amount(amount);

        let stake = get_provider_stake(&env, &provider);
        if stake < amount {
            panic!("Insufficient stake");
        }

        let avail = get_available_capital(&env);
        if avail < amount {
            panic!("Insufficient available capital in pool");
        }

        let token: Address = env.storage().instance().get(&DataKey::Token)
            .unwrap_or_else(|| panic!("Contract not initialized"));
        let client = soroban_sdk::token::Client::new(&env, &token);
        client.transfer(&env.current_contract_address(), &provider, &amount);

        let new_stake = stake - amount;
        env.storage().persistent().set(&DataKey::ProviderStake(provider.clone()), &new_stake);

        // If provider withdrew all their stake, decrement provider count
        if new_stake == 0 {
            let current_provider_count = get_provider_count(&env);
            env.storage().instance().set(&DataKey::ProviderCount, &(current_provider_count - 1));
        }
        
        let new_total = get_total_capital(&env) - amount;
        let new_available = avail - amount;
        env.storage().instance().set(&DataKey::TotalCapital, &new_total);
        env.storage().instance().set(&DataKey::AvailableCapital, &new_available);

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("pool"), symbol_short!("withdraw")),
            (provider, amount, new_stake),
        );
    }

    pub fn payout_claim(env: Env, recipient: Address, amount: i128) {
        let admin = get_admin(&env);
        admin.require_auth();

        // #410: Verify available capital before payout
        let avail = get_available_capital(&env);
        if avail < amount {
            panic!("Insufficient pool funds for payout");
        }

        let token: Address = env.storage().instance().get(&DataKey::Token)
            .unwrap_or_else(|| panic!("Contract not initialized"));
        let client = soroban_sdk::token::Client::new(&env, &token);
        client.transfer(&env.current_contract_address(), &recipient, &amount);

        let new_available = avail - amount;
        env.storage().instance().set(&DataKey::AvailableCapital, &new_available);

        let paid = env.storage().instance().get(&DataKey::ClaimsPaid).unwrap_or(0);
        env.storage().instance().set(&DataKey::ClaimsPaid, &(paid + amount));

        // #412: Enhanced event emission with recipient info
        env.events().publish(
            (symbol_short!("pool"), symbol_short!("payout")),
            (recipient, amount, new_available),
        );
    }
}

#[contractimpl]
impl RiskPoolContract {
    pub fn get_pool_stats(env: Env) -> PoolStats {
        PoolStats {
            total_capital: get_total_capital(&env),
            available_capital: get_available_capital(&env),
            total_claims_paid: env.storage().instance().get(&DataKey::ClaimsPaid).unwrap_or(0),
            provider_count: get_provider_count(&env),
        }
    }

    pub fn get_provider_info(env: Env, provider: Address) -> i128 {
        get_provider_stake(&env, &provider)
    }
}

// ─── Input validation (#52) ───────────────────────────────────────────────────

/// Validate the minimum stake set at initialisation.
///
/// A negative minimum lets every deposit through: `amount < min_stake` is false
/// for any non-negative amount once `min_stake` is negative, so the threshold
/// stops being a threshold. Zero is permitted — a pool with no minimum is a
/// legitimate configuration.
pub fn validate_min_stake(min_stake: i128) {
    if min_stake < 0 {
        panic!("Minimum stake cannot be negative");
    }
}

/// Validate a liquidity amount.
///
/// Deposits and withdrawals must be strictly positive. The existing
/// `amount < min_stake` guard does not catch a negative deposit when
/// `min_stake` is itself negative or zero, and a negative withdrawal would
/// increase the caller's stake while transferring nothing.
pub fn validate_liquidity_amount(amount: i128) {
    if amount <= 0 {
        panic!("Amount must be positive");
    }
}

#[cfg(test)]
mod validation_tests {
    use super::{validate_liquidity_amount, validate_min_stake};

    #[test]
    fn accepts_a_positive_minimum_stake() {
        validate_min_stake(1_000);
    }

    #[test]
    fn accepts_a_zero_minimum_stake() {
        // A pool with no minimum is a valid choice.
        validate_min_stake(0);
    }

    #[test]
    #[should_panic(expected = "Minimum stake cannot be negative")]
    fn rejects_a_negative_minimum_stake() {
        validate_min_stake(-1);
    }

    #[test]
    fn accepts_a_positive_amount() {
        validate_liquidity_amount(500);
    }

    #[test]
    #[should_panic(expected = "Amount must be positive")]
    fn rejects_a_zero_amount() {
        validate_liquidity_amount(0);
    }

    #[test]
    #[should_panic(expected = "Amount must be positive")]
    fn rejects_a_negative_amount() {
        validate_liquidity_amount(-1);
    }

    #[test]
    #[should_panic(expected = "Amount must be positive")]
    fn rejects_a_negative_amount_that_the_min_stake_check_would_admit() {
        // With min_stake = -100, `amount < min_stake` is false for -1, so the
        // existing guard admits it. This check is what stops it.
        validate_liquidity_amount(-1);
    }
}