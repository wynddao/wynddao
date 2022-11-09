#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_slice, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Storage, SubMsg, Uint128, WasmMsg,
};

use crate::distribution::{
    apply_points_correction, execute_delegate_withdrawal, execute_distribute_rewards,
    execute_withdraw_rewards, query_delegated, query_distributed_rewards,
    query_undistributed_rewards, query_withdraw_adjustment_data, query_withdrawable_rewards,
};
use cw2::{get_contract_version, set_contract_version};
use cw20_vesting::{Cw20ReceiveDelegationMsg, ExecuteMsg as VestingExecuteMsg};
use cw_core_interface::voting::{
    InfoResponse, TotalPowerAtHeightResponse, VotingPowerAtHeightResponse,
};
use cw_utils::{ensure_from_older_version, maybe_addr, Expiration};

use crate::error::ContractError;
use crate::hook::{MemberChangedHookMsg, MemberDiff};
use crate::msg::{
    AllStakedResponse, BondingInfoResponse, BondingPeriodInfo, ExecuteMsg, InstantiateMsg,
    MigrateMsg, QueryMsg, ReceiveDelegationMsg, RewardsResponse, StakedResponse,
    TotalRewardsResponse, TotalStakedResponse, TotalUnbondingResponse,
};
use crate::state::{
    Config, Distribution, TokenInfo, ADMIN, CLAIMS, CONFIG, DISTRIBUTION, HOOKS, MEMBERS, REWARDS,
    STAKE, STAKE_CONFIG, TOTAL_REWARDS, TOTAL_STAKED, TOTAL_VOTES,
};

// version info for migration info
const CONTRACT_NAME: &str = concat!("crates.io:", env!("CARGO_CRATE_NAME"));
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let api = deps.api;
    ADMIN.set(deps.branch(), maybe_addr(api, msg.admin)?)?;

    // min_bond is at least 1, so 0 stake -> non-membership
    let min_bond = std::cmp::max(msg.min_bond, Uint128::new(1));

    TOTAL_VOTES.save(deps.storage, &Uint128::zero(), env.block.height)?;
    TOTAL_STAKED.save(deps.storage, &TokenInfo::default())?;

    let mut unbonding_periods = vec![];
    for stake_config in msg.stake_config {
        unbonding_periods.push(stake_config.unbonding_period);
        STAKE_CONFIG.save(
            deps.storage,
            stake_config.unbonding_period,
            &stake_config.into(),
        )?;
    }

    let config = Config {
        cw20_contract: deps.api.addr_validate(&msg.cw20_contract)?,
        tokens_per_power: msg.tokens_per_power,
        min_bond,
        unbonding_periods,
    };
    CONFIG.save(deps.storage, &config)?;

    DISTRIBUTION.save(deps.storage, &Distribution::default())?;

    Ok(Response::default())
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let api = deps.api;
    match msg {
        ExecuteMsg::UpdateAdmin { admin } => {
            Ok(ADMIN.execute_update_admin(deps, info, maybe_addr(api, admin)?)?)
        }
        ExecuteMsg::AddHook { addr } => {
            Ok(HOOKS.execute_add_hook(&ADMIN, deps, info, api.addr_validate(&addr)?)?)
        }
        ExecuteMsg::RemoveHook { addr } => {
            Ok(HOOKS.execute_remove_hook(&ADMIN, deps, info, api.addr_validate(&addr)?)?)
        }
        ExecuteMsg::Rebond {
            tokens,
            bond_from,
            bond_to,
        } => execute_rebond(deps, env, info, tokens, bond_from, bond_to),
        ExecuteMsg::Unbond {
            tokens: amount,
            unbonding_period,
        } => execute_unbond(deps, env, info, amount, unbonding_period),
        ExecuteMsg::Claim {} => execute_claim(deps, env, info),
        ExecuteMsg::ReceiveDelegation(msg) => execute_receive_delegation(deps, env, info, msg),
        ExecuteMsg::DistributeRewards { sender } => {
            execute_distribute_rewards(deps, env, info, sender)
        }
        ExecuteMsg::WithdrawRewards { owner, receiver } => {
            execute_withdraw_rewards(deps, info, owner, receiver)
        }
        ExecuteMsg::DelegateWithdrawal { delegated } => {
            execute_delegate_withdrawal(deps, info, delegated)
        }
    }
}

pub fn execute_rebond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    bond_from: u64,
    bond_to: u64,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    // Raise if no amount was provided
    if amount == Uint128::zero() {
        return Err(ContractError::NoRebondAmount {});
    }
    // Short out with an error if trying to rebond to itself
    if bond_from == bond_to {
        return Err(ContractError::SameUnbondingRebond {});
    }

    // Validate both bond_from and bond_to are valid and get their relevant voting multiplier
    // Validate bond_from is a valid time period
    let bond_from_staking_multipliers =
        STAKE_CONFIG.update::<_, ContractError>(deps.storage, bond_from, |multipliers| {
            let mut multipliers =
                multipliers.ok_or(ContractError::NoUnbondingPeriodFound(bond_from))?;
            multipliers.staked = multipliers.staked.checked_sub(amount)?;
            Ok(multipliers)
        })?;

    // Validate bond_to is a valid time period for Unbonding
    let bond_to_staking_multipliers =
        STAKE_CONFIG.update::<_, ContractError>(deps.storage, bond_to, |multipliers| {
            let mut multipliers =
                multipliers.ok_or(ContractError::NoUnbondingPeriodFound(bond_to))?;
            multipliers.staked += amount;
            Ok(multipliers)
        })?;

    // update the sender's stake
    let mut old_votes_from = Uint128::zero();
    let mut old_votes_to = Uint128::zero();

    let mut old_rewards_from = Uint128::zero();
    let mut old_rewards_to = Uint128::zero();

    // Reduce the bond_from
    let bond_from_stake_change = STAKE.update(
        deps.storage,
        (&info.sender, bond_from),
        |bonding_info| -> StdResult<_> {
            let mut bonding_info = bonding_info.unwrap_or_default();
            // Release the stake, also accounting for locked tokens, raising if there is not enough tokens
            bonding_info.release_stake(&env, amount)?;
            let stake = bonding_info.total_stake();
            let votes = calc_power(&cfg, stake, bond_from_staking_multipliers.voting);
            let rewards = calc_power(&cfg, stake, bond_from_staking_multipliers.reward);

            old_votes_from = bonding_info.votes;
            old_rewards_from = bonding_info.rewards;
            bonding_info.votes = votes;
            bonding_info.rewards = rewards;
            Ok(bonding_info)
        },
    )?;

    // Increase the bond_to
    let bond_to_stake_change = STAKE.update(
        deps.storage,
        (&info.sender, bond_to),
        |bonding_info| -> StdResult<_> {
            let mut bonding_info = bonding_info.unwrap_or_default();
            if bond_from > bond_to {
                bonding_info
                    .add_locked_tokens(env.block.time.plus_seconds(bond_from - bond_to), amount);
            } else {
                bonding_info.add_unlocked_tokens(amount);
            };
            let stake = bonding_info.total_stake();
            let voting_power = calc_power(&cfg, stake, bond_to_staking_multipliers.voting);
            let rewards = calc_power(&cfg, stake, bond_to_staking_multipliers.reward);

            old_votes_to = bonding_info.votes;
            old_rewards_to = bonding_info.rewards;
            bonding_info.votes = voting_power;
            bonding_info.rewards = rewards;
            Ok(bonding_info)
        },
    )?;
    let bond_update_messages = update_membership(
        deps.storage,
        info.sender.clone(),
        &[old_votes_to, old_votes_from],
        &[bond_to_stake_change.votes, bond_from_stake_change.votes],
        env.block.height,
    )?;
    update_rewards(
        deps.storage,
        info.sender,
        &[old_rewards_to, old_rewards_from],
        &[bond_to_stake_change.rewards, bond_from_stake_change.rewards],
    )?;

    Ok(Response::new()
        .add_submessages(bond_update_messages)
        .add_attribute("action", "rebond")
        .add_attribute("amount", amount)
        .add_attribute("bond_from", bond_from.to_string())
        .add_attribute("bond_to", bond_to.to_string()))
}

