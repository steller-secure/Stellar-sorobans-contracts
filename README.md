🧠 Stellar Insured
Decentralized Insurance Infrastructure on Stellar Soroban & Polkadot ink!

Stellar Insured is a suite of smart contracts that powers decentralized insurance products across the Stellar (Soroban) and Polkadot (ink!) ecosystems.

The protocol enables transparent policy issuance, deterministic claims processing, decentralized risk pools, DAO governance, and on-chain slashing mechanisms. All critical insurance logic is executed on-chain, ensuring security, transparency, and auditability.

The platform is designed for:

Policyholders
Liquidity providers
DAO participants
Auditors
Developers
Insurance protocol operators
✨ Features
Insurance policy lifecycle management
Multi-stage claims processing workflow
Decentralized liquidity risk pools
Automated claims settlement
DAO-based governance
On-chain slashing mechanism
Configurable voting thresholds and quorum requirements
Progressive penalties for malicious actors
Fully auditable smart contract execution
Dual-framework architecture (ink! + Soroban)
🏗 Architecture

The protocol spans **20 contract crates** across two frameworks. The Stellar Insured line (Soroban) handles policy lifecycle, claims, risk pools, governance, and slashing. The PropChain line (ink!) covers property tokenization, fractional ownership, compliance, AI valuation, oracles, and more.

**Soroban contracts** (stellar-insured):
- `policy` — Insurance policy management
- `claims` — Multi-stage claims processing
- `risk_pool` — Liquidity risk pool
- `governance` — DAO governance
- `slashing` — On-chain slashing and penalties
- `bridge` — Cross-chain property-token bridge
- `escrow` — Property transaction escrow
- `lib` — Shared Soroban library

**ink! contracts** (propchain):
- `ai-valuation`, `analytics`, `compliance_registry`, `fees`, `fractional`, `insurance`, `ipfs-metadata`, `oracle`, `property-token`, `proxy`, `traits`, `zk-compliance`

See [stellar-insured-contracts/contracts/README.md](stellar-insured-contracts/contracts/README.md) for the full breakdown.
📜 Contract Overview
1. Policy Contract

Manages insurance policy issuance, renewal, cancellation, and expiration.

Capabilities
Issue policies
Renew active policies
Cancel policies
Manage policy lifecycle
Retrieve policy information
Core Functions
initialize(admin, risk_pool)

issue_policy(
    holder,
    coverage_amount,
    premium_amount,
    duration_days,
    policy_type
)

get_policy(policy_id)

renew_policy(policy_id, duration_days)

cancel_policy(policy_id)

expire_policy(policy_id)

get_stats()
2. Claims Contract

Processes insurance claims using a deterministic multi-stage approval workflow.

Claim Lifecycle
Submitted
    ↓
UnderReview
    ↓
Approved / Rejected
    ↓
Settled (Approved only)
Workflow Rules
Only administrators can transition claim states.
Claims cannot be settled before approval.
Invalid state transitions are prevented.
Core Functions
initialize(admin, policy_contract, risk_pool)

submit_claim(policy_id, amount)

start_review(claim_id)

approve_claim(claim_id)

reject_claim(claim_id)

settle_claim(claim_id)

get_claim(claim_id)

get_stats()
3. Risk Pool Contract

Provides liquidity used to settle approved insurance claims.

Capabilities
Accept liquidity deposits
Handle withdrawals
Reserve funds for claims
Execute claim payouts
Track provider positions
Core Functions
initialize(
    admin,
    xlm_token,
    min_provider_stake
)

deposit_liquidity(provider, amount)

withdraw_liquidity(provider, amount)

payout_claim(recipient, amount)

get_pool_stats()

get_provider_info(provider)
4. Governance Contract

DAO governance system used to manage protocol decisions.

Governance Features
Proposal creation
Community voting
Quorum enforcement
Proposal execution
Slashing proposal management
Proposal analytics
Core Functions
initialize(
    admin,
    token_contract,
    voting_period_days,
    min_voting_percentage,
    min_quorum_percentage,
    slashing_contract
)

create_proposal(
    title,
    description,
    execution_data,
    threshold_percentage
)

vote(
    proposal_id,
    vote_weight,
    is_yes
)

finalize_proposal(proposal_id)

execute_proposal(proposal_id)

