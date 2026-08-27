//! Two-step, time-locked admin transfer shared by every Soroban contract in
//! this repository.
//!
//! This is the Soroban counterpart of `src/OwnershipTransfer.sol`. The Solidity
//! layer already establishes the rule that ownership never moves in a single
//! call: the current owner *nominates* a successor, and the successor must
//! *accept*. A typo, a contract that cannot sign, or an address on the wrong
//! network therefore cannot strand the contract — the nomination simply never
//! gets accepted and the incumbent stays in place.
//!
//! Two properties are added on top of the Solidity pattern, both of which the
//! Soroban contracts need because their admin is the only route to every
//! parameter change the system has:
//!
//! * **Time lock.** A nomination only becomes acceptable after `delay_seconds`
//!   have elapsed. The pending record is public for that whole window, so a
//!   stolen admin key cannot silently rotate itself out from under the
//!   operators — they can see the nomination and call [`cancel_transfer`]
//!   before it becomes acceptable.
//! * **Acceptance deadline.** A nomination stops being acceptable
//!   [`ACCEPTANCE_WINDOW_SECONDS`] after it becomes eligible, so a forgotten
//!   nomination cannot be revived years later by whoever still controls that
//!   key.
//!
//! Every function here is generic over the caller's storage-key type, so each
//! contract keeps its own `DataKey` enum and simply passes the two variants it
//! wants used:
//!
//! ```ignore
//! pub fn transfer_admin(env: Env, new_admin: Address, delay_seconds: u64) -> Result<(), AdminError> {
//!     admin::propose_transfer(&env, &DataKey::Admin, &DataKey::PendingAdmin, new_admin, delay_seconds)
//! }
//! ```
//!
//! Note on error codes: [`AdminError`] deliberately numbers its variants from
//! 900 so they never collide with the per-contract error enums, which all start
//! at 1. A caller that sees `Error(Contract, #903)` knows it came from the
//! shared admin module regardless of which contract it called.

use soroban_sdk::{contracterror, contracttype, symbol_short, Address, Env, IntoVal, Val};

/// Shortest time lock a nomination may carry.
///
/// Zero is deliberately not allowed: a nomination that can be accepted in the
/// same ledger it was made is a one-step transfer wearing a two-step costume,
/// and it removes the window in which a compromised key's nomination can be
/// spotted and cancelled. One hour is short enough to rotate a key in an
/// incident and long enough to be noticed.
pub const MIN_TRANSFER_DELAY_SECONDS: u64 = 60 * 60;

/// Longest time lock a nomination may carry. Bounds `eligible_at` so a
/// nomination cannot be parked in storage indefinitely.
pub const MAX_TRANSFER_DELAY_SECONDS: u64 = 30 * 24 * 60 * 60;

/// How long a nomination stays acceptable once its time lock has elapsed.
pub const ACCEPTANCE_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60;

/// A nomination awaiting acceptance.
///
/// Stored under the caller's pending-admin key and readable by anyone, which is
/// the point: the whole security value of the time lock is that the nomination
/// is visible before it can take effect.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAdmin {
    /// Address nominated to become the new admin.
    pub new_admin: Address,
    /// Admin that made the nomination, recorded for audit — the admin may have
    /// changed by the time anyone reads this.
    pub proposed_by: Address,
    /// Ledger timestamp at which the nomination was made.
    pub proposed_at: u64,
    /// First timestamp at which `new_admin` may accept.
    pub eligible_at: u64,
    /// Last timestamp at which `new_admin` may accept.
    pub expires_at: u64,
}

/// Failure reasons for the shared admin-transfer flow.
///
/// Numbered from 900 to stay clear of every per-contract error enum.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AdminError {
    /// No admin is stored — the contract was never initialized.
    NotInitialized = 900,
    /// There is no nomination to accept or cancel.
    NoPendingTransfer = 901,
    /// The nominated address is the current admin, so the transfer is a no-op.
    SameAsCurrentAdmin = 902,
    /// `delay_seconds` is below [`MIN_TRANSFER_DELAY_SECONDS`].
    DelayTooShort = 903,
    /// `delay_seconds` is above [`MAX_TRANSFER_DELAY_SECONDS`].
    DelayTooLong = 904,
    /// The time lock has not elapsed yet.
    TimeLockActive = 905,
    /// The acceptance window has closed; the admin must nominate again.
    TransferExpired = 906,
}