pub fn execute_bond(
    deps: DepsMut,
    env: Env,
    sender_cw20_contract: Addr,
    amount: Uint128,
    unbonding_period: u64,
    sender: Addr,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    // ensure that cw20 token contract's addresses matches
    if cfg.cw20_contract != sender_cw20_contract {
        return Err(ContractError::Cw20AddressesNotMatch {
            got: sender_cw20_contract.into(),
            expected: cfg.cw20_contract.into(),
        });
    }

    // load staking_multipliers to calculate votes and rewards
    let staking_multipliers =
        STAKE_CONFIG.update::<_, ContractError>(deps.storage, unbonding_period, |multipliers| {
            let mut multipliers =
                multipliers.ok_or(ContractError::NoUnbondingPeriodFound(unbonding_period))?;
            multipliers.staked += amount;
            Ok(multipliers)
        })?;

    // update the sender's stake
    let mut old_votes = Uint128::zero();
    let mut old_rewards = Uint128::zero();
    let new_stake = STAKE.update(
        deps.storage,
        (&sender, unbonding_period),
        |bonding_info| -> StdResult<_> {
            let mut bonding_info = bonding_info.unwrap_or_default();
            // Release the stake, also accounting for locked tokens, raising if there is not enough tokens

            bonding_info.add_unlocked_tokens(amount);
            let new_stake = bonding_info.total_stake();
            let voting_power = calc_power(&cfg, new_stake, staking_multipliers.voting);
            let rewards = calc_power(&cfg, new_stake, staking_multipliers.reward);
            old_votes = bonding_info.votes;
            old_rewards = bonding_info.rewards;
            bonding_info.votes = voting_power;
            bonding_info.rewards = rewards;
            Ok(bonding_info)
        },
    )?;

    let messages = update_membership(
        deps.storage,
        sender.clone(),
        &[old_votes],
        &[new_stake.votes],
        env.block.height,
    )?;
    update_rewards(
        deps.storage,
        sender.clone(),
        &[old_rewards],
        &[new_stake.rewards],
    )?;

    TOTAL_STAKED.update::<_, StdError>(deps.storage, |token_info| {
        Ok(TokenInfo {
            staked: token_info.staked + amount,
            unbonding: token_info.unbonding,
        })
    })?;

    Ok(Response::new()
        .add_submessages(messages)
        .add_attribute("action", "bond")
        .add_attribute("amount", amount)
        .add_attribute("sender", sender))
}

pub fn execute_receive_delegation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveDelegationMsg,
) -> Result<Response, ContractError> {
    // info.sender is the address of the cw20 contract (that re-sent this message).
    // wrapper.sender is the address of the user that requested the cw20 contract to send this.
    // This cannot be fully trusted (the cw20 contract can fake it), so only use it for actions
    // in the address's favor (like paying/bonding tokens, not withdrawls)
    let msg: ReceiveDelegationMsg = from_slice(&wrapper.msg)?;
    let api = deps.api;
    match msg {
        ReceiveDelegationMsg::Delegate { unbonding_period } => execute_bond(
            deps,
            env,
            info.sender,
            wrapper.amount,
            unbonding_period,
            api.addr_validate(&wrapper.sender)?,
        ),
    }
}

pub fn execute_unbond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    unbonding_period: u64,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    // load voting and reward multiplier to calculate votes and rewards
    // also update the amount staked here
    let staking_multipliers =
        STAKE_CONFIG.update::<_, ContractError>(deps.storage, unbonding_period, |multipliers| {
            let mut multipliers =
                multipliers.ok_or(ContractError::NoUnbondingPeriodFound(unbonding_period))?;
            multipliers.staked = multipliers.staked.checked_sub(amount)?;
            Ok(multipliers)
        })?;

    // reduce the sender's stake - aborting if insufficient
    let mut old_votes = Uint128::zero();
    let mut old_rewards = Uint128::zero();
    let new_stake = STAKE.update(
        deps.storage,
        (&info.sender, unbonding_period),
        |bonding_info| -> StdResult<_> {
            let mut bonding_info = bonding_info.unwrap_or_default();

            bonding_info.release_stake(&env, amount)?;
            let new_stake = bonding_info.total_stake();
            let voting_power = calc_power(&cfg, new_stake, staking_multipliers.voting);
            let rewards = calc_power(&cfg, new_stake, staking_multipliers.reward);
            old_votes = bonding_info.votes;
            old_rewards = bonding_info.rewards;

            bonding_info.votes = voting_power;
            bonding_info.rewards = rewards;
            Ok(bonding_info)
        },
    )?;

    // provide them a claim
    CLAIMS.create_claim(
        deps.storage,
        &info.sender,
        amount,
        Expiration::AtTime(env.block.time.plus_seconds(unbonding_period)),
    )?;

    let messages = update_membership(
        deps.storage,
        info.sender.clone(),
        &[old_votes],
        &[new_stake.votes],
        env.block.height,
    )?;
    update_rewards(
        deps.storage,
        info.sender.clone(),
        &[old_rewards],
        &[new_stake.rewards],
    )?;

    TOTAL_STAKED.update::<_, StdError>(deps.storage, |token_info| {
        Ok(TokenInfo {
            staked: token_info.staked.saturating_sub(amount),
            unbonding: token_info.unbonding + amount,
        })
    })?;

    Ok(Response::new()
        .add_submessages(messages)
        .add_attribute("action", "unbond")
        .add_attribute("amount", amount)
        .add_attribute("sender", info.sender))
}

fn update_membership(
    storage: &mut dyn Storage,
    sender: Addr,
    old_votes: &[Uint128],
    new_votes: &[Uint128],
    height: u64,
) -> StdResult<Vec<SubMsg>> {
    let old_voting_power: Uint128 = old_votes.iter().sum();
    let new_voting_power: Uint128 = new_votes.iter().sum();

    // short-circuit if no change
    if new_voting_power == old_voting_power {
        return Ok(vec![]);
    }

    // otherwise, record change of power
    let old_total_power = MEMBERS.may_load(storage, &sender)?;
    let new_total_power = old_total_power.unwrap_or_default() + new_voting_power - old_voting_power;

    let new_hook = if new_total_power.is_zero() {
        MEMBERS.remove(storage, &sender, height)?;
        None
    } else {
        MEMBERS.save(storage, &sender, &new_total_power, height)?;
        Some(new_total_power)
    };

    // update total
    TOTAL_VOTES.update(storage, height, |total| -> StdResult<_> {
        Ok(total.unwrap_or_default() + new_voting_power - old_voting_power)
    })?;

    // alert the hooks
    let diff = MemberDiff::new(sender, old_total_power, new_hook);
    HOOKS.prepare_hooks(storage, |h| {
        MemberChangedHookMsg::one(diff.clone())
            .into_cosmos_msg(h)
            .map(SubMsg::new)
    })
}

