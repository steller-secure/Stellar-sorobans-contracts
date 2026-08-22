#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Vec, Symbol};
use stellar_insured_lib::{Proposal, GovernanceAction};

/// Typed errors for the Governance contract — enables callers to match on
/// specific failure reasons rather than parsing panic messages, and avoids
/// paying to encode/store human-readable strings in the WASM binary and
/// revert data (#50).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum GovernanceError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    ProposalNotFound = 3,
    InvalidWeight = 4,
    InsufficientTokenBalance = 5,
    VotingPeriodEnded = 6,
    AlreadyVoted = 7,
    VotingPeriodNotEnded = 8,
    ProposalNotFinalized = 9,
    ProposalAlreadyExecuted = 10,
    TokenNotSet = 11,
    QuorumNotMet = 12,
    ThresholdNotMet = 13,
    ClaimsContractNotSet = 14,
    RiskPoolContractNotSet = 15,
    PolicyContractNotSet = 16,
    InvalidQuorumPercentage = 17,
    InvalidThresholdPercentage = 18,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Token,
    SlashingContract,
    ClaimsContract,
    RiskPoolContract,
    PolicyContract,
    Proposal(u64),
    ProposalCounter,
    VoterRecord(u64, Address),
    VotingPeriod,
    GovernanceActionPending(u64),  // proposal_id -> GovernanceAction
    MinQuorumPercentage,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoteRecord {
    pub voter: Address,
    pub weight: i128,
    pub is_yes: bool,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposalStats {
    pub yes_votes: i128,
    pub no_votes: i128,
    pub total_votes: i128,
    pub status: Symbol,
}

// --- Storage helpers (#378: data access abstraction) ---

fn get_admin(env: &Env) -> Result<Address, GovernanceError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)
}

fn get_voting_period(env: &Env) -> Result<u64, GovernanceError> {
    env.storage()
        .instance()
        .get(&DataKey::VotingPeriod)
        .ok_or(GovernanceError::NotInitialized)
}

fn get_proposal_counter(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::ProposalCounter).unwrap_or(0)
}

fn get_proposal_inner(env: &Env, proposal_id: u64) -> Result<Proposal, GovernanceError> {
    env.storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)
}

fn set_proposal(env: &Env, proposal_id: u64, proposal: &Proposal) {
    env.storage().persistent().set(&DataKey::Proposal(proposal_id), proposal);
}

