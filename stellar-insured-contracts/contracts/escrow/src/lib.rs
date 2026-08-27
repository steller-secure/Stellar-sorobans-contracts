#![no_std]

mod storage;
mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Vec};
use stellar_insured_lib::admin::{self, AdminError, PendingAdmin};

use storage::DataKey;
use types::{ApprovalType, EscrowData, EscrowError, EscrowStatus, MultiSigConfig};
use validation::{
    get_admin, require_future_timestamp, require_non_zero_address, require_non_zero_u64,
    require_not_paused, require_positive_amount, require_valid_multisig,
};

const CONTRACT_VERSION: u32 = 1;
const MAX_PARTICIPANTS: u32 = 10;

#[contract]
pub struct AdvancedEscrow;

#[contractimpl]
impl AdvancedEscrow {
    pub fn init(env: Env, admin: Address) -> Result<(), EscrowError> {
        require_non_zero_address(&admin)?;
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(EscrowError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Version, &CONTRACT_VERSION);
        env.storage().instance().set(&DataKey::EscrowCount, &0u64);
        env.storage().instance().set(&DataKey::Paused, &false);

        env.events()
            .publish((symbol_short!("escrow"), symbol_short!("init")), admin);

        Ok(())
    }

    pub fn set_pause(env: Env, admin: Address, paused: bool) -> Result<(), EscrowError> {
        admin.require_auth();
        require_non_zero_address(&admin)?;
        // Use shared helper to read admin — one read, no duplication (#351, #353).
        if admin != get_admin(&env)? {
            return Err(EscrowError::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Paused, &paused);

        env.events()
            .publish((symbol_short!("escrow"), symbol_short!("pause")), paused);

        Ok(())
    }

    pub fn create_escrow_advanced(
        env: Env,
        property_id: u64,
        amount: i128,
        buyer: Address,
        seller: Address,
        participants: Vec<Address>,
        required_signatures: u32,
        release_time_lock: Option<u64>,
        nonce: u64,
    ) -> Result<u64, EscrowError> {
        require_not_paused(&env)?;
        require_non_zero_u64(property_id)?;
        require_positive_amount(amount)?;

        // Nonce validation for replay protection (#349)
        let current_nonce: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::Nonce(buyer.clone()))
            .unwrap_or(0);
        if nonce != current_nonce + 1 {
            return Err(EscrowError::InvalidNonce);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Nonce(buyer.clone()), &nonce);

        if participants.len() > MAX_PARTICIPANTS {
            return Err(EscrowError::TooManyParticipants);
        }
        require_valid_multisig(required_signatures, participants.len())?;
        require_non_zero_address(&buyer)?;
        require_non_zero_address(&seller)?;
        for participant in participants.iter() {
            require_non_zero_address(participant)?;
        }
        if let Some(time_lock) = release_time_lock {
            require_future_timestamp(time_lock, env.ledger().timestamp())?;
        }

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCount)
            .unwrap_or(0);
        count += 1;
        env.storage().instance().set(&DataKey::EscrowCount, &count);

