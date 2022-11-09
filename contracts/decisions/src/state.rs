use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::msg::DecisionResponse;
use cosmwasm_std::{Addr, Deps, Order, StdResult};
use cw_storage_plus::{Item, Map};

/// Configuration Item
pub const CONFIG: Item<Config> = Item::new("config");

/// Desicion Map <Decision ID, Decision>
pub const DECISIONS: Map<u64, Decision> = Map::new("decisions");

/// Configuration
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct Config {
    /// contract owner, wynd foundation
    pub owner: Addr,
}

/// Decision
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct Decision {
    /// Creation time as unix time stamp (in seconds)
    pub created: u64,
    /// Title of the decision
    pub title: String,
    /// Text body of the decision
    pub body: String,
    /// Optional off-chain URL to PDF document or other support. Ideally immutable IPFS link
    pub url: Option<String>,
    /// Optional document hash. Intended when this refers to a privately shared document
    /// in order to assert which version was approved.
    pub hash: Option<String>,
}

impl Decision {
    /// ## Description
    /// Return a [`DecisionResponse`] from [`Decision`].
    ///
    /// Returns a new object [`DecisionResponse`].
    /// ## Arguments
    /// * `id` - unique id that index a Decision.
    pub fn into_response(self, id: u64) -> DecisionResponse {
        DecisionResponse {
            id,
            created: self.created,
            title: self.title,
            body: self.body,
            url: self.url,
            hash: self.hash,
        }
    }
}

/// Returns the last recorded decision id (auto-incremented count)
pub fn last_decision(deps: Deps) -> StdResult<u64> {
    DECISIONS
        .keys(deps.storage, None, None, Order::Descending)
        .next()
        .unwrap_or(Ok(0))
}
