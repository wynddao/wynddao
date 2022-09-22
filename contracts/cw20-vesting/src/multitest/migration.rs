use cosmwasm_std::{Addr, Empty, StdResult};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};

use crate::{
    msg::{MaxVestingComplexityResponse, MigrateMsg},
    QueryMsg,
};

use super::suite::contract_vesting;

#[test]
fn migrate_to_0_5() {
    let mut app = App::default();

    let admin = Addr::unchecked("admin");

    // upload old contract and create instance
    let old_contract: Box<dyn Contract<Empty>> = Box::new(ContractWrapper::new_with_empty(
        cw20_vesting_0_4_1::contract::execute,
        cw20_vesting_0_4_1::contract::instantiate,
        cw20_vesting_0_4_1::contract::query,
    ));
    let old_id = app.store_code(old_contract);

    let instance = app
        .instantiate_contract(
            old_id,
            admin.clone(),
            &cw20_vesting_0_4_1::msg::InstantiateMsg {
                name: "vesting".to_owned(),
                symbol: "VEST".to_owned(),
                decimals: 9,
                initial_balances: vec![],
                mint: None,
                marketing: None,
                allowed_vesters: None,
            },
            &[],
            "vesting",
            Some(admin.to_string()),
        )
        .unwrap();

    // this version should not have max complexity query yet
    let response: StdResult<MaxVestingComplexityResponse> = app
        .wrap()
        .query_wasm_smart(instance.clone(), &QueryMsg::MaxVestingComplexity {});
    response.unwrap_err();

    // upload new code and migrate
    let new_id = app.store_code(contract_vesting());

    app.migrate_contract(
        admin,
        instance.clone(),
        &MigrateMsg {
            max_curve_complexity: 15,
        },
        new_id,
    )
    .unwrap();

    // check if complexity was set
    let response: MaxVestingComplexityResponse = app
        .wrap()
        .query_wasm_smart(instance, &QueryMsg::MaxVestingComplexity {})
        .unwrap();

    assert_eq!(15, response.complexity);
}
