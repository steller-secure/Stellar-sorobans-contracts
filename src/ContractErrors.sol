// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title  ContractErrors
/// @notice Single source of truth for all custom errors across the contract suite.
/// @dev    Import this file in every contract that needs to revert with a typed
///         error.  Using custom errors (introduced in Solidity 0.8.4) saves gas
///         vs. require() strings and makes errors ABI-queryable.
///
///         Error code domains:
///           Governance     — proposal lifecycle
///           Ownership      — access control & transfers
///           Initialization — setup guards
///           InputValidation— parameter checks
///           Fallback       — fallback handler
///           Storage        — state integrity
///           Insurance      — policy & claims logic

library Errors {

    // -------------------------------------------------------------------------
    // Governance
    // -------------------------------------------------------------------------

    /// @dev Thrown when executeProposal() is called with an already-executed ID.
    /// @param proposalId The duplicate proposal identifier.
    error ProposalAlreadyExecuted(bytes32 proposalId);

    /// @dev Thrown when a proposal ID does not exist.
    error ProposalNotFound(bytes32 proposalId);

    // -------------------------------------------------------------------------
    // Ownership & Access Control
    // -------------------------------------------------------------------------

    /// @dev Thrown when msg.sender is not the contract owner.
    error Unauthorized(address caller);

    /// @dev Thrown when the proposed new owner is the zero address.
    error InvalidNewOwner();

    /// @dev Thrown when there is no pending ownership transfer to accept.
    error NoPendingTransfer();

    /// @dev Thrown when the pending transfer deadline has passed.
    error TransferExpired();

    /// @dev Thrown when msg.sender is not the pending owner.
    error NotPendingOwner(address caller);

    // -------------------------------------------------------------------------
    // Initialization
    // -------------------------------------------------------------------------

    /// @dev Thrown when initialize() is called on an already-initialized contract.
    error AlreadyInitialized();

    /// @dev Thrown when a function requires the contract to be initialized first.
    error NotInitialized();

    // -------------------------------------------------------------------------
    // Input Validation
    // -------------------------------------------------------------------------

    /// @dev Thrown when a required value is zero.
    /// @param paramName  Human-readable parameter name for off-chain tooling.
    error ZeroValue(string paramName);

    /// @dev Thrown when an amount exceeds the allowed maximum.
    error AmountTooLarge(uint256 provided, uint256 maximum);

    /// @dev Thrown when an amount is below the required minimum.
    error AmountTooSmall(uint256 provided, uint256 minimum);

    /// @dev Thrown when an address parameter is the zero address.
    error InvalidAddress(string paramName);

    /// @dev Thrown when a deadline timestamp is already in the past.
    error DeadlineExpired(uint256 deadline, uint256 currentTime);

    /// @dev Thrown when a numeric parameter is outside the accepted range.
    error OutOfRange(uint256 value, uint256 min, uint256 max);

    // -------------------------------------------------------------------------
    // Fallback Handler
    // -------------------------------------------------------------------------

    /// @dev Thrown when an unrecognized function selector is called.
    error UnknownSelector(bytes4 selector);

    /// @dev Thrown when no fallback handler has been configured.
    error FallbackNotSet();

    /// @dev Thrown when a direct ETH transfer is not permitted.
    error DirectTransferNotAllowed();

    /// @dev Thrown when a plain ETH transfer carries zero value.
    error ZeroValueTransfer();

    /// @dev Thrown when a low-level ETH transfer (`.call{value: ...}`) fails.
    error TransferFailed();

    // -------------------------------------------------------------------------
    // Storage / State
    // -------------------------------------------------------------------------

    /// @dev Thrown when a required storage slot is unexpectedly empty.
    error StorageSlotEmpty(bytes32 slot);

    // -------------------------------------------------------------------------
    // Insurance Logic
    // -------------------------------------------------------------------------

    /// @dev Thrown when a referenced policy does not exist.
    error PolicyNotFound(uint256 policyId);

    /// @dev Thrown when a policy has already been claimed.
    error PolicyAlreadyClaimed(uint256 policyId);

    /// @dev Thrown when a policy has lapsed.
    error PolicyExpired(uint256 policyId, uint256 expiredAt);

    /// @dev Thrown when the claim amount exceeds policy coverage.
    error ClaimExceedsCoverage(uint256 claimed, uint256 coverage);

    /// @dev Thrown when the premium payment is insufficient.
    error InsufficientPremium(uint256 paid, uint256 required);

    /// @dev Thrown when insured event conditions are not satisfied.
    error ConditionsNotMet(uint256 policyId);
}
