#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
    Uint128,
};

use cw2::set_contract_version;
use cw20::{
    BalanceResponse, Cw20ReceiveMsg, DownloadLogoResponse, EmbeddedLogo, Logo, LogoInfo,
    MarketingInfoResponse, TokenInfoResponse,
};

use cw_utils::ensure_from_older_version;
use wynd_utils::Curve;

use crate::allowances::{
    execute_burn_from, execute_decrease_allowance, execute_increase_allowance, execute_send_from,
    execute_transfer_from, query_allowance,
};
use crate::enumerable::{query_all_accounts, query_all_allowances};
use crate::error::ContractError;
use crate::msg::{
    assert_schedule_vests_amount, fully_vested, DelegatedResponse, ExecuteMsg, InitBalance,
    InstantiateMsg, MaxVestingComplexityResponse, MigrateMsg, MinterResponse, QueryMsg,
    StakingAddressResponse, VestingAllowListResponse, VestingResponse,
};
use crate::receive_delegate::Cw20ReceiveDelegationMsg;
use crate::state::{
    deduct_coins, MinterData, TokenInfo, ALLOWLIST, BALANCES, DELEGATED, LOGO, MARKETING_INFO,
    MAX_VESTING_COMPLEXITY, STAKING, TOKEN_INFO, VESTING,
};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw20-vesting";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const LOGO_SIZE_CAP: usize = 5 * 1024;

/// Checks if data starts with XML preamble
fn verify_xml_preamble(data: &[u8]) -> Result<(), ContractError> {
    // The easiest way to perform this check would be just match on regex, however regex
    // compilation is heavy and probably not worth it.

    let preamble = data
        .split_inclusive(|c| *c == b'>')
        .next()
        .ok_or(ContractError::InvalidXmlPreamble {})?;

    const PREFIX: &[u8] = b"<?xml ";
    const POSTFIX: &[u8] = b"?>";

    if !(preamble.starts_with(PREFIX) && preamble.ends_with(POSTFIX)) {
        Err(ContractError::InvalidXmlPreamble {})
    } else {
        Ok(())
    }

    // Additionally attributes format could be validated as they are well defined, as well as
    // comments presence inside of preable, but it is probably not worth it.
}

/// Validates XML logo
fn verify_xml_logo(logo: &[u8]) -> Result<(), ContractError> {
    verify_xml_preamble(logo)?;

    if logo.len() > LOGO_SIZE_CAP {
        Err(ContractError::LogoTooBig {})
    } else {
        Ok(())
    }
}

/// Validates png logo
fn verify_png_logo(logo: &[u8]) -> Result<(), ContractError> {
    // PNG header format:
    // 0x89 - magic byte, out of ASCII table to fail on 7-bit systems
    // "PNG" ascii representation
    // [0x0d, 0x0a] - dos style line ending
    // 0x1a - dos control character, stop displaying rest of the file
    // 0x0a - unix style line ending
    const HEADER: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if logo.len() > LOGO_SIZE_CAP {
        Err(ContractError::LogoTooBig {})
    } else if !logo.starts_with(&HEADER) {
        Err(ContractError::InvalidPngHeader {})
    } else {
        Ok(())
    }
}

/// Checks if passed logo is correct, and if not, returns an error
fn verify_logo(logo: &Logo) -> Result<(), ContractError> {
    match logo {
        Logo::Embedded(EmbeddedLogo::Svg(logo)) => verify_xml_logo(logo),
        Logo::Embedded(EmbeddedLogo::Png(logo)) => verify_png_logo(logo),
        Logo::Url(_) => Ok(()), // Any reasonable url validation would be regex based, probably not worth it
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    // check valid token info
    msg.validate()?;
    let cap = msg.get_cap(&env.block.time);

    // set maximum vesting complexity
    MAX_VESTING_COMPLEXITY.save(deps.storage, &msg.max_curve_complexity)?;

    // create initial accounts
    let total_supply = create_accounts(&mut deps, &env, msg.initial_balances)?;

    if let Some(limit) = cap {
        if total_supply > limit {
            return Err(StdError::generic_err("Initial supply greater than cap").into());
        }
    }

    let mint = match msg.mint {
        Some(m) => Some(MinterData {
            minter: deps.api.addr_validate(&m.minter)?,
            cap: m.cap,
        }),
        None => None,
    };

    // store token info
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply,
        mint,
    };
    TOKEN_INFO.save(deps.storage, &data)?;

    if let Some(marketing) = msg.marketing {
        let logo = if let Some(logo) = marketing.logo {
            verify_logo(&logo)?;
            LOGO.save(deps.storage, &logo)?;

            match logo {
                Logo::Url(url) => Some(LogoInfo::Url(url)),
                Logo::Embedded(_) => Some(LogoInfo::Embedded),
            }
        } else {
            None
        };

        let data = MarketingInfoResponse {
            project: marketing.project,
            description: marketing.description,
            marketing: marketing
                .marketing
                .map(|addr| deps.api.addr_validate(&addr))
                .transpose()?,
            logo,
        };
        MARKETING_INFO.save(deps.storage, &data)?;
    }

    // We initially add by default info.sender to the list
    let address_list = match msg.allowed_vesters {
        Some(addrs) => addrs
            .into_iter()
            .map(|a| deps.api.addr_validate(&a))
            .collect::<StdResult<_>>()?,
        None => vec![info.sender],
    };
    ALLOWLIST.save(deps.storage, &address_list)?;

    Ok(Response::default())
}

pub fn create_accounts(
    deps: &mut DepsMut,
    env: &Env,
    accounts: Vec<InitBalance>,
) -> Result<Uint128, ContractError> {
    validate_accounts(&accounts)?;

    let mut total_supply = Uint128::zero();
    for row in accounts.into_iter() {
        // ensure vesting schedule is valid
        let vesting = match &row.vesting {
            Some(s) => {
                assert_schedule_vests_amount(s, row.amount)?;
                if fully_vested(s, &env.block) {
                    None
                } else {
                    Some(s)
                }
            }
            None => None,
        };

        let address = deps.api.addr_validate(&row.address)?;
        if let Some(vest) = vesting {
            let max_complexity = MAX_VESTING_COMPLEXITY.load(deps.storage)?;
            vest.validate_complexity(max_complexity as usize)?;
            VESTING.save(deps.storage, &address, vest)?;
        }
        BALANCES.save(deps.storage, &address, &row.amount)?;
        total_supply += row.amount;
    }

    Ok(total_supply)
}

pub fn validate_accounts(accounts: &[InitBalance]) -> Result<(), ContractError> {
    let mut addresses = accounts.iter().map(|c| &c.address).collect::<Vec<_>>();
    addresses.sort();
    addresses.dedup();

    if addresses.len() != accounts.len() {
        Err(ContractError::DuplicateInitialBalanceAddresses {})
    } else {
        Ok(())
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }
        ExecuteMsg::TransferVesting {
            recipient,
            amount,
            schedule,
        } => execute_transfer_vesting(deps, env, info, recipient, amount, schedule),
        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, amount),
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => execute_send(deps, env, info, contract, amount, msg),
        ExecuteMsg::Mint { recipient, amount } => execute_mint(deps, env, info, recipient, amount),
        ExecuteMsg::UpdateMinter { minter } => execute_update_minter(deps, env, info, minter),
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_increase_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_decrease_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => execute_transfer_from(deps, env, info, owner, recipient, amount),
        ExecuteMsg::BurnFrom { owner, amount } => execute_burn_from(deps, env, info, owner, amount),
        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => execute_send_from(deps, env, info, owner, contract, amount, msg),
        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => execute_update_marketing(deps, env, info, project, description, marketing),
        ExecuteMsg::UploadLogo(logo) => execute_upload_logo(deps, env, info, logo),
        ExecuteMsg::AllowVester { address } => execute_add_address(deps, info, address),
        ExecuteMsg::DenyVester { address } => execute_remove_address(deps, info, address),
        ExecuteMsg::UpdateStakingAddress { address } => {
            execute_update_staking_address(deps, info, address)
        }
        ExecuteMsg::Delegate { amount, msg } => execute_delegate(deps, info, amount, msg),
        ExecuteMsg::Undelegate { recipient, amount } => {
            execute_undelegate(deps, env, info, recipient, amount)
        }
    }
}

