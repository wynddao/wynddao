#![warn(missing_docs)]
#![doc(html_logo_url = "../../../uml/logo.png")]
//! # WYND Governance Decisions Record
//!
//! ## Description
//!
//! We need a project that record the governance decisions to provide transparency.
//!
//! ## Objectives
//!
//! The main goal of the **WYND decisions** is to:
//!   - Define a way to record the decisions data.
//!

/// Main Decisions Module
pub mod contract;

/// custom error handler
pub mod error;

/// custom input output messages
pub mod msg;

/// state on the blockchain
pub mod state;