fn update_rewards(
    storage: &mut dyn Storage,
    sender: Addr,
    old_rewards: &[Uint128],
    new_rewards: &[Uint128],
) -> StdResult<()> {
    let old_reward_power: Uint128 = old_rewards.iter().sum();
    let new_reward_power: Uint128 = new_rewards.iter().sum();

    // short-circuit if no change
    if old_reward_power == new_reward_power {
        return Ok(());
    }

    let old_total_power = REWARDS.may_load(storage, &sender)?.unwrap_or_default();
    // otherwise, record change of power
    if new_reward_power.is_zero() && old_total_power == old_reward_power {
        REWARDS.remove(storage, &sender);
    } else {
        let new_total_power = old_total_power + new_reward_power - old_reward_power;
        REWARDS.save(storage, &sender, &new_total_power)?;
    }

    // update total
    let old_total = TOTAL_REWARDS.may_load(storage)?.unwrap_or_default();
    TOTAL_REWARDS.save(storage, &(old_total + new_reward_power - old_reward_power))?;

    // update their share of the distribution
    let ppw = DISTRIBUTION.load(storage)?.shares_per_point.u128();
    let diff = new_reward_power.u128() as i128 - old_reward_power.u128() as i128;
    apply_points_correction(storage, &sender, ppw, diff)?;

    Ok(())
}

fn calc_power(cfg: &Config, stake: Uint128, multiplier: Decimal) -> Uint128 {
    if stake < cfg.min_bond {
        Uint128::zero()
    } else {
        stake * multiplier / cfg.tokens_per_power
    }
}

pub fn execute_claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let release = CLAIMS.claim_tokens(deps.storage, &info.sender, &env.block, None)?;
    if release.is_zero() {
        return Err(ContractError::NothingToClaim {});
    }

    let config = CONFIG.load(deps.storage)?;
    let amount_str = coin_to_string(release, config.cw20_contract.as_str());
    let undelegate = VestingExecuteMsg::Undelegate {
        recipient: info.sender.to_string(),
        amount: release,
    };
    let undelegate_msg = SubMsg::new(WasmMsg::Execute {
        contract_addr: config.cw20_contract.to_string(),
        msg: to_binary(&undelegate)?,
        funds: vec![],
    });

    TOTAL_STAKED.update::<_, StdError>(deps.storage, |token_info| {
        Ok(TokenInfo {
            staked: token_info.staked,
            unbonding: token_info.unbonding.saturating_sub(release),
        })
    })?;

    Ok(Response::new()
        .add_submessage(undelegate_msg)
        .add_attribute("action", "claim")
        .add_attribute("tokens", amount_str)
        .add_attribute("sender", info.sender))
}

#[inline]
fn coin_to_string(amount: Uint128, address: &str) -> String {
    format!("{} {}", amount, address)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Claims { address } => {
            to_binary(&CLAIMS.query_claims(deps, &deps.api.addr_validate(&address)?)?)
        }
        QueryMsg::Staked {
            address,
            unbonding_period,
        } => to_binary(&query_staked(deps, &env, address, unbonding_period)?),
        QueryMsg::BondingInfo {} => to_binary(&query_bonding_info(deps)?),
        QueryMsg::AllStaked { address } => to_binary(&query_all_staked(deps, env, address)?),
        QueryMsg::TotalStaked {} => to_binary(&query_total_staked(deps)?),
        QueryMsg::TotalUnbonding {} => to_binary(&query_total_unbonding(deps)?),
        QueryMsg::Admin {} => to_binary(&ADMIN.query_admin(deps)?),
        QueryMsg::Hooks {} => to_binary(&HOOKS.query_hooks(deps)?),
        QueryMsg::VotingPowerAtHeight { address, height } => {
            to_binary(&query_voting_power(deps, env, address, height)?)
        }
        QueryMsg::TotalPowerAtHeight { height } => {
            to_binary(&query_total_power(deps, env, height)?)
        }
        QueryMsg::Info {} => to_binary(&query_info(deps)?),
        QueryMsg::TokenContract {} => to_binary(&query_token_contract(deps)?),
        QueryMsg::TotalRewards {} => to_binary(&query_total_rewards(deps)?),
        QueryMsg::Rewards { address } => to_binary(&query_rewards(deps, address)?),
        QueryMsg::WithdrawableRewards { owner } => {
            to_binary(&query_withdrawable_rewards(deps, owner)?)
        }
        QueryMsg::DistributedRewards {} => to_binary(&query_distributed_rewards(deps)?),
        QueryMsg::UndistributedRewards {} => to_binary(&query_undistributed_rewards(deps, env)?),
        QueryMsg::Delegated { owner } => to_binary(&query_delegated(deps, owner)?),
        QueryMsg::DistributionData {} => to_binary(&DISTRIBUTION.may_load(deps.storage)?),
        QueryMsg::WithdrawAdjustmentData { addr } => {
            to_binary(&query_withdraw_adjustment_data(deps, addr)?)
        }
    }
}

fn query_voting_power(
    deps: Deps,
    env: Env,
    addr: String,
    height: Option<u64>,
) -> StdResult<VotingPowerAtHeightResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    let power = match height {
        Some(h) => MEMBERS.may_load_at_height(deps.storage, &addr, h),
        None => MEMBERS.may_load(deps.storage, &addr),
    }?;

    let power = power.unwrap_or_default();
    let height = height.unwrap_or(env.block.height);

    Ok(VotingPowerAtHeightResponse { power, height })
}

fn query_total_power(
    deps: Deps,
    env: Env,
    height: Option<u64>,
) -> StdResult<TotalPowerAtHeightResponse> {
    let power = match height {
        Some(h) => TOTAL_VOTES.may_load_at_height(deps.storage, h),
        None => TOTAL_VOTES.may_load(deps.storage),
    }?
    .unwrap_or_default();

    let height = height.unwrap_or(env.block.height);

    Ok(TotalPowerAtHeightResponse { power, height })
}

fn query_rewards(deps: Deps, addr: String) -> StdResult<RewardsResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    Ok(RewardsResponse {
        rewards: REWARDS.may_load(deps.storage, &addr)?.unwrap_or_default(),
    })
}

fn query_total_rewards(deps: Deps) -> StdResult<TotalRewardsResponse> {
    Ok(TotalRewardsResponse {
        rewards: TOTAL_REWARDS.may_load(deps.storage)?.unwrap_or_default(),
    })
}

