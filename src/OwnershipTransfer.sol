// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./ContractErrors.sol";

/// @title OwnershipTransfer
/// @notice Two-step ownership transfer to prevent accidental loss of ownership.
/// @dev All custom errors are imported from ContractErrors for consistency (#50).
contract OwnershipTransfer {
    address public owner;
    address public pendingOwner;

    event OwnershipTransferInitiated(address indexed currentOwner, address indexed pendingOwner);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    constructor() {
        owner = msg.sender;
    }

    modifier onlyOwner() {
        if (msg.sender != owner) revert Errors.Unauthorized(msg.sender);
        _;
    }

    /// @notice Initiate an ownership transfer to `newOwner`.
    ///         The new owner must call `acceptOwnership()` to confirm.
    /// @param newOwner The address nominated to become the new owner.
    function transferOwnership(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert Errors.InvalidNewOwner();
        pendingOwner = newOwner;
        emit OwnershipTransferInitiated(owner, newOwner);
    }

    /// @notice Confirm ownership transfer. Must be called by the pending owner.
    function acceptOwnership() external {
        if (msg.sender != pendingOwner) revert Errors.NotPendingOwner(msg.sender);
        address previous = owner;
        owner = pendingOwner;
        pendingOwner = address(0);
        emit OwnershipTransferred(previous, owner);
    }
}
