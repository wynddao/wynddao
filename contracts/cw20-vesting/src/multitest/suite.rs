use anyhow::Result as AnyResult;

use cosmwasm_std::{to_binary, Addr, Empty, StdResult, Uint128};
use cw20::BalanceResponse;
use cw_multi_test::{App, AppResponse, Contract, ContractWrapper, Executor};

use super::staking_contract::{
    staking_contract, DelegateMsg, EmptyMsg, QueryMsg as StakingQueryMsg,
};
use crate::msg::{
    DelegatedResponse, ExecuteMsg, InitBalance, InstantiateMarketingInfo, InstantiateMsg,
    MinterInfo, QueryMsg, StakingAddressResponse, VestingResponse,
};
use wynd_utils::Curve;

pub fn contract_vesting() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_migrate(crate::contract::migrate);

    Box::new(contract)
}

#[derive(Debug, Default)]
pub struct SuiteBuilder {
    pub token_name: String,
    pub token_symbol: String,
    pub token_decimals: u8,
    pub initial_balances: Vec<InitBalance>,
    pub mint: Option<MinterInfo>,
    pub marketing: Option<InstantiateMarketingInfo>,
    pub allowed_vesters: Option<Vec<String>>,
}

impl SuiteBuilder {
    pub fn new() -> Self {
        Self {
            token_name: "vesting".to_owned(),
            token_symbol: "VEST".to_owned(),
            token_decimals: 9,
            initial_balances: vec![],
            mint: None,
            marketing: None,
            allowed_vesters: None,
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

    pub fn with_minter(mut self, minter: &str, cap: impl Into<Option<Curve>>) -> Self {
        let mint = MinterInfo {
            minter: minter.to_owned(),
            cap: cap.into(),
        };
        self.mint = Some(mint);
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
                &InstantiateMsg {
                    name: self.token_name.clone(),
                    symbol: self.token_symbol.clone(),
                    decimals: self.token_decimals,
                    initial_balances: self.initial_balances.clone(),
                    mint: self.mint.clone(),
                    marketing: self.marketing.clone(),
                    allowed_vesters: self.allowed_vesters,
                    max_curve_complexity: 10,
                },
                &[],
                "vesting",
                None,
            )
            .unwrap();

        let staking_id = app.store_code(staking_contract());
        let staking = app
            .instantiate_contract(staking_id, admin, &EmptyMsg {}, &[], "staking", None)
            .unwrap();

        Suite {
            app,
            vesting_contract,
            staking_contract: staking,
        }
    }
}

pub struct Suite {
    app: App,
    vesting_contract: Addr,
    staking_contract: Addr,
}

impl Suite {
    pub fn staking_contract(&mut self) -> String {
        self.staking_contract.to_string()
    }

    pub fn delegate(&mut self, sender: &str, amount: u128) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.vesting_contract.clone(),
            &ExecuteMsg::Delegate {
                amount: amount.into(),
                msg: to_binary(&DelegateMsg::Delegate)?,
            },
            &[],
        )
    }

    pub fn undelegate(
        &mut self,
        sender: &str,
        recipient: &str,
        amount: u128,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.vesting_contract.clone(),
            &ExecuteMsg::Undelegate {
                recipient: recipient.to_owned(),
                amount: amount.into(),
            },
            &[],
        )
    }

    pub fn update_staking_address(
        &mut self,
        sender: &str,
        address: &str,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.vesting_contract.clone(),
            &ExecuteMsg::UpdateStakingAddress {
                address: address.into(),
            },
            &[],
        )
    }

    pub fn query_balance(&self, address: &str) -> StdResult<u128> {
        let balance: BalanceResponse = self.app.wrap().query_wasm_smart(
            self.vesting_contract.clone(),
            &QueryMsg::Balance {
                address: address.to_owned(),
            },
        )?;
        Ok(balance.balance.u128())
    }

    /// Returns amount of token delegated by address passed in parameter
    pub fn query_delegated(&self, address: &str) -> StdResult<u128> {
        let delegated: DelegatedResponse = self.app.wrap().query_wasm_smart(
            self.vesting_contract.clone(),
            &QueryMsg::Delegated {
                address: address.to_owned(),
            },
        )?;
        Ok(delegated.delegated.u128())
    }

    /// Returns amount of token vested by address passed in parameter
    pub fn query_vested(&self, address: &str) -> StdResult<u128> {
        let vested: VestingResponse = self.app.wrap().query_wasm_smart(
            self.vesting_contract.clone(),
            &QueryMsg::Vesting {
                address: address.to_owned(),
            },
        )?;
        Ok(vested.locked.u128())
    }

    /// Performs only available query on mocked staking contract
    /// Returns sum of all staked tokens
    pub fn query_staking_contract(&self) -> StdResult<u128> {
        let delegated: Uint128 = self.app.wrap().query_wasm_smart(
            self.staking_contract.clone(),
            &StakingQueryMsg::Delegated {},
        )?;
        Ok(delegated.u128())
    }

    /// Returns currently assigned address of staking contract.
    /// At first it is not set and returns None.
    /// It can be set via ExecuteMsg::UpdateStakingAddress
    pub fn query_staking_address(&self) -> StdResult<Option<Addr>> {
        let response: StakingAddressResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.vesting_contract.clone(), &QueryMsg::StakingAddress {})?;
        Ok(response.address)
    }
}