fn query_bonding_info(deps: Deps) -> StdResult<BondingInfoResponse> {
    let config = CONFIG.load(deps.storage)?;

    let bonding = config
        .unbonding_periods
        .into_iter()
        .filter_map(|up| match STAKE_CONFIG.may_load(deps.storage, up) {
            Ok(Some(multipliers)) => Some(Ok(BondingPeriodInfo {
                voting_multiplier: multipliers.voting,
                reward_multiplier: multipliers.reward,
                unbonding_period: up,
                total_staked: multipliers.staked,
            })),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<StdResult<Vec<BondingPeriodInfo>>>()?;
    Ok(BondingInfoResponse { bonding })
}

fn query_info(deps: Deps) -> StdResult<InfoResponse> {
    let info = get_contract_version(deps.storage)?;
    Ok(cw_core_interface::voting::InfoResponse { info })
}

fn query_token_contract(deps: Deps) -> StdResult<Addr> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(cfg.cw20_contract)
}

pub fn query_staked(
    deps: Deps,
    env: &Env,
    addr: String,
    unbonding_period: u64,
) -> StdResult<StakedResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    // sanity check if such unbonding period exists
    STAKE_CONFIG
        .load(deps.storage, unbonding_period)
        .map_err(|_| {
            StdError::generic_err(format!("No unbonding period found: {}", unbonding_period))
        })?;
    let stake = STAKE
        .may_load(deps.storage, (&addr, unbonding_period))?
        .unwrap_or_default();
    let cw20_contract = CONFIG.load(deps.storage)?.cw20_contract.to_string();
    Ok(StakedResponse {
        stake: stake.total_stake(),
        total_locked: stake.total_locked(env),
        unbonding_period,
        cw20_contract,
    })
}

