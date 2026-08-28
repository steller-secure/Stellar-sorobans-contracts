#![no_std]

//! Shared contracts library with common reusable primitives.

pub mod admin;
pub mod random;
pub mod insurance_types;

pub use random::Randomness;
pub use insurance_types::*;

// Re-exported by name rather than with a glob: the admin module also exports
// constants (`MIN_TRANSFER_DELAY_SECONDS` and friends) that read as ambiguous
// outside `admin::`, so callers reach those through the module path.
pub use admin::{AdminError, PendingAdmin};
