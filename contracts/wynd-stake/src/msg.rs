use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Uint128};
use cw20_vesting::Cw20ReceiveDelegationMsg;
pub use cw_controllers::ClaimsResponse;
use cw_core_macros::{token_query, voting_query};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    /// address of cw20 contract token
    pub cw20_contract: String,
    pub tokens_per_power: Uint128,
    pub min_bond: Uint128,
    pub stake_config: Vec<StakeConfig>,

    // admin can only add/remove hooks, not change other parameters
    pub admin: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Rebond will update an amount of bonded tokens from one bond period to the other
    Rebond {
        tokens: Uint128,
        // these must be valid time periods
        bond_from: u64,
        bond_to: u64,
    },
    /// Unbond will start the unbonding process for the given number of tokens.
    /// The sender immediately loses power from these tokens, and can claim them
    /// back to his wallet after `unbonding_period`
    Unbond {
        tokens: Uint128,
        /// As each unbonding period in delegation corresponds to particular voting
        /// multiplier, unbonding_period needs to be passed in unbond as well
        unbonding_period: u64,
    },
    /// Claim is used to claim your native tokens that you previously "unbonded"
    /// after the contract-defined waiting period (eg. 1 week)
    Claim {},

    /// Change the admin
    UpdateAdmin { admin: Option<String> },
    /// Add a new hook to be informed of all membership changes. Must be called by Admin
    AddHook { addr: String },
    /// Remove a hook. Must be called by Admin
    RemoveHook { addr: String },

    /// This accepts a properly-encoded ReceiveMsg from a cw20 contract
    ReceiveDelegation(Cw20ReceiveDelegationMsg),

    /// Distributes rewards sent with this message, and all rewards transferred since last call of this
    /// to members, proportionally to their points. Rewards are not immediately send to members, but
    /// assigned to them for later withdrawal (see: `ExecuteMsg::WithdrawFunds`)
    DistributeRewards {
        /// Original source of rewards, informational. If present overwrites "sender" field on
        /// propagated event.
        sender: Option<String>,
    },
    /// Withdraws rewards which were previously distributed and assigned to sender.
    WithdrawRewards {
        /// Account from which assigned rewards would be withdrawn; `sender` by default. `sender` has
        /// to be eligible for withdrawal from `owner` address to perform this call (`owner` has to
        /// call `DelegateWithdrawal { delegated: sender }` before)
        owner: Option<String>,
        /// Address where to transfer funds. If not present, funds would be sent to `sender`.
        receiver: Option<String>,
    },
    /// Sets given address as allowed for senders funds withdrawal. Funds still can be withdrawn by
    /// sender himself, but this additional account is allowed to perform it as well. There can be only
    /// one account delegated for withdrawal for any owner at any single time.
    DelegateWithdrawal {
        /// Account delegated for withdrawal. To disallow current withdrawal, the best is to set it
        /// to own address.
        delegated: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveDelegationMsg {
    Delegate {
        /// Unbonding period in seconds
        unbonding_period: u64,
    },
}

#[voting_query]
#[token_query]
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Claims shows the tokens in process of unbonding for this address
    Claims {
        address: String,
    },
    /// Show the number of tokens currently staked by this address.
    Staked {
        address: String,
        /// Unbonding period in seconds
        unbonding_period: u64,
    },
    /// Show the number of tokens currently staked by this address for all unbonding periods
    AllStaked {
        address: String,
    },
    /// Show the number of all, not unbonded tokens delegated by all users for all unbonding periods
    TotalStaked {},
    /// Show the number of all tokens being unbonded for all unbonding periods
    TotalUnbonding {},
    /// Show the total number of outstanding rewards
    TotalRewards {},
    /// Show the outstanding rewards for this address
    Rewards {
        address: String,
    },
    /// Return AdminResponse
    Admin {},
    /// Shows all registered hooks. Returns HooksResponse.
    Hooks {},
    BondingInfo {},

    /// Return how many rewards are assigned for withdrawal from the given address. Returns
    /// `RewardsResponse`.
    WithdrawableRewards {
        owner: String,
    },
    /// Return how many rewards were distributed in total by this contract. Returns
    /// `RewardsResponse`.
    DistributedRewards {},
    /// Return how many funds were sent to this contract since last `ExecuteMsg::DistributeFunds`,
    /// and await for distribution. Returns `RewardsResponse`.
    UndistributedRewards {},
    /// Return address allowed for withdrawal of the funds assigned to owner. Returns `DelegateResponse`
    Delegated {
        owner: String,
    },
    /// Returns rewards distribution data
    DistributionData {},
    /// Returns withdraw adjustment data
    WithdrawAdjustmentData {
        addr: String,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct StakeConfig {
    pub unbonding_period: u64,      // seconds
    pub voting_multiplier: Decimal, // stake * voting_ratio = voting_power
    pub reward_multiplier: Decimal, // stake * reward_ratio = reward_power
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StakedResponse {
    pub stake: Uint128,
    pub total_locked: Uint128,
    pub unbonding_period: u64,
    pub cw20_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AllStakedResponse {
    pub stakes: Vec<StakedResponse>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TotalStakedResponse {
    pub total_staked: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TotalUnbondingResponse {
    pub total_unbonding: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct TotalRewardsResponse {
    pub rewards: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct RewardsResponse {
    pub rewards: Uint128,
}

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct BondingPeriodInfo {
    pub unbonding_period: u64,
    pub voting_multiplier: Decimal,
    pub reward_multiplier: Decimal,
    pub total_staked: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct BondingInfoResponse {
    pub bonding: Vec<BondingPeriodInfo>,
}

// just for the proper json outputs
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct TokenContractResponse(Addr);

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct WithdrawableRewardsResponse {
    /// Amount of rewards assigned for withdrawal from the given address.
    pub rewards: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct DelegatedResponse {
    pub delegated: Addr,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct DistributedRewardsResponse {
    /// Total number of tokens sent to the contract over all time.
    pub distributed: Uint128,
    /// Total number of tokens available to be withdrawn.
    pub withdrawable: Uint128,
}

pub type UndistributedRewardsResponse = WithdrawableRewardsResponse;
pub type DistributionDataResponse = crate::state::Distribution;
pub type WithdrawAdjustmentDataResponse = crate::state::WithdrawAdjustment;