// --------------------------------------------------------

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        slashing_contract: Address,
        voting_period: u64,
        claims_contract: Address,
        risk_pool_contract: Address,
        policy_contract: Address,
    ) -> Result<(), GovernanceError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(GovernanceError::AlreadyInitialized);
        }
        // Validate voting period is reasonable to prevent overflow in expiry calculations
        if voting_period == 0 {
            panic!("Voting period must be greater than zero");
        }
        // Maximum voting period to prevent overflow: u64::MAX / 2 is a safe limit that
        // ensures even if current timestamp is large, adding voting_period won't overflow
        if voting_period > u64::MAX / 2 {
            panic!("Voting period is too large");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::SlashingContract, &slashing_contract);
        env.storage().instance().set(&DataKey::VotingPeriod, &voting_period);
        env.storage().instance().set(&DataKey::ProposalCounter, &0u64);
        env.storage().instance().set(&DataKey::ClaimsContract, &claims_contract);
        env.storage().instance().set(&DataKey::RiskPoolContract, &risk_pool_contract);
        env.storage().instance().set(&DataKey::PolicyContract, &policy_contract);

        // #379: emit event for initialization
        env.events().publish(
            (symbol_short!("admin"), symbol_short!("init")),
            admin,
        );

        Ok(())
    }

    pub fn create_proposal(
        env: Env,
        creator: Address,
        title: String,
        description: String,
        execution_data: String,
        threshold_percentage: u32,
    ) -> Result<u64, GovernanceError> {
        creator.require_auth();

        // #52: an unbounded threshold yields a proposal that cannot pass, or
        // one that passes with no support.
        validate_threshold_percentage(threshold_percentage)?;

        let mut counter = get_proposal_counter(&env);
        counter += 1;
        env.storage().instance().set(&DataKey::ProposalCounter, &counter);

        // Read once and reuse. This previously read VotingPeriod directly and
        // then called `get_voting_period`, which reads the same key again — so
        // the binding below was dead and the storage access doubled.
        let voting_period = get_voting_period(&env);
        
        let current_timestamp = env.ledger().timestamp();
        let expires_at = current_timestamp.checked_add(voting_period).expect("Proposal expiry timestamp overflow");
        let proposal = Proposal {
            id: counter,
            title,
            description,
            execution_data,
            creator: creator.clone(),
            expires_at,
            threshold_percentage,
            yes_votes: 0,
            no_votes: 0,
            is_finalized: false,
            is_executed: false,
        };

        set_proposal(&env, counter, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("created")),
            (counter, creator),
        );

        Ok(counter)
    }

    pub fn create_slashing_proposal(
        env: Env,
        creator: Address,
        target: Address,
        role: Symbol,
        reason: String,
        amount: i128,
        threshold: u32,
    ) -> Result<u64, GovernanceError> {
        creator.require_auth();

        // #52: same bounds as an ordinary proposal.
        validate_threshold_percentage(threshold)?;

        let title = String::from_str(&env, "Slashing Proposal");
        let execution_data = String::from_str(&env, "slash_call");

        let mut counter = get_proposal_counter(&env);
        counter += 1;
        env.storage().instance().set(&DataKey::ProposalCounter, &counter);

        let voting_period = get_voting_period(&env);
        let current_timestamp = env.ledger().timestamp();
        let expires_at = current_timestamp.checked_add(voting_period).expect("Proposal expiry timestamp overflow");
        let proposal = Proposal {
            id: counter,
            title,
            description: reason,
            execution_data,
            creator: creator.clone(),
            expires_at: env.ledger().timestamp() + get_voting_period(&env)?,
            threshold_percentage: threshold,
            yes_votes: 0,
            no_votes: 0,
            is_finalized: false,
            is_executed: false,
        };

        set_proposal(&env, counter, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("slash_p")),
            (counter, target, role, amount),
        );

        Ok(counter)
    }

    // #411: Create governance proposal for claim approval
    pub fn create_claim_approval_proposal(
        env: Env,
        creator: Address,
        claim_id: u64,
        threshold: u32,
    ) -> Result<u64, GovernanceError> {
        creator.require_auth();

        let title = String::from_str(&env, "Claim Approval Proposal");
        let description = String::from_str(&env, "DAO vote required for claim approval");
        let execution_data = String::from_str(&env, "approve_claim");

        let mut counter = get_proposal_counter(&env);
        counter += 1;
        env.storage().instance().set(&DataKey::ProposalCounter, &counter);

        // Read once and reuse. This previously read VotingPeriod directly and
        // then called `get_voting_period`, which reads the same key again — so
        // the binding below was dead and the storage access doubled.
        let voting_period = get_voting_period(&env)?;

        let current_timestamp = env.ledger().timestamp();
        let expires_at = current_timestamp.checked_add(voting_period).expect("Proposal expiry timestamp overflow");
        let proposal = Proposal {
            id: counter,
            title,
            description,
            execution_data,
            creator: creator.clone(),
            expires_at,
            threshold_percentage: threshold,
            yes_votes: 0,
            no_votes: 0,
            is_finalized: false,
            is_executed: false,
        };

        set_proposal(&env, counter, &proposal);

        // Store the governance action
        let action = GovernanceAction::ClaimApproval(claim_id);
        env.storage().persistent().set(&DataKey::GovernanceActionPending(counter), &action);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("claim_prop")),
            (counter, claim_id, creator),
        );

        Ok(counter)
    }

    // #411: Create governance proposal for fund allocation
    pub fn create_fund_allocation_proposal(
        env: Env,
        creator: Address,
        recipient: Address,
        amount: i128,
        threshold: u32,
    ) -> Result<u64, GovernanceError> {
        creator.require_auth();

        let title = String::from_str(&env, "Fund Allocation Proposal");
        let description = String::from_str(&env, "DAO vote required for fund allocation");
        let execution_data = String::from_str(&env, "allocate_funds");

        let mut counter = get_proposal_counter(&env);
        counter += 1;
        env.storage().instance().set(&DataKey::ProposalCounter, &counter);

        // Read once and reuse. This previously read VotingPeriod directly and
        // then called `get_voting_period`, which reads the same key again — so
        // the binding below was dead and the storage access doubled.
        let voting_period = get_voting_period(&env)?;

        let current_timestamp = env.ledger().timestamp();
        let expires_at = current_timestamp.checked_add(voting_period).expect("Proposal expiry timestamp overflow");
        let proposal = Proposal {
            id: counter,
            title,
            description,
            execution_data,
            creator: creator.clone(),
            expires_at,
            threshold_percentage: threshold,
            yes_votes: 0,
            no_votes: 0,
            is_finalized: false,
            is_executed: false,
        };

        set_proposal(&env, counter, &proposal);

        // Store the governance action
        let action = GovernanceAction::FundAllocation(recipient, amount);
        env.storage().persistent().set(&DataKey::GovernanceActionPending(counter), &action);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("fund_prop")),
            (counter, recipient, amount, creator),
        );

        counter
    }

    pub fn vote(env: Env, voter: Address, proposal_id: u64, weight: i128, is_yes: bool) -> Result<(), GovernanceError> {
        voter.require_auth();

        if weight <= 0 {
            return Err(GovernanceError::InvalidWeight);
        }

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(GovernanceError::TokenNotSet)?;
        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);
        let actual_balance = token_client.balance(&voter);

        if weight > actual_balance {
            return Err(GovernanceError::InsufficientTokenBalance);
        }

        let mut proposal = get_proposal_inner(&env, proposal_id)?;

        if env.ledger().timestamp() > proposal.expires_at {
            return Err(GovernanceError::VotingPeriodEnded);
        }

        let record_key = DataKey::VoterRecord(proposal_id, voter.clone());
        if env.storage().persistent().has(&record_key) {
            return Err(GovernanceError::AlreadyVoted);
        }

        if is_yes {
            proposal.yes_votes += weight;
        } else {
            proposal.no_votes += weight;
        }

        let record = VoteRecord {
            voter: voter.clone(),
            weight,
            is_yes,
            timestamp: env.ledger().timestamp(),
        };

        set_proposal(&env, proposal_id, &proposal);
        env.storage().persistent().set(&record_key, &record);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("vote")),
            (proposal_id, voter),
        );

        Ok(())
    }

    pub fn finalize_proposal(env: Env, proposal_id: u64) -> Result<(), GovernanceError> {
        let mut proposal = get_proposal_inner(&env, proposal_id)?;

        if env.ledger().timestamp() <= proposal.expires_at {
            return Err(GovernanceError::VotingPeriodNotEnded);
        }

        proposal.is_finalized = true;
        set_proposal(&env, proposal_id, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("final")),
            proposal_id,
        );

        Ok(())
    }

    pub fn execute_proposal(env: Env, proposal_id: u64) -> Result<(), GovernanceError> {
        let mut proposal = get_proposal_inner(&env, proposal_id)?;

        if !proposal.is_finalized {
            return Err(GovernanceError::ProposalNotFinalized);
        }

        if proposal.is_executed {
            return Err(GovernanceError::ProposalAlreadyExecuted);
        }

        let total_votes = proposal.yes_votes + proposal.no_votes;

        // Quorum Check
        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(GovernanceError::TokenNotSet)?;
        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);
        let total_supply = token_client.total_supply();

        let quorum_percentage: u32 = env.storage().instance().get(&DataKey::MinQuorumPercentage).unwrap_or(10); // default 10%
        let required_quorum = total_supply * (quorum_percentage as i128) / 100;
        if total_votes < required_quorum {
            return Err(GovernanceError::QuorumNotMet);
        }

        if total_votes == 0 || (proposal.yes_votes * 100 / total_votes) < proposal.threshold_percentage as i128 {
            return Err(GovernanceError::ThresholdNotMet);
        }

        // #411: Execute governance action if exists
        let action_key = DataKey::GovernanceActionPending(proposal_id);
        if env.storage().persistent().has(&action_key) {
            let action: GovernanceAction = env.storage().persistent().get(&action_key).unwrap();

            match action {
                GovernanceAction::ClaimApproval(claim_id) => {
                    // Call claims contract to approve the claim
                    let claims_contract: Address = env
                        .storage()
                        .instance()
                        .get(&DataKey::ClaimsContract)
                        .ok_or(GovernanceError::ClaimsContractNotSet)?;
                    env.invoke_contract::<()>(
                        &claims_contract,
                        &symbol_short!("approve"),
                        (claim_id,).into(),
                    );
                }
                GovernanceAction::FundAllocation(recipient, amount) => {
                    // Call risk pool to allocate funds
                    let risk_pool: Address = env
                        .storage()
                        .instance()
                        .get(&DataKey::RiskPoolContract)
                        .ok_or(GovernanceError::RiskPoolContractNotSet)?;
                    env.invoke_contract::<()>(
                        &risk_pool,
                        &symbol_short!("payout"),
                        (recipient, amount).into(),
                    );
                }
                GovernanceAction::PolicyChange(policy_id) => {
                    // Handle policy change through policy contract
                    let policy_contract: Address = env
                        .storage()
                        .instance()
                        .get(&DataKey::PolicyContract)
                        .ok_or(GovernanceError::PolicyContractNotSet)?;
                    env.invoke_contract::<()>(
                        &policy_contract,
                        &symbol_short!("update"),
                        (policy_id,).into(),
                    );
                }
            }

            // Remove the pending action
            env.storage().persistent().remove(&action_key);
        }

        proposal.is_executed = true;
        set_proposal(&env, proposal_id, &proposal);

        // #379: emit event for admin/governance action
        // #412: Enhanced event emission
        env.events().publish(
            (symbol_short!("admin"), symbol_short!("exec")),
            (proposal_id, proposal.creator),
        );

        Ok(())
    }

    pub fn execute_slashing_proposal(env: Env, proposal_id: u64) -> Result<(), GovernanceError> {
        Self::execute_proposal(env, proposal_id)
    }

    pub fn set_min_quorum_percentage(env: Env, value: u32) -> Result<(), GovernanceError> {
        let admin = get_admin(&env)?;
        admin.require_auth();
        // #52: the previous check admitted zero, which disables quorum entirely.
        validate_quorum_percentage(value)?;
        env.storage().instance().set(&DataKey::MinQuorumPercentage, &value);
        Ok(())
    }
}

