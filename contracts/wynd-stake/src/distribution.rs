use cosmwasm_std::{
    to_binary, Addr, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Storage, Uint128,
    WasmMsg,
};

use crate::error::ContractError;
use crate::msg::{
    DelegatedResponse, DistributedRewardsResponse, UndistributedRewardsResponse,
    WithdrawAdjustmentDataResponse, WithdrawableRewardsResponse,
};
use crate::state::{
    Distribution, WithdrawAdjustment, CONFIG, DISTRIBUTION, REWARDS, SHARES_SHIFT, TOTAL_REWARDS,
    TOTAL_STAKED, WITHDRAW_ADJUSTMENT,
};

pub fn execute_distribute_rewards(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Option<String>,
) -> Result<Response, ContractError> {
    let total = TOTAL_REWARDS.load(deps.storage)?.u128();

    // There are no shares in play - noone to distribute to
    if total == 0 {
        return Err(ContractError::NoMembersToDistributeTo {});
    }

    let sender = sender
        .map(|sender| deps.api.addr_validate(&sender))
        .transpose()?
        .unwrap_or(info.sender);

    let mut distribution = DISTRIBUTION.load(deps.storage)?;
    let withdrawable: u128 = distribution.withdrawable_total.into();

    // Query current cw20 reward balance, we assume we pay out rewards in
    // the same token that is used to stake.
    let balance = undistributed_rewards(deps.as_ref(), env.contract.address)?.u128();

    // Calculate how much we have received since the last time Distributed was called.
    // This is the amount we will distribute to all members.
    let amount = balance - withdrawable;
    if amount == 0 {
        return Ok(Response::new());
    }

    let leftover: u128 = distribution.shares_leftover.into();
    let points = (amount << SHARES_SHIFT) + leftover;
    let points_per_share = points / total;
    distribution.shares_leftover = (points % total) as u64;

    // Everything goes back to 128-bits/16-bytes
    // Full amount is added here to total withdrawable, as it should not be considered on its own
    // on future distributions - even if because of calculation offsets it is not fully
    // distributed, the error is handled by leftover.
    distribution.shares_per_point += Uint128::new(points_per_share);
    distribution.distributed_total += Uint128::new(amount);
    distribution.withdrawable_total += Uint128::new(amount);

    DISTRIBUTION.save(deps.storage, &distribution)?;

    let resp = Response::new()
        .add_attribute("action", "distribute_rewards")
        .add_attribute("sender", sender.as_str())
        .add_attribute("amount", &amount.to_string());

    Ok(resp)
}

/// Query current cw20 reward balance.
/// We assume we pay out rewards in the same token that is used to stake.
fn undistributed_rewards(deps: Deps, contract_address: Addr) -> StdResult<Uint128> {
    // Query current cw20 reward balance, we assume we pay out rewards in
    // the same token that is used to stake.
    let cw20 = CONFIG.load(deps.storage)?.cw20_contract;
    let query = cw20_vesting::QueryMsg::Balance {
        address: contract_address.into_string(),
    };
    let cw20::BalanceResponse { balance } = deps.querier.query_wasm_smart(cw20, &query)?;
    // we don't distribute the staked tokens (including currently unbonding ones)
    let staked = TOTAL_STAKED.load(deps.storage)?.total();
    Ok(balance - staked)
}

