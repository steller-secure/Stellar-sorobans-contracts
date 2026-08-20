#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env};
use stellar_insured_lib::{InsurancePolicy, PolicyStatus, PolicyType};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    RiskPool,
    ClaimsContract,
    Policy(u64),
    PolicyCounter,
}

/// Typed errors for the Policy contract — enables callers to match on
/// specific failure reasons rather than parsing panic messages, and avoids
/// paying to encode/store human-readable strings in the WASM binary and
/// revert data (#50).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PolicyError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    PolicyNotFound = 3,
    InvalidDuration = 4,
    InvalidCoverageAmount = 5,
    InvalidPremiumAmount = 6,
    PolicyNotActive = 7,
    PolicyExpired = 8,
    ClaimsContractNotSet = 9,
    ClaimExceedsCoverage = 10,
}

// --- Storage helpers (#378: data access abstraction) ---

fn get_admin(env: &Env) -> Result<Address, PolicyError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(PolicyError::NotInitialized)
}

fn get_policy_counter(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::PolicyCounter).unwrap_or(0)
}

fn get_policy_inner(env: &Env, policy_id: u64) -> Result<InsurancePolicy, PolicyError> {
    env.storage()
        .persistent()
        .get(&DataKey::Policy(policy_id))
        .ok_or(PolicyError::PolicyNotFound)
}

fn set_policy(env: &Env, policy_id: u64, policy: &InsurancePolicy) {
    env.storage().persistent().set(&DataKey::Policy(policy_id), policy);
}

// --------------------------------------------------------

#[contract]
pub struct PolicyContract;

#[contractimpl]
impl PolicyContract {
    pub fn initialize(env: Env, admin: Address, risk_pool: Address) -> Result<(), PolicyError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(PolicyError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::RiskPool, &risk_pool);
        env.storage().instance().set(&DataKey::PolicyCounter, &0u64);
        Ok(())
    }

    pub fn issue_policy(
        env: Env,
        holder: Address,
        coverage_amount: i128,
        premium_amount: i128,
        duration_days: u32,
        policy_type: PolicyType,
    ) -> Result<u64, PolicyError> {
        // The admin was previously read twice here: once directly from storage
        // and again through `get_admin`, with the first binding immediately
        // shadowed and discarded. `get_admin` performs the same read and
        // carries the same not-initialised error.
        let admin = get_admin(&env)?;
        admin.require_auth();

        // #52: reject parameters that would produce an incoherent policy.
        validate_policy_params(coverage_amount, premium_amount, duration_days)?;

        let mut counter = get_policy_counter(&env);
        counter += 1;
        env.storage().instance().set(&DataKey::PolicyCounter, &counter);

        let risk_pool: Address = env
            .storage()
            .instance()
            .get(&DataKey::RiskPool)
            .ok_or(PolicyError::NotInitialized)?;

        let policy = InsurancePolicy {
            policy_id: counter,
            holder: holder.clone(),
            coverage_amount,
            premium_amount,
            start_time: env.ledger().timestamp(),
            duration_days,
            policy_type,
            status: PolicyStatus::Active,
            risk_pool,
            total_claimed: 0,
        };

        set_policy(&env, counter, &policy);

        // #412: Enhanced event emission with more details
        env.events().publish(
            (symbol_short!("policy"), symbol_short!("issued")),
            (counter, holder, coverage_amount, premium_amount, duration_days),
        );

        Ok(counter)
    }

    pub fn get_policy(env: Env, policy_id: u64) -> Result<InsurancePolicy, PolicyError> {
        get_policy_inner(&env, policy_id)
    }

    // Alias used by claims contract cross-contract call
    pub fn get_pol(env: Env, policy_id: u64) -> Result<InsurancePolicy, PolicyError> {
        get_policy_inner(&env, policy_id)
    }

    pub fn is_active(env: Env, policy_id: u64) -> Result<bool, PolicyError> {
        let policy = get_policy_inner(&env, policy_id)?;
        if policy.status != PolicyStatus::Active && policy.status != PolicyStatus::Renewed {
            return Ok(false);
        }

        let now = env.ledger().timestamp();
        let expiry = policy.start_time + (policy.duration_days as u64 * 86400);
        Ok(now <= expiry)
    }

    pub fn renew_policy(env: Env, policy_id: u64, duration_days: u32) -> Result<(), PolicyError> {
        let mut policy = get_policy_inner(&env, policy_id)?;
        policy.holder.require_auth();

        if policy.status != PolicyStatus::Active && policy.status != PolicyStatus::Renewed {
            return Err(PolicyError::PolicyNotActive);
        }

        // #407: Ensure policy hasn't expired before renewal
        let now = env.ledger().timestamp();
        let expiry = policy.start_time + (policy.duration_days as u64 * 86400);
        if now > expiry {
            return Err(PolicyError::PolicyExpired);
        }

        policy.duration_days += duration_days;
        policy.status = PolicyStatus::Renewed;

        set_policy(&env, policy_id, &policy);

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("policy"), symbol_short!("renewed")),
            (policy_id, policy.holder, duration_days),
        );

        Ok(())
    }

    pub fn cancel_policy(env: Env, policy_id: u64) -> Result<(), PolicyError> {
        let mut policy = get_policy_inner(&env, policy_id)?;
        policy.holder.require_auth();

        // #407: Ensure policy hasn't expired before cancellation
        let now = env.ledger().timestamp();
        let expiry = policy.start_time + (policy.duration_days as u64 * 86400);
        if now > expiry {
            return Err(PolicyError::PolicyExpired);
        }

        policy.status = PolicyStatus::Cancelled;
        set_policy(&env, policy_id, &policy);

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("policy"), symbol_short!("cancelled")),
            (policy_id, policy.holder, policy.coverage_amount),
        );

        Ok(())
    }

    pub fn set_claims_contract(env: Env, claims_contract: Address) -> Result<(), PolicyError> {
        get_admin(&env)?.require_auth();
        env.storage().instance().set(&DataKey::ClaimsContract, &claims_contract);
        Ok(())
    }

    pub fn update_claimed(env: Env, policy_id: u64, amount: i128) -> Result<(), PolicyError> {
        let claims_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::ClaimsContract)
            .ok_or(PolicyError::ClaimsContractNotSet)?;
        claims_contract.require_auth();

        let mut policy = get_policy_inner(&env, policy_id)?;
        policy.total_claimed += amount;

        if policy.total_claimed > policy.coverage_amount {
            return Err(PolicyError::ClaimExceedsCoverage);
        }

        set_policy(&env, policy_id, &policy);
        Ok(())
    }

    pub fn expire_policy(env: Env, policy_id: u64) -> Result<(), PolicyError> {
        let mut policy = get_policy_inner(&env, policy_id)?;

        let now = env.ledger().timestamp();
        let expiry = policy.start_time + (policy.duration_days as u64 * 86400);

        if now < expiry {
            return Err(PolicyError::PolicyNotActive);
        }

        policy.status = PolicyStatus::Expired;
        set_policy(&env, policy_id, &policy);

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("policy"), symbol_short!("expired")),
            (policy_id, policy.holder),
        );

        Ok(())
    }
}

