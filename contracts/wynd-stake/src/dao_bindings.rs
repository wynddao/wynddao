// Custom helpers that provide adapter from standard dao-dao tests to our codebase.
// They are used by dao_tests.rs and contain code we want to maintain.
// Much of the other file should be replace by imports

use cosmwasm_std::{to_binary, Addr, Decimal, Empty, Uint128};
use cw20::Cw20Coin;
use cw_multi_test::{App, Contract, ContractWrapper, Executor};

const CREATOR_ADDR: &str = "creator";

fn cw20_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw20_vesting::contract::execute,
        cw20_vesting::contract::instantiate,
        cw20_vesting::contract::query,
    );
    Box::new(contract)
}

fn wynd_staked_voting_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );
    Box::new(contract)
}

pub fn single_proposal_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw_proposal_single::contract::execute,
        cw_proposal_single::contract::instantiate,
        cw_proposal_single::contract::query,
    )
    .with_reply(cw_proposal_single::contract::reply);
    Box::new(contract)
}

fn cw_gov_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw_core::contract::execute,
        cw_core::contract::instantiate,
        cw_core::contract::query,
    )
    .with_reply(cw_core::contract::reply);
    Box::new(contract)
}

pub fn instantiate_with_wynd_stake(
    app: &mut App,
    governance_code_id: u64,
    governance_instantiate: cw_proposal_single::msg::InstantiateMsg,
    initial_balances: Option<Vec<Cw20Coin>>,
) -> Addr {
    let cw20_id = app.store_code(cw20_contract());
    let core_id = app.store_code(cw_gov_contract());
    let votemod_id = app.store_code(wynd_staked_voting_contract());

    let initial_balances: Vec<_> = initial_balances.unwrap_or_else(|| {
        vec![Cw20Coin {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    // Collapse balances so that we can test double votes.
    let initial_balances: Vec<Cw20Coin> = {
        let mut already_seen = vec![];
        initial_balances
            .into_iter()
            .filter(|Cw20Coin { address, amount: _ }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .collect()
    };

    let initial_balances: Vec<_> = initial_balances
        .into_iter()
        .map(|acct| cw20_vesting::msg::InitBalance {
            address: acct.address,
            amount: acct.amount,
            vesting: None,
        })
        .collect();

    // make the vesting token (but don't use vesting)
    let cw20_instantiate = cw20_vesting::msg::InstantiateMsg {
        name: "DAO".to_string(),
        symbol: "DAO".to_string(),
        decimals: 6,
        initial_balances: initial_balances.clone(),
        marketing: None,
        mint: Some(cw20_vesting::msg::MinterInfo {
            minter: CREATOR_ADDR.to_string(),
            cap: None,
        }),
        allowed_vesters: None,
        max_curve_complexity: 10,
    };
    let cw20_addr = app
        .instantiate_contract(
            cw20_id,
            Addr::unchecked(CREATOR_ADDR),
            &cw20_instantiate,
            &[],
            "DAO DAO governance token".to_string(),
            Some(CREATOR_ADDR.into()),
        )
        .unwrap();

    let unbonding_period = 86400u64;
    let governance_instantiate = cw_core::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        image_url: None,
        automatically_add_cw20s: true,
        automatically_add_cw721s: true,
        voting_module_instantiate_info: cw_core::msg::ModuleInstantiateInfo {
            code_id: votemod_id,
            msg: to_binary(&crate::msg::InstantiateMsg {
                cw20_contract: cw20_addr.to_string(),
                tokens_per_power: Uint128::new(1),
                min_bond: Uint128::new(1),
                stake_config: vec![
                    // just one simple slot for the basic tests
                    crate::msg::StakeConfig {
                        unbonding_period,
                        voting_multiplier: Decimal::one(),
                        reward_multiplier: Decimal::one(),
                    },
                ],
                admin: None,
            })
            .unwrap(),
            admin: cw_core::msg::Admin::CoreContract {},
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![cw_core::msg::ModuleInstantiateInfo {
            code_id: governance_code_id,
            msg: to_binary(&governance_instantiate).unwrap(),
            admin: cw_core::msg::Admin::CoreContract {},
            label: "DAO DAO governance module".to_string(),
        }],
        initial_items: None,
    };

    let governance_addr = app
        .instantiate_contract(
            core_id,
            Addr::unchecked(CREATOR_ADDR),
            &governance_instantiate,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    // FIXME: this should be easier with the cw_core APIs - maybe in dao dao v2??
    let gov_state: cw_core::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            governance_addr.clone(),
            &cw_core::msg::QueryMsg::DumpState {},
        )
        .unwrap();
    let staking_contract = gov_state.voting_module;

    // cw20 vesting needs to know where to delegate to
    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        cw20_addr.clone(),
        &cw20_vesting::ExecuteMsg::UpdateStakingAddress {
            address: staking_contract.to_string(),
        },
        &[],
    )
    .unwrap();

    // Stake all the initial balances.
    for cw20_vesting::msg::InitBalance {
        address,
        amount,
        vesting: _,
    } in initial_balances
    {
        app.execute_contract(
            Addr::unchecked(&address),
            cw20_addr.clone(),
            &cw20_vesting::ExecuteMsg::Delegate {
                amount,
                msg: to_binary(&crate::msg::ReceiveDelegationMsg::Delegate { unbonding_period })
                    .unwrap(),
            },
            &[],
        )
        .unwrap();
    }

    // Update the block so that those staked balances appear.
    app.update_block(|block| block.height += 1);

    governance_addr
}
