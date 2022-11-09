use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use cw_utils::{Expiration, Scheduled};
use wynd_utils::ScalableCurve;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Config {
    /// Owner If None set, contract is frozen.
    pub owner: Option<Addr>,
    pub cw20_token_address: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const LATEST_STAGE: Item<u8> = Item::new("latest_stage");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StageDetails {
    pub expiration: Expiration,
    pub start: Scheduled,
    pub vesting: Option<ScalableCurve>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct StageAmounts {
    pub total: Uint128,
    pub claimed: Uint128,
}

pub const STAGE_DETAILS: Map<u8, StageDetails> = Map::new("stage_details");
pub const STAGE_AMOUNTS: Map<u8, StageAmounts> = Map::new("stage_amounts");

pub const MERKLE_ROOT_PREFIX: &str = "merkle_root";
pub const MERKLE_ROOT: Map<u8, String> = Map::new(MERKLE_ROOT_PREFIX);

pub const CLAIM_PREFIX: &str = "claim";
pub const CLAIM: Map<(&Addr, u8), bool> = Map::new(CLAIM_PREFIX);

pub const CLAIMED_AMOUNT_PREFIX: &str = "claimed_amount";
pub const CLAIMED_AMOUNT: Map<(&Addr, u8), bool> = Map::new(CLAIMED_AMOUNT_PREFIX);
