use soroban_sdk::{Address, Env};

use crate::storage::DataKey;
use crate::types::EscrowError;

/// Returns `EscrowError::ContractPaused` if the contract is paused.
///
/// Always pass `&Env` (by reference) to avoid unnecessary clones — `Env` is
/// cheap to clone but passing by reference is the idiomatic pattern for
/// helper/validation functions that do not need ownership (#353).
pub fn require_not_paused(env: &Env) -> Result<(), EscrowError> {
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused {
        return Err(EscrowError::ContractPaused);
    }
    Ok(())
}

/// Returns `EscrowError::ZeroAddress` if `address` is zero (all bytes zero).
pub fn require_non_zero_address(address: &Address) -> Result<(), EscrowError> {
    if address == &Address::from([0u8; 32]) {
        return Err(EscrowError::ZeroAddress);
    }
    Ok(())
}

/// Returns `EscrowError::InvalidParameter` if the identifier is zero.
pub fn require_non_zero_u64(value: u64) -> Result<(), EscrowError> {
    if value == 0 {
        return Err(EscrowError::InvalidParameter);
    }
    Ok(())
}

/// Returns `EscrowError::InvalidParameter` if the amount is zero or negative.
pub fn require_positive_amount(amount: i128) -> Result<(), EscrowError> {
    if amount <= 0 {
        return Err(EscrowError::InvalidParameter);
    }
    Ok(())
}

/// Returns `EscrowError::InvalidTimestamp` if `timestamp` is in the past or
/// present relative to `now`.
pub fn require_future_timestamp(timestamp: u64, now: u64) -> Result<(), EscrowError> {
    if timestamp <= now {
        return Err(EscrowError::InvalidTimestamp);
    }
    Ok(())
}

/// Returns `EscrowError::InvalidMultisigConfig` if `required_signatures` is
/// zero, `participants` is empty, or `required_signatures` exceeds the
/// number of participants.
pub fn require_valid_multisig(required_signatures: u32, participant_count: u32) -> Result<(), EscrowError> {
    if required_signatures == 0 || participant_count == 0 || required_signatures > participant_count
    {
        return Err(EscrowError::InvalidMultisigConfig);
    }
    Ok(())
}

/// Reads and returns the stored admin address.
///
/// Centralises the admin lookup so callers avoid a raw `.get(&DataKey::Admin)`
/// scattered across functions — one read, one place (#353, #351).
pub fn get_admin(env: &Env) -> Result<Address, EscrowError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(EscrowError::NotInitialized)
}
