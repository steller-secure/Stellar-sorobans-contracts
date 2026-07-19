#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, symbol_short, Symbol};

pub mod migration; // Kept fully consistent with your existing modular tree config

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeConfig {
    pub service_fee: i128,      // Canonical parameter preserved
    pub fee_token: Address,     // Canonical parameter preserved
    pub fee_recipient: Address, // Canonical parameter preserved
}

#[contract]
pub struct BridgeContract;

#[contractimpl]
impl BridgeContract {
    /// Configures the base parameters for cross-chain ledger transfers.
    pub fn configure(env: Env, service_fee: i128, fee_token: Address, fee_recipient: Address) {
        let config = BridgeConfig {
            service_fee,
            fee_token,
            fee_recipient,
        };
        env.storage().instance().set(&symbol_short!("config"), &config);
    }

    /// Returns the live operational config telemetry parameters.
    pub fn get_config(env: Env) -> BridgeConfig {
        env.storage()
            .instance()
            .get(&symbol_short!("config"))
            .expect("Bridge contract not configured")
    }
}