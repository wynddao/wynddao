use crate::error::ContractError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Initialization message that only setup an owner
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// The address who can add decisions to the log
    pub owner: String,
}

/// Execute message enumeration
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Store a Decision
    Record(RecordMsg),
}

/// Represents a Decision track
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
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
    /// ## Description
    /// Sanity check of received [`RecordMsg`].
    /// This will check if interna fields are valid
    /// Returns a [`Empty`] on successful,
    /// or a [`ContractError`] if the contract was not created.
    /// # Examples
    ///
    /// ```rust
    /// use wynd_decisions::msg::RecordMsg;
    /// use wynd_decisions::error::ContractError;
    /// let record: RecordMsg = RecordMsg {
    ///     title: String::from("title"),
    ///     body: String::from("description"),
    ///     url: Some(String::from("wrong url")),
    ///     hash: Some(String::from("HASH")),
    /// };
    /// let error: ContractError = record.validate().unwrap_err();
    /// println!("{}",error.to_string());
    /// assert!(error.to_string() == String::from("Body must be between 20 and 9192 characters"));
    /// ```
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

/// Query input Message enumeration
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Query for an specific Decision represented by an ID
    Decision {
        /// Decision ID
        id: u64,
    },
    /// Query all Decision makes using pagination as optional
    ListDecisions {
        /// ID to start from. If None, it will start from 1
        start_after: Option<u64>,
        /// Represents how many rows will return the [`DecisionResponse`]
        limit: Option<u32>,
    },
}

/// Decision Response that may contain the public IPFS link or private hash for the document
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema, Debug)]
pub struct DecisionResponse {
    /// Decision UID
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

/// Decision Response list wrapper
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema, Debug)]
pub struct ListDecisionsResponse {
    /// Decision Response list
    pub decisions: Vec<DecisionResponse>,
}

/// Message that is passed during migration
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct MigrateMsg {}
