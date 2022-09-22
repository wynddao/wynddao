use cosmwasm_std::{Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdResult};
use cw_multi_test::{Contract, ContractWrapper};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::msg::RecipientExecuteMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecipientQuery {}

fn instantiate(_deps: DepsMut, _env: Env, _info: MessageInfo, _msg: Empty) -> StdResult<Response> {
    Ok(Response::default())
}

fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: RecipientExecuteMsg,
) -> StdResult<Response> {
    match msg {
        RecipientExecuteMsg::DistributeRewards { .. } => {}
    }
    Ok(Response::new())
}

fn query(_deps: Deps, _env: Env, _msg: RecipientQuery) -> StdResult<Binary> {
    todo!("save distributed rewards and allow querying it")
}

pub(crate) fn mock_recipient() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query);

    Box::new(contract)
}
