#![no_std]

mod migration;
mod migration_framework;
mod storage;
mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, String, Vec};

use storage::{DataKey, MAX_HISTORY_ITEMS};
use types::{
    BridgeConfig, BridgeError, BridgeOperationStatus, BridgeTransaction, ChainBridgeInfo,
    MultisigBridgeRequest, PropertyMetadata, RecoveryAction,
};
use validation::{
    require_admin, require_future_timestamp, require_non_zero_address, require_non_zero_u128,
    require_non_zero_u32, require_non_zero_u64, require_not_paused, require_operator,
    require_supported_chain, require_valid_signatures,
};

const CONTRACT_VERSION: u32 = 1;
const MAX_SUPPORTED_CHAINS: u32 = 20;
const MAX_OPERATORS: u32 = 10;

#[contract]
pub struct PropertyBridge;

#[contractimpl]
impl PropertyBridge {
    pub fn init(
        env: Env,
        admin: Address,
        supported_chains: Vec<u32>,
        min_signatures: u32,
        max_signatures: u32,
        default_timeout: u64,
        gas_limit: u64,
        service_fee: i128,
        fee_token: Address,
        fee_recipient: Address,
    ) -> Result<(), BridgeError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(BridgeError::AlreadyInitialized);
        }
        require_non_zero_address(&admin)?;
        require_non_zero_address(&fee_token)?;
        require_non_zero_address(&fee_recipient)?;
        if supported_chains.is_empty() {
            return Err(BridgeError::InvalidConfig);
        }
        require_non_zero_u32(min_signatures)?;
        require_non_zero_u32(max_signatures)?;
        require_non_zero_u64(default_timeout)?;
        require_non_zero_u64(gas_limit)?;

        if supported_chains.len() > MAX_SUPPORTED_CHAINS {
            return Err(BridgeError::InvalidConfig);
        }
        for chain_id in supported_chains.iter() {
            require_non_zero_u32(chain_id)?;
        }
        if min_signatures > max_signatures {
            return Err(BridgeError::InvalidConfig);
        }

        let config = BridgeConfig {
            supported_chains: supported_chains.clone(),
            min_signatures_required: min_signatures,
            max_signatures_required: max_signatures,
            default_timeout_seconds: default_timeout,
            gas_limit_per_bridge: gas_limit,
            emergency_pause: false,
            metadata_preservation: true,
            service_fee,
            fee_token,
            fee_recipient,
        };

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Version, &CONTRACT_VERSION);
        env.storage().instance().set(&DataKey::ReqCounter, &0u64);
        env.storage().instance().set(&DataKey::TxCounter, &0u64);

        let mut operators = Vec::new(&env);
        operators.push_back(admin.clone());
        env.storage().instance().set(&DataKey::Operators, &operators);

        for chain_id in supported_chains.iter() {
            let chain_info = ChainBridgeInfo {
                chain_id,
                chain_name: String::from_str(&env, "Chain"),
                bridge_contract_address: String::from_str(&env, ""),
                is_active: true,
                gas_multiplier: 100,
                confirmation_blocks: 6,
                supported_tokens: Vec::new(&env),
            };
            env.storage()
                .persistent()
                .set(&DataKey::ChainInfo(chain_id), &chain_info);
        }

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("init")),
            (admin, min_signatures, max_signatures),
        );

        Ok(())
    }

    pub fn initiate_bridge_multisig(
        env: Env,
        caller: Address,
        token_id: u64,
        destination_chain: u32,
        recipient: Address,
        required_signatures: u32,
        timeout_seconds: Option<u64>,
        metadata: PropertyMetadata,
        nonce: u64,
    ) -> Result<u64, BridgeError> {
        caller.require_auth();
        require_non_zero_address(&caller)?;
        require_non_zero_address(&recipient)?;
        require_non_zero_u64(token_id)?;
        require_non_zero_u32(required_signatures)?;
        require_non_zero_u64(metadata.size)?;
        require_non_zero_u128(metadata.valuation)?;
        if let Some(seconds) = timeout_seconds {
            require_non_zero_u64(seconds)?;
        }

        let current_nonce: u64 = env.storage().persistent().get(&DataKey::Nonce(caller.clone())).unwrap_or(0);
        if nonce != current_nonce + 1 {
            return Err(BridgeError::InvalidNonce);
        }
        env.storage().persistent().set(&DataKey::Nonce(caller.clone()), &nonce);

        let config: BridgeConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(BridgeError::NotInitialized)?;

        require_not_paused(&env)?;
        require_supported_chain(&config, destination_chain)?;
        require_valid_signatures(&config, required_signatures)?;

        let mut counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ReqCounter)
            .unwrap_or(0);
        counter += 1;
        env.storage().instance().set(&DataKey::ReqCounter, &counter);

        if config.service_fee > 0 {
            use soroban_sdk::token;
            let client = token::Client::new(&env, &config.fee_token);
            client.transfer(&caller, &env.current_contract_address(), &config.service_fee);
            env.storage()
                .persistent()
                .set(&DataKey::FeeEscrow(counter), &config.service_fee);
        }

        let now = env.ledger().timestamp();
        let expires_at = timeout_seconds.map(|s| now + s);

        if let Some(expiry) = expires_at {
            require_future_timestamp(expiry, now)?;
        }

        let request = MultisigBridgeRequest {
            request_id: counter,
            token_id,
            source_chain: 1,
            destination_chain,
            sender: caller.clone(),
            recipient,
            required_signatures,
            signatures: Vec::new(&env),
            rejections: Vec::new(&env),
            created_at: now,
            expires_at,
            status: BridgeOperationStatus::Pending,
            metadata,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Request(counter), &request);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("created")),
            (counter, token_id, caller),
        );

        Ok(counter)
    }

    pub fn sign_bridge_request(env: Env, operator: Address, request_id: u64, approve: bool) -> Result<(), BridgeError> {
        operator.require_auth();
        require_non_zero_address(&operator)?;
        require_non_zero_u64(request_id)?;
        require_operator(&env, &operator)?;
        require_not_paused(&env)?;

        let mut request: MultisigBridgeRequest = env
            .storage()
            .persistent()
            .get(&DataKey::Request(request_id))
            .ok_or(BridgeError::RequestNotFound)?;

        if request.status != BridgeOperationStatus::Pending {
            return Err(BridgeError::RequestNotPending);
        }

        if let Some(expires_at) = request.expires_at {
            if env.ledger().timestamp() > expires_at {
                return Err(BridgeError::RequestExpired);
            }
        }

        if request.signatures.contains(operator.clone()) || request.rejections.contains(operator.clone()) {
            return Err(BridgeError::AlreadySigned);
        }

        if approve {
            request.signatures.push_back(operator.clone());
            if request.signatures.len() >= request.required_signatures {
                request.status = BridgeOperationStatus::Locked;
            }
        } else {
            request.rejections.push_back(operator.clone());
            if request.rejections.len() >= request.required_signatures {
                request.status = BridgeOperationStatus::Failed;
            }
        }

        env.storage()
            .persistent()
            .set(&DataKey::Request(request_id), &request);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("signed")),
            (request_id, operator, approve),
        );

        Ok(())
    }

    pub fn execute_bridge(env: Env, operator: Address, request_id: u64) -> Result<(), BridgeError> {
        operator.require_auth();
        require_non_zero_address(&operator)?;
        require_non_zero_u64(request_id)?;
        require_operator(&env, &operator)?;
        require_not_paused(&env)?;

        let mut request: MultisigBridgeRequest = env
            .storage()
            .persistent()
            .get(&DataKey::Request(request_id))
            .ok_or(BridgeError::RequestNotFound)?;

        if request.status != BridgeOperationStatus::Locked {
            return Err(BridgeError::RequestNotReady);
        }

        let tx_hash = env
            .crypto()
            .sha256(&Bytes::from_slice(&env, &request_id.to_be_bytes()));

        let mut tx_counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TxCounter)
            .unwrap_or(0);
        tx_counter += 1;
        env.storage().instance().set(&DataKey::TxCounter, &tx_counter);

        let sender = request.sender.clone();

        let transaction = BridgeTransaction {
            transaction_id: tx_counter,
            token_id: request.token_id,
            source_chain: request.source_chain,
            destination_chain: request.destination_chain,
            sender: sender.clone(),
            recipient: request.recipient.clone(),
            transaction_hash: tx_hash.clone(),
            timestamp: env.ledger().timestamp(),
            gas_used: 0,
            status: BridgeOperationStatus::InTransit,
            metadata: request.metadata.clone(),
        };

        request.status = BridgeOperationStatus::Completed;
        env.storage()
            .persistent()
            .set(&DataKey::Request(request_id), &request);

        if let Some(fee) = env.storage().persistent().get::<_, i128>(&DataKey::FeeEscrow(request_id)) {
            if fee > 0 {
                let config: BridgeConfig = env
                    .storage()
                    .instance()
                    .get(&DataKey::Config)
                    .ok_or(BridgeError::NotInitialized)?;
                use soroban_sdk::token;
                let client = token::Client::new(&env, &config.fee_token);
                client.transfer(&env.current_contract_address(), &config.fee_recipient, &fee);
            }
            env.storage().persistent().remove(&DataKey::FeeEscrow(request_id));
        }

        env.storage()
            .persistent()
            .set(&DataKey::VerifiedTx(tx_hash.clone()), &true);

        let mut history: Vec<BridgeTransaction> = env
            .storage()
            .persistent()
            .get(&DataKey::History(sender.clone()))
            .unwrap_or(Vec::new(&env));

        if history.len() >= MAX_HISTORY_ITEMS {
            history.remove(0);
        }
        history.push_back(transaction);
        env.storage()
            .persistent()
            .set(&DataKey::History(sender), &history);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("executed")),
            (request_id, tx_hash),
        );

        Ok(())
    }

    pub fn recover_failed_bridge(
        env: Env,
        admin: Address,
        request_id: u64,
        recovery_action: RecoveryAction,
    ) -> Result<(), BridgeError> {
        admin.require_auth();
        require_non_zero_address(&admin)?;
        require_non_zero_u64(request_id)?;
        require_admin(&env, &admin)?;
        require_not_paused(&env)?;

        let mut request: MultisigBridgeRequest = env
            .storage()
            .persistent()
            .get(&DataKey::Request(request_id))
            .ok_or(BridgeError::RequestNotFound)?;

        if !matches!(
            request.status,
            BridgeOperationStatus::Failed | BridgeOperationStatus::Expired
        ) {
            return Err(BridgeError::RequestNotFailed);
        }

        if let Some(fee) = env.storage().persistent().get::<_, i128>(&DataKey::FeeEscrow(request_id)) {
            if fee > 0 {
                let config: BridgeConfig = env
                    .storage()
                    .instance()
                    .get(&DataKey::Config)
                    .ok_or(BridgeError::NotInitialized)?;
                use soroban_sdk::token;
                let client = token::Client::new(&env, &config.fee_token);
                client.transfer(&env.current_contract_address(), &request.sender, &fee);
            }
            env.storage().persistent().remove(&DataKey::FeeEscrow(request_id));
        }

        match recovery_action {
            RecoveryAction::RetryBridge => {
                request.status = BridgeOperationStatus::Pending;
                request.signatures = Vec::new(&env);
                request.rejections = Vec::new(&env);
            }
            RecoveryAction::CancelBridge
            | RecoveryAction::UnlockToken
            | RecoveryAction::RefundGas => {
                request.status = BridgeOperationStatus::Failed;
            }
        }

        env.storage()
            .persistent()
            .set(&DataKey::Request(request_id), &request);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("recover")),
            request_id,
        );

        Ok(())
    }

    pub fn set_pause(env: Env, admin: Address, paused: bool) -> Result<(), BridgeError> {
        admin.require_auth();
        require_non_zero_address(&admin)?;
        require_admin(&env, &admin)?;

        let mut config: BridgeConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(BridgeError::NotInitialized)?;
        config.emergency_pause = paused;
        env.storage().instance().set(&DataKey::Config, &config);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("pause")),
            paused,
        );

        Ok(())
    }

    pub fn add_operator(env: Env, admin: Address, operator: Address) -> Result<(), BridgeError> {
        admin.require_auth();
        require_non_zero_address(&admin)?;
        require_non_zero_address(&operator)?;
        require_admin(&env, &admin)?;

        let mut operators: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Operators)
            .ok_or(BridgeError::NotInitialized)?;

        if operators.len() >= MAX_OPERATORS {
            return Err(BridgeError::TooManyOperators);
        }

        if !operators.contains(operator.clone()) {
            operators.push_back(operator.clone());
            env.storage().instance().set(&DataKey::Operators, &operators);

            env.events().publish(
                (symbol_short!("bridge"), symbol_short!("opadd")),
                operator,
            );
        }

        Ok(())
    }

    pub fn remove_operator(env: Env, admin: Address, operator: Address) -> Result<(), BridgeError> {
        admin.require_auth();
        require_non_zero_address(&admin)?;
        require_non_zero_address(&operator)?;
        require_admin(&env, &admin)?;

        let operators: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Operators)
            .ok_or(BridgeError::NotInitialized)?;

        let config: BridgeConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(BridgeError::NotInitialized)?;

        if operators.len() <= config.min_signatures_required {
            return Err(BridgeError::OperatorRemovalWouldBreakQuorum);
        }

        let mut new_operators = Vec::new(&env);
        for op in operators.iter() {
            if op != operator {
                new_operators.push_back(op);
            }
        }
        env.storage().instance().set(&DataKey::Operators, &new_operators);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("oprm")),
            operator,
        );

        Ok(())
    }
}

#[contractimpl]
impl PropertyBridge {
    pub fn version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(CONTRACT_VERSION)
    }

    pub fn get_config(env: Env) -> Result<BridgeConfig, BridgeError> {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(BridgeError::NotInitialized)
    }

    pub fn get_admin(env: Env) -> Result<Address, BridgeError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BridgeError::NotInitialized)
    }

    pub fn get_request(env: Env, request_id: u64) -> Option<MultisigBridgeRequest> {
        env.storage().persistent().get(&DataKey::Request(request_id))
    }

    pub fn get_history(env: Env, address: Address) -> Vec<BridgeTransaction> {
        env.storage()
            .persistent()
            .get(&DataKey::History(address))
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_chain_info(env: Env, chain_id: u32) -> Option<ChainBridgeInfo> {
        env.storage().persistent().get(&DataKey::ChainInfo(chain_id))
    }

    pub fn is_operator(env: Env, address: Address) -> bool {
        let operators: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Operators)
            .unwrap_or(Vec::new(&env));
        operators.contains(address)
    }

    pub fn get_nonce(env: Env, address: Address) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::Nonce(address))
            .unwrap_or(0)
    }
}
