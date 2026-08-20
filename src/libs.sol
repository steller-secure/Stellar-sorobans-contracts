// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./ContractErrors.sol";
import "./SecurityEvents.sol";

/// @title  GovernanceManager
/// @notice Executes governance proposals with replay protection.
/// @dev    Inherits SecurityEvents for audit-trail emission.
///         All custom errors are imported from ContractErrors for consistency
///         (#50) — this is the canonical GovernanceManager; see src/lib.rs for
///         the duplicate that predates this convergence.
contract GovernanceManager is SecurityEvents {

    // -------------------------------------------------------------------------
    // Storage
    // -------------------------------------------------------------------------

    /// @dev Tracks executed proposal IDs.
    mapping(bytes32 => bool) private _executedProposals;

    // -------------------------------------------------------------------------
    // External functions
    // -------------------------------------------------------------------------

    /// @notice Marks a governance proposal as executed and emits the action
    ///         for audit tracking.
    /// @dev    Reverts with ProposalAlreadyExecuted (defined in ContractErrors)
    ///         when the proposal ID has already been executed, preventing
    ///         replayed actions.
    /// @param  proposalId  Unique identifier for the proposal.
    /// @param  action      Human-readable description of the governance action.
    function executeProposal(
        bytes32 proposalId,
        string calldata action
    ) external {
        // Centralized error — no more inline require strings
        if (_executedProposals[proposalId]) {
            revert Errors.ProposalAlreadyExecuted(proposalId);
        }

        _executedProposals[proposalId] = true;

        emit GovernanceActionExecuted(
            msg.sender,
            proposalId,
            action,
            block.timestamp
        );
    }

    // -------------------------------------------------------------------------
    // View helpers
    // -------------------------------------------------------------------------

    /// @notice Returns whether a proposal has already been executed.
    function isExecuted(bytes32 proposalId) external view returns (bool) {
        return _executedProposals[proposalId];
    }
}
