use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct Config {
    /// Address of the cw20 token the contract should pay out
    pub cw20_contract: Addr,
    /// Number of seconds between payments
    pub epoch: u64,
    /// How many tokens to pay out per epoch
    pub payment: Uint128,
    /// Address of the recipient. It must be able to handle [`wynd-stake::ExecuteMsg::DistributeRewards`]
    pub recipient: Addr,
    /// Address which can adjust the config
    pub admin: Addr,
    /// The last timestamp where payments were distributed
    pub last_payment: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");