pub fn query_all_staked(deps: Deps, env: Env, addr: String) -> StdResult<AllStakedResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    let config = CONFIG.load(deps.storage)?;
    let cw20_contract = config.cw20_contract.to_string();

    let stakes = config
        .unbonding_periods
        .into_iter()
        .filter_map(|up| match STAKE.may_load(deps.storage, (&addr, up)) {
            Ok(Some(stake)) => Some(Ok(StakedResponse {
                stake: stake.total_stake(),
                total_locked: stake.total_locked(&env),
                unbonding_period: up,
                cw20_contract: cw20_contract.clone(),
            })),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<StdResult<Vec<StakedResponse>>>()?;

    Ok(AllStakedResponse { stakes })
}

pub fn query_total_staked(deps: Deps) -> StdResult<TotalStakedResponse> {
    Ok(TotalStakedResponse {
        total_staked: TOTAL_STAKED.load(deps.storage).unwrap_or_default().staked,
    })
}

pub fn query_total_unbonding(deps: Deps) -> StdResult<TotalUnbondingResponse> {
    Ok(TotalUnbondingResponse {
        total_unbonding: TOTAL_STAKED
            .load(deps.storage)
            .unwrap_or_default()
            .unbonding,
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
    use cosmwasm_std::{from_slice, CosmosMsg, Decimal, Order, Storage};
    use cw2::ContractVersion;
    use cw4::{member_key, TOTAL_KEY};
    use cw_controllers::{AdminError, Claim, HookError};
    use cw_utils::Duration;
    use test_case::test_case;

    use crate::error::ContractError;
    use crate::msg::{StakeConfig, WithdrawAdjustmentDataResponse};
    use crate::state::{Distribution, WithdrawAdjustment};

    use super::*;

    const INIT_ADMIN: &str = "admin";
    const USER1: &str = "user1";
    const USER2: &str = "user2";
    const USER3: &str = "user3";
    const TOKENS_PER_POWER: Uint128 = Uint128::new(1_000);
    const MIN_BOND: Uint128 = Uint128::new(5_000);
    const UNBONDING_BLOCKS: u64 = 100;
    const UNBONDING_PERIOD: u64 = UNBONDING_BLOCKS / 5;
    const UNBONDING_PERIOD_2: u64 = 2 * UNBONDING_PERIOD;
    const CW20_ADDRESS: &str = "wasm1234567890";

    #[test]
    fn check_crate_name() {
        assert_eq!(CONTRACT_NAME, "crates.io:wynd_stake");
    }

    fn default_instantiate(deps: DepsMut, env: Env) {
        cw20_instantiate(
            deps,
            env,
            TOKENS_PER_POWER,
            MIN_BOND,
            vec![StakeConfig {
                unbonding_period: UNBONDING_PERIOD,
                voting_multiplier: Decimal::one(),
                reward_multiplier: Decimal::one(),
            }],
        )
    }

    fn cw20_instantiate(
        deps: DepsMut,
        env: Env,
        tokens_per_power: Uint128,
        min_bond: Uint128,
        stake_config: Vec<StakeConfig>,
    ) {
        let msg = InstantiateMsg {
            cw20_contract: CW20_ADDRESS.to_owned(),
            tokens_per_power,
            min_bond,
            stake_config,
            admin: Some(INIT_ADMIN.into()),
        };
        let info = mock_info("creator", &[]);
        instantiate(deps, env, info, msg).unwrap();
    }

    fn bond_cw20_with_period(
        mut deps: DepsMut,
        user1: u128,
        user2: u128,
        user3: u128,
        unbonding_period: u64,
        time_delta: u64,
    ) {
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(time_delta);

        for (addr, stake) in &[(USER1, user1), (USER2, user2), (USER3, user3)] {
            if *stake != 0 {
                let msg = ExecuteMsg::ReceiveDelegation(Cw20ReceiveDelegationMsg {
                    sender: addr.to_string(),
                    amount: Uint128::new(*stake),
                    msg: to_binary(&ReceiveDelegationMsg::Delegate { unbonding_period }).unwrap(),
                });
                let info = mock_info(CW20_ADDRESS, &[]);
                execute(deps.branch(), env.clone(), info, msg).unwrap();
            }
        }
    }

    fn bond_cw20(deps: DepsMut, user1: u128, user2: u128, user3: u128, time_delta: u64) {
        bond_cw20_with_period(deps, user1, user2, user3, UNBONDING_PERIOD, time_delta);
    }

    fn rebond_with_period(
        mut deps: DepsMut,
        user1: u128,
        user2: u128,
        user3: u128,
        bond_from: u64,
        bond_to: u64,
        time_delta: u64,
    ) {
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(time_delta);

        for (addr, stake) in &[(USER1, user1), (USER2, user2), (USER3, user3)] {
            if *stake != 0 {
                let msg = ExecuteMsg::Rebond {
                    bond_from,
                    bond_to,
                    tokens: Uint128::new(*stake),
                };
                let info = mock_info(addr, &[]);
                execute(deps.branch(), env.clone(), info, msg).unwrap();
            }
        }
    }

    fn unbond_with_period(
        mut deps: DepsMut,
        user1: u128,
        user2: u128,
        user3: u128,
        time_delta: u64,
        unbonding_period: u64,
    ) {
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(time_delta);

        for (addr, stake) in &[(USER1, user1), (USER2, user2), (USER3, user3)] {
            if *stake != 0 {
                let msg = ExecuteMsg::Unbond {
                    tokens: Uint128::new(*stake),
                    unbonding_period,
                };
                let info = mock_info(addr, &[]);
                execute(deps.branch(), env.clone(), info, msg).unwrap();
            }
        }
    }

    fn unbond(deps: DepsMut, user1: u128, user2: u128, user3: u128, time_delta: u64) {
        unbond_with_period(deps, user1, user2, user3, time_delta, UNBONDING_PERIOD);
    }

    #[test]
    fn proper_instantiation() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        default_instantiate(deps.as_mut(), env.clone());

        // it worked, let's query the state
        let res = ADMIN.query_admin(deps.as_ref()).unwrap();
        assert_eq!(Some(INIT_ADMIN.into()), res.admin);

        let res = query_total_power(deps.as_ref(), env, None).unwrap();
        assert_eq!(Uint128::zero(), res.power);

        // make sure distribution logic is set up properly
        let raw = query(deps.as_ref(), mock_env(), QueryMsg::DistributionData {}).unwrap();
        let res: Distribution = from_slice(&raw).unwrap();
        assert_eq!(
            res,
            Distribution {
                shares_per_point: Uint128::zero(),
                shares_leftover: 0,
                distributed_total: Uint128::zero(),
                withdrawable_total: Uint128::zero(),
            }
        );

        let raw = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::WithdrawAdjustmentData {
                addr: USER1.to_owned(),
            },
        )
        .unwrap();
        let res: WithdrawAdjustmentDataResponse = from_slice(&raw).unwrap();
        assert_eq!(
            res,
            WithdrawAdjustment {
                shares_correction: 0,
                withdrawn_rewards: Uint128::zero(),
                delegated: Addr::unchecked(USER1),
            }
        );
    }

    fn get_member(deps: Deps, env: Env, address: String, height: Option<u64>) -> u128 {
        let raw = query(deps, env, QueryMsg::VotingPowerAtHeight { address, height }).unwrap();
        let res: VotingPowerAtHeightResponse = from_slice(&raw).unwrap();
        res.power.u128()
    }

    // this tests the member queries
    fn assert_users(
        deps: Deps,
        env: Env,
        user1_power: Option<u128>,
        user2_power: Option<u128>,
        user3_power: Option<u128>,
        height: Option<u64>,
    ) {
        let member1 = get_member(deps, env.clone(), USER1.into(), height);
        assert_eq!(member1, user1_power.unwrap_or_default());

        let member2 = get_member(deps, env.clone(), USER2.into(), height);
        assert_eq!(member2, user2_power.unwrap_or_default());

        let member3 = get_member(deps, env.clone(), USER3.into(), height);
        assert_eq!(member3, user3_power.unwrap_or_default());

        // compute expected metrics
        let powers = vec![user1_power, user2_power, user3_power];
        let sum: u128 = powers.iter().map(|x| x.unwrap_or_default()).sum();
        let count = powers.iter().filter(|x| x.is_some()).count();

        let total = query_total_power(deps, env, height).unwrap();
        assert_eq!(Uint128::new(sum), total.power);

        // this is only valid if we are not doing a historical query
        if height.is_none() {
            let members = MEMBERS
                .range(deps.storage, None, None, Order::Ascending)
                .count();
            assert_eq!(count, members);
        }
    }

    fn assert_stake_in_period(
        deps: Deps,
        env: &Env,
        user1_stake: u128,
        user2_stake: u128,
        user3_stake: u128,
        unbonding_period: u64,
    ) {
        let stake1 = query_staked(deps, env, USER1.into(), unbonding_period).unwrap();
        assert_eq!(stake1.stake.u128(), user1_stake);

        let stake2 = query_staked(deps, env, USER2.into(), unbonding_period).unwrap();
        assert_eq!(stake2.stake.u128(), user2_stake);

        let stake3 = query_staked(deps, env, USER3.into(), unbonding_period).unwrap();
        assert_eq!(stake3.stake.u128(), user3_stake);
    }

    // this tests the member queries
    fn assert_stake(
        deps: Deps,
        env: &Env,
        user1_stake: u128,
        user2_stake: u128,
        user3_stake: u128,
    ) {
        assert_stake_in_period(
            deps,
            env,
            user1_stake,
            user2_stake,
            user3_stake,
            UNBONDING_PERIOD,
        );
    }

    fn assert_cw20_undelegate(res: cosmwasm_std::Response, recipient: &str, amount: u128) {
        match &res.messages[0].msg {
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr,
                msg,
                funds,
            }) => {
                assert_eq!(contract_addr.as_str(), CW20_ADDRESS);
                assert_eq!(funds.len(), 0);
                let parsed: VestingExecuteMsg = from_slice(msg).unwrap();
                assert_eq!(
                    parsed,
                    VestingExecuteMsg::Undelegate {
                        recipient: recipient.into(),
                        amount: Uint128::new(amount)
                    }
                );
            }
            _ => panic!("Must initiate undelegate!"),
        }
    }

    #[test]
    fn cw20_token_bond() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        cw20_instantiate(
            deps.as_mut(),
            env.clone(),
            TOKENS_PER_POWER,
            MIN_BOND,
            vec![StakeConfig {
                unbonding_period: UNBONDING_PERIOD,
                voting_multiplier: Decimal::one(),
                reward_multiplier: Decimal::one(),
            }],
        );

        let initial_height = env.block.height;

        // Assert original powers
        assert_users(deps.as_ref(), env.clone(), None, None, None, None);

        // ensure it rounds down, and respects cut-off
        bond_cw20(deps.as_mut(), 12_000, 7_500, 4_000, 1);

        // Assert updated powers
        assert_stake(deps.as_ref(), &env, 12_000, 7_500, 4_000);
        assert_users(deps.as_ref(), env.clone(), Some(12), Some(7), None, None);

        // assert users at initial height
        assert_users(deps.as_ref(), env, None, None, None, Some(initial_height));
    }

    #[test]
    fn cw20_token_claim() {
        let unbonding_period: u64 = 20;

        let mut deps = mock_dependencies();
        let mut env = mock_env();
        let unbonding = Duration::Time(unbonding_period);
        cw20_instantiate(
            deps.as_mut(),
            env.clone(),
            TOKENS_PER_POWER,
            MIN_BOND,
            vec![StakeConfig {
                unbonding_period,
                voting_multiplier: Decimal::one(),
                reward_multiplier: Decimal::one(),
            }],
        );

        // bond some tokens
        bond_cw20(deps.as_mut(), 20_000, 13_500, 500, 5);

        // unbond part
        unbond(deps.as_mut(), 7_900, 4_600, 0, unbonding_period);

        // Assert updated powers
        assert_stake(deps.as_ref(), &env, 12_100, 8_900, 500);
        assert_users(deps.as_ref(), env.clone(), Some(12), Some(8), None, None);

        // with proper claims
        env.block.time = env.block.time.plus_seconds(unbonding_period);
        let expires = unbonding.after(&env.block);
        assert_eq!(
            get_claims(deps.as_ref(), &Addr::unchecked(USER1)),
            vec![Claim::new(7_900, expires)]
        );

        // wait til they expire and get payout
        env.block.time = env.block.time.plus_seconds(unbonding_period);
        let res = execute(
            deps.as_mut(),
            env,
            mock_info(USER1, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_cw20_undelegate(res, USER1, 7_900)
    }

    #[test]
    fn raw_queries_work() {
        // add will over-write and remove have no effect
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut(), mock_env());
        // Set values as (11, 6, None)
        bond_cw20(deps.as_mut(), 11_000, 6_000, 0, 1);

        // get total from raw key
        let total_raw = deps.storage.get(TOTAL_KEY.as_bytes()).unwrap();
        let total: Uint128 = from_slice(&total_raw).unwrap();
        assert_eq!(17, total.u128());

        // get member votes from raw key
        let member2_raw = deps.storage.get(&member_key(USER2)).unwrap();
        let member2: Uint128 = from_slice(&member2_raw).unwrap();
        assert_eq!(6, member2.u128());

        // and execute misses
        let member3_raw = deps.storage.get(&member_key(USER3));
        assert_eq!(None, member3_raw);
    }

    fn get_claims(deps: Deps, addr: &Addr) -> Vec<Claim> {
        CLAIMS.query_claims(deps, addr).unwrap().claims
    }

    #[test]
    fn unbond_claim_workflow() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        default_instantiate(deps.as_mut(), env.clone());

        // create some data
        bond_cw20(deps.as_mut(), 12_000, 7_500, 4_000, 5);
        unbond(deps.as_mut(), 4_500, 2_600, 0, 10);
        env.block.time = env.block.time.plus_seconds(10);

        // check the claims for each user
        let expires = Duration::Time(UNBONDING_PERIOD).after(&env.block);
        assert_eq!(
            get_claims(deps.as_ref(), &Addr::unchecked(USER1)),
            vec![Claim::new(4_500, expires)]
        );
        assert_eq!(
            get_claims(deps.as_ref(), &Addr::unchecked(USER2)),
            vec![Claim::new(2_600, expires)]
        );
        assert_eq!(get_claims(deps.as_ref(), &Addr::unchecked(USER3)), vec![]);

        // do another unbond later on
        let mut env2 = mock_env();
        env2.block.time = env2.block.time.plus_seconds(22);
        unbond(deps.as_mut(), 0, 1_345, 1_500, 22);

        // with updated claims
        let expires2 = Duration::Time(UNBONDING_PERIOD).after(&env2.block);
        assert_eq!(
            get_claims(deps.as_ref(), &Addr::unchecked(USER1)),
            vec![Claim::new(4_500, expires)]
        );
        assert_eq!(
            get_claims(deps.as_ref(), &Addr::unchecked(USER2)),
            vec![Claim::new(2_600, expires), Claim::new(1_345, expires2)]
        );
        assert_eq!(
            get_claims(deps.as_ref(), &Addr::unchecked(USER3)),
            vec![Claim::new(1_500, expires2)]
        );

        // nothing can be withdrawn yet
        let err = execute(
            deps.as_mut(),
            env2,
            mock_info(USER1, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap_err();
        assert_eq!(err, ContractError::NothingToClaim {});

        // now mature first section, withdraw that
        let mut env3 = mock_env();
        env3.block.time = env3.block.time.plus_seconds(UNBONDING_PERIOD + 10);
        // first one can now release
        let res = execute(
            deps.as_mut(),
            env3.clone(),
            mock_info(USER1, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap();
        assert_cw20_undelegate(res, USER1, 4_500);

        // second releases partially
        let res = execute(
            deps.as_mut(),
            env3.clone(),
            mock_info(USER2, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap();
        assert_cw20_undelegate(res, USER2, 2_600);

        // but the third one cannot release
        let err = execute(
            deps.as_mut(),
            env3,
            mock_info(USER3, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap_err();
        assert_eq!(err, ContractError::NothingToClaim {});

        // claims updated properly
        assert_eq!(get_claims(deps.as_ref(), &Addr::unchecked(USER1)), vec![]);
        assert_eq!(
            get_claims(deps.as_ref(), &Addr::unchecked(USER2)),
            vec![Claim::new(1_345, expires2)]
        );
        assert_eq!(
            get_claims(deps.as_ref(), &Addr::unchecked(USER3)),
            vec![Claim::new(1_500, expires2)]
        );

        // add another few claims for 2
        unbond(deps.as_mut(), 0, 600, 0, 6 + UNBONDING_PERIOD);
        unbond(deps.as_mut(), 0, 1_005, 0, 10 + UNBONDING_PERIOD);

        // ensure second can claim all tokens at once
        let mut env4 = mock_env();
        env4.block.time = env4.block.time.plus_seconds(UNBONDING_PERIOD * 2 + 12);
        let res = execute(
            deps.as_mut(),
            env4,
            mock_info(USER2, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap();
        assert_cw20_undelegate(res, USER2, 2_950); // 1_345 + 600 + 1_005
        assert_eq!(get_claims(deps.as_ref(), &Addr::unchecked(USER2)), vec![]);
    }

    fn rewards(deps: Deps, user: &str) -> u128 {
        query_rewards(deps, user.to_string())
            .unwrap()
            .rewards
            .u128()
    }

    #[test]
    fn rewards_saved() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        cw20_instantiate(
            deps.as_mut(),
            env,
            TOKENS_PER_POWER,
            MIN_BOND,
            vec![StakeConfig {
                unbonding_period: UNBONDING_PERIOD,
                voting_multiplier: Decimal::one(),
                reward_multiplier: Decimal::percent(1),
            }],
        );

        // assert original rewards
        assert_eq!(0, rewards(deps.as_ref(), USER1));
        assert_eq!(0, rewards(deps.as_ref(), USER2));
        assert_eq!(0, rewards(deps.as_ref(), USER3));

        // ensure it rounds down, and respects cut-off
        bond_cw20(deps.as_mut(), 1_200_000, 770_000, 4_000_000, 1);

        // assert updated rewards
        assert_eq!(
            12,
            rewards(deps.as_ref(), USER1),
            "1_200_000 * 1% / 1_000 = 12"
        );
        assert_eq!(7, rewards(deps.as_ref(), USER2), "770_000 * 1% / 1_000 = 7");
        assert_eq!(
            40,
            rewards(deps.as_ref(), USER3),
            "4_000_000 * 1% / 1_000 = 4"
        );

        // unbond some tokens
        unbond(deps.as_mut(), 100_000, 99_600, 3_600_000, UNBONDING_PERIOD);

        assert_eq!(
            11,
            rewards(deps.as_ref(), USER1),
            "1_100_000 * 1% / 1_000 = 11"
        );
        assert_eq!(6, rewards(deps.as_ref(), USER2), "600_955 * 1% / 1_000 = 6");
        // USER3 has 400_000 left, this is above min_bound. But the rewards (4_000) would have been less
        assert_eq!(
            4,
            rewards(deps.as_ref(), USER3),
            "min_bound applied to stake (400_000), before reward multiplier (4_000)"
        );
    }

    #[test]
    fn rewards_rebonding() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        cw20_instantiate(
            deps.as_mut(),
            env.clone(),
            TOKENS_PER_POWER,
            Uint128::new(1000),
            vec![
                StakeConfig {
                    unbonding_period: UNBONDING_PERIOD,
                    voting_multiplier: Decimal::one(),
                    reward_multiplier: Decimal::percent(1),
                },
                StakeConfig {
                    unbonding_period: UNBONDING_PERIOD_2,
                    voting_multiplier: Decimal::from_ratio(Uint128::new(2), Uint128::one()),
                    reward_multiplier: Decimal::percent(10),
                },
            ],
        );

        // assert original rewards
        assert_eq!(0, rewards(deps.as_ref(), USER1));
        assert_eq!(0, rewards(deps.as_ref(), USER2));
        assert_eq!(0, rewards(deps.as_ref(), USER3));

        // bond some tokens for first period
        bond_cw20(deps.as_mut(), 1_000_000, 180_000, 10_000, 1);

        // assert updated rewards
        assert_eq!(
            10,
            rewards(deps.as_ref(), USER1),
            "1_000_000 * 1% / 1_000 = 10"
        );
        assert_eq!(1, rewards(deps.as_ref(), USER2), "180_000 * 1% / 1_000 = 1");
        assert_eq!(
            0,
            rewards(deps.as_ref(), USER3),
            "10_000 * 1% = 100 < min_bond"
        );

        // bond some more tokens for second period
        bond_cw20_with_period(
            deps.as_mut(),
            1_000_000,
            100_000,
            9_000,
            UNBONDING_PERIOD_2,
            2,
        );

        // assert updated rewards
        assert_eq!(
            110,
            rewards(deps.as_ref(), USER1),
            "10 + 1_000_000 * 10% / 1_000 = 110"
        );
        assert_eq!(
            11,
            rewards(deps.as_ref(), USER2),
            "1 + 100_000 * 10% / 1_000 = 11"
        );
        assert_eq!(
            0,
            rewards(deps.as_ref(), USER3),
            "0 + 9_000 * 10% = 900 < min_bond"
        );

        // rebond tokens
        rebond_with_period(
            deps.as_mut(),
            100_000,
            180_000,
            10_000,
            UNBONDING_PERIOD,
            UNBONDING_PERIOD_2,
            3,
        );

        // assert stake
        assert_stake(deps.as_ref(), &env, 900_000, 0, 0);
        assert_stake_in_period(
            deps.as_ref(),
            &env,
            1_100_000,
            280_000,
            19_000,
            UNBONDING_PERIOD_2,
        );
        // assert updated rewards
        assert_eq!(
            119,
            rewards(deps.as_ref(), USER1),
            "900_000 * 1% / 1_000 + 1_100_000 * 10% / 1_000 = 9 + 110 = 119"
        );
        assert_eq!(
            28,
            rewards(deps.as_ref(), USER2),
            "0 + 280_000 * 10% / 1_000 = 28"
        );
        assert_eq!(
            1,
            rewards(deps.as_ref(), USER3),
            "0 + 19_000 * 10% / 1_000 = 1"
        );
    }

    #[test]
    fn add_remove_hooks() {
        // add will over-write and remove have no effect
        let mut deps = mock_dependencies();
        let env = mock_env();
        default_instantiate(deps.as_mut(), env.clone());

        let hooks = HOOKS.query_hooks(deps.as_ref()).unwrap();
        assert!(hooks.hooks.is_empty());

        let contract1 = String::from("hook1");
        let contract2 = String::from("hook2");

        let add_msg = ExecuteMsg::AddHook {
            addr: contract1.clone(),
        };

        // non-admin cannot add hook
        let user_info = mock_info(USER1, &[]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            user_info.clone(),
            add_msg.clone(),
        )
        .unwrap_err();
        assert_eq!(err, HookError::Admin(AdminError::NotAdmin {}).into());

        // admin can add it, and it appears in the query
        let admin_info = mock_info(INIT_ADMIN, &[]);
        let _ = execute(
            deps.as_mut(),
            env.clone(),
            admin_info.clone(),
            add_msg.clone(),
        )
        .unwrap();
        let hooks = HOOKS.query_hooks(deps.as_ref()).unwrap();
        assert_eq!(hooks.hooks, vec![contract1.clone()]);

        // cannot remove a non-registered contract
        let remove_msg = ExecuteMsg::RemoveHook {
            addr: contract2.clone(),
        };
        let err = execute(deps.as_mut(), env.clone(), admin_info.clone(), remove_msg).unwrap_err();
        assert_eq!(err, HookError::HookNotRegistered {}.into());

        // add second contract
        let add_msg2 = ExecuteMsg::AddHook {
            addr: contract2.clone(),
        };
        let _ = execute(deps.as_mut(), env.clone(), admin_info.clone(), add_msg2).unwrap();
        let hooks = HOOKS.query_hooks(deps.as_ref()).unwrap();
        assert_eq!(hooks.hooks, vec![contract1.clone(), contract2.clone()]);

        // cannot re-add an existing contract
        let err = execute(deps.as_mut(), env.clone(), admin_info.clone(), add_msg).unwrap_err();
        assert_eq!(err, HookError::HookAlreadyRegistered {}.into());

        // non-admin cannot remove
        let remove_msg = ExecuteMsg::RemoveHook { addr: contract1 };
        let err = execute(deps.as_mut(), env.clone(), user_info, remove_msg.clone()).unwrap_err();
        assert_eq!(err, HookError::Admin(AdminError::NotAdmin {}).into());

        // remove the original
        let _ = execute(deps.as_mut(), env, admin_info, remove_msg).unwrap();
        let hooks = HOOKS.query_hooks(deps.as_ref()).unwrap();
        assert_eq!(hooks.hooks, vec![contract2]);
    }

    #[test]
    fn hooks_fire() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        default_instantiate(deps.as_mut(), env.clone());

        let hooks = HOOKS.query_hooks(deps.as_ref()).unwrap();
        assert!(hooks.hooks.is_empty());

        let contract1 = String::from("hook1");
        let contract2 = String::from("hook2");

        // register 2 hooks
        let admin_info = mock_info(INIT_ADMIN, &[]);
        let add_msg = ExecuteMsg::AddHook {
            addr: contract1.clone(),
        };
        let add_msg2 = ExecuteMsg::AddHook {
            addr: contract2.clone(),
        };
        for msg in vec![add_msg, add_msg2] {
            let _ = execute(deps.as_mut(), env.clone(), admin_info.clone(), msg).unwrap();
        }

        // check firing on bond
        assert_users(deps.as_ref(), env.clone(), None, None, None, None);
        let info = mock_info(CW20_ADDRESS, &[]);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::ReceiveDelegation(Cw20ReceiveDelegationMsg {
                sender: USER1.to_string(),
                amount: Uint128::new(13_800),
                msg: to_binary(&ReceiveDelegationMsg::Delegate {
                    unbonding_period: UNBONDING_PERIOD,
                })
                .unwrap(),
            }),
        )
        .unwrap();
        assert_users(deps.as_ref(), env.clone(), Some(13), None, None, None);

        // ensure messages for each of the 2 hooks
        assert_eq!(res.messages.len(), 2);
        let diff = MemberDiff::new(USER1, None, Some(13u128.into()));
        let hook_msg = MemberChangedHookMsg::one(diff);
        let msg1 = SubMsg::new(hook_msg.clone().into_cosmos_msg(contract1.clone()).unwrap());
        let msg2 = SubMsg::new(hook_msg.into_cosmos_msg(contract2.clone()).unwrap());
        assert_eq!(res.messages, vec![msg1, msg2]);

        // check firing on unbond
        let msg = ExecuteMsg::Unbond {
            tokens: Uint128::new(7_300),
            unbonding_period: UNBONDING_PERIOD,
        };
        let info = mock_info(USER1, &[]);
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_users(deps.as_ref(), env, Some(6), None, None, None);

        // ensure messages for each of the 2 hooks
        assert_eq!(res.messages.len(), 2);
        let diff = MemberDiff::new(USER1, Some(13u128.into()), Some(6u128.into()));
        let hook_msg = MemberChangedHookMsg::one(diff);
        let msg1 = SubMsg::new(hook_msg.clone().into_cosmos_msg(contract1).unwrap());
        let msg2 = SubMsg::new(hook_msg.into_cosmos_msg(contract2).unwrap());
        assert_eq!(res.messages, vec![msg1, msg2]);
    }

    #[test]
    fn ensure_bonding_edge_cases() {
        // use min_bond 0, tokens_per_power 500
        let mut deps = mock_dependencies();
        let env = mock_env();
        cw20_instantiate(
            deps.as_mut(),
            env.clone(),
            Uint128::new(100),
            Uint128::zero(),
            vec![StakeConfig {
                unbonding_period: UNBONDING_PERIOD,
                voting_multiplier: Decimal::one(),
                reward_multiplier: Decimal::one(),
            }],
        );

        // setting 50 tokens, gives us None power
        bond_cw20(deps.as_mut(), 50, 1, 102, 1);
        assert_users(deps.as_ref(), env.clone(), None, None, Some(1), None);

        // reducing to 0 token makes us None even with min_bond 0
        unbond(deps.as_mut(), 49, 1, 102, 2);
        assert_users(deps.as_ref(), env, None, None, None, None);
    }

    // we apply a 50% multiplier, which should affect min_bound
    #[test_case(4 ,0, 1 => panics "attempt to divide by zero")]
    #[test_case(4 ,6, 1 => 0; "when tokens_per_power greater than stake")]
    #[test_case(4 ,2, 1 => 1; "when tokens_per_power equals stake")]
    #[test_case(4 ,2, 3 => 1; "when tokens_per_power equals stake, min_bond higher than power")]
    #[test_case(4 ,1, 5 => 0; "when tokens_per_power equals stake, min_bond higher than stake")]
    fn test_update_membership_calc_power(stake: u128, tpower: u128, min_bound: u128) -> u128 {
        let cfg = Config {
            cw20_contract: Addr::unchecked("cw20_contract"),
            tokens_per_power: Uint128::new(tpower),
            min_bond: Uint128::new(min_bound),
            unbonding_periods: vec![0u64],
        };
        calc_power(&cfg, Uint128::new(stake), Decimal::percent(50)).u128()
    }

    #[test_case(1000 ,1, 1, 5 => 1000; "should success")]
    #[test_case(1000 ,0, 1, 5 => panics "attempt to divide by zero")]
    #[test_case(2 ,2, 1, 10 => 1; "when tokens_per_power equals stake should success")]
    #[test_case(2 ,2, 1, 0 => 1; "when unbonding_period equals zero should success")]
    fn test_update_membership_on_execute_bond(
        new_stake: u128,
        tokens_per_power: u128,
        min_bound: u128,
        unbonding_period: u64,
    ) -> u128 {
        // Given a cw20 instantiate
        let mut deps = mock_dependencies();
        let env = mock_env();
        cw20_instantiate(
            deps.as_mut(),
            env.clone(),
            Uint128::new(tokens_per_power),
            Uint128::new(min_bound),
            vec![StakeConfig {
                unbonding_period,
                voting_multiplier: Decimal::one(),
                reward_multiplier: Decimal::one(),
            }],
        );

        let cfg = CONFIG.load(&deps.storage).unwrap();
        let contract1 = String::from("hook1");

        // and a registered hook
        let admin_info = mock_info(INIT_ADMIN, &[]);
        let hook_msg = ExecuteMsg::AddHook {
            addr: contract1.clone(),
        };
        let _ = execute(deps.as_mut(), env.clone(), admin_info, hook_msg).unwrap();

        // and a User initial stake state
        let initial_stake =
            query_staked(deps.as_ref(), &env, USER1.into(), unbonding_period).unwrap();
        assert_eq!(initial_stake.stake, Uint128::zero());

        let initial_power =
            query_voting_power(deps.as_ref(), env.clone(), USER1.into(), None).unwrap();
        assert_eq!(initial_power.power, Uint128::zero());

        // and the expected result
        let new = calc_power(&cfg, Uint128::new(new_stake), Decimal::one());
        let diff = MemberDiff::new(USER1, None, Some(new));
        let hook_msg = MemberChangedHookMsg::one(diff);
        let msg = SubMsg::new(hook_msg.into_cosmos_msg(contract1).unwrap());

        // When called Cw20ReceiveDelegationMsg call execute_bond
        let info = mock_info(CW20_ADDRESS, &[]);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::ReceiveDelegation(Cw20ReceiveDelegationMsg {
                sender: USER1.to_string(),
                amount: Uint128::new(new_stake),
                msg: to_binary(&ReceiveDelegationMsg::Delegate { unbonding_period }).unwrap(),
            }),
        )
        .unwrap();

        // Then it should generate a Hook Submessage
        assert_eq!(res.messages, vec![msg]);

        // And staked value should increase
        let stake1 = query_staked(deps.as_ref(), &env, USER1.into(), unbonding_period).unwrap();
        assert_eq!(stake1.stake, Uint128::new(new_stake));

        let power1 = query_voting_power(deps.as_ref(), env, USER1.into(), None).unwrap();
        power1.power.u128()
    }

    #[test]
    fn test_query_info() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut(), mock_env());

        let info_response = query_info(deps.as_ref()).unwrap();
        assert_eq!(
            info_response,
            InfoResponse {
                info: ContractVersion {
                    contract: CONTRACT_NAME.to_owned(),
                    version: CONTRACT_VERSION.to_owned()
                }
            }
        );
    }

    #[test]
    fn test_query_bonding_info() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut(), mock_env());

        let bonding_info_response = query_bonding_info(deps.as_ref()).unwrap();
        assert_eq!(
            bonding_info_response,
            BondingInfoResponse {
                bonding: vec!(BondingPeriodInfo {
                    unbonding_period: 20,
                    voting_multiplier: Decimal::one(),
                    reward_multiplier: Decimal::one(),
                    total_staked: Uint128::zero(),
                })
            }
        );
    }

    #[test]
    fn test_token_contract() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut(), mock_env());

        let address = query_token_contract(deps.as_ref()).unwrap();
        let config = CONFIG.load(&deps.storage).unwrap();
        assert_eq!(address, config.cw20_contract);
    }

    #[test]
    fn migrate_same_version() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut(), mock_env());

        migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();
    }

    #[test]
    fn migrate_older_version() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut(), mock_env());

        let new_version = "0.0.1";
        cw2::set_contract_version(deps.as_mut().storage, CONTRACT_NAME, new_version).unwrap();

        migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();
    }

    #[test]
    fn migrate_newer_version() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut(), mock_env());

        let new_version = "10.0.0";
        cw2::set_contract_version(deps.as_mut().storage, CONTRACT_NAME, new_version).unwrap();

        let err = migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err(format!(
                "Cannot migrate from newer version ({}) to older ({})",
                new_version, CONTRACT_VERSION
            ))
            .into()
        );
    }
}
