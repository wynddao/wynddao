//#![warn(missing_docs)]
#![doc(html_logo_url = "../../../uml/logo.png")]
//! # WYND Vesting Airdrop
//!
//! ## Description
//!
//! We need a project that allow us to launch the WYND TOKEN and distribute to our community in a vested way.
//!
//! ## Objectives
//!
//! The main goal of the **WYND airdrop** is to:
//!   - Distribute WYND TOKEN allowing community members to claim it.
//!   - Vest the airdrop tokens to avoid sell presure and make the price of the token more stable at release
//!

/// Main vesting-airdrop Module
pub mod contract;

/// custom error handler
mod error;

/// custom input output messages
pub mod msg;

/// state on the blockchain
pub mod state;

pub use crate::error::ContractError;