#[contractimpl]
impl PolicyContract {
    // `get_policy` and `get_pol` are defined in the primary impl block above.
    // They were duplicated here verbatim, which made the crate fail to compile
    // with E0592 duplicate definitions.

    pub fn get_stats(env: Env) -> u64 {
        get_policy_counter(&env)
    }

    pub fn update_cl(env: Env, policy_id: u64, amount: i128) -> Result<(), PolicyError> {
        Self::update_claimed(env, policy_id, amount)
    }

    // New placeholder for Governance PolicyChange action
    pub fn policy_update(env: Env, policy_id: u64) {
        // Placeholder: could implement policy parameter changes.
        // Emit an event for visibility.
        env.events().publish(
            (symbol_short!("policy"), symbol_short!("updated")),
            policy_id,
        );
    }
}

// ─── Input validation (#52) ───────────────────────────────────────────────────

/// Validate the financially significant parameters of a new policy.
///
/// Kept as a free function over primitives, with no `Env`, so the rules are
/// unit-testable without the Soroban test harness.
///
/// Each check exists because the value is used in arithmetic or comparison that
/// silently produces nonsense otherwise:
///
/// * `duration_days == 0` yields `expiry = start_time + 0`, i.e. a policy that
///   is already expired the moment it is issued.
/// * `coverage_amount < 0` makes `total_claimed > coverage_amount` trivially
///   true — `total_claimed` starts at 0, and 0 exceeds any negative value — so
///   claims become payable against a policy with no coverage.
/// * `coverage_amount == 0` is a policy that can never pay out; issuing one
///   takes a premium for nothing.
/// * `premium_amount < 0` corrupts pool accounting, since the premium is added
///   to pool capital.
pub fn validate_policy_params(
    coverage_amount: i128,
    premium_amount: i128,
    duration_days: u32,
) -> Result<(), PolicyError> {
    if duration_days == 0 {
        return Err(PolicyError::InvalidDuration);
    }
    if coverage_amount <= 0 {
        return Err(PolicyError::InvalidCoverageAmount);
    }
    if premium_amount < 0 {
        return Err(PolicyError::InvalidPremiumAmount);
    }
    Ok(())
}

#[cfg(test)]
mod validation_tests {
    use super::{validate_policy_params, PolicyError};

    const COVERAGE: i128 = 10_000;
    const PREMIUM: i128 = 100;
    const DURATION: u32 = 30;

    #[test]
    fn accepts_sane_parameters() {
        assert_eq!(validate_policy_params(COVERAGE, PREMIUM, DURATION), Ok(()));
    }

    #[test]
    fn accepts_a_zero_premium() {
        // A promotional or sponsored policy is legitimate; only negative is not.
        assert_eq!(validate_policy_params(COVERAGE, 0, DURATION), Ok(()));
    }

    #[test]
    fn rejects_zero_duration() {
        assert_eq!(
            validate_policy_params(COVERAGE, PREMIUM, 0),
            Err(PolicyError::InvalidDuration)
        );
    }

    #[test]
    fn rejects_negative_coverage() {
        // The case that makes `total_claimed > coverage_amount` trivially true.
        assert_eq!(
            validate_policy_params(-1, PREMIUM, DURATION),
            Err(PolicyError::InvalidCoverageAmount)
        );
    }

    #[test]
    fn rejects_zero_coverage() {
        assert_eq!(
            validate_policy_params(0, PREMIUM, DURATION),
            Err(PolicyError::InvalidCoverageAmount)
        );
    }

    #[test]
    fn rejects_negative_premium() {
        assert_eq!(
            validate_policy_params(COVERAGE, -1, DURATION),
            Err(PolicyError::InvalidPremiumAmount)
        );
    }

    #[test]
    fn reports_coverage_first_when_several_are_invalid() {
        // Duration is checked first, so pass a valid duration to assert the
        // coverage rule is reached rather than masked.
        assert_eq!(
            validate_policy_params(-5, -5, DURATION),
            Err(PolicyError::InvalidCoverageAmount)
        );
    }
}