#[contractimpl]
impl GovernanceContract {
    pub fn get_proposal(env: Env, proposal_id: u64) -> Result<Proposal, GovernanceError> {
        get_proposal_inner(&env, proposal_id)
    }

    pub fn get_active_proposals(env: Env) -> Vec<u64> {
        let counter = get_proposal_counter(&env);
        let mut list = Vec::new(&env);
        let now = env.ledger().timestamp();
        for i in 1..=counter {
            if let Some(p) = env.storage().persistent().get::<DataKey, Proposal>(&DataKey::Proposal(i)) {
                if !p.is_finalized && now <= p.expires_at {
                    list.push_back(i);
                }
            }
        }
        list
    }

    pub fn get_proposal_stats(env: Env, proposal_id: u64) -> Result<ProposalStats, GovernanceError> {
        let p = get_proposal_inner(&env, proposal_id)?;
        let now = env.ledger().timestamp();
        let status = if p.is_executed {
            symbol_short!("executed")
        } else if p.is_finalized {
            symbol_short!("finalized")
        } else if now > p.expires_at {
            symbol_short!("expired")
        } else {
            symbol_short!("active")
        };

        Ok(ProposalStats {
            yes_votes: p.yes_votes,
            no_votes: p.no_votes,
            total_votes: p.yes_votes + p.no_votes,
            status,
        })
    }

