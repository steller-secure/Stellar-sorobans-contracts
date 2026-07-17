# Project Structure

This file is the high-level map for the repository so contributors can find the main logic without walking every folder manually.

## Folder Structure

```text
.
|-- CHANGELOG.md
|-- IMPLEMENTATION_SUMMARY.md
|-- MIGRATION_GUIDE.md
|-- README.md
|-- foundry.toml                          # Solidity build config
|-- src/                                  # Solidity governance contracts
|   |-- ContractErrors.sol
|   |-- FallbackHandler.sol
|   |-- InitializationGuard.sol
|   |-- InputValidation.sol
|   |-- OwnershipTransfer.sol
|   |-- SecurityEvents.sol
|   |-- lib.rs                            # Solidity governance event bridge
|   `-- libs.sol
|-- stellar-insured-contracts/            # Rust workspace (ink! + Soroban)
|   |-- Cargo.toml                        # Workspace manifest
|   |-- common/
|   |-- contracts/                        # 20 contract crates (see below)
|   |-- docs/
|   |-- oracle/
|   |-- scripts/
|   |-- sdk/
|   |-- security-audit/
|   |-- security-tests/
|   `-- tests/
`-- structure.md
```

### Contracts (`stellar-insured-contracts/contracts/`)

The repository uses **two smart-contract frameworks**:

| # | Contract | Framework | Description |
|---|----------|-----------|-------------|
| 1 | `ai-valuation` | ink! | AI valuation model registration, prediction, drift, A/B test |
| 2 | `analytics` | ink! | Market analytics & report generation |
| 3 | `bridge` | **Soroban** | Cross-chain bridge for property-token transfers |
| 4 | `claims` | **Soroban** | Insurance claims processing (Stellar Insured) |
| 5 | `compliance_registry` | ink! | KYC/AML, sanctions, GDPR consent, compliance reporting |
| 6 | `escrow` | **Soroban** | Advanced escrow for property transactions |
| 7 | `fees` | ink! | Dynamic fee & premium auction mechanism |
| 8 | `fractional` | ink! | Fractional ownership share accounting & dividends |
| 9 | `governance` | **Soroban** | DAO governance (Stellar Insured) |
| 10 | `insurance` | ink! | Decentralized Property Insurance (PropChain) |
| 11 | `ipfs-metadata` | ink! | IPFS document & metadata registry |
| 12 | `lib` | **Soroban** | Shared library (random, insurance types) for Soroban crates |
| 13 | `oracle` | ink! | Property valuation oracle with source aggregation |
| 14 | `policy` | **Soroban** | Insurance policy management (Stellar Insured) |
| 15 | `property-token` | ink! | Property token (ERC-721/1155 compatible) |
| 16 | `proxy` | ink! | Upgradeable proxy pattern |
| 17 | `risk_pool` | **Soroban** | Liquidity risk pool management (Stellar Insured) |
| 18 | `slashing` | **Soroban** | Slashing & penalty mechanism (Stellar Insured) |
| 19 | `traits` | ink! | Shared trait definitions |
| 20 | `zk-compliance` | ink! | Zero-knowledge privacy-preserving compliance |

- **ink! contracts** (12): `ai-valuation`, `analytics`, `compliance_registry`, `fees`, `fractional`, `insurance`, `ipfs-metadata`, `oracle`, `property-token`, `proxy`, `traits`, `zk-compliance`
- **Soroban contracts** (8): `bridge`, `claims`, `escrow`, `governance`, `lib`, `policy`, `risk_pool`, `slashing`

## Logic Map

To find the Solidity governance event bridge visit [src/lib.rs](src/lib.rs).

To find the Rust workspace README visit [stellar-insured-contracts/README.md](stellar-insured-contracts/README.md).

To find contract implementations visit [stellar-insured-contracts/contracts/README.md](stellar-insured-contracts/contracts/README.md).

To find contributor-facing architecture and integration notes visit [stellar-insured-contracts/docs/README.md](stellar-insured-contracts/docs/README.md).

To find SDK integration surfaces visit [stellar-insured-contracts/sdk/README.md](stellar-insured-contracts/sdk/README.md).

To find integration and benchmark test entry points visit [stellar-insured-contracts/tests/README.md](stellar-insured-contracts/tests/README.md).

The security test suite can be found in [stellar-insured-contracts/security-tests/README.md](stellar-insured-contracts/security-tests/README.md).

The mobile SDK guide can be found in [stellar-insured-contracts/sdk/mobile/README.md](stellar-insured-contracts/sdk/mobile/README.md).

## Architectural Decisions

The top-level Solidity file stays separate from the Rust workspace because it models a small governance execution/event concern, while the Rust workspace contains the larger PropChain and Stellar Insured contract system across **two frameworks**: ink! (Substrate/Polkadot ecosystem) and Soroban (Stellar ecosystem).

The `stellar-insured-contracts/contracts` directory is the main production contract layer. The `sdk`, `tests`, and `docs` directories support that layer instead of defining primary on-chain behavior.

## Tradeoffs

Focused folder documentation is used instead of README files in every small contract subdirectory. That keeps navigation useful while avoiding documentation churn in a large open-source repository.

Source files are linked with relative paths so GitHub and local clones both resolve the references.
