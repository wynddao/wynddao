use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    from_binary, to_binary, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdError,
    StdResult, Uint128,
};
use cw_multi_test::{Contract, ContractWrapper};
use cw_storage_plus::Item;

use crate::receive_delegate::Cw20ReceiveDelegationMsg;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    ReceiveDelegation(Cw20ReceiveDelegationMsg),
    Undelegate { amount: Uint128 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegateMsg {
    Delegate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // I don't count delegated amount per user, because it's not required for simple mock contract
    Delegated {},
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmptyMsg {}

const DELEGATED: Item<Uint128> = Item::new("DELEGATED");

fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: Empty,
) -> Result<Response, StdError> {
    DELEGATED.save(deps.storage, &Uint128::zero())?;
    Ok(Response::default())
}

fn execute(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, StdError> {
    match msg {
        ExecuteMsg::ReceiveDelegation(wrapped) => {
            let amount = wrapped.amount;
            let msg: DelegateMsg = from_binary(&wrapped.msg)?;
            match msg {
                DelegateMsg::Delegate => {
                    DELEGATED.update(deps.storage, |sum| -> StdResult<_> { Ok(sum + amount) })?
                }
            };
        }
        ExecuteMsg::Undelegate { amount } => {
            DELEGATED.update(deps.storage, |sum| -> StdResult<_> { Ok(sum - amount) })?;
        }
    }
    Ok(Response::new())
}

fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, StdError> {
    match msg {
        QueryMsg::Delegated {} => to_binary(&DELEGATED.may_load(deps.storage)?.unwrap_or_default()),
    }
}

pub fn staking_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query);
    Box::new(contract)
}