    pub fn get_all_proposals(env: Env) -> Vec<u64> {
        let counter = get_proposal_counter(&env);
        let mut list = Vec::new(&env);
        for i in 1..=counter {
            list.push_back(i);
        }
        list
    }

    pub fn get_vote_record(env: Env, proposal_id: u64, voter: Address) -> Option<VoteRecord> {
        env.storage().persistent().get(&DataKey::VoterRecord(proposal_id, voter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, Address, String, symbol_short};
    use soroban_sdk::token::Client as TokenClient;
    use soroban_sdk::token::StellarAssetClient as TokenAdminClient;

    fn setup_test_token(env: &Env) -> (Address, TokenAdminClient, TokenClient) {
        let token_addr = env.register_stellar_asset_contract(Address::generate(env));
        let admin_client = TokenAdminClient::new(env, &token_addr);
        let client = TokenClient::new(env, &token_addr);
        (token_addr, admin_client, client)
    }

    fn setup_governance(env: &Env, token_addr: &Address) -> GovernanceContractClient {
        let gov_addr = env.register_contract(None, GovernanceContract);
        let client = GovernanceContractClient::new(env, &gov_addr);
        let admin = Address::generate(env);
        let slashing = Address::generate(env);
        let claims = Address::generate(env);
        let risk_pool = Address::generate(env);
        let policy = Address::generate(env);
        client.initialize(&admin, token_addr, &slashing, &3600, &claims, &risk_pool, &policy);
        client
    }

    #[test]
    #[should_panic]
    fn test_attacker_with_zero_tokens_cannot_vote() {
        let env = Env::default();
        env.mock_all_auths();

        let (token_addr, _, _) = setup_test_token(&env);
        let gov_client = setup_governance(&env, &token_addr);

        let creator = Address::generate(&env);
        let proposal_id = gov_client.create_proposal(
            &creator,
            &String::from_str(&env, "Test Prop"),
            &String::from_str(&env, "Desc"),
            &String::from_str(&env, "Data"),
            &50,
        );

        let attacker = Address::generate(&env);
        // Attacker has 0 tokens, trying to vote with weight 1 should fail.
        gov_client.vote(&attacker, &proposal_id, &1, &true);
    }

    #[test]
    #[should_panic]
    fn test_cannot_vote_exceeding_token_balance() {
        let env = Env::default();
        env.mock_all_auths();

        let (token_addr, token_admin, _) = setup_test_token(&env);
        let gov_client = setup_governance(&env, &token_addr);

        let voter = Address::generate(&env);
        token_admin.mint(&voter, &100);

        let creator = Address::generate(&env);
        let proposal_id = gov_client.create_proposal(
            &creator,
            &String::from_str(&env, "Test Prop"),
            &String::from_str(&env, "Desc"),
            &String::from_str(&env, "Data"),
            &50,
        );

        // Voter has 100 tokens, trying to vote with 101 weight should fail.
        gov_client.vote(&voter, &proposal_id, &101, &true);
    }

    #[test]
    fn test_successful_voting_up_to_balance() {
        let env = Env::default();
        env.mock_all_auths();

        let (token_addr, token_admin, _) = setup_test_token(&env);
        let gov_client = setup_governance(&env, &token_addr);

        let voter = Address::generate(&env);
        token_admin.mint(&voter, &100);

        let creator = Address::generate(&env);
        let proposal_id = gov_client.create_proposal(
            &creator,
            &String::from_str(&env, "Test Prop"),
            &String::from_str(&env, "Desc"),
            &String::from_str(&env, "Data"),
            &50,
        );

        // Vote exactly 100 weight (voter's balance).
        gov_client.vote(&voter, &proposal_id, &100, &true);

        let stats = gov_client.get_proposal_stats(&proposal_id);
        assert_eq!(stats.yes_votes, 100);
    }

    #[test]
    #[should_panic]
    fn test_quorum_enforcement_rejection() {
        let env = Env::default();
        env.mock_all_auths();

        let (token_addr, token_admin, _) = setup_test_token(&env);
        let gov_client = setup_governance(&env, &token_addr);

        // Total supply = 1000 tokens
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        token_admin.mint(&voter1, &90); // 9% of total supply
        token_admin.mint(&voter2, &910); // remaining 91%

        let creator = Address::generate(&env);
        let proposal_id = gov_client.create_proposal(
            &creator,
            &String::from_str(&env, "Test Prop"),
            &String::from_str(&env, "Desc"),
            &String::from_str(&env, "Data"),
            &50,
        );

        // Vote with voter1 (9% of supply).
        gov_client.vote(&voter1, &proposal_id, &90, &true);

        // Fast forward past voting period (3600 seconds).
        env.ledger().with_mut(|l| l.timestamp = 3601);

        gov_client.finalize_proposal(&proposal_id);

        // Execution should fail because 9% quorum is below default 10% quorum requirement.
        gov_client.execute_proposal(&proposal_id);
    }

    #[test]
    fn test_quorum_and_threshold_met_success() {
        let env = Env::default();
        env.mock_all_auths();

        let (token_addr, token_admin, _) = setup_test_token(&env);
        let gov_client = setup_governance(&env, &token_addr);

        // Total supply = 1000 tokens
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        token_admin.mint(&voter1, &150); // 15% of total supply
        token_admin.mint(&voter2, &850);

        let creator = Address::generate(&env);
        let proposal_id = gov_client.create_proposal(
            &creator,
            &String::from_str(&env, "Test Prop"),
            &String::from_str(&env, "Desc"),
            &String::from_str(&env, "Data"),
            &50,
        );

        // Vote with voter1 (15% of supply).
        gov_client.vote(&voter1, &proposal_id, &150, &true);

        // Fast forward past voting period (3600 seconds).
        env.ledger().with_mut(|l| l.timestamp = 3601);

        gov_client.finalize_proposal(&proposal_id);

        // Quorum is met (15% >= 10%) and threshold is met (100% yes votes >= 50%).
        gov_client.execute_proposal(&proposal_id);

        let stats = gov_client.get_proposal_stats(&proposal_id);
        assert_eq!(stats.status, symbol_short!("executed"));
    }
}

// ─── Input validation (#52) ───────────────────────────────────────────────────

/// Validate a quorum percentage.
///
/// The existing check rejected values above 100 but admitted zero, which
/// disables the quorum requirement entirely: any proposal with a single vote
/// would meet a 0% quorum. A quorum of nothing is not a quorum.
pub fn validate_quorum_percentage(value: u32) -> Result<(), GovernanceError> {
    if value == 0 {
        return Err(GovernanceError::InvalidQuorumPercentage);
    }
    if value > 100 {
        return Err(GovernanceError::InvalidQuorumPercentage);
    }
    Ok(())
}

/// Validate a proposal approval threshold.
///
/// Unbounded before: a threshold above 100 makes a proposal mathematically
/// impossible to pass, and zero makes it pass with no support at all. Both
/// produce a proposal that looks valid and cannot behave sensibly.
pub fn validate_threshold_percentage(value: u32) -> Result<(), GovernanceError> {
    if value == 0 {
        return Err(GovernanceError::InvalidThresholdPercentage);
    }
    if value > 100 {
        return Err(GovernanceError::InvalidThresholdPercentage);
    }
    Ok(())
}

#[cfg(test)]
mod validation_tests {
    use super::{validate_quorum_percentage, validate_threshold_percentage, GovernanceError};

    #[test]
    fn quorum_accepts_the_valid_range() {
        for value in [1, 50, 100] {
            assert_eq!(validate_quorum_percentage(value), Ok(()));
        }
    }

    #[test]
    fn quorum_rejects_zero() {
        // The gap in the original check: > 100 was rejected, 0 was not.
        assert_eq!(
            validate_quorum_percentage(0),
            Err(GovernanceError::InvalidQuorumPercentage)
        );
    }

    #[test]
    fn quorum_rejects_above_one_hundred() {
        assert_eq!(
            validate_quorum_percentage(101),
            Err(GovernanceError::InvalidQuorumPercentage)
        );
    }

    #[test]
    fn threshold_accepts_the_valid_range() {
        for value in [1, 51, 100] {
            assert_eq!(validate_threshold_percentage(value), Ok(()));
        }
    }

    #[test]
    fn threshold_rejects_zero() {
        // Would let a proposal pass with no support.
        assert_eq!(
            validate_threshold_percentage(0),
            Err(GovernanceError::InvalidThresholdPercentage)
        );
    }

    #[test]
    fn threshold_rejects_above_one_hundred() {
        // Would make the proposal impossible to pass.
        assert_eq!(
            validate_threshold_percentage(101),
            Err(GovernanceError::InvalidThresholdPercentage)
        );
    }
}