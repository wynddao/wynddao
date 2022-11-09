use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::Config;

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Address of the cw20 token the contract should pay out
    pub cw20_contract: String,
    /// Number of seconds between payments
    pub epoch: u64,
    /// How many tokens to pay out per epoch
    pub payment: Uint128,
    /// Address of the recipient. It must be able to handle [`wynd-stake::ExecuteMsg::DistributeRewards`]
    pub recipient: String,
    /// Address which can adjust the config
    pub admin: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    UpdateConfig {
        cw20_contract: Option<String>,
        epoch: Option<u64>,
        payment: Option<Uint128>,
        recipient: Option<String>,
        admin: Option<String>,
    },
    /// Triggers the payout (if one is due)
    Payout {},
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns the current configuration
    /// Return type: ConfigResponse.
    Config {},
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigResponse {
    pub config: Config,
}

/// TODO: remove when wynd-stake has this variant
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecipientExecuteMsg {
    DistributeRewards { sender: Option<String> },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct MigrateMsg {}
