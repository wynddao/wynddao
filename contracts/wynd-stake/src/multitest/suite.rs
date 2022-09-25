use anyhow::Result as AnyResult;

use cosmwasm_std::{to_binary, Addr, Decimal, Empty, StdResult, Uint128};
use cw20::BalanceResponse;
use cw_controllers::{Claim, ClaimsResponse};
use cw_core_interface::voting::VotingPowerAtHeightResponse;
use cw_multi_test::{App, AppResponse, Contract, ContractWrapper, Executor};

use crate::msg::{
    AllStakedResponse, BondingInfoResponse, BondingPeriodInfo, DelegatedResponse,
    DistributedRewardsResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveDelegationMsg,
    RewardsResponse, StakeConfig, StakedResponse, TotalRewardsResponse, TotalStakedResponse,
    UndistributedRewardsResponse, WithdrawableRewardsResponse,
};
use cw20_vesting::{
    ExecuteMsg as VestingExecuteMsg, InitBalance, InstantiateMsg as VestingInstantiateMsg,
    MinterInfo, QueryMsg as VestingQueryMsg,
};
use wynd_utils::Curve;

pub const SEVEN_DAYS: u64 = 604800;

fn contract_stake() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

fn contract_vesting() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(
        cw20_vesting::contract::execute,
        cw20_vesting::contract::instantiate,
        cw20_vesting::contract::query,
    );

    Box::new(contract)
}

#[derive(Debug)]
pub struct SuiteBuilder {
    pub cw20_contract: String,
    pub tokens_per_power: Uint128,
    pub min_bond: Uint128,
    pub stake_config: Vec<StakeConfig>,
    pub admin: Option<String>,
    pub initial_balances: Vec<InitBalance>,
}

impl SuiteBuilder {
    pub fn new() -> Self {
        Self {
            cw20_contract: "".to_owned(),
            tokens_per_power: Uint128::new(1000),
            min_bond: Uint128::new(5000),
            stake_config: vec![StakeConfig {
                unbonding_period: SEVEN_DAYS,
                voting_multiplier: Decimal::one(),
                reward_multiplier: Decimal::one(),
            }],
            admin: None,
            initial_balances: vec![],
        }
    }

    pub fn with_initial_balances(
        mut self,
        balances: Vec<(&str, u128, impl Into<Option<Curve>>)>,
    ) -> Self {
        let initial_balances = balances
            .into_iter()
            .map(|(address, amount, vesting)| InitBalance {
                address: address.to_owned(),
                amount: amount.into(),
                vesting: vesting.into(),
            })
            .collect::<Vec<InitBalance>>();
        self.initial_balances = initial_balances;
        self
    }

    pub fn with_min_bond(mut self, min_bond: u128) -> Self {
        self.min_bond = min_bond.into();
        self
    }

    pub fn with_stake_config_voting(self, stake_config: Vec<(u64, Decimal)>) -> Self {
        self.with_stake_config(
            stake_config
                .into_iter()
                .map(|(p, v)| (p, v, Decimal::one()))
                .collect(),
        )
    }

    pub fn with_stake_config(mut self, stake_config: Vec<(u64, Decimal, Decimal)>) -> Self {
        let stake_config = stake_config
            .into_iter()
            .map(
                |(unbonding_period, voting_multiplier, reward_multiplier)| StakeConfig {
                    unbonding_period,
                    voting_multiplier,
                    reward_multiplier,
                },
            )
            .collect::<Vec<StakeConfig>>();
        self.stake_config = stake_config;
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let mut app: App = App::default();

        let admin = Addr::unchecked("admin");

        let vesting_id = app.store_code(contract_vesting());
        let vesting_contract = app
            .instantiate_contract(
                vesting_id,
                admin.clone(),
                &VestingInstantiateMsg {
                    name: "vesting".to_owned(),
                    symbol: "VEST".to_owned(),
                    decimals: 9,
                    initial_balances: self.initial_balances,
                    mint: Some(MinterInfo {
                        minter: "minter".to_owned(),
                        cap: None,
                    }),
                    marketing: None,
                    allowed_vesters: None,
                    max_curve_complexity: 10,
                },
                &[],
                "vesting",
                None,
            )
            .unwrap();

        let stake_id = app.store_code(contract_stake());
        let stake_contract = app
            .instantiate_contract(
                stake_id,
                admin,
                &InstantiateMsg {
                    cw20_contract: vesting_contract.to_string(),
                    tokens_per_power: self.tokens_per_power,
                    min_bond: self.min_bond,
                    stake_config: self.stake_config,
                    admin: self.admin,
                },
                &[],
                "stake",
                None,
            )
            .unwrap();

        // Now update staking address on vesting contract
        app.execute_contract(
            Addr::unchecked("minter"),
            vesting_contract.clone(),
            &VestingExecuteMsg::UpdateStakingAddress {
                address: stake_contract.to_string(),
            },
            &[],
        )
        .unwrap();

        Suite {
            app,
            stake_contract,
            vesting_contract,
        }
    }
}