        let escrow_data = EscrowData {
            id: count,
            property_id,
            buyer,
            seller,
            amount,
            deposited_amount: 0,
            status: EscrowStatus::Created,
            created_at: env.ledger().timestamp(),
            release_time_lock,
            participants: participants.clone(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(count), &escrow_data);

        let config = MultiSigConfig {
            required_signatures,
            signers: participants,
        };
        env.storage()
            .persistent()
            .set(&DataKey::MultiSig(count), &config);

        // Standardized event format: (contract, action) topics + structured payload (#352).
        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("created")),
            (count, property_id, amount),
        );

        Ok(count)
    }

    pub fn deposit_funds(env: Env, escrow_id: u64, amount: i128) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        require_non_zero_u64(escrow_id)?;
        require_positive_amount(amount)?;

        let mut escrow: EscrowData = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .ok_or(EscrowError::EscrowNotFound)?;

        if escrow.status != EscrowStatus::Created && escrow.status != EscrowStatus::Funded {
            return Err(EscrowError::InvalidStatus);
        }
        let new_deposit_total = escrow
            .deposited_amount
            .checked_add(amount)
            .ok_or(EscrowError::DepositExceedsAmount)?;
        if new_deposit_total > escrow.amount {
            return Err(EscrowError::DepositExceedsAmount);
        }

        escrow.deposited_amount = new_deposit_total;
        escrow.status = if escrow.deposited_amount >= escrow.amount {
            EscrowStatus::Active
        } else {
            EscrowStatus::Funded
        };

        // Single write after all mutations — avoids intermediate writes (#351).
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("funded")),
            (escrow_id, amount),
        );

        Ok(())
    }

    pub fn release_funds(env: Env, escrow_id: u64) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        require_non_zero_u64(escrow_id)?;

        let mut escrow: EscrowData = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .ok_or(EscrowError::EscrowNotFound)?;

        if escrow.status != EscrowStatus::Active {
            return Err(EscrowError::InvalidStatus);
        }

        if let Some(time_lock) = escrow.release_time_lock {
            if env.ledger().timestamp() < time_lock {
                return Err(EscrowError::TimeLockActive);
            }
        }

        let sig_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SigCount(escrow_id, ApprovalType::Release))
            .unwrap_or(0);
        let config: MultiSigConfig = env
            .storage()
            .persistent()
            .get(&DataKey::MultiSig(escrow_id))
            .ok_or(EscrowError::EscrowNotFound)?;

        if sig_count < config.required_signatures {
            return Err(EscrowError::SignatureThresholdNotMet);
        }

        let amount = escrow.deposited_amount;
        escrow.status = EscrowStatus::Released;
        escrow.deposited_amount = 0;
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("released")),
            (escrow_id, amount),
        );

        Ok(())
    }

    pub fn sign_approval(env: Env, escrow_id: u64, approval_type: ApprovalType, signer: Address) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        require_non_zero_u64(escrow_id)?;
        signer.require_auth();
        require_non_zero_address(&signer)?;

        let config: MultiSigConfig = env
            .storage()
            .persistent()
            .get(&DataKey::MultiSig(escrow_id))
            .ok_or(EscrowError::EscrowNotFound)?;

        if !config.signers.contains(signer.clone()) {
            return Err(EscrowError::Unauthorized);
        }

        if env.storage().persistent().has(&DataKey::Signature(
            escrow_id,
            approval_type,
            signer.clone(),
        )) {
            return Err(EscrowError::AlreadySigned);
        }

        env.storage().persistent().set(
            &DataKey::Signature(escrow_id, approval_type, signer.clone()),
            &true,
        );

        // Read-increment-write in one place; no separate read before the set (#351).
        let mut count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SigCount(escrow_id, approval_type))
            .unwrap_or(0);
        count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::SigCount(escrow_id, approval_type), &count);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("signed")),
            (escrow_id, signer, count),
        );

        Ok(())
    }
}

#[contractimpl]
impl AdvancedEscrow {
    pub fn version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(CONTRACT_VERSION)
    }

    pub fn get_admin(env: Env) -> Result<Address, EscrowError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(EscrowError::NotInitialized)
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn get_escrow(env: Env, escrow_id: u64) -> Option<EscrowData> {
        env.storage().persistent().get(&DataKey::Escrow(escrow_id))
    }

    pub fn get_escrow_count(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::EscrowCount).unwrap_or(0)
    }

    pub fn get_multisig_config(env: Env, escrow_id: u64) -> Option<MultiSigConfig> {
        env.storage().persistent().get(&DataKey::MultiSig(escrow_id))
    }

    pub fn get_sig_count(env: Env, escrow_id: u64, approval_type: ApprovalType) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::SigCount(escrow_id, approval_type))
            .unwrap_or(0)
    }

    pub fn get_nonce(env: Env, address: Address) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::Nonce(address))
            .unwrap_or(0)
    }
}

// ─── Admin transfer (two-step, time-locked) ───────────────────────────────────
//
// `set_pause` is admin-gated and the admin was written once by `init` with no
// other writer, so a lost key left the contract permanently pausable-by-nobody.
// These entry points give the role a migration path with the same
// nominate-then-accept shape as `src/OwnershipTransfer.sol`; the mechanics live
// in `stellar_insured_lib::admin`, shared by all seven Soroban contracts.
//
// Unlike the escrow operations, these do not take the admin as a parameter:
// `stellar_insured_lib::admin` authorizes against the admin *stored* in the
// contract, which is the same check `set_pause` performs by hand and cannot be
// passed the wrong address.

#[contractimpl]
impl AdvancedEscrow {
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
