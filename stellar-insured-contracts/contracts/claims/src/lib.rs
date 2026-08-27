#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String};
use stellar_insured_lib::{
    admin::{self, AdminError, PendingAdmin},
    ClaimError, ClaimStatus, InsuranceClaim, InsurancePolicy, PolicyStatus, PoolStats,
};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    /// Successor nominated by the current admin, awaiting acceptance.
    PendingAdmin,
    PolicyContract,
    RiskPool,
    Claim(u64),
    ClaimCounter,
    /// Maps policy_id -> active claim_id. Present only while a claim is active
    /// (Submitted / UnderReview / Approved / Disputed). Cleared on terminal states.
    PolicyActiveClaim(u64),
    /// List of all claim IDs for a given policy (for enumeration)
    PolicyClaimList(u64),
    /// Global claim ID list for admin-level queries
    GlobalClaimList,
    /// Default deadline duration in seconds (configurable)
    DefaultDeadlineSeconds,
}

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

fn get_admin(env: &Env) -> Result<Address, ClaimError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(ClaimError::Unauthorized)
}

fn get_claim_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::ClaimCounter)
        .unwrap_or(0)
}

fn get_claim_inner(env: &Env, claim_id: u64) -> Result<InsuranceClaim, ClaimError> {
    env.storage()
        .persistent()
        .get(&DataKey::Claim(claim_id))
        .ok_or(ClaimError::ClaimNotFound)
}

fn set_claim(env: &Env, claim_id: u64, claim: &InsuranceClaim) {
    env.storage()
        .persistent()
        .set(&DataKey::Claim(claim_id), claim);
}

/// Add a claim ID to the per-policy claim list
fn append_policy_claim(env: &Env, policy_id: u64, claim_id: u64) {
    let key = DataKey::PolicyClaimList(policy_id);
    let mut list: soroban_sdk::Vec<u64> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| soroban_sdk::Vec::new(env));
    list.push_back(claim_id);
    env.storage().persistent().set(&key, &list);
}

/// Retrieve all claim IDs ever submitted for a policy
fn get_policy_claims(env: &Env, policy_id: u64) -> soroban_sdk::Vec<u64> {
    let key = DataKey::PolicyClaimList(policy_id);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| soroban_sdk::Vec::new(env))
}

// ---------------------------------------------------------------------------
// Active claim helpers
// ---------------------------------------------------------------------------

/// Returns true if the given status is "active" (a claim is in-flight)
fn is_active_status(status: &ClaimStatus) -> bool {
    matches!(
        status,
        ClaimStatus::Submitted
            | ClaimStatus::UnderReview
            | ClaimStatus::Approved
            | ClaimStatus::Disputed
    )
}

/// Check whether a claim has expired (deadline passed and still in an active state)
fn is_expired(env: &Env, claim: &InsuranceClaim) -> bool {
    if let Some(deadline) = claim.deadline {
        is_active_status(&claim.status) && env.ledger().timestamp() > deadline
    } else {
        false
    }
}

// ===========================================================================
// Contract implementation
// ===========================================================================

#[contract]
pub struct ClaimsContract;