/// Read the stored admin.
pub fn read_admin<K>(env: &Env, admin_key: &K) -> Result<Address, AdminError>
where
    K: IntoVal<Env, Val>,
{
    env.storage()
        .instance()
        .get(admin_key)
        .ok_or(AdminError::NotInitialized)
}

/// Read the pending nomination, if any.
///
/// A returned record is not necessarily still acceptable — compare
/// `eligible_at` and `expires_at` against the current ledger timestamp.
pub fn read_pending<K>(env: &Env, pending_key: &K) -> Option<PendingAdmin>
where
    K: IntoVal<Env, Val>,
{
    env.storage().instance().get(pending_key)
}

/// Nominate `new_admin` as the successor, acceptable after `delay_seconds`.
///
/// Requires authorization from the current admin. A nomination made while
/// another is outstanding replaces it and restarts the time lock, which is how
/// an admin corrects a mistake without a separate cancel.
///
/// The admin is *not* changed here. It changes only in [`accept_transfer`].
pub fn propose_transfer<K>(
    env: &Env,
    admin_key: &K,
    pending_key: &K,
    new_admin: Address,
    delay_seconds: u64,
) -> Result<(), AdminError>
where
    K: IntoVal<Env, Val>,
{
    let current = read_admin(env, admin_key)?;
    current.require_auth();

    if new_admin == current {
        return Err(AdminError::SameAsCurrentAdmin);
    }
    if delay_seconds < MIN_TRANSFER_DELAY_SECONDS {
        return Err(AdminError::DelayTooShort);
    }
    if delay_seconds > MAX_TRANSFER_DELAY_SECONDS {
        return Err(AdminError::DelayTooLong);
    }

    let now = env.ledger().timestamp();
    // Both delays are bounded above, so these additions only overflow on a
    // ledger timestamp near u64::MAX. Checked rather than wrapped, because a
    // wrapped `eligible_at` would land in the past and defeat the time lock.
    let eligible_at = now
        .checked_add(delay_seconds)
        .ok_or(AdminError::DelayTooLong)?;
    let expires_at = eligible_at
        .checked_add(ACCEPTANCE_WINDOW_SECONDS)
        .ok_or(AdminError::DelayTooLong)?;

    let pending = PendingAdmin {
        new_admin: new_admin.clone(),
        proposed_by: current.clone(),
        proposed_at: now,
        eligible_at,
        expires_at,
    };
    env.storage().instance().set(pending_key, &pending);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("xfer_prop")),
        (current, new_admin, eligible_at, expires_at),
    );

    Ok(())
}

/// Accept an outstanding nomination and become the admin.
///
/// Requires authorization from the nominated address — this is the step that
/// proves the successor exists and can sign, which is the entire reason the
/// transfer is two-step.
pub fn accept_transfer<K>(env: &Env, admin_key: &K, pending_key: &K) -> Result<(), AdminError>
where
    K: IntoVal<Env, Val>,
{
    let pending = read_pending(env, pending_key).ok_or(AdminError::NoPendingTransfer)?;
    pending.new_admin.require_auth();

    let now = env.ledger().timestamp();
    if now < pending.eligible_at {
        return Err(AdminError::TimeLockActive);
    }
    if now > pending.expires_at {
        return Err(AdminError::TransferExpired);
    }

    let previous = read_admin(env, admin_key)?;
    env.storage().instance().set(admin_key, &pending.new_admin);
    env.storage().instance().remove(pending_key);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("xfer_ok")),
        (previous, pending.new_admin),
    );

    Ok(())
}

