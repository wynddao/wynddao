use cosmwasm_std::{Addr, Binary, BlockInfo, Timestamp, Uint128};
use cw20::Logo;
use cw_utils::Expiration;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ContractError;
use wynd_utils::Curve;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct InstantiateMarketingInfo {
    pub project: Option<String>,
    pub description: Option<String>,
    pub marketing: Option<String>,
    pub logo: Option<Logo>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct InstantiateMsg {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_balances: Vec<InitBalance>,
    pub mint: Option<MinterInfo>,
    pub marketing: Option<InstantiateMarketingInfo>,
    pub allowed_vesters: Option<Vec<String>>,
    pub max_curve_complexity: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct InitBalance {
    pub address: String,
    pub amount: Uint128,
    /// Optional vesting schedule
    /// It must be a decreasing curve, ending at 0, and never exceeding amount
    pub vesting: Option<Curve>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct MinterInfo {
    pub minter: String,
    /// cap is a hard cap on total supply that can be achieved by minting.
    /// This can be a monotonically increasing curve based on block time
    /// (constant value being a special case of this).
    ///
    /// Note that cap refers to total_supply.
    /// If None, there is unlimited cap.
    pub cap: Option<Curve>,
}

impl InstantiateMsg {
    pub fn get_curve(&self) -> Option<&Curve> {
        self.mint.as_ref().and_then(|v| v.cap.as_ref())
    }

    pub fn get_cap(&self, block_time: &Timestamp) -> Option<Uint128> {
        self.get_curve().map(|v| v.value(block_time.seconds()))
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        // Check name, symbol, decimals
        if !is_valid_name(&self.name) {
            return Err(ContractError::InvalidName);
        }
        if !is_valid_symbol(&self.symbol) {
            return Err(ContractError::InvalidSymbol);
        }
        if self.decimals > 18 {
            return Err(ContractError::TooManyDecimals);
        }
        if let Some(curve) = self.get_curve() {
            curve.validate_monotonic_increasing()?;
        }
        Ok(())
    }
}

fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || bytes.len() > 50 {
        return false;
    }
    true
}

fn is_valid_symbol(symbol: &str) -> bool {
    let bytes = symbol.as_bytes();
    if bytes.len() < 3 || bytes.len() > 12 {
        return false;
    }
    for byte in bytes.iter() {
        if (*byte != 45) && (*byte < 65 || *byte > 90) && (*byte < 97 || *byte > 122) {
            return false;
        }
    }
    true
}

/// Asserts the vesting schedule decreases to 0 eventually, and is never more than the
/// amount being sent. If it doesn't match these conditions, returns an error.
pub fn assert_schedule_vests_amount(
    schedule: &Curve,
    amount: Uint128,
) -> Result<(), ContractError> {
    schedule.validate_monotonic_decreasing()?;
    let (low, high) = schedule.range();
    if low != 0 {
        Err(ContractError::NeverFullyVested)
    } else if high > amount.u128() {
        Err(ContractError::VestsMoreThanSent)
    } else {
        Ok(())
    }
}

/// Returns true if curve is already at 0
pub fn fully_vested(schedule: &Curve, block: &BlockInfo) -> bool {
    schedule.value(block.time.seconds()).is_zero()
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Transfer is a base message to move tokens to another account without triggering actions
    Transfer { recipient: String, amount: Uint128 },
    /// TransferVesting is a base message to move tokens to another account without triggering actions.
    /// The sent tokens will be slowly released based on the attached schedule.
    /// If the recipient already has an existing vesting schedule, this will fail.
    TransferVesting {
        recipient: String,
        amount: Uint128,
        /// VestingSchedule.
        /// It must be a decreasing curve, ending at 0, and never exceeding amount
        schedule: Curve,
    },
    /// Burn is a base message to destroy tokens forever
    Burn { amount: Uint128 },
    /// Send is a base message to transfer tokens to a contract and trigger an action
    /// on the receiving contract.
    Send {
        contract: String,
        amount: Uint128,
        msg: Binary,
    },
    /// Only with "approval" extension. Allows spender to access an additional amount tokens
    /// from the owner's (env.sender) account. If expires is Some(), overwrites current allowance
    /// expiration with this one.
    IncreaseAllowance {
        spender: String,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    /// Only with "approval" extension. Lowers the spender's access of tokens
    /// from the owner's (env.sender) account by amount. If expires is Some(), overwrites current
    /// allowance expiration with this one.
    DecreaseAllowance {
        spender: String,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    /// Only with "approval" extension. Transfers amount tokens from owner -> recipient
    /// if `env.sender` has sufficient pre-approval.
    TransferFrom {
        owner: String,
        recipient: String,
        amount: Uint128,
    },
    /// Only with "approval" extension. Sends amount tokens from owner -> contract
    /// if `env.sender` has sufficient pre-approval.
    SendFrom {
        owner: String,
        contract: String,
        amount: Uint128,
        msg: Binary,
    },
    /// Only with "approval" extension. Destroys tokens forever
    BurnFrom { owner: String, amount: Uint128 },
    /// Only with the "mintable" extension. If authorized, creates amount new tokens
    /// and adds to the recipient balance.
    Mint { recipient: String, amount: Uint128 },
    /// Only with the "mintable" extension. If minter set and authorized by current
    /// minter, makes the new address the minter.
    UpdateMinter { minter: String },
    /// Only with the "marketing" extension. If authorized, updates marketing metadata.
    /// Setting None/null for any of these will leave it unchanged.
    /// Setting Some("") will clear this field on the contract storage
    UpdateMarketing {
        /// A URL pointing to the project behind this token.
        project: Option<String>,
        /// A longer description of the token and it's utility. Designed for tooltips or such
        description: Option<String>,
        /// The address (if any) who can update this data structure
        marketing: Option<String>,
    },
    /// If set as the "marketing" role on the contract, upload a new URL, SVG, or PNG for the token
    UploadLogo(Logo),
    /// If set, it will add an address to a permission list on TransferVesting
    AllowVester { address: String },
    /// If set, it will remove an address to a permission list on TransferVesting
    DenyVester { address: String },
    /// Allows minter to update staking address
    UpdateStakingAddress { address: String },
    /// Delegates excess of tokens
    Delegate { amount: Uint128, msg: Binary },
    /// Undelegates previously delegated tokens
    Undelegate { recipient: String, amount: Uint128 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns the current balance of the given address, 0 if unset.
    /// Return type: BalanceResponse.
    Balance { address: String },
    /// Returns the current vesting schedule for the given account.
    /// Return type: VestingResponse.
    Vesting { address: String },
    /// Returns the amount of delegated tokens for the given account.
    /// Return type: DelegatedResponse.
    Delegated { address: String },
    /// Returns the allow list who can transfer vesting tokens.
    /// Return type: VestingAllowListResponse.
    VestingAllowList {},
    /// Returns metadata on the contract - name, decimals, supply, etc.
    /// Return type: TokenInfoResponse.
    TokenInfo {},
    /// Returns maximum allowed complexity of vesting curves
    /// Return type: MaxVestingComplexityResponse
    MaxVestingComplexity {},
    /// Only with "mintable" extension.
    /// Returns who can mint and the hard cap on maximum tokens after minting.
    /// Return type: MinterResponse.
    Minter {},
    /// Only with "allowance" extension.
    /// Returns how much spender can use from owner account, 0 if unset.
    /// Return type: AllowanceResponse.
    Allowance { owner: String, spender: String },
    /// Only with "enumerable" extension (and "allowances")
    /// Returns all allowances this owner has approved. Supports pagination.
    /// Return type: AllAllowancesResponse.
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Only with "enumerable" extension
    /// Returns all accounts that have balances. Supports pagination.
    /// Return type: AllAccountsResponse.
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Only with "marketing" extension
    /// Returns more metadata on the contract to display in the client:
    /// - description, logo, project url, etc.
    /// Return type: MarketingInfoResponse
    MarketingInfo {},
    /// Only with "marketing" extension
    /// Downloads the mbeded logo data (if stored on chain). Errors if no logo data ftored for this
    /// contract.
    /// Return type: DownloadLogoResponse.
    DownloadLogo {},
    /// Returns staking address used to delegate tokens.
    /// Return type: StakingAddressResponse.
    StakingAddress {},
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
pub struct MigrateMsg {
    pub picewise_linear_curve: Curve,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct MinterResponse {
    pub minter: String,
    /// cap is a hard cap on total supply that can be achieved by minting.
    /// This can be a monotonically increasing curve based on block time
    /// (constant value being a special case of this).
    ///
    /// Note that cap refers to total_supply.
    /// If None, there is unlimited cap.
    pub cap: Option<Curve>,
    /// This is cap evaluated at the current time
    pub current_cap: Option<Uint128>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct VestingResponse {
    /// The total vesting schedule
    pub schedule: Option<Curve>,
    /// The current amount locked. Always 0 if schedule is None
    pub locked: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct VestingAllowListResponse {
    pub allow_list: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DelegatedResponse {
    pub delegated: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct StakingAddressResponse {
    pub address: Option<Addr>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct MaxVestingComplexityResponse {
    pub complexity: u64,
}