pub struct Suite {
    app: App,
    stake_contract: Addr,
    vesting_contract: Addr,
}

impl Suite {
    pub fn stake_contract(&self) -> String {
        self.stake_contract.to_string()
    }

    pub fn vesting_contract(&self) -> String {
        self.vesting_contract.to_string()
    }

    // update block's time to simulate passage of time
    pub fn update_time(&mut self, time_update: u64) {
        let mut block = self.app.block_info();
        block.time = block.time.plus_seconds(time_update);
        self.app.set_block(block);
    }

    fn unbonding_period_or_default(&self, unbonding_period: impl Into<Option<u64>>) -> u64 {
        // Use default SEVEN_DAYS unbonding period if none provided
        if let Some(up) = unbonding_period.into() {
            up
        } else {
            SEVEN_DAYS
        }
    }

    // call to vesting contract by sender
    pub fn delegate(
        &mut self,
        sender: &str,
        amount: u128,
        unbonding_period: impl Into<Option<u64>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.vesting_contract.clone(),
            &VestingExecuteMsg::Delegate {
                amount: amount.into(),
                msg: to_binary(&ReceiveDelegationMsg::Delegate {
                    unbonding_period: self.unbonding_period_or_default(unbonding_period),
                })?,
            },
            &[],
        )
    }

    // call to stake contract by sender
    pub fn rebond(
        &mut self,
        sender: &str,
        amount: u128,
        bond_from: impl Into<Option<u64>>,
        bond_to: impl Into<Option<u64>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.stake_contract.clone(),
            &ExecuteMsg::Rebond {
                tokens: amount.into(),
                bond_from: self.unbonding_period_or_default(bond_from),
                bond_to: self.unbonding_period_or_default(bond_to),
            },
            &[],
        )
    }

    pub fn unbond(
        &mut self,
        sender: &str,
        amount: u128,
        unbonding_period: impl Into<Option<u64>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.stake_contract.clone(),
            &ExecuteMsg::Unbond {
                tokens: amount.into(),
                unbonding_period: self.unbonding_period_or_default(unbonding_period),
            },
            &[],
        )
    }

    pub fn claim(&mut self, sender: &str) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.stake_contract.clone(),
            &ExecuteMsg::Claim {},
            &[],
        )
    }

    // call to vesting contract
    pub fn transfer(
        &mut self,
        sender: &str,
        recipient: &str,
        amount: impl Into<Uint128>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.vesting_contract.clone(),
            &VestingExecuteMsg::Transfer {
                recipient: recipient.into(),
                amount: amount.into(),
            },
            &[],
        )
    }

    pub fn distribute_funds<'s>(
        &mut self,
        executor: &str,
        sender: impl Into<Option<&'s str>>,
        funds: u128,
    ) -> AnyResult<AppResponse> {
        self.transfer(executor, self.stake_contract.clone().as_str(), funds)?;
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.stake_contract.clone(),
            &ExecuteMsg::DistributeRewards {
                sender: sender.into().map(str::to_owned),
            },
            &[],
        )
    }

    pub fn withdraw_funds<'s>(
        &mut self,
        executor: &str,
        owner: impl Into<Option<&'s str>>,
        receiver: impl Into<Option<&'s str>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.stake_contract.clone(),
            &ExecuteMsg::WithdrawRewards {
                owner: owner.into().map(str::to_owned),
                receiver: receiver.into().map(str::to_owned),
            },
            &[],
        )
    }

    #[allow(dead_code)]
    pub fn delegate_withdrawal(
        &mut self,
        executor: &str,
        delegated: &str,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.stake_contract.clone(),
            &ExecuteMsg::DelegateWithdrawal {
                delegated: delegated.to_owned(),
            },
            &[],
        )
    }

    pub fn withdrawable_rewards(&self, owner: &str) -> StdResult<u128> {
        let resp: WithdrawableRewardsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::WithdrawableRewards {
                owner: owner.to_owned(),
            },
        )?;
        Ok(resp.rewards.u128())
    }

    pub fn distributed_funds(&self) -> StdResult<u128> {
        let resp: DistributedRewardsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::DistributedRewards {},
        )?;
        Ok(resp.distributed.u128())
    }

    pub fn withdrawable_funds(&self) -> StdResult<u128> {
        let resp: DistributedRewardsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::DistributedRewards {},
        )?;
        Ok(resp.withdrawable.u128())
    }

    pub fn undistributed_funds(&self) -> StdResult<u128> {
        let resp: UndistributedRewardsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::UndistributedRewards {},
        )?;
        Ok(resp.rewards.u128())
    }

    #[allow(dead_code)]
    pub fn delegated(&self, owner: &str) -> StdResult<Addr> {
        let resp: DelegatedResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::Delegated {
                owner: owner.to_owned(),
            },
        )?;
        Ok(resp.delegated)
    }

    // returns address' balance on vesting contract
    pub fn query_balance_vesting_contract(&self, address: &str) -> StdResult<u128> {
        let balance: BalanceResponse = self.app.wrap().query_wasm_smart(
            self.vesting_contract.clone(),
            &VestingQueryMsg::Balance {
                address: address.to_owned(),
            },
        )?;
        Ok(balance.balance.u128())
    }

    // returns address' balance on vesting contract
    pub fn query_balance_staking_contract(&self) -> StdResult<u128> {
        let balance: BalanceResponse = self.app.wrap().query_wasm_smart(
            self.vesting_contract.clone(),
            &VestingQueryMsg::Balance {
                address: self.stake_contract.to_string(),
            },
        )?;
        Ok(balance.balance.u128())
    }

    pub fn query_staked(
        &self,
        address: &str,
        unbonding_period: impl Into<Option<u64>>,
    ) -> StdResult<u128> {
        let staked: StakedResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::Staked {
                address: address.to_owned(),
                unbonding_period: self.unbonding_period_or_default(unbonding_period),
            },
        )?;
        Ok(staked.stake.u128())
    }

    pub fn query_staked_periods(&self) -> StdResult<Vec<BondingPeriodInfo>> {
        let info: BondingInfoResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.stake_contract.clone(), &QueryMsg::BondingInfo {})?;
        Ok(info.bonding)
    }

    pub fn query_all_staked(&self, address: &str) -> StdResult<AllStakedResponse> {
        let all_staked: AllStakedResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::AllStaked {
                address: address.to_owned(),
            },
        )?;
        Ok(all_staked)
    }

    pub fn query_total_staked(&self) -> StdResult<u128> {
        let total_staked: TotalStakedResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.stake_contract.clone(), &QueryMsg::TotalStaked {})?;
        Ok(total_staked.total_staked.u128())
    }

    pub fn query_claims(&self, address: &str) -> StdResult<Vec<Claim>> {
        let claims: ClaimsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::Claims {
                address: address.to_owned(),
            },
        )?;
        Ok(claims.claims)
    }

    pub fn query_voting_power(
        &self,
        address: &str,
        height: impl Into<Option<u64>>,
    ) -> StdResult<u128> {
        let member: VotingPowerAtHeightResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::VotingPowerAtHeight {
                address: address.to_owned(),
                height: height.into(),
            },
        )?;
        Ok(member.power.u128())
    }

    pub fn query_total_power(&self, height: impl Into<Option<u64>>) -> StdResult<u128> {
        let total_power: VotingPowerAtHeightResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::TotalPowerAtHeight {
                height: height.into(),
            },
        )?;
        Ok(total_power.power.u128())
    }

    pub fn query_rewards(&self, address: &str) -> StdResult<u128> {
        let rewards: RewardsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::Rewards {
                address: address.to_owned(),
            },
        )?;

        Ok(rewards.rewards.u128())
    }

    pub fn query_total_rewards(&self) -> StdResult<u128> {
        let rewards: TotalRewardsResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.stake_contract.clone(), &QueryMsg::TotalRewards {})?;

        Ok(rewards.rewards.u128())
    }
}
