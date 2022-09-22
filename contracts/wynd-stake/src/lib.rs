//#![warn(missing_docs)]
#![doc(html_logo_url = "../../../uml/logo.png")]
//! # WYND Staking
//!
//! ## Description
//!
//! We need a project that allow the WYND TOKEN to be staked.
//!
//! ## Objectives
//!
//! The main goal of the **WYND staking** is to:
//!   - Allow the WYND TOKEN to be staked with a proper curve and time.
//!   - Define a way to give voting power on the governance side.
//!

/// Main contract logic
pub mod contract;
/// Lazy reward distribution, mostly can be reused by other contracts
pub mod distribution;

/// custom error handler
mod error;

/// custom input output messages
pub mod msg;

/// copy of cw4 MemberChangedHookMsg using Uint128 instead of u64
pub mod hook;
/// state on the blockchain
pub mod state;

#[cfg(test)]
mod multitest;

#[cfg(test)]
mod dao_bindings;
#[cfg(test)]
mod dao_tests;

pub use crate::error::ContractError;
