#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};
use stellar_insured_lib::{InsuranceClaim, ClaimStatus, InsurancePolicy, PolicyStatus, PoolStats};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    PolicyContract,
    RiskPool,
    Claim(u64),
    ClaimCounter,
    /// #409: Maps policy_id -> active claim_id. Present only while a claim is active
    /// (Submitted / UnderReview / Approved). Cleared on Rejected or Settled.
    PolicyActiveClaim(u64),
}

// --- Storage helpers (#378: data access abstraction) ---

fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

fn get_claim_counter(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::ClaimCounter).unwrap_or(0)
}

fn get_claim_inner(env: &Env, claim_id: u64) -> InsuranceClaim {
    env.storage().persistent().get(&DataKey::Claim(claim_id)).expect("Claim not found")
}

fn set_claim(env: &Env, claim_id: u64, claim: &InsuranceClaim) {
    env.storage().persistent().set(&DataKey::Claim(claim_id), claim);
}

// --------------------------------------------------------

#[contract]
pub struct ClaimsContract;

#[contractimpl]
impl ClaimsContract {
    pub fn initialize(env: Env, admin: Address, policy_contract: Address, risk_pool: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        if admin == policy_contract || admin == risk_pool || policy_contract == risk_pool {
            panic!("Addresses must be distinct");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::PolicyContract, &policy_contract);
        env.storage().instance().set(&DataKey::RiskPool, &risk_pool);
        env.storage().instance().set(&DataKey::ClaimCounter, &0u64);
    }

    pub fn submit_claim(env: Env, policy_id: u64, amount: i128) -> u64 {
        // #381: fetch policy and validate consistency before accepting claim
        let policy_contract: Address = env.storage().instance().get(&DataKey::PolicyContract).unwrap();
        // #407: Centralized validation via Policy contract (includes expiration check)
        let is_active: bool = env.invoke_contract(
            &policy_contract,
            &symbol_short!("is_active"),
            (policy_id,).into(),
        );
        if !is_active {
            panic!("Policy is not active or has expired");
        }

        let policy: InsurancePolicy = env.invoke_contract(
            &policy_contract,
            &symbol_short!("get_pol"),
            (policy_id,).into(),
        );

        // Consistency check: claim amount must not exceed coverage
        if amount <= 0 || (amount + policy.total_claimed) > policy.coverage_amount {
            panic!("Claim amount invalid or exceeds remaining coverage");
        }

        // #409: O(1) duplicate claim check — reject if an active claim already exists for this policy
        if env.storage().persistent().has(&DataKey::PolicyActiveClaim(policy_id)) {
            panic!("Policy already has an active claim");
        }

        let claimant = policy.holder.clone();
        claimant.require_auth();

        let mut counter = get_claim_counter(&env);
        counter += 1;
        env.storage().instance().set(&DataKey::ClaimCounter, &counter);

        let claim = InsuranceClaim {
            claim_id: counter,
            policy_id,
            claimant,
            amount,
            status: ClaimStatus::Submitted,
            submitted_at: env.ledger().timestamp(),
        };

        set_claim(&env, counter, &claim);

        // #409: Record the active claim for this policy (O(1) dedup key)
        env.storage().persistent().set(&DataKey::PolicyActiveClaim(policy_id), &counter);

        // #412: Enhanced event emission with more details
        env.events().publish(
            (symbol_short!("claim"), symbol_short!("submitted")),
            (counter, policy_id, claimant, amount),
        );

        counter
    }

    pub fn start_review(env: Env, claim_id: u64) {
        let admin = get_admin(&env);
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id);
        if claim.status != ClaimStatus::Submitted {
            panic!("Invalid claim status for review");
        }

        claim.status = ClaimStatus::UnderReview;
        set_claim(&env, claim_id, &claim);

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("claim"), symbol_short!("review")),
            (claim_id, claim.policy_id, claim.amount),
        );
    }

    pub fn approve_claim(env: Env, claim_id: u64) {
        let admin = get_admin(&env);
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id);
        if claim.status != ClaimStatus::UnderReview {
            panic!("Claim must be under review to approve");
        }

        claim.status = ClaimStatus::Approved;
        set_claim(&env, claim_id, &claim);

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("claim"), symbol_short!("approved")),
            (claim_id, claim.policy_id, claim.amount, claim.claimant),
        );
    }

    pub fn reject_claim(env: Env, claim_id: u64) {
        let admin = get_admin(&env);
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id);
        if claim.status != ClaimStatus::UnderReview {
            panic!("Claim must be under review to reject");
        }

        claim.status = ClaimStatus::Rejected;
        set_claim(&env, claim_id, &claim);

        // #409: Clear the active-claim lock so a new claim can be submitted for this policy
        env.storage().persistent().remove(&DataKey::PolicyActiveClaim(claim.policy_id));

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("claim"), symbol_short!("rejected")),
            (claim_id, claim.policy_id, claim.amount),
        );
    }

    pub fn settle_claim(env: Env, claim_id: u64) {
        let admin = get_admin(&env);
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id);
        if claim.status != ClaimStatus::Approved {
            panic!("Only approved claims can be settled");
        }

        // #410: Check risk pool balance before payout
        let risk_pool: Address = env.storage().instance().get(&DataKey::RiskPool).unwrap();
        
        // Get pool stats to verify available capital
        let pool_stats: PoolStats = env.invoke_contract(
            &risk_pool,
            &symbol_short!("get_pool_stats"),
            ().into(),
        );
        
        if pool_stats.available_capital < claim.amount {
            panic!("Insufficient risk pool funds for payout");
        }

        // Cross-contract call to Risk Pool to payout
        // payout_claim(recipient, amount)
        let risk_pool: Address = env.storage().instance().get(&DataKey::RiskPool).unwrap();

        // Provide admin auth for RiskPool payout
        let admin = get_admin(&env);
        env.auths(&[admin]);
        env.invoke_contract::<> (
            &risk_pool,
            &symbol_short!("payout"),
            (claim.claimant.clone(), claim.amount).into(),
        );

        // Update total claimed in policy contract
        // Provide contract auth for Policy update
        let policy_contract: Address = env.storage().instance().get(&DataKey::PolicyContract).unwrap();
        let claims_addr = env.current_contract_address();
        env.auths(&[claims_addr]);
        env.invoke_contract::<> (
            &policy_contract,
            &symbol_short!("update_cl"),
            (claim.policy_id, claim.amount).into(),
        );

        claim.status = ClaimStatus::Settled;
        set_claim(&env, claim_id, &claim);

        // #409: Clear the active-claim lock after settlement
        env.storage().persistent().remove(&DataKey::PolicyActiveClaim(claim.policy_id));

        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("claim"), symbol_short!("settled")),
            (claim_id, claim.amount, claim.claimant),
        );
    }
}

#[contractimpl]
impl ClaimsContract {
    pub fn get_claim(env: Env, claim_id: u64) -> InsuranceClaim {
        get_claim_inner(&env, claim_id)
    }

    pub fn get_stats(env: Env) -> u64 {
        get_claim_counter(&env)
    }
}
