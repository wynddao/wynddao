use crate::ContractError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// The address who can add decisions to the log
    pub owner: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Record(RecordMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RecordMsg {
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

impl RecordMsg {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.title.len() < 4 || self.title.len() > 128 {
            return Err(ContractError::InvalidLength("Title", 4, 128));
        }
        if self.body.len() < 20 || self.title.len() > 9192 {
            return Err(ContractError::InvalidLength("Body", 20, 9192));
        }
        if let Some(url) = &self.url {
            if url.len() < 3 || url.len() > 1024 {
                return Err(ContractError::InvalidLength("Url", 3, 1024));
            }
        }
        if let Some(hash) = &self.hash {
            if hash.len() < 20 || hash.len() > 128 {
                return Err(ContractError::InvalidLength("Hash", 20, 128));
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Decision {
        id: u64,
    },
    ListDecisions {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct DecisionResponse {
    pub id: u64,
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

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListDecisionsResponse {
    pub decisions: Vec<DecisionResponse>,
}
