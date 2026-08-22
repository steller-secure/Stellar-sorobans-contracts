// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./ContractErrors.sol";
import "./SecurityEvents.sol";

/// @dev All custom errors are imported from ContractErrors for consistency
///      (#50). This contract duplicates libs.sol's GovernanceManager; see
///      that file for the canonical version.
contract GovernanceManager is SecurityEvents {
    mapping(bytes32 => bool) public executedProposals;

    /// @notice Marks a governance proposal as executed and emits the action for audit tracking.
    /// @dev Reverts when the proposal ID has already been executed to prevent replayed actions.
    function executeProposal(
        bytes32 proposalId,
        string calldata action
    ) external {
        if (executedProposals[proposalId]) {
            revert Errors.ProposalAlreadyExecuted(proposalId);
        }

        executedProposals[proposalId] = true;

        emit GovernanceActionExecuted(
            msg.sender,
            proposalId,
            action,
            block.timestamp
        );
    }
}
