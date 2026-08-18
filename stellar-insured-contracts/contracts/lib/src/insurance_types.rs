use soroban_sdk::{contracterror, contracttype, Address, String, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyStatus {
    Active,
    Renewed,
    Expired,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyType {
    Standard,
    Parametric,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsurancePolicy {
    pub policy_id: u64,
    pub holder: Address,
    pub coverage_amount: i128,
    pub premium_amount: i128,
    pub start_time: u64,
    pub duration_days: u32,
    pub policy_type: PolicyType,
    pub status: PolicyStatus,
    pub risk_pool: Address,
    pub total_claimed: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClaimStatus {
    Submitted,
    UnderReview,
    Approved,
    Rejected,
    Settled,
    Withdrawn,
    Disputed,
    Expired,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceClaim {
    pub claim_id: u64,
    pub policy_id: u64,
    pub claimant: Address,
    pub amount: i128,
    pub status: ClaimStatus,
    pub submitted_at: u64,
    /// Timestamp when the claim moved to UnderReview
    pub reviewed_at: Option<u64>,
    /// Timestamp when the claim was approved
    pub approved_at: Option<u64>,
    /// Timestamp when the claim was settled (payout completed)
    pub settled_at: Option<u64>,
    /// Free-text reason / description for the claim
    pub description: String,
    /// Deadline by which the claim must be settled; expired claims become ClaimStatus::Expired
    pub deadline: Option<u64>,
    /// If disputed, the dispute reason from the claimant
    pub dispute_reason: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolStats {
    pub total_capital: i128,
    pub available_capital: i128,
    pub total_claims_paid: i128,
    pub provider_count: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub execution_data: String,
    pub creator: Address,
    pub expires_at: u64,
    pub threshold_percentage: u32,
    pub yes_votes: i128,
    pub no_votes: i128,
    pub is_finalized: bool,
    pub is_executed: bool,
}

// #411: Add governance action types for DAO integration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GovernanceAction {
    ClaimApproval(u64),            // claim_id
    FundAllocation(Address, i128), // recipient, amount
    PolicyChange(u64),             // policy_id
}

/// Typed errors for the Claims contract — enables callers to match
/// on specific failure reasons rather than parsing panic messages.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ClaimError {
    AlreadyInitialized = 1,
    InvalidAddress = 2,
    PolicyNotActive = 3,
    ClaimAmountInvalid = 4,
    PolicyHasActiveClaim = 5,
    ClaimNotFound = 6,
    InvalidStatusTransition = 7,
    InsufficientPoolFunds = 8,
    ClaimNotWithdrawable = 9,
    ClaimNotDisputable = 10,
    OnlyClaimantCanDispute = 11,
    ClaimExpired = 12,
    Unauthorized = 13,
}