#[contractimpl]
impl ClaimsContract {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    pub fn initialize(
        env: Env,
        admin: Address,
        policy_contract: Address,
        risk_pool: Address,
        default_deadline_seconds: Option<u64>,
    ) -> Result<(), ClaimError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ClaimError::AlreadyInitialized);
        }
        if admin == policy_contract || admin == risk_pool || policy_contract == risk_pool {
            return Err(ClaimError::InvalidAddress);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PolicyContract, &policy_contract);
        env.storage().instance().set(&DataKey::RiskPool, &risk_pool);
        env.storage().instance().set(&DataKey::ClaimCounter, &0u64);

        // Default claim resolution deadline: 30 days if not specified
        let deadline_secs = default_deadline_seconds.unwrap_or(30 * 24 * 60 * 60);
        env.storage()
            .instance()
            .set(&DataKey::DefaultDeadlineSeconds, &deadline_secs);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Core claim lifecycle
    // -----------------------------------------------------------------------

    /// Submit a new insurance claim against an active policy.
    pub fn submit_claim(
        env: Env,
        policy_id: u64,
        amount: i128,
        description: String,
    ) -> Result<u64, ClaimError> {
        // Fetch policy and validate consistency
        let policy_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::PolicyContract)
            .unwrap();

        let is_active: bool = env.invoke_contract(
            &policy_contract,
            &symbol_short!("is_active"),
            (policy_id,).into(),
        );
        if !is_active {
            return Err(ClaimError::PolicyNotActive);
        }

        let policy: InsurancePolicy = env.invoke_contract(
            &policy_contract,
            &symbol_short!("get_pol"),
            (policy_id,).into(),
        );

        // Validate claim amount
        if amount <= 0 || (amount + policy.total_claimed) > policy.coverage_amount {
            return Err(ClaimError::ClaimAmountInvalid);
        }

        // O(1) duplicate claim check
        if env
            .storage()
            .persistent()
            .has(&DataKey::PolicyActiveClaim(policy_id))
        {
            return Err(ClaimError::PolicyHasActiveClaim);
        }

        let claimant = policy.holder.clone();
        claimant.require_auth();

        let mut counter = get_claim_counter(&env);
        counter += 1;
        env.storage()
            .instance()
            .set(&DataKey::ClaimCounter, &counter);

        // Compute deadline
        let default_deadline: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DefaultDeadlineSeconds)
            .unwrap_or(30 * 24 * 60 * 60);
        let now = env.ledger().timestamp();

        let claim = InsuranceClaim {
            claim_id: counter,
            policy_id,
            claimant: claimant.clone(),
            amount,
            status: ClaimStatus::Submitted,
            submitted_at: now,
            reviewed_at: None,
            approved_at: None,
            settled_at: None,
            description,
            deadline: Some(now + default_deadline),
            dispute_reason: None,
        };

        set_claim(&env, counter, &claim);
        append_policy_claim(&env, policy_id, counter);

        // Record the active claim for this policy (O(1) dedup key)
        env.storage()
            .persistent()
            .set(&DataKey::PolicyActiveClaim(policy_id), &counter);

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("submitted")),
            (counter, policy_id, claimant, amount),
        );

        Ok(counter)
    }

    /// Admin transitions a submitted claim to UnderReview.
    pub fn start_review(env: Env, claim_id: u64) -> Result<(), ClaimError> {
        let admin = get_admin(&env)?;
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id)?;

        // Expire check
        if is_expired(&env, &claim) {
            claim.status = ClaimStatus::Expired;
            set_claim(&env, claim_id, &claim);
            env.storage()
                .persistent()
                .remove(&DataKey::PolicyActiveClaim(claim.policy_id));
            return Err(ClaimError::ClaimExpired);
        }

        if claim.status != ClaimStatus::Submitted {
            return Err(ClaimError::InvalidStatusTransition);
        }

        claim.status = ClaimStatus::UnderReview;
        claim.reviewed_at = Some(env.ledger().timestamp());
        set_claim(&env, claim_id, &claim);

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("review")),
            (claim_id, claim.policy_id, claim.amount),
        );

        Ok(())
    }

    /// Admin approves a claim that is UnderReview.
    pub fn approve_claim(env: Env, claim_id: u64) -> Result<(), ClaimError> {
        let admin = get_admin(&env)?;
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id)?;

        if is_expired(&env, &claim) {
            claim.status = ClaimStatus::Expired;
            set_claim(&env, claim_id, &claim);
            env.storage()
                .persistent()
                .remove(&DataKey::PolicyActiveClaim(claim.policy_id));
            return Err(ClaimError::ClaimExpired);
        }

        if claim.status != ClaimStatus::UnderReview {
            return Err(ClaimError::InvalidStatusTransition);
        }

        claim.status = ClaimStatus::Approved;
        claim.approved_at = Some(env.ledger().timestamp());
        set_claim(&env, claim_id, &claim);

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("approved")),
            (claim_id, claim.policy_id, claim.amount, claim.claimant),
        );

        Ok(())
    }

    /// Admin rejects a claim that is UnderReview.
    pub fn reject_claim(env: Env, claim_id: u64) -> Result<(), ClaimError> {
        let admin = get_admin(&env)?;
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id)?;

        if claim.status != ClaimStatus::UnderReview {
            return Err(ClaimError::InvalidStatusTransition);
        }

        claim.status = ClaimStatus::Rejected;
        set_claim(&env, claim_id, &claim);

        // Clear the active-claim lock so a new claim can be submitted for this policy
        env.storage()
            .persistent()
            .remove(&DataKey::PolicyActiveClaim(claim.policy_id));

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("rejected")),
            (claim_id, claim.policy_id, claim.amount),
        );

        Ok(())
    }

    /// Admin settles an approved claim: pays out from the risk pool and
    /// updates the policy contract's total claimed amount.
    pub fn settle_claim(env: Env, claim_id: u64) -> Result<(), ClaimError> {
        let admin = get_admin(&env)?;
        admin.require_auth();

        let mut claim = get_claim_inner(&env, claim_id)?;

        // Expire check
        if is_expired(&env, &claim) {
            claim.status = ClaimStatus::Expired;
            set_claim(&env, claim_id, &claim);
            env.storage()
                .persistent()
                .remove(&DataKey::PolicyActiveClaim(claim.policy_id));
            return Err(ClaimError::ClaimExpired);
        }

        if claim.status != ClaimStatus::Approved {
            return Err(ClaimError::InvalidStatusTransition);
        }

        // Check risk pool balance before payout
        let risk_pool: Address = env.storage().instance().get(&DataKey::RiskPool).unwrap();

        let pool_stats: PoolStats =
            env.invoke_contract(&risk_pool, &symbol_short!("get_pool_stats"), ().into());

        if pool_stats.available_capital < claim.amount {
            return Err(ClaimError::InsufficientPoolFunds);
        }

        // Cross-contract call to Risk Pool to payout
        env.invoke_contract::<()>(
            &risk_pool,
            &symbol_short!("payout_claim"),
            (claim.claimant.clone(), claim.amount).into(),
        );

        // Update total claimed in policy contract
        let policy_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::PolicyContract)
            .unwrap();
        env.invoke_contract::<()>(
            &policy_contract,
            &symbol_short!("update_claimed"),
            (claim.policy_id, claim.amount).into(),
        );

        claim.status = ClaimStatus::Settled;
        claim.settled_at = Some(env.ledger().timestamp());
        set_claim(&env, claim_id, &claim);

        // Clear the active-claim lock after settlement
        env.storage()
            .persistent()
            .remove(&DataKey::PolicyActiveClaim(claim.policy_id));

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("settled")),
            (claim_id, claim.amount, claim.claimant),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Claimant-initiated actions
    // -----------------------------------------------------------------------

    /// Claimant withdraws their own claim before it is settled.
    /// Only allowed in Submitted or UnderReview status.
    pub fn withdraw_claim(env: Env, claim_id: u64) -> Result<(), ClaimError> {
        let mut claim = get_claim_inner(&env, claim_id)?;

        // Only claimant can withdraw
        claim.claimant.require_auth();

        if !matches!(
            claim.status,
            ClaimStatus::Submitted | ClaimStatus::UnderReview
        ) {
            return Err(ClaimError::ClaimNotWithdrawable);
        }

        claim.status = ClaimStatus::Withdrawn;
        set_claim(&env, claim_id, &claim);

        // Free the active-claim lock
        env.storage()
            .persistent()
            .remove(&DataKey::PolicyActiveClaim(claim.policy_id));

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("withdrawn")),
            (claim_id, claim.claimant),
        );

        Ok(())
    }

    /// Claimant disputes a rejected claim, moving it to Disputed status.
    /// The admin can then re-review. Only allowed within 7 days of rejection.
    pub fn dispute_claim(
        env: Env,
        claim_id: u64,
        dispute_reason: String,
    ) -> Result<(), ClaimError> {
        let mut claim = get_claim_inner(&env, claim_id)?;

        // Only claimant can dispute
        claim.claimant.require_auth();

        if claim.status != ClaimStatus::Rejected {
            return Err(ClaimError::ClaimNotDisputable);
        }

        // Enforce a 7-day dispute window from submission time
        let now = env.ledger().timestamp();
        let dispute_window: u64 = 7 * 24 * 60 * 60;
        if now > claim.submitted_at + dispute_window {
            return Err(ClaimError::ClaimExpired);
        }

        claim.status = ClaimStatus::Disputed;
        claim.dispute_reason = Some(dispute_reason.clone());
        set_claim(&env, claim_id, &claim);

        // Re-lock the active claim for this policy
        env.storage()
            .persistent()
            .set(&DataKey::PolicyActiveClaim(claim.policy_id), &claim_id);

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("disputed")),
            (claim_id, claim.policy_id, dispute_reason),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin-only bulk / maintenance actions
    // -----------------------------------------------------------------------

    /// Admin can expire a claim whose deadline has passed.
    /// This is a permissionless-friendly cleanup: anyone can call it, but it
    /// only succeeds if the deadline has actually passed.
    pub fn expire_claim(env: Env, claim_id: u64) -> Result<(), ClaimError> {
        let mut claim = get_claim_inner(&env, claim_id)?;

        if !is_expired(&env, &claim) {
            return Err(ClaimError::ClaimExpired); // reuse: "not expired yet"
        }

        claim.status = ClaimStatus::Expired;
        set_claim(&env, claim_id, &claim);

        // Free the active-claim lock
        env.storage()
            .persistent()
            .remove(&DataKey::PolicyActiveClaim(claim.policy_id));

        env.events().publish(
            (symbol_short!("claim"), symbol_short!("expired")),
            (claim_id, claim.policy_id),
        );

        Ok(())
    }

    /// Batch-expire all claims past their deadline. Returns the number of
    /// claims that were expired.
    pub fn expire_stale_claims(env: Env, claim_ids: soroban_sdk::Vec<u64>) -> u32 {
        let mut expired_count: u32 = 0;
        for cid in claim_ids.iter() {
            if let Ok(mut claim) = get_claim_inner(&env, cid) {
                if is_expired(&env, &claim) {
                    claim.status = ClaimStatus::Expired;
                    set_claim(&env, cid, &claim);
                    env.storage()
                        .persistent()
                        .remove(&DataKey::PolicyActiveClaim(claim.policy_id));
                    env.events().publish(
                        (symbol_short!("claim"), symbol_short!("expired")),
                        (cid, claim.policy_id),
                    );
                    expired_count += 1;
                }
            }
        }
        expired_count
    }

    // -----------------------------------------------------------------------
    // Query functions
    // -----------------------------------------------------------------------

    /// Get a single claim by ID.
    pub fn get_claim(env: Env, claim_id: u64) -> Result<InsuranceClaim, ClaimError> {
        get_claim_inner(&env, claim_id)
    }

    /// Total number of claims submitted (counter value).
    pub fn get_stats(env: Env) -> u64 {
        get_claim_counter(&env)
    }

    /// Get all claim IDs ever submitted for a given policy.
    pub fn get_claims_by_policy(env: Env, policy_id: u64) -> soroban_sdk::Vec<u64> {
        get_policy_claims(&env, policy_id)
    }

    /// Return the active claim ID for a policy, if one exists.
    pub fn get_active_claim(env: Env, policy_id: u64) -> Option<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::PolicyActiveClaim(policy_id))
    }

    /// Check whether a specific claim has expired.
    pub fn is_claim_expired(env: Env, claim_id: u64) -> Result<bool, ClaimError> {
        let claim = get_claim_inner(&env, claim_id)?;
        Ok(is_expired(&env, &claim))
    }
}