pub fn execute_withdraw_rewards(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    let owner = owner.map_or_else(
        || Ok(info.sender.clone()),
        |owner| deps.api.addr_validate(&owner),
    )?;

    let mut distribution = DISTRIBUTION.load(deps.storage)?;
    let mut adjustment = WITHDRAW_ADJUSTMENT.load(deps.storage, &owner)?;

    if ![&owner, &adjustment.delegated].contains(&&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    let reward = withdrawable_rewards(deps.as_ref(), &owner, &distribution, &adjustment)?;
    let receiver = receiver
        .map(|receiver| deps.api.addr_validate(&receiver))
        .transpose()?
        .unwrap_or_else(|| info.sender.clone());

    if reward.is_zero() {
        // Just do nothing
        return Ok(Response::new());
    }

    adjustment.withdrawn_rewards += reward;
    WITHDRAW_ADJUSTMENT.save(deps.storage, &owner, &adjustment)?;
    distribution.withdrawable_total -= reward;
    DISTRIBUTION.save(deps.storage, &distribution)?;

    // send via cw20
    let msg = WasmMsg::Execute {
        contract_addr: CONFIG.load(deps.storage)?.cw20_contract.into_string(),
        msg: to_binary(&cw20_vesting::ExecuteMsg::Transfer {
            recipient: receiver.to_string(),
            amount: reward,
        })?,
        funds: vec![],
    };

    let resp = Response::new()
        .add_attribute("action", "withdraw_rewards")
        .add_attribute("sender", info.sender.as_str())
        .add_attribute("owner", owner.as_str())
        .add_attribute("receiver", receiver.as_str())
        .add_attribute("reward", reward)
        .add_message(msg);

    Ok(resp)
}

pub fn execute_delegate_withdrawal(
    deps: DepsMut,
    info: MessageInfo,
    delegated: String,
) -> Result<Response, ContractError> {
    let delegated = deps.api.addr_validate(&delegated)?;

    WITHDRAW_ADJUSTMENT.update(deps.storage, &info.sender, |data| -> StdResult<_> {
        Ok(data.map_or_else(
            || WithdrawAdjustment {
                shares_correction: 0.into(),
                withdrawn_rewards: Uint128::zero(),
                delegated: delegated.clone(),
            },
            |mut data| {
                data.delegated = delegated.clone();
                data
            },
        ))
    })?;

    let resp = Response::new()
        .add_attribute("action", "delegate_withdrawal")
        .add_attribute("sender", info.sender.as_str())
        .add_attribute("delegated", &delegated);

    Ok(resp)
}

pub fn query_withdrawable_rewards(
    deps: Deps,
    owner: String,
) -> StdResult<WithdrawableRewardsResponse> {
    // Not checking address, as if it is invalid it is guaranteed not to appear in maps, so
    // `withdrawable_rewards` would return error itself.
    let owner = Addr::unchecked(&owner);
    let distribution = DISTRIBUTION.load(deps.storage)?;
    let adjustment = if let Some(adj) = WITHDRAW_ADJUSTMENT.may_load(deps.storage, &owner)? {
        adj
    } else {
        return Ok(WithdrawableRewardsResponse {
            rewards: Uint128::zero(),
        });
    };

    let rewards = withdrawable_rewards(deps, &owner, &distribution, &adjustment)?;
    Ok(WithdrawableRewardsResponse { rewards })
}

pub fn query_undistributed_rewards(
    deps: Deps,
    env: Env,
) -> StdResult<UndistributedRewardsResponse> {
    let distribution = DISTRIBUTION.load(deps.storage)?;
    let balance = undistributed_rewards(deps, env.contract.address)?;

    Ok(UndistributedRewardsResponse {
        rewards: (balance - distribution.withdrawable_total),
    })
}

pub fn query_distributed_rewards(deps: Deps) -> StdResult<DistributedRewardsResponse> {
    let distribution = DISTRIBUTION.load(deps.storage)?;
    Ok(DistributedRewardsResponse {
        distributed: distribution.distributed_total,
        withdrawable: distribution.withdrawable_total,
    })
}

pub fn query_delegated(deps: Deps, owner: String) -> StdResult<DelegatedResponse> {
    let owner = deps.api.addr_validate(&owner)?;

    let delegated = WITHDRAW_ADJUSTMENT
        .may_load(deps.storage, &owner)?
        .map_or(owner, |data| data.delegated);

    Ok(DelegatedResponse { delegated })
}

pub fn query_withdraw_adjustment_data(
    deps: Deps,
    owner: String,
) -> StdResult<WithdrawAdjustmentDataResponse> {
    let addr = deps.api.addr_validate(&owner)?;
    let adjust = WITHDRAW_ADJUSTMENT
        .may_load(deps.storage, &addr)?
        .unwrap_or_else(|| WithdrawAdjustmentDataResponse {
            shares_correction: 0,
            withdrawn_rewards: Default::default(),
            delegated: addr,
        });
    Ok(adjust)
}

/// Applies points correction for given address.
/// `shares_per_point` is current value from `SHARES_PER_POINT` - not loaded in function, to
/// avoid multiple queries on bulk updates.
/// `diff` is the points change
pub fn apply_points_correction(
    storage: &mut dyn Storage,
    addr: &Addr,
    shares_per_point: u128,
    diff: i128,
) -> StdResult<()> {
    WITHDRAW_ADJUSTMENT.update(storage, addr, |old| -> StdResult<_> {
        let mut old = old.unwrap_or_else(|| {
            // This happens the first time a user stakes tokens
            WithdrawAdjustment {
                shares_correction: 0.into(),
                withdrawn_rewards: Uint128::zero(),
                delegated: addr.clone(),
            }
        });
        let shares_correction: i128 = old.shares_correction;
        old.shares_correction = shares_correction - shares_per_point as i128 * diff;
        Ok(old)
    })?;
    Ok(())
}

/// This is customized for the use case of the contract
/// Since it is cw20, we just return the number, not the denom
pub fn withdrawable_rewards(
    deps: Deps,
    owner: &Addr,
    distribution: &Distribution,
    adjustment: &WithdrawAdjustment,
) -> StdResult<Uint128> {
    let ppw = distribution.shares_per_point.u128();
    let points = REWARDS
        .may_load(deps.storage, owner)?
        .unwrap_or_default()
        .u128();
    let correction = adjustment.shares_correction;
    let withdrawn = adjustment.withdrawn_rewards.u128();
    let points = (ppw * points) as i128;
    let points = points + correction;
    let amount = points as u128 >> SHARES_SHIFT;
    let amount = amount - withdrawn;

    Ok(amount.into())
}
