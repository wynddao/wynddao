use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;
use wynd_utils::CurveError;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Curve(#[from] CurveError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Name is not in the expected format (3-50 UTF-8 bytes)")]
    InvalidName,

    #[error("Ticker symbol is not in expected format [a-zA-Z\\-]{{3,12}}")]
    InvalidSymbol,

    #[error("Decimals must not exceed 18")]
    TooManyDecimals,

    #[error("Cannot set to own account")]
    CannotSetOwnAccount {},

    #[error("Invalid zero amount")]
    InvalidZeroAmount {},

    #[error("Allowance is expired")]
    Expired {},

    #[error("No allowance for this account")]
    NoAllowance {},

    #[error("Minting cannot exceed the cap")]
    CannotExceedCap {},

    #[error("Logo binary data exceeds 5KB limit")]
    LogoTooBig {},

    #[error("Invalid xml preamble for SVG")]
    InvalidXmlPreamble {},

    #[error("Invalid png header")]
    InvalidPngHeader {},

    #[error("Duplicate initial balance addresses")]
    DuplicateInitialBalanceAddresses {},

    #[error("The transfer will never become fully vested. Must hit 0 eventually")]
    NeverFullyVested,

    #[error("The transfer tries to vest more tokens than it sends")]
    VestsMoreThanSent,

    #[error("The transfer would have moved tokens still locked by a vesting schedule")]
    CantMoveVestingTokens,

    #[error("Can't delegate tokens - sum of balance plus vested is too low")]
    NotEnoughToDelegate,

    #[error("Address Not Found")]
    AddressNotFound {},

    #[error("Address Already Exist")]
    AddressAlreadyExist {},

    #[error("At least one Address must be on the Allow List")]
    AtLeastOneAddressMustExist {},

    #[error("Staking token address not set")]
    StakingAddressNotSet {},

    #[error("Staking token address was already updated once")]
    StakingAddressAlreadyUpdated {},

    #[error("Minter address not set")]
    MinterAddressNotSet {},

    #[error("Unauthorized - only minter is allowed to modify staking address")]
    UnauthorizedUpdateStakingAddress {},

    #[error("Only staking contract is allowed to undelegate tokens")]
    UnauthorizedUndelegate {},

    #[error("No tokens delegated")]
    NoTokensDelegated {},

    #[error("Migration error - provided curve is not picewise linear!")]
    MigrationIncorrectCurve {},
}

impl From<OverflowError> for ContractError {
    fn from(err: OverflowError) -> Self {
        ContractError::Std(err.into())
    }
}
