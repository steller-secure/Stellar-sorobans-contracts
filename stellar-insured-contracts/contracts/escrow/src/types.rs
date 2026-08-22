use soroban_sdk::{contracterror, contracttype, Address, Vec};

/// Typed errors for the Escrow contract — enables callers to match on
/// specific failure reasons rather than parsing panic messages, and avoids
/// paying to encode/store human-readable strings in the WASM binary and
/// revert data (#50).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    InvalidParameter = 5,
    InvalidNonce = 6,
    TooManyParticipants = 7,
    InvalidMultisigConfig = 8,
    ZeroAddress = 9,
    InvalidTimestamp = 10,
    EscrowNotFound = 11,
    InvalidStatus = 12,
    DepositExceedsAmount = 13,
    TimeLockActive = 14,
    SignatureThresholdNotMet = 15,
    AlreadySigned = 16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum EscrowStatus {
    Created,
    Funded,
    Active,
    Released,
    Refunded,
    Disputed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum ApprovalType {
    Release,
    Refund,
    EmergencyOverride,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct EscrowData {
    pub id: u64,
    pub property_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub deposited_amount: i128,
    pub status: EscrowStatus,
    pub created_at: u64,
    pub release_time_lock: Option<u64>,
    pub participants: Vec<Address>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct MultiSigConfig {
    pub required_signatures: u32,
    pub signers: Vec<Address>,
}
