use soroban_sdk::{Address, Env, Vec};

use crate::storage::DataKey;
use crate::types::{BridgeConfig, BridgeError};

/// Returns `BridgeError::BridgePaused` if the bridge is paused.
///
/// Reads the config from storage via `&Env` — consistent with the escrow
/// contract's `require_not_paused` signature so all validation helpers
/// follow the same `&Env` convention (#353).
pub fn require_not_paused(env: &Env) -> Result<(), BridgeError> {
    let config: BridgeConfig = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(BridgeError::NotInitialized)?;
    if config.emergency_pause {
        return Err(BridgeError::BridgePaused);
    }
    Ok(())
}

/// Returns `BridgeError::UnsupportedChain` if `destination_chain` is not in
/// the supported chains list.
pub fn require_supported_chain(config: &BridgeConfig, destination_chain: u32) -> Result<(), BridgeError> {
    if !config.supported_chains.contains(destination_chain) {
        return Err(BridgeError::UnsupportedChain);
    }
    Ok(())
}

/// Returns `BridgeError::InvalidSignatureRequirement` if `required_signatures`
/// is outside the configured [min, max] range.
pub fn require_valid_signatures(config: &BridgeConfig, required_signatures: u32) -> Result<(), BridgeError> {
    if required_signatures < config.min_signatures_required
        || required_signatures > config.max_signatures_required
    {
        return Err(BridgeError::InvalidSignatureRequirement);
    }
    Ok(())
}

/// Returns `BridgeError::NotOperator` if `caller` is not in the operators list.
pub fn require_operator(env: &Env, caller: &Address) -> Result<(), BridgeError> {
    let operators: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Operators)
        .ok_or(BridgeError::NotInitialized)?;
    if !operators.contains(caller.clone()) {
        return Err(BridgeError::NotOperator);
    }
    Ok(())
}

/// Returns `BridgeError::Unauthorized` if `caller` is not the stored admin.
pub fn require_admin(env: &Env, caller: &Address) -> Result<(), BridgeError> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(BridgeError::NotInitialized)?;
    if *caller != admin {
        return Err(BridgeError::Unauthorized);
    }
    Ok(())
}

/// No-op in Soroban as Address is always a valid host-managed object and cannot be zero.
pub fn require_non_zero_address(_address: &Address) -> Result<(), BridgeError> {
    Ok(())
}

/// Returns `BridgeError::InvalidParameter` if the value is zero.
pub fn require_non_zero_u32(value: u32) -> Result<(), BridgeError> {
    if value == 0 {
        return Err(BridgeError::InvalidParameter);
    }
    Ok(())
}

/// Returns `BridgeError::InvalidParameter` if the value is zero.
pub fn require_non_zero_u64(value: u64) -> Result<(), BridgeError> {
    if value == 0 {
        return Err(BridgeError::InvalidParameter);
    }
    Ok(())
}

/// Returns `BridgeError::InvalidParameter` if the value is zero.
pub fn require_non_zero_u128(value: u128) -> Result<(), BridgeError> {
    if value == 0 {
        return Err(BridgeError::InvalidParameter);
    }
    Ok(())
}

/// Returns `BridgeError::InvalidTimestamp` if the timestamp is not in the future.
pub fn require_future_timestamp(timestamp: u64, now: u64) -> Result<(), BridgeError> {
    if timestamp <= now {
        return Err(BridgeError::InvalidTimestamp);
    }
    Ok(())
}
