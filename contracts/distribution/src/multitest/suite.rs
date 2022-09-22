use anyhow::Result as AnyResult;
use cosmwasm_std::{Addr, Empty, StdResult};
use cw20::{Cw20Coin, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use cw_multi_test::{App, AppResponse, Contract, ContractWrapper, Executor};

use crate::msg::{ExecuteMsg, InstantiateMsg};

use super::mock_recipient::mock_recipient;

pub fn contract_distribution() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

fn contract_cw20() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    );

    Box::new(contract)
}

#[derive(Debug, Default)]
pub struct SuiteBuilder {
    epoch: u64,
    payment: u128,
    admin: String,

    initial_balances: Vec<Cw20Coin>,
}

impl SuiteBuilder {
    pub fn new(epoch: u64, payment: u128, admin: impl Into<String>) -> Self {
        Self {
            epoch,
            payment,
            admin: admin.into(),
            initial_balances: vec![],
        }
    }

    pub fn with_initial_balances(mut self, balances: Vec<(&str, u128)>) -> Self {
        let initial_balances = balances
            .into_iter()
            .map(|(address, amount)| Cw20Coin {
                address: address.to_owned(),
                amount: amount.into(),
            })
            .collect::<Vec<_>>();
        self.initial_balances = initial_balances;
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let mut app = App::default();

        let cw20_id = app.store_code(contract_cw20());
        let cw20_contract = app
            .instantiate_contract(
                cw20_id,
                Addr::unchecked(&self.admin),
                &cw20_base::msg::InstantiateMsg {
                    name: "Test Token".into(),
                    symbol: "TEST".into(),
                    decimals: 8,
                    initial_balances: self.initial_balances,
                    mint: Some(MinterResponse {
                        cap: None,
                        minter: self.admin.clone(),
                    }),
                    marketing: None,
                },
                &[],
                "token",
                Some(self.admin.to_string()),
            )
            .unwrap();

        let id = app.store_code(mock_recipient());
        let recipient_contract = app
            .instantiate_contract(
                id,
                Addr::unchecked(&self.admin),
                &Empty {},
                &[],
                "receiver",
                Some(self.admin.clone()),
            )
            .unwrap();

        let dist_id = app.store_code(contract_distribution());
        let dist_contract = app
            .instantiate_contract(
                dist_id,
                Addr::unchecked(&self.admin),
                &InstantiateMsg {
                    cw20_contract: cw20_contract.to_string(),
                    epoch: self.epoch,
                    payment: self.payment.into(),
                    recipient: recipient_contract.to_string(),
                    admin: self.admin.clone(),
                },
                &[],
                "vesting",
                Some(self.admin),
            )
            .unwrap();

        Suite {
            app,
            cw20_contract,
            dist_contract,
            recipient_contract,
        }
    }
}

pub struct Suite {
    app: App,
    /// a standard cw20 contract whose tokens will be sent around here
    cw20_contract: Addr,
    /// the contract that is implemented in this crate
    dist_contract: Addr,
    /// receives the tokens from `dist_contract`
    recipient_contract: Addr,
}

impl Suite {
    pub fn distribution_contract(&self) -> String {
        self.dist_contract.to_string()
    }

    pub fn recipient_contract(&self) -> String {
        self.recipient_contract.to_string()
    }

    pub fn query_token_balance(&self, addr: impl Into<String>) -> StdResult<u128> {
        let resp: cw20::BalanceResponse = self.app.wrap().query_wasm_smart(
            self.cw20_contract.clone(),
            &Cw20QueryMsg::Balance {
                address: addr.into(),
            },
        )?;
        Ok(resp.balance.u128())
    }

    pub fn burn_token(
        &mut self,
        sender: impl Into<String>,
        amount: u128,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.cw20_contract.clone(),
            &Cw20ExecuteMsg::Burn {
                amount: amount.into(),
            },
            &[],
        )
    }

    pub fn transfer_token(
        &mut self,
        sender: impl Into<String>,
        recipient: impl Into<String>,
        amount: u128,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.cw20_contract.clone(),
            &Cw20ExecuteMsg::Transfer {
                recipient: recipient.into(),
                amount: amount.into(),
            },
            &[],
        )
    }

    pub fn trigger_payout(&mut self, sender: impl Into<String>) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.dist_contract.clone(),
            &ExecuteMsg::Payout {},
            &[],
        )
    }

    /// Moves the given amount of blocks and seconds forward
    pub fn fast_forward(&mut self, blocks: u64, seconds: u64) {
        self.app.update_block(|block| {
            block.height += blocks;
            block.time = block.time.plus_seconds(seconds)
        });
    }
}
