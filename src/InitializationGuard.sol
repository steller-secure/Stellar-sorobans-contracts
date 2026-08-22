// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./ContractErrors.sol";

/// @title InitializationGuard
/// @notice Prevents a contract from being initialized more than once.
/// @dev All custom errors are imported from ContractErrors for consistency (#50).
contract InitializationGuard {
    bool public initialized;
    address public admin;

    event Initialized(address indexed admin);

    /// @dev Reverts if the contract has already been initialized.
    modifier notInitialized() {
        if (initialized) revert Errors.AlreadyInitialized();
        _;
    }

    /// @notice Initialize the contract with an admin address.
    ///         Can only be called once.
    /// @param _admin The address to set as admin.
    function initialize(address _admin) external notInitialized {
        if (_admin == address(0)) revert Errors.InvalidAddress("_admin");
        initialized = true;
        admin = _admin;
        emit Initialized(_admin);
    }
}
