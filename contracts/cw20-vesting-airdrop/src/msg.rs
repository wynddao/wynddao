use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Uint128;
use cw_utils::{Expiration, Scheduled};
use wynd_utils::ScalableCurve;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InstantiateMsg {
    /// Owner if none set to info.sender.
    pub owner: Option<String>,
    pub cw20_token_address: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    UpdateConfig {
        /// NewOwner if non sent, contract gets locked. Recipients can receive airdrops
        /// but owner cannot register new stages.
        new_owner: Option<String>,
    },
    RegisterMerkleRoot {
        /// MerkleRoot is hex-encoded merkle root.
        merkle_root: String,
        expiration: Expiration,
        start: Scheduled,
        total_amount: Uint128,
        vesting: Option<ScalableCurve>,
    },
    /// Claim does not check if contract has enough funds, owner must ensure it.
    Claim {
        stage: u8,
        amount: Uint128,
        /// Proof is hex-encoded merkle proof.
        proof: Vec<String>,
    },
    /// Burn the remaining tokens after expire time (only owner)
    Burn { stage: u8 },
    /// Recycle the remaining tokens to specified address after expire time (only owner).
    /// Don't use Option<String> to avoid typo turning ClawBack into Burn
    ClawBack { stage: u8, recipient: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    MerkleRoot { stage: u8 },
    LatestStage {},
    IsClaimed { stage: u8, address: String },
    TotalClaimed { stage: u8 },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ConfigResponse {
    pub owner: Option<String>,
    pub cw20_token_address: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MerkleRootResponse {
    pub stage: u8,
    /// MerkleRoot is hex-encoded merkle root.
    pub merkle_root: String,
    pub expiration: Expiration,
    pub start: Scheduled,
    pub vesting: Option<ScalableCurve>,
    pub total_amount: Uint128,
    pub claimed_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct LatestStageResponse {
    pub latest_stage: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct IsClaimedResponse {
    pub is_claimed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct TotalClaimedResponse {
    pub total: Uint128,
    pub claimed: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

// some test helpers
#[cfg(test)]
impl ExecuteMsg {
    pub(crate) fn default_start() -> Scheduled {
        use cosmwasm_std::testing::mock_env;
        let env = mock_env();
        Scheduled::AtHeight(env.block.height)
    }

    pub(crate) fn default_total() -> u128 {
        123000000
    }

    // this is a slightly more powerful helper for testing
    pub(crate) fn default_merkle_root(root: impl Into<String>) -> Self {
        Self::register_merkle_root(root, Self::default_total(), None, None, None)
    }

    // this is a slightly more powerful helper for testing
    pub(crate) fn register_merkle_root(
        root: impl Into<String>,
        amount: u128,
        expiration: Option<Expiration>,
        start: Option<Scheduled>,
        vesting: Option<ScalableCurve>,
    ) -> Self {
        ExecuteMsg::RegisterMerkleRoot {
            merkle_root: root.into(),
            expiration: expiration.unwrap_or(Expiration::Never {}),
            start: start.unwrap_or_else(Self::default_start),
            total_amount: Uint128::new(amount),
            vesting,
        }
    }
}
