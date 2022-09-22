//#![warn(missing_docs)]
#![doc(html_logo_url = "../../../uml/logo.png")]
//! # WYND CW-20 Vesting
//!
//! ## Description
//!
//! We need a project that define the WYND TOKEN and distribute to our community in a vested way.
//!
//! ## Objectives
//!
//! The main goal of the **WYND cw20-vesting** is to:
//!   - Define a cw-20 token contract.
//!   - Allow to apply differents curve when vesting it.
//!

/// cw-20 allowance
pub mod allowances;

/// Main cw-20 Module
pub mod contract;

/// paginated query Module
pub mod enumerable;

/// custom error handler
mod error;

/// custom input output messages
pub mod msg;

/// state on the blockchain
pub mod state;
pub use crate::error::ContractError;
pub use crate::msg::{ExecuteMsg, InitBalance, InstantiateMsg, MinterInfo, QueryMsg};
pub use crate::receive_delegate::Cw20ReceiveDelegationMsg;

#[cfg(test)]
mod multitest;
mod receive_delegate;
