#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure_eq, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
    WasmMsg,
};
use cw2::set_contract_version;
use cw20::Cw20ExecuteMsg;
use cw_utils::ensure_from_older_version;

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, RecipientExecuteMsg,
};
use crate::state::{Config, CONFIG};

const CONTRACT_NAME: &str = "crates.io:wynd-distribution";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.save(
        deps.storage,
        &Config {
            cw20_contract: deps.api.addr_validate(&msg.cw20_contract)?,
            epoch: msg.epoch,
            payment: msg.payment,
            recipient: deps.api.addr_validate(&msg.recipient)?,
            admin: deps.api.addr_validate(&msg.admin)?,
            last_payment: env.block.time.seconds(), // start now
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            cw20_contract,
            epoch,
            payment,
            recipient,
            admin,
        } => execute_update_config(deps, info, cw20_contract, epoch, payment, recipient, admin),
        ExecuteMsg::Payout {} => execute_payout(deps, env),
    }
}

fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    cw20_contract: Option<String>,
    epoch: Option<u64>,
    payment: Option<Uint128>,
    recipient: Option<String>,
    admin: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // check if sender is current admin
    ensure_eq!(info.sender, config.admin, ContractError::Unauthorized);

    let mut resp = Response::new().add_attribute("action", "update_config");

    if let Some(cw20_contract) = cw20_contract {
        config.cw20_contract = deps.api.addr_validate(&cw20_contract)?;
        resp = resp.add_attribute("cw20_contract", cw20_contract);
    }
    if let Some(epoch) = epoch {
        config.epoch = epoch;
        resp = resp.add_attribute("epoch", epoch.to_string());
    }
    if let Some(payment) = payment {
        config.payment = payment;
        resp = resp.add_attribute("payment", config.payment);
    }
    if let Some(recipient) = recipient {
        config.recipient = deps.api.addr_validate(&recipient)?;
        resp = resp.add_attribute("recipient", &config.recipient);
    }
    if let Some(admin) = admin {
        config.admin = deps.api.addr_validate(&admin)?;
        resp = resp.add_attribute("admin", &config.admin);
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(resp.add_attribute("sender", info.sender))
}

fn execute_payout(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    // check how long has elapsed
    let elapsed = env.block.time.seconds() - cfg.last_payment;
    let epochs = elapsed / cfg.epoch;
    if epochs == 0 {
        return Err(ContractError::EpochNotComplete);
    }

    // update the last payment tracker
    cfg.last_payment += epochs * cfg.epoch;
    CONFIG.save(deps.storage, &cfg)?;

    // pay out the value
    if cfg.payment.is_zero() {
        // nothing to do
        return Ok(Response::new());
    }

    let recipient = cfg.recipient.to_string();
    let amount = cfg.payment * Uint128::from(epochs);

    // transfer the amount to recipient using the cw20 contract
    let transfer_msg = WasmMsg::Execute {
        contract_addr: cfg.cw20_contract.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer { recipient, amount })?,
        funds: vec![],
    };

    // cause the recipient to distribute it
    let distribute_msg = WasmMsg::Execute {
        contract_addr: cfg.recipient.to_string(),
        msg: to_binary(&RecipientExecuteMsg::DistributeRewards { sender: None })?,
        funds: vec![],
    };
    Ok(Response::new()
        .add_message(transfer_msg)
        .add_message(distribute_msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    Ok(ConfigResponse {
        config: CONFIG.load(deps.storage)?,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{Addr, Uint128};

    use crate::contract::{execute, query_config};
    use crate::msg::ExecuteMsg;
    use crate::ContractError;
    use crate::{contract::instantiate, msg::InstantiateMsg, state::Config};

    #[test]
    fn instantiate_and_update_save_config() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let admin_info = mock_info("admin", &[]);
        // instantiate contract with config
        instantiate(
            deps.as_mut(),
            env.clone(),
            admin_info.clone(),
            InstantiateMsg {
                cw20_contract: "token".to_string(),
                epoch: 1,
                payment: Uint128::one(),
                recipient: "recipient".to_string(),
                admin: "admin".to_string(),
            },
        )
        .unwrap();
        let original_config = Config {
            cw20_contract: Addr::unchecked("token"),
            epoch: 1,
            payment: Uint128::new(1),
            recipient: Addr::unchecked("recipient"),
            admin: Addr::unchecked("admin"),
            last_payment: env.block.time.seconds(),
        };

        // check that it is set
        let result = query_config(deps.as_ref()).unwrap();
        assert_eq!(
            result.config, original_config,
            "instantiate should set the config"
        );

        // updating config as non-admin should error
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user", &[]),
            ExecuteMsg::UpdateConfig {
                cw20_contract: None,
                epoch: None,
                payment: None,
                recipient: None,
                admin: Some("user".into()),
            },
        )
        .unwrap_err();
        assert!(matches!(res, ContractError::Unauthorized));

        // check that it is still the same
        let result = query_config(deps.as_ref()).unwrap();
        assert_eq!(
            result.config, original_config,
            "instantiate should set the config"
        );

        // update config as admin should work
        let res = execute(
            deps.as_mut(),
            env,
            admin_info,
            ExecuteMsg::UpdateConfig {
                cw20_contract: None,
                epoch: Some(2),
                payment: None,
                recipient: None,
                admin: None,
            },
        )
        .unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(
            ("epoch", "2"),
            (
                res.attributes[1].key.as_str(),
                res.attributes[1].value.as_str()
            )
        );

        // check that it is set now
        let mut new_expected_config = original_config;
        new_expected_config.epoch = 2;
        let result = query_config(deps.as_ref()).unwrap();
        assert_eq!(
            result.config, new_expected_config,
            "instantiate should set the config"
        );
    }
}