// ===========================================================================
// Admin transfer (two-step, time-locked)
// ===========================================================================
//
// Review, approval, rejection and settlement all run through the admin key, so
// losing it stops every claim in flight with no way to recover. These entry
// points give the role a migration path with the same nominate-then-accept
// shape as `src/OwnershipTransfer.sol`; the mechanics live in
// `stellar_insured_lib::admin`, shared by all seven Soroban contracts.

#[contractimpl]
impl ClaimsContract {
    /// The current admin — the counterpart of `owner()` in `OwnershipTransfer.sol`.
    pub fn get_admin(env: Env) -> Result<Address, ClaimError> {
        crate::get_admin(&env)
    }

    /// Nominate `new_admin`, acceptable once `delay_seconds` have elapsed.
    ///
    /// Requires the current admin's authorization, and does **not** change the
    /// admin — the nominee must call [`Self::accept_admin`] within the
    /// acceptance window. See `stellar_insured_lib::admin` for the delay bounds.
    pub fn transfer_admin(
        env: Env,
        new_admin: Address,
        delay_seconds: u64,
    ) -> Result<(), AdminError> {
        admin::propose_transfer(
            &env,
            &DataKey::Admin,
            &DataKey::PendingAdmin,
            new_admin,
            delay_seconds,
        )
    }

    /// Accept an outstanding nomination. Requires the nominee's authorization.
    pub fn accept_admin(env: Env) -> Result<(), AdminError> {
        admin::accept_transfer(&env, &DataKey::Admin, &DataKey::PendingAdmin)
    }

    /// Withdraw an outstanding nomination. Requires the current admin's
    /// authorization, and is the lever the time lock exists to make usable.
    pub fn cancel_admin_transfer(env: Env) -> Result<(), AdminError> {
        admin::cancel_transfer(&env, &DataKey::Admin, &DataKey::PendingAdmin)
    }

    /// The outstanding nomination, if any.
    pub fn get_pending_admin(env: Env) -> Option<PendingAdmin> {
        admin::read_pending(&env, &DataKey::PendingAdmin)
    }
}