/// Withdraw an outstanding nomination.
///
/// Requires authorization from the current admin. This is the lever the time
/// lock exists to make usable: as long as the nomination has not been accepted,
/// the incumbent can revoke it.
pub fn cancel_transfer<K>(env: &Env, admin_key: &K, pending_key: &K) -> Result<(), AdminError>
where
    K: IntoVal<Env, Val>,
{
    let current = read_admin(env, admin_key)?;
    current.require_auth();

    let pending = read_pending(env, pending_key).ok_or(AdminError::NoPendingTransfer)?;
    env.storage().instance().remove(pending_key);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("xfer_cncl")),
        (current, pending.new_admin),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger as _},
    };

    /// A stand-in for the `DataKey` enum each real contract defines: the module
    /// under test never sees the key type, only the two variants it is handed.
    #[contracttype]
    #[derive(Clone)]
    pub enum DataKey {
        Admin,
        PendingAdmin,
    }

    #[contract]
    pub struct AdminHarness;

    #[contractimpl]
    impl AdminHarness {
        pub fn initialize(env: Env, admin: Address) {
            env.storage().instance().set(&DataKey::Admin, &admin);
        }

        pub fn transfer_admin(
            env: Env,
            new_admin: Address,
            delay_seconds: u64,
        ) -> Result<(), AdminError> {
            propose_transfer(
                &env,
                &DataKey::Admin,
                &DataKey::PendingAdmin,
                new_admin,
                delay_seconds,
            )
        }

        pub fn accept_admin(env: Env) -> Result<(), AdminError> {
            accept_transfer(&env, &DataKey::Admin, &DataKey::PendingAdmin)
        }

        pub fn cancel_admin_transfer(env: Env) -> Result<(), AdminError> {
            cancel_transfer(&env, &DataKey::Admin, &DataKey::PendingAdmin)
        }

        pub fn get_admin(env: Env) -> Result<Address, AdminError> {
            read_admin(&env, &DataKey::Admin)
        }

        pub fn get_pending_admin(env: Env) -> Option<PendingAdmin> {
            read_pending(&env, &DataKey::PendingAdmin)
        }
    }

    struct Fixture {
        env: Env,
        contract_id: Address,
        admin: Address,
        successor: Address,
    }

    impl Fixture {
        fn client(&self) -> AdminHarnessClient {
            AdminHarnessClient::new(&self.env, &self.contract_id)
        }
    }

    fn setup() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, AdminHarness);
        let admin = Address::generate(&env);
        let successor = Address::generate(&env);
        AdminHarnessClient::new(&env, &contract_id).initialize(&admin);

        Fixture {
            env,
            contract_id,
            admin,
            successor,
        }
    }

    fn set_timestamp(env: &Env, timestamp: u64) {
        env.ledger().with_mut(|l| l.timestamp = timestamp);
    }

    #[test]
    fn nomination_does_not_change_the_admin() {
        let f = setup();
        f.client()
            .transfer_admin(&f.successor, &MIN_TRANSFER_DELAY_SECONDS);

        // The whole point of the two-step flow: proposing is not transferring.
        assert_eq!(f.client().get_admin(), f.admin);

        let pending = f.client().get_pending_admin().unwrap();
        assert_eq!(pending.new_admin, f.successor);
        assert_eq!(pending.proposed_by, f.admin);
        assert_eq!(pending.eligible_at, MIN_TRANSFER_DELAY_SECONDS);
        assert_eq!(
            pending.expires_at,
            MIN_TRANSFER_DELAY_SECONDS + ACCEPTANCE_WINDOW_SECONDS
        );
    }

    #[test]
    fn acceptance_after_the_time_lock_moves_the_admin() {
        let f = setup();
        f.client()
            .transfer_admin(&f.successor, &MIN_TRANSFER_DELAY_SECONDS);

        set_timestamp(&f.env, MIN_TRANSFER_DELAY_SECONDS);
        f.client().accept_admin();

        assert_eq!(f.client().get_admin(), f.successor);
        // The nomination is consumed, so it cannot be replayed.
        assert_eq!(f.client().get_pending_admin(), None);
    }

    #[test]
    fn acceptance_before_the_time_lock_is_rejected() {
        let f = setup();
        f.client()
            .transfer_admin(&f.successor, &MIN_TRANSFER_DELAY_SECONDS);

        set_timestamp(&f.env, MIN_TRANSFER_DELAY_SECONDS - 1);
        assert_eq!(
            f.client().try_accept_admin(),
            Err(Ok(AdminError::TimeLockActive))
        );
        assert_eq!(f.client().get_admin(), f.admin);
    }

    #[test]
    fn acceptance_after_the_window_is_rejected() {
        let f = setup();
        f.client()
            .transfer_admin(&f.successor, &MIN_TRANSFER_DELAY_SECONDS);

        set_timestamp(
            &f.env,
            MIN_TRANSFER_DELAY_SECONDS + ACCEPTANCE_WINDOW_SECONDS + 1,
        );
        assert_eq!(
            f.client().try_accept_admin(),
            Err(Ok(AdminError::TransferExpired))
        );
        assert_eq!(f.client().get_admin(), f.admin);
    }

    #[test]
    fn cancelling_withdraws_the_nomination() {
        let f = setup();
        f.client()
            .transfer_admin(&f.successor, &MIN_TRANSFER_DELAY_SECONDS);
        f.client().cancel_admin_transfer();

        assert_eq!(f.client().get_pending_admin(), None);

        // A cancelled nomination is not acceptable even once its original time
        // lock would have elapsed.
        set_timestamp(&f.env, MIN_TRANSFER_DELAY_SECONDS);
        assert_eq!(
            f.client().try_accept_admin(),
            Err(Ok(AdminError::NoPendingTransfer))
        );
        assert_eq!(f.client().get_admin(), f.admin);
    }

    #[test]
    fn a_second_nomination_replaces_the_first() {
        let f = setup();
        let other = Address::generate(&f.env);

        f.client()
            .transfer_admin(&f.successor, &MIN_TRANSFER_DELAY_SECONDS);
        f.client().transfer_admin(&other, &MIN_TRANSFER_DELAY_SECONDS);

        let pending = f.client().get_pending_admin().unwrap();
        assert_eq!(pending.new_admin, other);

        set_timestamp(&f.env, MIN_TRANSFER_DELAY_SECONDS);
        f.client().accept_admin();
        assert_eq!(f.client().get_admin(), other);
    }

    #[test]
    fn the_delay_is_bounded_at_both_ends() {
        let f = setup();

        assert_eq!(
            f.client()
                .try_transfer_admin(&f.successor, &(MIN_TRANSFER_DELAY_SECONDS - 1)),
            Err(Ok(AdminError::DelayTooShort))
        );
        assert_eq!(
            f.client()
                .try_transfer_admin(&f.successor, &(MAX_TRANSFER_DELAY_SECONDS + 1)),
            Err(Ok(AdminError::DelayTooLong))
        );
        assert_eq!(f.client().get_pending_admin(), None);
    }

    #[test]
    fn nominating_the_current_admin_is_rejected() {
        let f = setup();
        assert_eq!(
            f.client()
                .try_transfer_admin(&f.admin, &MIN_TRANSFER_DELAY_SECONDS),
            Err(Ok(AdminError::SameAsCurrentAdmin))
        );
    }

    #[test]
    fn cancelling_with_nothing_pending_is_rejected() {
        let f = setup();
        assert_eq!(
            f.client().try_cancel_admin_transfer(),
            Err(Ok(AdminError::NoPendingTransfer))
        );
    }

    #[test]
    fn the_new_admin_can_hand_the_role_on_again() {
        let f = setup();
        let third = Address::generate(&f.env);

        f.client()
            .transfer_admin(&f.successor, &MIN_TRANSFER_DELAY_SECONDS);
        set_timestamp(&f.env, MIN_TRANSFER_DELAY_SECONDS);
        f.client().accept_admin();

        // Rotation is repeatable — the successor holds exactly the same powers.
        f.client().transfer_admin(&third, &MIN_TRANSFER_DELAY_SECONDS);
        set_timestamp(&f.env, MIN_TRANSFER_DELAY_SECONDS * 2);
        f.client().accept_admin();

        assert_eq!(f.client().get_admin(), third);
    }
}
