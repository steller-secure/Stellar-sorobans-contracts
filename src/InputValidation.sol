// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./ContractErrors.sol";

/// @title InputValidation
/// @notice Demonstrates strict input type constraints to prevent invalid state transitions.
/// @dev All custom errors are imported from ContractErrors for consistency (#50).
contract InputValidation {
    mapping(address => uint256) public balances;

    event Transfer(address indexed from, address indexed to, uint256 amount);

    /// @notice Transfer tokens with strict input constraints.
    /// @param to  Recipient address — must not be the zero address.
    /// @param amount  Amount to transfer — must be > 0 and fit within uint128.
    function transfer(address to, uint256 amount) external {
        if (to == address(0)) revert Errors.InvalidAddress("to");
        if (amount == 0) revert Errors.ZeroValue("amount");
        if (amount > type(uint128).max) {
            revert Errors.AmountTooLarge(amount, type(uint128).max);
        }
        if (balances[msg.sender] < amount) {
            revert Errors.AmountTooSmall(balances[msg.sender], amount);
        }

        balances[msg.sender] -= amount;
        balances[to] += amount;

        emit Transfer(msg.sender, to, amount);
    }

    /// @notice Deposit ether to receive token balance (1:1 for demo purposes).
    function deposit() external payable {
        if (msg.value == 0) revert Errors.ZeroValue("msg.value");
        balances[msg.sender] += msg.value;
    }
}