pub fn execute_transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = deps.api.addr_validate(&recipient)?;

    // this will handle vesting checks as well
    deduct_coins(deps.storage, &env, &info.sender, amount)?;

    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("from", info.sender)
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

pub fn execute_transfer_vesting(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
    schedule: Curve,
) -> Result<Response, ContractError> {
    // info.sender must be at least on the allow_list to allow execute trasnfer vesting
    let allow_list = ALLOWLIST.load(deps.storage)?;
    if !allow_list.contains(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // ensure vesting schedule is valid
    assert_schedule_vests_amount(&schedule, amount)?;

    let rcpt_addr = deps.api.addr_validate(&recipient)?;

    // if it is not already fully vested, we store this
    if !fully_vested(&schedule, &env.block) {
        let max_complexity = MAX_VESTING_COMPLEXITY.load(deps.storage)?;
        VESTING.update(
            deps.storage,
            &rcpt_addr,
            |old| -> Result<_, ContractError> {
                let schedule = old.map(|old| old.combine(&schedule)).unwrap_or(schedule);
                // make sure the vesting curve does not get too complex, rendering the account useless
                schedule.validate_complexity(max_complexity as usize)?;
                Ok(schedule)
            },
        )?;
    }

    // this will handle vesting checks as well
    deduct_coins(deps.storage, &env, &info.sender, amount)?;

    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        // use same action as we want explorers to show this as a transfer
        .add_attribute("action", "transfer")
        .add_attribute("type", "vesting")
        .add_attribute("from", info.sender)
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // lower balance
    // this will handle vesting checks as well
    deduct_coins(deps.storage, &env, &info.sender, amount)?;
    // reduce total_supply
    TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = info.total_supply.checked_sub(amount)?;
        Ok(info)
    })?;

    let res = Response::new()
        .add_attribute("action", "burn")
        .add_attribute("from", info.sender)
        .add_attribute("amount", amount);
    Ok(res)
}

pub fn execute_mint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut config = TOKEN_INFO.load(deps.storage)?;
    if config.mint.is_none() || config.mint.as_ref().unwrap().minter != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    // update supply and enforce cap
    config.total_supply += amount;
    if let Some(limit) = config.get_cap(&env.block.time) {
        if config.total_supply > limit {
            return Err(ContractError::CannotExceedCap {});
        }
    }
    TOKEN_INFO.save(deps.storage, &config)?;

    // add amount to recipient balance
    let rcpt_addr = deps.api.addr_validate(&recipient)?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "mint")
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

pub fn execute_update_minter(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    minter: String,
) -> Result<Response, ContractError> {
    let mut config = TOKEN_INFO.load(deps.storage)?;
    let mint_addr = deps.api.addr_validate(&minter)?;

    match config.mint.as_mut() {
        Some(mut old) => {
            if old.minter != info.sender {
                return Err(ContractError::Unauthorized {});
            }
            old.minter = mint_addr;
        }
        None => return Err(ContractError::Unauthorized {}),
    };

    TOKEN_INFO.save(deps.storage, &config)?;

    let res = Response::new()
        .add_attribute("action", "update_minter")
        .add_attribute("minter", minter);
    Ok(res)
}

pub fn execute_send(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = deps.api.addr_validate(&contract)?;

    // move the tokens to the contract
    // this will handle vesting checks as well
    deduct_coins(deps.storage, &env, &info.sender, amount)?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "send")
        .add_attribute("from", &info.sender)
        .add_attribute("to", &contract)
        .add_attribute("amount", amount)
        .add_message(
            Cw20ReceiveMsg {
                sender: info.sender.into(),
                amount,
                msg,
            }
            .into_cosmos_msg(contract)?,
        );
    Ok(res)
}

pub fn execute_update_marketing(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    project: Option<String>,
    description: Option<String>,
    marketing: Option<String>,
) -> Result<Response, ContractError> {
    let mut marketing_info = MARKETING_INFO
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;

    if marketing_info
        .marketing
        .as_ref()
        .ok_or(ContractError::Unauthorized {})?
        != &info.sender
    {
        return Err(ContractError::Unauthorized {});
    }

    match project {
        Some(empty) if empty.trim().is_empty() => marketing_info.project = None,
        Some(project) => marketing_info.project = Some(project),
        None => (),
    }

    match description {
        Some(empty) if empty.trim().is_empty() => marketing_info.description = None,
        Some(description) => marketing_info.description = Some(description),
        None => (),
    }

    match marketing {
        Some(empty) if empty.trim().is_empty() => marketing_info.marketing = None,
        Some(marketing) => marketing_info.marketing = Some(deps.api.addr_validate(&marketing)?),
        None => (),
    }

    if marketing_info.project.is_none()
        && marketing_info.description.is_none()
        && marketing_info.marketing.is_none()
        && marketing_info.logo.is_none()
    {
        MARKETING_INFO.remove(deps.storage);
    } else {
        MARKETING_INFO.save(deps.storage, &marketing_info)?;
    }

    let res = Response::new().add_attribute("action", "update_marketing");
    Ok(res)
}

pub fn execute_upload_logo(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    logo: Logo,
) -> Result<Response, ContractError> {
    let mut marketing_info = MARKETING_INFO
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;

    verify_logo(&logo)?;

    if marketing_info
        .marketing
        .as_ref()
        .ok_or(ContractError::Unauthorized {})?
        != &info.sender
    {
        return Err(ContractError::Unauthorized {});
    }

    LOGO.save(deps.storage, &logo)?;

    let logo_info = match logo {
        Logo::Url(url) => LogoInfo::Url(url),
        Logo::Embedded(_) => LogoInfo::Embedded,
    };

    marketing_info.logo = Some(logo_info);
    MARKETING_INFO.save(deps.storage, &marketing_info)?;

    let res = Response::new().add_attribute("action", "upload_logo");
    Ok(res)
}

