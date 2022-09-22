use cosmwasm_std::StdError;
use cw_utils::PaymentError;
use thiserror::Error;

/// Error Handler for Decision contract
#[derive(Error, Debug)]
pub enum ContractError {
    /// Wrapper to StdError
    #[error("{0}")]
    Std(#[from] StdError),

    /// Handle error on Messages when sending funds or a different token as expected
    #[error("{0}")]
    Payment(#[from] PaymentError),

    /// Authorization handler
    #[error("Unauthorized")]
    Unauthorized,

    /// length handler error for RecordMessage
    #[error("{0} must be between {1} and {2} characters")]
    InvalidLength(&'static str, u64, u64),
}
