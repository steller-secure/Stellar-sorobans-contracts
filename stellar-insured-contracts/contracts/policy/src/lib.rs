#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};
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

// --- Storage helpers (#378: data access abstraction) ---

fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

fn get_policy_counter(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::PolicyCounter).unwrap_or(0)
}

fn get_policy_inner(env: &Env, policy_id: u64) -> InsurancePolicy {
    env.storage().persistent().get(&DataKey::Policy(policy_id)).expect("Policy not found")
}

fn set_policy(env: &Env, policy_id: u64, policy: &InsurancePolicy) {
    env.storage().persistent().set(&DataKey::Policy(policy_id), policy);
}

// --------------------------------------------------------

#[contract]
pub struct PolicyContract;

#[contractimpl]
impl PolicyContract {
    pub fn initialize(env: Env, admin: Address, risk_pool: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::RiskPool, &risk_pool);
        env.storage().instance().set(&DataKey::PolicyCounter, &0u64);
    }

    pub fn issue_policy(
        env: Env,
        holder: Address,
        coverage_amount: i128,
        premium_amount: i128,
        duration_days: u32,
        policy_type: PolicyType,
    ) -> u64 {
        let admin: Address = env.storage().instance().get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("Contract not initialized"));
        let admin = get_admin(&env);
        admin.require_auth();

        let mut counter = get_policy_counter(&env);
        counter += 1;
        env.storage().instance().set(&DataKey::PolicyCounter, &counter);

        let risk_pool: Address = env.storage().instance().get(&DataKey::RiskPool)
            .unwrap_or_else(|| panic!("Contract not initialized"));

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

        counter
    }

    pub fn get_policy(env: Env, policy_id: u64) -> InsurancePolicy {
        get_policy_inner(&env, policy_id)
    }

    // Alias used by claims contract cross-contract call
    pub fn get_pol(env: Env, policy_id: u64) -> InsurancePolicy {
        get_policy_inner(&env, policy_id)
    }

    pub fn is_active(env: Env, policy_id: u64) -> bool {
        let policy = get_policy_inner(&env, policy_id);
        if policy.status != PolicyStatus::Active && policy.status != PolicyStatus::Renewed {
            return false;
        }

        let now = env.ledger().timestamp();
        let expiry = policy.start_time + (policy.duration_days as u64 * 86400);
        now <= expiry
    }

    pub fn renew_policy(env: Env, policy_id: u64, duration_days: u32) {
        let mut policy = get_policy_inner(&env, policy_id);
        policy.holder.require_auth();

        if policy.status != PolicyStatus::Active && policy.status != PolicyStatus::Renewed {
            panic!("Policy not active");
        }

        // #407: Ensure policy hasn't expired before renewal
        let now = env.ledger().timestamp();
        let expiry = policy.start_time + (policy.duration_days as u64 * 86400);
        if now > expiry {
            panic!("Policy has expired and cannot be renewed");
        }

        policy.duration_days += duration_days;
        policy.status = PolicyStatus::Renewed;

        set_policy(&env, policy_id, &policy);

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("policy"), symbol_short!("renewed")),
            (policy_id, policy.holder, duration_days),
        );
    }

    pub fn cancel_policy(env: Env, policy_id: u64) {
        let mut policy = get_policy_inner(&env, policy_id);
        policy.holder.require_auth();

        // #407: Ensure policy hasn't expired before cancellation
        let now = env.ledger().timestamp();
        let expiry = policy.start_time + (policy.duration_days as u64 * 86400);
        if now > expiry {
            panic!("Policy has already expired");
        }

        policy.status = PolicyStatus::Cancelled;
        set_policy(&env, policy_id, &policy);

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("policy"), symbol_short!("cancelled")),
            (policy_id, policy.holder, policy.coverage_amount),
        );
    }

    pub fn set_claims_contract(env: Env, claims_contract: Address) {
        get_admin(&env).require_auth();
        env.storage().instance().set(&DataKey::ClaimsContract, &claims_contract);
    }

    pub fn update_claimed(env: Env, policy_id: u64, amount: i128) {
        let claims_contract: Address = env.storage().instance().get(&DataKey::ClaimsContract)
            .expect("Claims contract not set");
        claims_contract.require_auth();

        let mut policy = get_policy_inner(&env, policy_id);
        policy.total_claimed += amount;

        if policy.total_claimed > policy.coverage_amount {
            panic!("Total claimed exceeds coverage amount");
        }

        set_policy(&env, policy_id, &policy);
    }

    pub fn expire_policy(env: Env, policy_id: u64) {
        let mut policy = get_policy_inner(&env, policy_id);

        let now = env.ledger().timestamp();
        let expiry = policy.start_time + (policy.duration_days as u64 * 86400);

        if now < expiry {
            panic!("Policy not yet expired");
        }

        policy.status = PolicyStatus::Expired;
        set_policy(&env, policy_id, &policy);

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("policy"), symbol_short!("expired")),
            (policy_id, policy.holder),
        );
    }
}

#[contractimpl]
impl PolicyContract {
    pub fn get_policy(env: Env, policy_id: u64) -> InsurancePolicy {
        get_policy_inner(&env, policy_id)
    }

    // Alias used by claims contract cross-contract call
    pub fn get_pol(env: Env, policy_id: u64) -> InsurancePolicy {
        get_policy_inner(&env, policy_id)
    }

    pub fn get_stats(env: Env) -> u64 {
        get_policy_counter(&env)
    }

    pub fn update_cl(env: Env, policy_id: u64, amount: i128) {
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
