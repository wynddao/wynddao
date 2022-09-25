use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

use cw_controllers::{AdminError, HookError};

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Admin(#[from] AdminError),

    #[error("{0}")]
    Hook(#[from] HookError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Cannot rebond to the same unbonding period")]
    SameUnbondingRebond {},

    #[error("Rebond amount is invalid")]
    NoRebondAmount {},

    #[error("No claims that can be released currently")]
    NothingToClaim {},

    #[error(
        "Sender's CW20 token contract address {got} does not match one from config {expected}"
    )]
    Cw20AddressesNotMatch { got: String, expected: String },

    #[error("No funds sent")]
    NoFunds {},

    #[error("No data in ReceiveMsg")]
    NoData {},

    #[error("No unbonding period found: {0}")]
    NoUnbondingPeriodFound(u64),

    #[error("No members to distribute tokens to")]
    NoMembersToDistributeTo {},
}

impl From<OverflowError> for ContractError {
    fn from(e: OverflowError) -> Self {
        ContractError::Std(e.into())
    }
}