pub fn execute_add_address(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    // info.sender must be at least on the allow_list to add address to the list
    let mut allow_list = ALLOWLIST.load(deps.storage)?;
    if !allow_list.contains(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    // validate address and ensure unique
    let addr = deps.api.addr_validate(&address)?;
    if allow_list.contains(&addr) {
        return Err(ContractError::AddressAlreadyExist {});
    }

    // Add the new address to the allow list
    allow_list.push(addr);
    ALLOWLIST.save(deps.storage, &allow_list)?;

    let res = Response::new().add_attribute("action", "add address");
    Ok(res)
}

pub fn execute_remove_address(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    // info.sender must be at least on the allow_list to remove address to the list
    let allow_list = ALLOWLIST.load(deps.storage)?;
    if !allow_list.contains(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    // validate address and remove
    let addr = deps.api.addr_validate(&address)?;
    let prev_len = allow_list.len();
    let allow_list: Vec<Addr> = allow_list
        .into_iter()
        .filter(|item| *item != addr)
        .collect();

    // ensure it was found and left something
    if prev_len == allow_list.len() {
        return Err(ContractError::AddressNotFound {});
    }
    if allow_list.is_empty() {
        return Err(ContractError::AtLeastOneAddressMustExist {});
    }

    ALLOWLIST.save(deps.storage, &allow_list)?;
    let res = Response::new().add_attribute("action", "remove address");
    Ok(res)
}

pub fn execute_update_staking_address(
    deps: DepsMut,
    info: MessageInfo,
    staking: String,
) -> Result<Response, ContractError> {
    // Staking address can be updated only once
    // If load is failing, it means it wasn't set before
    match STAKING.load(deps.storage) {
        Ok(_) => Err(ContractError::StakingAddressAlreadyUpdated {}),
        Err(_) => {
            if let Some(mint) = TOKEN_INFO.load(deps.storage)?.mint {
                if info.sender == mint.minter {
                    let staking_address = deps.api.addr_validate(&staking)?;
                    STAKING.save(deps.storage, &staking_address)?;
                    Ok(Response::new().add_attribute("update staking address", staking))
                } else {
                    Err(ContractError::UnauthorizedUpdateStakingAddress {})
                }
            } else {
                Err(ContractError::MinterAddressNotSet {})
            }
        }
    }
}

pub fn execute_delegate(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let token_address = match STAKING.load(deps.storage) {
        Ok(address) => address,
        Err(_) => return Err(ContractError::StakingAddressNotSet {}),
    };

    // this allows to delegate also vested tokens, because vested is included in balance anyway
    BALANCES.update(deps.storage, &info.sender, |balance| {
        let balance = balance.unwrap_or_default();
        balance
            .checked_sub(amount)
            .map_err(|_| ContractError::NotEnoughToDelegate)
    })?;
    // make sure we add it to the other side
    BALANCES.update(deps.storage, &token_address, |balance| -> StdResult<_> {
        let balance = balance.unwrap_or_default() + amount;
        Ok(balance)
    })?;

    DELEGATED.update(
        deps.storage,
        &info.sender,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "delegate")
        .add_attribute("from", &info.sender)
        .add_attribute("to", &token_address)
        .add_attribute("amount", amount)
        .add_message(
            Cw20ReceiveDelegationMsg {
                sender: info.sender.into(),
                amount,
                msg,
            }
            .into_cosmos_msg(token_address)?,
        );
    Ok(res)
}

pub fn execute_undelegate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    match STAKING.load(deps.storage) {
        Ok(staking) => {
            if staking != info.sender {
                return Err(ContractError::UnauthorizedUndelegate {});
            }
        }
        Err(_) => return Err(ContractError::StakingAddressNotSet {}),
    };

    let recipient_address = deps.api.addr_validate(&recipient)?;

    if !DELEGATED.has(deps.storage, &recipient_address) {
        return Err(ContractError::NoTokensDelegated {});
    }
    DELEGATED.update(
        deps.storage,
        &recipient_address,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    deduct_coins(deps.storage, &env, &info.sender, amount)?;
    BALANCES.update(
        deps.storage,
        &recipient_address,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "undelegate")
        .add_attribute("from", &info.sender)
        .add_attribute("to", &recipient_address)
        .add_attribute("amount", amount);
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::Vesting { address } => to_binary(&query_vesting(deps, env, address)?),
        QueryMsg::Delegated { address } => to_binary(&query_delegated(deps, address)?),
        QueryMsg::VestingAllowList {} => to_binary(&query_allow_list(deps)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        QueryMsg::MaxVestingComplexity {} => to_binary(&query_max_complexity(deps)?),
        QueryMsg::Minter {} => to_binary(&query_minter(deps, env)?),
        QueryMsg::Allowance { owner, spender } => {
            to_binary(&query_allowance(deps, owner, spender)?)
        }
        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => to_binary(&query_all_allowances(deps, owner, start_after, limit)?),
        QueryMsg::AllAccounts { start_after, limit } => {
            to_binary(&query_all_accounts(deps, start_after, limit)?)
        }
        QueryMsg::MarketingInfo {} => to_binary(&query_marketing_info(deps)?),
        QueryMsg::DownloadLogo {} => to_binary(&query_download_logo(deps)?),
        QueryMsg::StakingAddress {} => to_binary(&query_staking_address(deps)?),
    }
}

pub fn query_balance(deps: Deps, address: String) -> StdResult<BalanceResponse> {
    let address = deps.api.addr_validate(&address)?;
    let balance = BALANCES
        .may_load(deps.storage, &address)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

pub fn query_vesting(deps: Deps, env: Env, address: String) -> StdResult<VestingResponse> {
    let address = deps.api.addr_validate(&address)?;
    let schedule = VESTING.may_load(deps.storage, &address)?;
    let time = env.block.time.seconds();
    let locked = schedule.as_ref().map(|c| c.value(time)).unwrap_or_default();
    Ok(VestingResponse { schedule, locked })
}

pub fn query_delegated(deps: Deps, address: String) -> StdResult<DelegatedResponse> {
    let address = deps.api.addr_validate(&address)?;
    let delegated = DELEGATED
        .may_load(deps.storage, &address)?
        .unwrap_or_default();
    Ok(DelegatedResponse { delegated })
}

pub fn query_token_info(deps: Deps) -> StdResult<TokenInfoResponse> {
    let info = TOKEN_INFO.load(deps.storage)?;
    let res = TokenInfoResponse {
        name: info.name,
        symbol: info.symbol,
        decimals: info.decimals,
        total_supply: info.total_supply,
    };
    Ok(res)
}

pub fn query_max_complexity(deps: Deps) -> StdResult<MaxVestingComplexityResponse> {
    let complexity = MAX_VESTING_COMPLEXITY.load(deps.storage)?;
    Ok(MaxVestingComplexityResponse { complexity })
}

pub fn query_minter(deps: Deps, env: Env) -> StdResult<Option<MinterResponse>> {
    let meta = TOKEN_INFO.load(deps.storage)?;
    let minter = match meta.mint {
        Some(m) => {
            let current_cap = m.cap.as_ref().map(|v| v.value(env.block.time.seconds()));
            Some(MinterResponse {
                minter: m.minter.into(),
                cap: m.cap,
                current_cap,
            })
        }
        None => None,
    };
    Ok(minter)
}

pub fn query_marketing_info(deps: Deps) -> StdResult<MarketingInfoResponse> {
    Ok(MARKETING_INFO.may_load(deps.storage)?.unwrap_or_default())
}

pub fn query_allow_list(deps: Deps) -> StdResult<VestingAllowListResponse> {
    let allow_list = ALLOWLIST
        .load(deps.storage)?
        .into_iter()
        .map(|a| a.into())
        .collect();
    Ok(VestingAllowListResponse { allow_list })
}

pub fn query_download_logo(deps: Deps) -> StdResult<DownloadLogoResponse> {
    let logo = LOGO.load(deps.storage)?;
    match logo {
        Logo::Embedded(EmbeddedLogo::Svg(logo)) => Ok(DownloadLogoResponse {
            mime_type: "image/svg+xml".to_owned(),
            data: logo,
        }),
        Logo::Embedded(EmbeddedLogo::Png(logo)) => Ok(DownloadLogoResponse {
            mime_type: "image/png".to_owned(),
            data: logo,
        }),
        Logo::Url(_) => Err(StdError::not_found("logo")),
    }
}

pub fn query_staking_address(deps: Deps) -> StdResult<StakingAddressResponse> {
    let address = STAKING.may_load(deps.storage)?;
    Ok(StakingAddressResponse { address })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // make sure picewise linear curve is passed in the message
    match msg.picewise_linear_curve {
        Curve::PiecewiseLinear(_) => (),
        _ => {
            return Err(ContractError::MigrationIncorrectCurve {});
        }
    };

    TOKEN_INFO.update(deps.storage, |mut token_info| -> StdResult<_> {
        // We can unwrap because we know cap is set
        token_info.mint.as_mut().unwrap().cap = Some(msg.picewise_linear_curve);
        Ok(token_info)
    })?;

    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{
        mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info,
    };
    use cosmwasm_std::{attr, coins, from_binary, Addr, CosmosMsg, StdError, SubMsg, WasmMsg};
    use wynd_utils::{Curve, CurveError, PiecewiseLinear};

    use super::*;
    use crate::msg::{InstantiateMarketingInfo, MinterInfo};

    fn get_balance<T: Into<String>>(deps: Deps, address: T) -> Uint128 {
        query_balance(deps, address.into()).unwrap().balance
    }

    fn constant_curve(val: Option<Uint128>) -> Option<Curve> {
        val.map(|y| Curve::Constant { y })
    }

    // this will set up the instantiation for other tests
    fn do_instantiate_with_minter(
        deps: DepsMut,
        addr: &str,
        amount: Uint128,
        minter: &str,
        cap: Option<Uint128>,
    ) -> TokenInfoResponse {
        _do_instantiate(
            deps,
            addr,
            amount,
            Some(MinterInfo {
                minter: minter.to_string(),
                cap: constant_curve(cap),
            }),
            None,
        )
    }

    // this will set up the instantiation for other tests
    fn do_instantiate(deps: DepsMut, addr: &str, amount: Uint128) -> TokenInfoResponse {
        _do_instantiate(deps, addr, amount, None, None)
    }

    // this will set up the instantiation for other tests
    fn _do_instantiate(
        mut deps: DepsMut,
        addr: &str,
        amount: Uint128,
        mint: Option<MinterInfo>,
        info: Option<MessageInfo>,
    ) -> TokenInfoResponse {
        let instantiate_msg = InstantiateMsg {
            name: "Auto Gen".to_string(),
            symbol: "AUTO".to_string(),
            decimals: 3,
            initial_balances: vec![InitBalance {
                address: addr.to_string(),
                amount,
                vesting: None,
            }],
            mint: mint.clone(),
            marketing: None,
            allowed_vesters: None,
            max_curve_complexity: 10,
        };
        let creator_info = match info {
            Some(info) => info,
            None => mock_info("creator", &[]),
        };
        //let info = mock_info("creator", &[]);
        let env = mock_env();
        let res = instantiate(deps.branch(), env, creator_info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        let meta = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(
            meta,
            TokenInfoResponse {
                name: "Auto Gen".to_string(),
                symbol: "AUTO".to_string(),
                decimals: 3,
                total_supply: amount,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), addr), amount);
        let qmint = query_minter(deps.as_ref(), mock_env()).unwrap();
        match mint {
            Some(m) => {
                let q = qmint.unwrap();
                assert_eq!(m.minter, q.minter);
                assert_eq!(m.cap, q.cap);
            }
            None => assert_eq!(qmint, None),
        }
        meta
    }

    const PNG_HEADER: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

    mod instantiate {
        use super::*;
        use crate::msg::MinterInfo;

        #[test]
        fn basic() {
            let mut deps = mock_dependencies();
            let amount = Uint128::from(11223344u128);
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![InitBalance {
                    address: String::from("addr0000"),
                    amount,
                    vesting: None,
                }],
                mint: None,
                marketing: None,
                allowed_vesters: None,
                max_curve_complexity: 10,
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
            assert_eq!(0, res.messages.len());

            assert_eq!(
                query_token_info(deps.as_ref()).unwrap(),
                TokenInfoResponse {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    total_supply: amount,
                }
            );
            assert_eq!(
                get_balance(deps.as_ref(), "addr0000"),
                Uint128::new(11223344)
            );
        }

        #[test]
        fn mintable_constant() {
            let mut deps = mock_dependencies();
            let amount = Uint128::new(11223344);
            let minter = String::from("asmodat");
            let y = Uint128::new(511223344);
            let limit = Curve::Constant { y };
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![InitBalance {
                    address: "addr0000".into(),
                    amount,
                    vesting: None,
                }],
                mint: Some(MinterInfo {
                    minter: minter.clone(),
                    cap: Some(limit.clone()),
                }),
                marketing: None,
                allowed_vesters: None,
                max_curve_complexity: 10,
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
            assert_eq!(0, res.messages.len());

            assert_eq!(
                query_token_info(deps.as_ref()).unwrap(),
                TokenInfoResponse {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    total_supply: amount,
                }
            );
            assert_eq!(
                get_balance(deps.as_ref(), "addr0000"),
                Uint128::new(11223344)
            );
            assert_eq!(
                query_minter(deps.as_ref(), mock_env()).unwrap(),
                Some(MinterResponse {
                    minter,
                    cap: Some(limit),
                    current_cap: Some(y),
                }),
            );
        }

        #[test]
        fn mintable_over_cap() {
            let mut deps = mock_dependencies();
            let amount = Uint128::new(11223344);
            let minter = String::from("asmodat");
            let y = Uint128::new(11223300);
            let limit = Curve::Constant { y };
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![InitBalance {
                    address: String::from("addr0000"),
                    amount,
                    vesting: None,
                }],
                mint: Some(MinterInfo {
                    minter,
                    cap: Some(limit),
                }),
                marketing: None,
                allowed_vesters: None,
                max_curve_complexity: 10,
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let err = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap_err();
            assert_eq!(
                err,
                StdError::generic_err("Initial supply greater than cap").into()
            );
        }

        #[test]
        fn init_vesting_accounts() {
            let mut deps = mock_dependencies();
            let addr1 = String::from("addr0001");
            let addr2 = String::from("addr0002");
            let amount = Uint128::new(11223344);
            let amount2 = Uint128::new(12345678);
            let start = mock_env().block.time.seconds();
            let end = start + 10_000;
            let schedule = Curve::saturating_linear((start, 10000000), (end, 0));

            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![
                    InitBalance {
                        address: addr1.clone(),
                        amount,
                        vesting: None,
                    },
                    InitBalance {
                        address: addr2.clone(),
                        amount: amount2,
                        vesting: Some(schedule.clone()),
                    },
                ],
                mint: None,
                marketing: None,
                allowed_vesters: None,
                max_curve_complexity: 10,
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
            assert_eq!(0, res.messages.len());

            // both have proper balance
            assert_eq!(get_balance(deps.as_ref(), &addr1), amount);
            assert_eq!(get_balance(deps.as_ref(), &addr2), amount2);

            // vesting is set for addr2
            let vesting = query_vesting(deps.as_ref(), mock_env(), addr2.clone()).unwrap();
            assert_eq!(vesting.locked, Uint128::new(10_000_000));
            assert_eq!(vesting.schedule.unwrap(), schedule);

            // and not on the original account
            let non = query_vesting(deps.as_ref(), mock_env(), addr1).unwrap();
            assert_eq!(non.locked, Uint128::zero());
            assert_eq!(non.schedule, None);

            // vesting changes over time, let's check half way through
            let mut middle = mock_env();
            middle.block.time = middle.block.time.plus_seconds(5_000);
            let vesting = query_vesting(deps.as_ref(), middle, addr2).unwrap();
            assert_eq!(vesting.locked, Uint128::new(5_000_000));
            assert_eq!(vesting.schedule.unwrap(), schedule);
        }

        #[test]
        fn init_complex_curve() {
            let mut deps = mock_dependencies();
            let addr1 = String::from("addr0001");
            let addr2 = String::from("addr0002");
            let amount = Uint128::new(11223344);
            let amount2 = Uint128::new(12345678);
            // curve is not fully vested yet and complexity is too high
            let start = mock_env().block.time.seconds();
            let complexity = 10_000;
            let steps: Vec<_> = (0..complexity)
                .map(|x| (start + x, amount2 - Uint128::from(x)))
                .chain(std::iter::once((start + complexity, Uint128::new(0)))) // fully vest
                .collect();
            let schedule = Curve::PiecewiseLinear(PiecewiseLinear {
                steps: steps.clone(),
            });

            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![
                    InitBalance {
                        address: addr1,
                        amount,
                        vesting: None,
                    },
                    InitBalance {
                        address: addr2.clone(),
                        amount: amount2,
                        vesting: Some(schedule),
                    },
                ],
                mint: None,
                marketing: None,
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            // should error because curve is too complex
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let error = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap_err();
            assert_eq!(ContractError::Curve(CurveError::TooComplex), error);

            // shift curve to the left, so it's fully vested already
            let steps = steps
                .into_iter()
                .map(|(x, y)| (x - complexity, y))
                .collect();
            let schedule = Curve::PiecewiseLinear(PiecewiseLinear { steps });

            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![InitBalance {
                    address: addr2.clone(),
                    amount: amount2,
                    vesting: Some(schedule),
                }],
                mint: None,
                marketing: None,
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            // should *not* error, even though curve is complex, because it's fully vested already
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
            assert_eq!(0, res.messages.len());

            // vesting is not set for addr2
            let vesting = query_vesting(deps.as_ref(), mock_env(), addr2).unwrap();
            assert_eq!(vesting.locked, Uint128::new(0));
            assert_eq!(vesting.schedule, None);
        }

        mod marketing {
            use super::*;

            #[test]
            fn basic() {
                let mut deps = mock_dependencies();
                let instantiate_msg = InstantiateMsg {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    initial_balances: vec![],
                    mint: None,
                    marketing: Some(InstantiateMarketingInfo {
                        project: Some("Project".to_owned()),
                        description: Some("Description".to_owned()),
                        marketing: Some("marketing".to_owned()),
                        logo: Some(Logo::Url("url".to_owned())),
                    }),
                    allowed_vesters: None,
                    max_curve_complexity: 10,
                };

                let info = mock_info("creator", &[]);
                let env = mock_env();
                let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
                assert_eq!(0, res.messages.len());

                assert_eq!(
                    query_marketing_info(deps.as_ref()).unwrap(),
                    MarketingInfoResponse {
                        project: Some("Project".to_owned()),
                        description: Some("Description".to_owned()),
                        marketing: Some(Addr::unchecked("marketing")),
                        logo: Some(LogoInfo::Url("url".to_owned())),
                    }
                );

                let err = query_download_logo(deps.as_ref()).unwrap_err();
                assert!(
                    matches!(err, StdError::NotFound { .. }),
                    "Expected StdError::NotFound, received {}",
                    err
                );
            }

            #[test]
            fn invalid_marketing() {
                let mut deps = mock_dependencies();
                let instantiate_msg = InstantiateMsg {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    initial_balances: vec![],
                    mint: None,
                    marketing: Some(InstantiateMarketingInfo {
                        project: Some("Project".to_owned()),
                        description: Some("Description".to_owned()),
                        marketing: Some("m".to_owned()),
                        logo: Some(Logo::Url("url".to_owned())),
                    }),
                    allowed_vesters: None,
                    max_curve_complexity: 10,
                };

                let info = mock_info("creator", &[]);
                let env = mock_env();
                instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap_err();

                let err = query_download_logo(deps.as_ref()).unwrap_err();
                assert!(
                    matches!(err, StdError::NotFound { .. }),
                    "Expected StdError::NotFound, received {}",
                    err
                );
            }
        }
    }

    #[test]
    fn can_mint_by_minter() {
        let mut deps = mock_dependencies();

        let genesis = String::from("genesis");
        let amount = Uint128::new(11223344);
        let minter = String::from("asmodat");
        let limit = Uint128::new(511223344);
        do_instantiate_with_minter(deps.as_mut(), &genesis, amount, &minter, Some(limit));

        // minter can mint coins to some winner
        let winner = String::from("lucky");
        let prize = Uint128::new(222_222_222);
        let msg = ExecuteMsg::Mint {
            recipient: winner.clone(),
            amount: prize,
        };

        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(get_balance(deps.as_ref(), genesis), amount);
        assert_eq!(get_balance(deps.as_ref(), winner.clone()), prize);

        // but cannot mint nothing
        let msg = ExecuteMsg::Mint {
            recipient: winner.clone(),
            amount: Uint128::zero(),
        };
        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // but if it exceeds cap (even over multiple rounds), it fails
        // cap is enforced
        let msg = ExecuteMsg::Mint {
            recipient: winner,
            amount: Uint128::new(333_222_222),
        };
        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::CannotExceedCap {});
    }

    #[test]
    fn minting_on_linear_curve() {
        let mut deps = mock_dependencies();

        let genesis = String::from("genesis");
        let amount = Uint128::new(95000);
        let minter = String::from("freddie");
        let start = mock_env().block.time.seconds();
        let limit = Curve::saturating_linear((start, 100_000), (start + 5000, 200_000));

        _do_instantiate(
            deps.as_mut(),
            &genesis,
            amount,
            Some(MinterInfo {
                minter: minter.to_string(),
                cap: Some(limit.clone()),
            }),
            None,
        );

        // can only mint up to initial cap (5_000 more)

        // minter can mint coins to some winner
        let winner = String::from("dupe");
        let prize = Uint128::new(5_000);
        let msg = ExecuteMsg::Mint {
            recipient: winner.clone(),
            amount: prize,
        };
        let info = mock_info(minter.as_ref(), &[]);
        execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

        // but any more is over cap
        let err = execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::CannotExceedCap {});

        // but if we wait a bit, we can easily mint this
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000); // this will open up 20_000 more for minting (20% of 100_000)
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // ensure winner got paid twice
        let bal = query_balance(deps.as_ref(), winner).unwrap();
        assert_eq!(bal.balance, Uint128::new(10_000));

        // check the mint query works properly
        let orig = query_minter(deps.as_ref(), mock_env()).unwrap().unwrap();
        assert_eq!(orig.cap.unwrap(), limit);
        assert_eq!(orig.current_cap.unwrap(), Uint128::new(100_000));
        // and higher after time passes
        let later = query_minter(deps.as_ref(), env).unwrap().unwrap();
        assert_eq!(later.cap.unwrap(), limit);
        assert_eq!(later.current_cap.unwrap(), Uint128::new(120_000));
    }

    #[test]
    fn others_cannot_mint() {
        let mut deps = mock_dependencies();
        do_instantiate_with_minter(
            deps.as_mut(),
            &String::from("genesis"),
            Uint128::new(1234),
            &String::from("minter"),
            None,
        );

        let msg = ExecuteMsg::Mint {
            recipient: String::from("lucky"),
            amount: Uint128::new(222),
        };
        let info = mock_info("anyone else", &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn no_one_mints_if_minter_unset() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut(), &String::from("genesis"), Uint128::new(1234));

        let msg = ExecuteMsg::Mint {
            recipient: String::from("lucky"),
            amount: Uint128::new(222),
        };
        let info = mock_info("genesis", &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn can_update_minter() {
        let mut deps = mock_dependencies();
        let orig_minter = "minter".to_string();
        do_instantiate_with_minter(
            deps.as_mut(),
            &String::from("genesis"),
            Uint128::new(1234),
            &orig_minter,
            None,
        );

        // ensure we can query it properly
        let minter = query_minter(deps.as_ref(), mock_env()).unwrap().unwrap();
        assert_eq!(minter.minter, orig_minter);

        // update the minter
        let new_minter = "changed".to_string();
        let msg = ExecuteMsg::UpdateMinter {
            minter: new_minter.clone(),
        };
        let info = mock_info(&orig_minter, &[]);
        execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

        // ensure updated properly
        let minter = query_minter(deps.as_ref(), mock_env()).unwrap().unwrap();
        assert_eq!(minter.minter, new_minter);

        // ensure orig minter can no longer change
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn instantiate_multiple_accounts() {
        let mut deps = mock_dependencies();
        let amount1 = Uint128::from(11223344u128);
        let addr1 = String::from("addr0001");
        let amount2 = Uint128::from(7890987u128);
        let addr2 = String::from("addr0002");
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // Fails with duplicate addresses
        let instantiate_msg = InstantiateMsg {
            name: "Bash Shell".to_string(),
            symbol: "BASH".to_string(),
            decimals: 6,
            initial_balances: vec![
                InitBalance {
                    address: addr1.clone(),
                    amount: amount1,
                    vesting: None,
                },
                InitBalance {
                    address: addr1.clone(),
                    amount: amount2,
                    vesting: None,
                },
            ],
            mint: None,
            marketing: None,
            allowed_vesters: None,
            max_curve_complexity: 10,
        };
        let err =
            instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap_err();
        assert_eq!(err, ContractError::DuplicateInitialBalanceAddresses {});

        // Works with unique addresses
        let instantiate_msg = InstantiateMsg {
            name: "Bash Shell".to_string(),
            symbol: "BASH".to_string(),
            decimals: 6,
            initial_balances: vec![
                InitBalance {
                    address: addr1.clone(),
                    amount: amount1,
                    vesting: None,
                },
                InitBalance {
                    address: addr2.clone(),
                    amount: amount2,
                    vesting: None,
                },
            ],
            mint: None,
            marketing: None,
            allowed_vesters: None,
            max_curve_complexity: 10,
        };
        let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap(),
            TokenInfoResponse {
                name: "Bash Shell".to_string(),
                symbol: "BASH".to_string(),
                decimals: 6,
                total_supply: amount1 + amount2,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), addr1), amount1);
        assert_eq!(get_balance(deps.as_ref(), addr2), amount2);
    }

    #[test]
    fn queries_work() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let amount1 = Uint128::from(12340000u128);

        let expected = do_instantiate(deps.as_mut(), &addr1, amount1);

        // check meta query
        let loaded = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(expected, loaded);

        let _info = mock_info("test", &[]);
        let env = mock_env();
        // check balance query (full)
        let data = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::Balance { address: addr1 },
        )
        .unwrap();
        let loaded: BalanceResponse = from_binary(&data).unwrap();
        assert_eq!(loaded.balance, amount1);

        // check balance query (empty)
        let data = query(
            deps.as_ref(),
            env,
            QueryMsg::Balance {
                address: String::from("addr0002"),
            },
        )
        .unwrap();
        let loaded: BalanceResponse = from_binary(&data).unwrap();
        assert_eq!(loaded.balance, Uint128::zero());
    }

    #[test]
    fn transfer() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let addr2 = String::from("addr0002");
        let amount1 = Uint128::from(12340000u128);
        let transfer = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot transfer nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // cannot send more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: too_much,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // cannot send from empty account
        let info = mock_info(addr2.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr1.clone(),
            amount: transfer,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // valid transfer
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: transfer,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let remainder = amount1.checked_sub(transfer).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1), remainder);
        assert_eq!(get_balance(deps.as_ref(), addr2), transfer);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );
    }

    #[test]
    fn transfer_vesting() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let addr2 = String::from("addr0002");
        let addr3 = String::from("addr0003");
        let amount1 = Uint128::from(150_000u128);

        // Lets make addr1 the admin of the contract
        let info = mock_info(addr1.as_ref(), &coins(amount1.u128(), "AUTO"));
        _do_instantiate(deps.as_mut(), &addr1, amount1, None, Some(info.clone()));

        let start = mock_env().block.time.seconds();
        let transfer = Uint128::from(100_000u128);
        // curve will be half-way through (at 40_000 locked) when we call
        let schedule = Curve::saturating_linear((start - 4000, 80_000), (start + 4000, 0));

        // send vesting tokens
        let env = mock_env();
        let msg = ExecuteMsg::TransferVesting {
            recipient: addr2.clone(),
            amount: transfer,
            schedule: schedule.clone(),
        };
        execute(deps.as_mut(), env, info, msg).unwrap();

        // ensure we see this here
        let vesting = query_vesting(deps.as_ref(), mock_env(), addr2.clone()).unwrap();
        assert_eq!(vesting.locked, Uint128::new(40_000));
        assert_eq!(vesting.schedule.unwrap(), schedule);

        // and not on the original account
        let non = query_vesting(deps.as_ref(), mock_env(), addr1.clone()).unwrap();
        assert_eq!(non.locked, Uint128::zero());
        assert_eq!(non.schedule, None);

        // we can send some from the new account
        let info = mock_info(addr2.as_ref(), &[]);
        let msg = ExecuteMsg::Transfer {
            recipient: addr3.clone(),
            amount: Uint128::new(45_000),
        };
        execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

        // but vesting schedule will get us next time
        let err = execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::CantMoveVestingTokens);

        // after a short wait, we can send more
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(3000);
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // ensure the balances (2 transfers)
        assert_eq!(
            get_balance(deps.as_ref(), addr2.clone()),
            Uint128::new(10_000)
        );
        assert_eq!(
            get_balance(deps.as_ref(), addr3.clone()),
            Uint128::new(90_000)
        );
        // and vesting
        let vesting = query_vesting(deps.as_ref(), env.clone(), addr2.clone()).unwrap();
        assert_eq!(vesting.locked, Uint128::new(10_000));
        assert_eq!(vesting.schedule.unwrap(), schedule);

        // add more vesting tokens
        let admin = mock_info(addr1.as_ref(), &coins(amount1.u128(), "AUTO"));
        let now = env.block.time.seconds();
        let schedule2 = Curve::saturating_linear((now, 50_000), (now + 1200, 0));
        let msg = ExecuteMsg::TransferVesting {
            recipient: addr2.clone(),
            amount: Uint128::new(50_000), // all remaining funds
            schedule: schedule2.clone(),
        };
        execute(deps.as_mut(), env.clone(), admin, msg).unwrap();

        // ensure the balance
        assert_eq!(
            get_balance(deps.as_ref(), addr2.clone()),
            Uint128::new(60_000)
        );
        // and vesting
        let vesting = query_vesting(deps.as_ref(), env.clone(), addr2.clone()).unwrap();
        assert_eq!(vesting.locked, Uint128::new(60_000));
        assert_eq!(vesting.schedule.unwrap(), schedule.combine(&schedule2));

        // go past the end of the vesting period
        env.block.time = env.block.time.plus_seconds(1200);
        let msg = ExecuteMsg::Transfer {
            recipient: addr3.clone(),
            amount: Uint128::new(1_000),
        };
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // ensure the balances (2 transfers)
        assert_eq!(
            get_balance(deps.as_ref(), addr2.clone()),
            Uint128::new(59_000)
        );
        assert_eq!(get_balance(deps.as_ref(), addr3), Uint128::new(91_000));
        // and vesting deleted
        let vesting = query_vesting(deps.as_ref(), env, addr2).unwrap();
        assert_eq!(vesting.locked, Uint128::new(0));
        assert_eq!(vesting.schedule, None);
    }

    #[test]
    fn transfer_vesting_error_cases() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let addr2 = String::from("addr0002");
        let addr3 = String::from("addr0003");
        let amount1 = Uint128::from(250_000u128);

        let info = mock_info(addr1.as_ref(), &coins(amount1.u128(), "AUTO"));
        _do_instantiate(deps.as_mut(), &addr1, amount1, None, Some(info.clone()));

        let start = mock_env().block.time.seconds();
        let end = start + 30 * 86_400;
        let transfer = Uint128::from(100_000u128);
        // curve will be half-way through (at 40_000 locked) when we call
        let schedule = Curve::saturating_linear((start - 4000, 80_000), (start + 4000, 0));

        // send vesting tokens works fine
        let msg = ExecuteMsg::TransferVesting {
            recipient: addr2,
            amount: transfer,
            schedule,
        };
        execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

        // Unauthorized
        let unauthorized_info = mock_info("addr3", &[]);
        let err = execute(deps.as_mut(), mock_env(), unauthorized_info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        // send with over-vesting curve
        let over_vesting = ExecuteMsg::TransferVesting {
            recipient: addr3.clone(),
            amount: Uint128::new(10_000),
            schedule: Curve::saturating_linear((start, 15_000), (end, 0)),
        };
        let err = execute(deps.as_mut(), mock_env(), info.clone(), over_vesting).unwrap_err();
        assert_eq!(err, ContractError::VestsMoreThanSent);

        // send with curve that never hits 0
        let never_vests = ExecuteMsg::TransferVesting {
            recipient: addr3.clone(),
            amount: Uint128::new(10_000),
            schedule: Curve::saturating_linear((start, 10_000), (end, 1_000)),
        };
        let err = execute(deps.as_mut(), mock_env(), info.clone(), never_vests).unwrap_err();
        assert_eq!(err, ContractError::NeverFullyVested);

        // send with curve that never hits 0
        let const_never_vests = ExecuteMsg::TransferVesting {
            recipient: addr3.clone(),
            amount: Uint128::new(10_000),
            schedule: Curve::constant(2),
        };
        let err = execute(deps.as_mut(), mock_env(), info.clone(), const_never_vests).unwrap_err();
        assert_eq!(err, ContractError::NeverFullyVested);

        // fails with increasing curve
        let increasing = ExecuteMsg::TransferVesting {
            recipient: addr3.clone(),
            amount: Uint128::new(10_000),
            schedule: Curve::saturating_linear((start, 5_000), (end, 6_000)),
        };
        let err = execute(deps.as_mut(), mock_env(), info.clone(), increasing).unwrap_err();
        assert_eq!(err, ContractError::Curve(CurveError::MonotonicIncreasing));

        // fails with too complex curve
        let amount = Uint128::new(10_000);
        let complex = ExecuteMsg::TransferVesting {
            recipient: addr3.clone(),
            amount,
            schedule: Curve::PiecewiseLinear(PiecewiseLinear {
                steps: (start..end)
                    .map(|x| (x, amount))
                    .chain(std::iter::once((end, Uint128::new(0)))) // fully vest
                    .collect(),
            }),
        };
        let err = execute(deps.as_mut(), mock_env(), info.clone(), complex).unwrap_err();
        assert_eq!(err, ContractError::Curve(CurveError::TooComplex));

        // works with almost too complex curve
        let max_complexity = MAX_VESTING_COMPLEXITY.load(&deps.storage).unwrap();
        let end = start + max_complexity as u64 - 1;
        let almost_too_complex = ExecuteMsg::TransferVesting {
            recipient: addr3.clone(),
            amount,
            schedule: Curve::PiecewiseLinear(PiecewiseLinear {
                steps: (start..end)
                    .map(|x| (x, amount))
                    .chain(std::iter::once((end, Uint128::new(0)))) // fully vest
                    .collect(),
            }),
        };
        let res = execute(deps.as_mut(), mock_env(), info.clone(), almost_too_complex).unwrap();
        assert_eq!(0, res.messages.len());

        // but fails when adding a simple curve if the combined curve becomes too complex
        let simple = ExecuteMsg::TransferVesting {
            recipient: addr3,
            amount,
            schedule: Curve::saturating_linear((end, amount.u128()), (end + 1, 0)),
        };
        let err = execute(deps.as_mut(), mock_env(), info, simple).unwrap_err();
        assert_eq!(err, ContractError::Curve(CurveError::TooComplex));
    }

    #[test]
    fn burn() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let amount1 = Uint128::from(12340000u128);
        let burn = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot burn nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn {
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );

        // cannot burn more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn { amount: too_much };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );

        // valid burn reduces total supply
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn { amount: burn };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let remainder = amount1.checked_sub(burn).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1), remainder);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            remainder
        );
    }

    #[test]
    fn send() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let contract = String::from("addr0002");
        let amount1 = Uint128::from(12340000u128);
        let transfer = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);
        let send_msg = Binary::from(r#"{"some":123}"#.as_bytes());

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot send nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: Uint128::zero(),
            msg: send_msg.clone(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // cannot send more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: too_much,
            msg: send_msg.clone(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // valid transfer
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: transfer,
            msg: send_msg.clone(),
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        // ensure proper send message sent
        // this is the message we want delivered to the other side
        let binary_msg = Cw20ReceiveMsg {
            sender: addr1.clone(),
            amount: transfer,
            msg: send_msg,
        }
        .into_binary()
        .unwrap();
        // and this is how it must be wrapped for the vm to process it
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract.clone(),
                msg: binary_msg,
                funds: vec![],
            }))
        );

        // ensure balance is properly transferred
        let remainder = amount1.checked_sub(transfer).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1), remainder);
        assert_eq!(get_balance(deps.as_ref(), contract), transfer);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );
    }

    mod marketing {
        use super::*;

        #[test]
        fn update_unauthorised() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("marketing".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let err = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: Some("New project".to_owned()),
                    description: Some("Better description".to_owned()),
                    marketing: Some("creator".to_owned()),
                },
            )
            .unwrap_err();

            assert_eq!(err, ContractError::Unauthorized {});

            // Ensure marketing didn't change
            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("marketing")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn update_project() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: Some("New project".to_owned()),
                    description: None,
                    marketing: None,
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("New project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn clear_project() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: Some("".to_owned()),
                    description: None,
                    marketing: None,
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: None,
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn update_description() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: None,
                    description: Some("Better description".to_owned()),
                    marketing: None,
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Better description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn clear_description() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: None,
                    description: Some("".to_owned()),
                    marketing: None,
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: None,
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn update_marketing() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: None,
                    description: None,
                    marketing: Some("marketing".to_owned()),
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("marketing")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn update_marketing_invalid() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let err = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: None,
                    description: None,
                    marketing: Some("m".to_owned()),
                },
            )
            .unwrap_err();

            assert!(
                matches!(err, ContractError::Std(_)),
                "Expected Std error, received: {}",
                err
            );

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn clear_marketing() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UpdateMarketing {
                    project: None,
                    description: None,
                    marketing: Some("".to_owned()),
                },
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: None,
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn update_logo_url() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UploadLogo(Logo::Url("new_url".to_owned())),
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Url("new_url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn update_logo_png() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UploadLogo(Logo::Embedded(EmbeddedLogo::Png(PNG_HEADER.into()))),
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Embedded),
                }
            );

            assert_eq!(
                query_download_logo(deps.as_ref()).unwrap(),
                DownloadLogoResponse {
                    mime_type: "image/png".to_owned(),
                    data: PNG_HEADER.into(),
                }
            );
        }

        #[test]
        fn update_logo_svg() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let img = "<?xml version=\"1.0\"?><svg></svg>".as_bytes();
            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UploadLogo(Logo::Embedded(EmbeddedLogo::Svg(img.into()))),
            )
            .unwrap();

            assert_eq!(res.messages, vec![]);

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Embedded),
                }
            );

            assert_eq!(
                query_download_logo(deps.as_ref()).unwrap(),
                DownloadLogoResponse {
                    mime_type: "image/svg+xml".to_owned(),
                    data: img.into(),
                }
            );
        }

        #[test]
        fn update_logo_png_oversized() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let img = [&PNG_HEADER[..], &[1; 6000][..]].concat();
            let err = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UploadLogo(Logo::Embedded(EmbeddedLogo::Png(img.into()))),
            )
            .unwrap_err();

            assert_eq!(err, ContractError::LogoTooBig {});

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn update_logo_svg_oversized() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let img = [
                "<?xml version=\"1.0\"?><svg>",
                std::str::from_utf8(&[b'x'; 6000]).unwrap(),
                "</svg>",
            ]
            .concat()
            .into_bytes();

            let err = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UploadLogo(Logo::Embedded(EmbeddedLogo::Svg(img.into()))),
            )
            .unwrap_err();

            assert_eq!(err, ContractError::LogoTooBig {});

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn update_logo_png_invalid() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let img = &[1];
            let err = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UploadLogo(Logo::Embedded(EmbeddedLogo::Png(img.into()))),
            )
            .unwrap_err();

            assert_eq!(err, ContractError::InvalidPngHeader {});

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }

        #[test]
        fn update_logo_svg_invalid() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let img = &[1];

            let err = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::UploadLogo(Logo::Embedded(EmbeddedLogo::Svg(img.into()))),
            )
            .unwrap_err();

            assert_eq!(err, ContractError::InvalidXmlPreamble {});

            assert_eq!(
                query_marketing_info(deps.as_ref()).unwrap(),
                MarketingInfoResponse {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some(Addr::unchecked("creator")),
                    logo: Some(LogoInfo::Url("url".to_owned())),
                }
            );

            let err = query_download_logo(deps.as_ref()).unwrap_err();
            assert!(
                matches!(err, StdError::NotFound { .. }),
                "Expected StdError::NotFound, received {}",
                err
            );
        }
    }
    mod address_list {
        use super::*;
        #[test]
        fn add_address_list() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::AllowVester {
                    address: "addr1".to_string(),
                },
            )
            .unwrap();

            assert_eq!(res.attributes, vec![attr("action", "add address")]);
            assert_eq!(
                query_allow_list(deps.as_ref()).unwrap().allow_list,
                vec!["creator".to_string(), "addr1".to_string()]
            );
        }

        #[test]
        fn add_address_list_error_cases() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info.clone(),
                ExecuteMsg::AllowVester {
                    address: "addr1".to_string(),
                },
            )
            .unwrap();

            assert_eq!(res.attributes, vec![attr("action", "add address")]);
            assert_eq!(
                query_allow_list(deps.as_ref()).unwrap().allow_list,
                vec!["creator".to_string(), "addr1".to_string()]
            );

            // Try to re add the same address
            let err_address_already_exist = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::AllowVester {
                    address: "addr1".to_string(),
                },
            )
            .unwrap_err();
            assert_eq!(
                err_address_already_exist,
                ContractError::AddressAlreadyExist {}
            );

            // Try to execute without Permission
            let addr2_info = mock_info("addr2", &[]);
            let err_unauthorized = execute(
                deps.as_mut(),
                mock_env(),
                addr2_info,
                ExecuteMsg::AllowVester {
                    address: "addr2".to_string(),
                },
            )
            .unwrap_err();
            assert_eq!(err_unauthorized, ContractError::Unauthorized {});
        }

        #[test]
        fn remove_address_list() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: Some(vec!["airdrop".to_string(), "creator".to_string()]),
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            let res = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::DenyVester {
                    address: "airdrop".to_string(),
                },
            )
            .unwrap();

            assert_eq!(res.attributes, vec![attr("action", "remove address")]);
            assert_eq!(
                query_allow_list(deps.as_ref()).unwrap().allow_list,
                vec!["creator".to_string()]
            );
        }

        #[test]
        fn remove_address_list_error_cases() {
            let mut deps = mock_dependencies();
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: Some(InstantiateMarketingInfo {
                    project: Some("Project".to_owned()),
                    description: Some("Description".to_owned()),
                    marketing: Some("creator".to_owned()),
                    logo: Some(Logo::Url("url".to_owned())),
                }),
                allowed_vesters: None,
                max_curve_complexity: 10,
            };

            let info = mock_info("creator", &[]);

            instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap();

            // Try to remove address that doesnt exist
            let err_address_doesnt_exist = execute(
                deps.as_mut(),
                mock_env(),
                info.clone(),
                ExecuteMsg::DenyVester {
                    address: "addr1".to_string(),
                },
            )
            .unwrap_err();
            assert_eq!(err_address_doesnt_exist, ContractError::AddressNotFound {});

            // Try to remove address the only address that exist
            let err_empty_list = execute(
                deps.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::DenyVester {
                    address: "creator".to_string(),
                },
            )
            .unwrap_err();
            assert_eq!(err_empty_list, ContractError::AtLeastOneAddressMustExist {});

            // Try to execute without Permission
            let addr2_info = mock_info("addr2", &[]);
            let err_unauthorized = execute(
                deps.as_mut(),
                mock_env(),
                addr2_info,
                ExecuteMsg::AllowVester {
                    address: "addr2".to_string(),
                },
            )
            .unwrap_err();
            assert_eq!(err_unauthorized, ContractError::Unauthorized {});
        }
    }
}
