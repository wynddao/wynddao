#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure_eq, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response, StdResult,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;
use cw_utils::{ensure_from_older_version, nonpayable};

use crate::error::ContractError;
use crate::msg::{
    DecisionResponse, ExecuteMsg, InstantiateMsg, ListDecisionsResponse, MigrateMsg, QueryMsg,
    RecordMsg,
};
use crate::state::{last_decision, Config, Decision, CONFIG, DECISIONS};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:wynd-decisions";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
/// This will set up the owner of the Decision Recrd contract
///
/// Returns a [`Response`] with the specified attributes if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Arguments
/// * `deps` - A [`DepsMut`] that contains the dependencies.
///
/// * `_env` - The [`Env`] of the blockchain.
///
/// * `_info` - The [`MessageInfo`] from the contract instantiator.
///
/// * `msg` - A [`InstantiateMsg`] which contains the parameters for creating the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let owner = deps.api.addr_validate(&msg.owner)?;
    CONFIG.save(deps.storage, &Config { owner })?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", msg.owner))
}

/// ## Description
/// Exposes all the execute functions available in the contract.
/// ## Arguments
/// * `deps` - A [`DepsMut`] that contains the dependencies.
///
/// * `env` - The [`Env`] of the blockchain.
///
/// * `info` - A [`MessageInfo`] that contains the message information.
///
/// * `msg` - The [`ExecuteMsg`] to run.
///
/// ## Execution Messages
/// * **ExecuteMsg::Record** Allow to store a decision.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Record(msg) => record(deps, env, info, msg),
    }
}

/// Write the decision if called by owner
fn record(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    record: RecordMsg,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;
    let cfg = CONFIG.load(deps.storage)?;
    ensure_eq!(cfg.owner, info.sender, ContractError::Unauthorized);

    record.validate()?;

    // record this in the next available slot
    let id = last_decision(deps.as_ref())? + 1;
    let decision = Decision {
        created: env.block.time.seconds(),
        title: record.title.clone(),
        body: record.body,
        url: record.url,
        hash: record.hash,
    };
    DECISIONS.save(deps.storage, id, &decision)?;

    Ok(Response::new()
        .add_attribute("method", "record")
        .add_attribute("title", record.title))
}

/// Query enumeration used to get an specific or all decisions
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Decision { id } => to_binary(&query_decision(deps, id)?),
        QueryMsg::ListDecisions { start_after, limit } => {
            to_binary(&list_decisions(deps, start_after, limit)?)
        }
    }
}

fn query_decision(deps: Deps, id: u64) -> StdResult<DecisionResponse> {
    Ok(DECISIONS.load(deps.storage, id)?.into_response(id))
}

// settings for pagination
const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 30;

fn list_decisions(
    deps: Deps,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ListDecisionsResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    let decisions = DECISIONS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (id, dec) = item?;
            Ok(dec.into_response(id))
        })
        .collect::<StdResult<Vec<_>>>()?;
    Ok(ListDecisionsResponse { decisions })
}

/// Entry point for migration
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::Timestamp;

    #[test]
    fn happy_path() {
        let mut deps = mock_dependencies();
        let owner = "the-man";

        // init
        let info = mock_info("someone", &[]);
        let msg = InstantiateMsg {
            owner: owner.to_string(),
        };
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // record something
        let record = RecordMsg {
            title: "My awesome decision".to_string(),
            body: "Let's all go to the beach and enjoy the sun!".to_string(),
            url: Some("https://ipfs.com/1234567890".to_string()),
            hash: None,
        };
        let time1 = 111_222_333;
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(time1);
        let msg = ExecuteMsg::Record(record.clone());
        execute(deps.as_mut(), env, mock_info(owner, &[]), msg).unwrap();

        // record second decision
        let record2 = RecordMsg {
            title: "One more thing".to_string(),
            body: "John will bring a twelve pack for us all".to_string(),
            url: None,
            hash: Some("deadbeef00deadbeef00deadbeef".to_string()),
        };
        let time2 = 111_444_555;
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(time2);
        let msg = ExecuteMsg::Record(record2.clone());
        execute(deps.as_mut(), env, mock_info(owner, &[]), msg).unwrap();

        // what do we expect?
        let expected1 = DecisionResponse {
            id: 1,
            created: time1,
            title: record.title,
            body: record.body,
            url: record.url,
            hash: record.hash,
        };
        let expected2 = DecisionResponse {
            id: 2,
            created: time2,
            title: record2.title,
            body: record2.body,
            url: record2.url,
            hash: record2.hash,
        };

        let dec1 = query_decision(deps.as_ref(), 1).unwrap();
        assert_eq!(dec1, expected1);
        let dec2 = query_decision(deps.as_ref(), 2).unwrap();
        assert_eq!(dec2, expected2);

        let all = list_decisions(deps.as_ref(), None, None).unwrap();
        assert_eq!(all.decisions, vec![expected1, expected2]);
    }
}