create_slashing_proposal(
    target,
    role,
    reason,
    amount,
    evidence,
    threshold
)

execute_slashing_proposal(proposal_id)
Query Functions
get_proposal(proposal_id)

get_active_proposals()

get_all_proposals()

get_vote_record(
    proposal_id,
    voter
)

get_proposal_stats(proposal_id)
5. Slashing Contract

Provides a governance-controlled mechanism for penalizing malicious or negligent actors.

Supported Roles
Oracle providers
Claim submitters
Governance participants
Liquidity providers
Features
Configurable penalties
Progressive punishment system
Repeat offender tracking
Cooldown periods
Treasury or pool fund redirection
Emergency pause controls
Core Functions
initialize(
    admin,
    governance_contract,
    risk_pool_contract
)

configure_penalty_parameters(
    role,
    reason,
    percentage,
    destination,
    multiplier,
    cooldown
)

slash_funds(
    target,
    role,
    reason,
    amount
)

add_slashable_role(role)

remove_slashable_role(role)

get_slashing_history(
    target,
    role
)

get_violation_count(
    target,
    role
)

can_be_slashed(
    target,
    role
)

pause()

unpause()
🧑‍💻 Technology Stack
Layer	Technology
Blockchain	Stellar (Soroban) & Polkadot (ink!)
Smart Contracts	Soroban SDK + ink!
Language	Rust
Testing	Soroban Test Framework / cargo-contract
Runtime	Soroban VM / Substrate (Polkadot)
📦 Getting Started
Prerequisites
- Rust (latest stable)
- Stellar CLI + cargo-contract (for ink!)
Install Dependencies
rustup update
cargo install stellar-cli
cargo install cargo-contract --locked
🔨 Build Contracts

Contracts are organized in a Cargo workspace under `stellar-insured-contracts/`:

cd stellar-insured-contracts
cargo build --release

Or build individual contracts:

cd stellar-insured-contracts/contracts/policy && cargo build --release
cd stellar-insured-contracts/contracts/claims && cargo build --release
cd stellar-insured-contracts/contracts/risk_pool && cargo build --release
cd stellar-insured-contracts/contracts/governance && cargo build --release
cd stellar-insured-contracts/contracts/slashing && cargo build --release
🧪 Run Tests
# Soroban contracts
cd stellar-insured-contracts && cargo test

# ink! contracts
cd stellar-insured-contracts && cargo test --package propchain-insurance
🌐 Network Configuration
Component	Value
Network	Stellar Testnet
Execution	Soroban VM
Wallets	Non-Custodial Stellar Wallets
🚀 Deployment

### Soroban Contracts (Stellar Insured)

Deploy each compiled WASM contract using Stellar CLI:

```bash
stellar contract deploy --wasm stellar-insured-contracts/target/wasm32-unknown-unknown/release/stellar_insured_policy.wasm
```

Initialize contracts in dependency order:

1. Risk Pool
2. Policy
3. Claims
4. Slashing
5. Governance

### ink! Contracts (PropChain)

Use the provided deployment script:

```bash
stellar-insured-contracts/scripts/deploy.sh --network local
```

### Local Node

Start a local development node:

```bash
stellar-insured-contracts/scripts/local-node.sh start
```
🔐 Security

Security is a first-class concern throughout the protocol.

Security Controls
Deterministic execution
Explicit authorization checks
Multi-stage claim validation
Settlement restrictions
Comprehensive state validation
Event-based audit trails
Governance-controlled slashing
Minimal trusted off-chain assumptions
Production Recommendations
Independent security audits
Rate limiting
Monitoring and alerting
Multi-signature governance controls
📚 Resources
Stellar Documentation
Soroban Documentation
Rust Documentation
🤝 Contributing

We welcome contributions from both the Stellar and Polkadot ecosystems.

Contribution Process
1. Fork the repository
2. Create a feature branch
3. Add tests for all changes
4. Follow Rust, Soroban, and ink! best practices
5. Submit a Pull Request

See [structure.md](structure.md) to navigate the codebase and [stellar-insured-contracts/Cargo.toml](stellar-insured-contracts/Cargo.toml) for the workspace layout.
📄 License

MIT License

Built with Rust, Soroban, and Stellar to create transparent, decentralized insurance infrastructure. 🚀
