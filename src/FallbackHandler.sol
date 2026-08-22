// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./ContractErrors.sol";

/// @title FallbackHandler
/// @author GreenCrestz Labs
/// @notice Handles ETH transfers and unknown function calls safely while providing detailed logging.
/// @dev Useful for proxy patterns, payment receivers, and debugging unexpected contract interactions.
///      All custom errors are imported from ContractErrors for consistency (#50).
contract FallbackHandler {
    /*//////////////////////////////////////////////////////////////
                                EVENTS
    //////////////////////////////////////////////////////////////*/

    /// @notice Emitted when ETH is received without calldata.
    event EtherReceived(
        address indexed sender,
        uint256 amount,
        uint256 timestamp
    );

    /// @notice Emitted when an unknown function selector is called.
    event UnknownCall(
        address indexed caller,
        bytes4 indexed selector,
        uint256 value,
        bytes data
    );

    /// @notice Emitted when ownership changes.
    event OwnershipTransferred(
        address indexed previousOwner,
        address indexed newOwner
    );

    /*//////////////////////////////////////////////////////////////
                                STORAGE
    //////////////////////////////////////////////////////////////*/

    address public owner;

    /*//////////////////////////////////////////////////////////////
                              MODIFIERS
    //////////////////////////////////////////////////////////////*/

    modifier onlyOwner() {
        if (msg.sender != owner) revert Errors.Unauthorized(msg.sender);
        _;
    }

    /*//////////////////////////////////////////////////////////////
                             CONSTRUCTOR
    //////////////////////////////////////////////////////////////*/

    constructor() {
        owner = msg.sender;

        emit OwnershipTransferred(
            address(0),
            msg.sender
        );
    }

    /*//////////////////////////////////////////////////////////////
                             RECEIVE LOGIC
    //////////////////////////////////////////////////////////////*/

    /// @notice Handles plain ETH transfers with no calldata.
    receive() external payable {
        if (msg.value == 0) revert Errors.ZeroValueTransfer();

        emit EtherReceived(
            msg.sender,
            msg.value,
            block.timestamp
        );
    }

    /*//////////////////////////////////////////////////////////////
                             FALLBACK LOGIC
    //////////////////////////////////////////////////////////////*/

    /// @notice Handles calls to non-existent functions.
    /// @dev Logs selector, caller, attached value, and calldata.
    fallback() external payable {
        emit UnknownCall(
            msg.sender,
            msg.sig,
            msg.value,
            msg.data
        );
    }

    /*//////////////////////////////////////////////////////////////
                            VIEW FUNCTIONS
    //////////////////////////////////////////////////////////////*/

    /// @notice Returns the current ETH balance of the contract.
    function getBalance() external view returns (uint256) {
        return address(this).balance;
    }

    /*//////////////////////////////////////////////////////////////
                           OWNER FUNCTIONS
    //////////////////////////////////////////////////////////////*/

    /// @notice Withdraws the entire contract balance.
    function withdraw() external onlyOwner {
        uint256 balance = address(this).balance;

        (bool success, ) = payable(owner).call{value: balance}("");
        if (!success) revert Errors.TransferFailed();
    }

    /// @notice Transfers ownership to a new address.
    function transferOwnership(address newOwner)
        external
        onlyOwner
    {
        if (newOwner == address(0)) revert Errors.InvalidNewOwner();

        emit OwnershipTransferred(
            owner,
            newOwner
        );

        owner = newOwner;
    }
}