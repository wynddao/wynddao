use cosmwasm_std::{Addr, Empty, Uint128};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};

use crate::{
    msg::{MigrateMsg, MinterResponse},
    ContractError, QueryMsg,
};
use wynd_utils::{Curve, PiecewiseLinear};

use super::suite::contract_vesting;

#[test]
fn migrate_max_cap_curve() {
    let mut app = App::default();

    let admin = Addr::unchecked("admin");

    // upload old contract and create instance
    let old_contract: Box<dyn Contract<Empty>> = Box::new(ContractWrapper::new_with_empty(
        cw20_vesting_1_1_0::contract::execute,
        cw20_vesting_1_1_0::contract::instantiate,
        cw20_vesting_1_1_0::contract::query,
    ));
    let old_id = app.store_code(old_contract);

    let instance = app
        .instantiate_contract(
            old_id,
            admin.clone(),
            &cw20_vesting_1_1_0::msg::InstantiateMsg {
                name: "vesting".to_owned(),
                symbol: "VEST".to_owned(),
                decimals: 9,
                initial_balances: vec![],
                mint: Some(cw20_vesting_1_1_0::msg::MinterInfo {
                    minter: "minteraddress".to_owned(),
                    cap: Some(wynd_utils_1_1_0::Curve::constant(42u128)),
                }),
                marketing: None,
                allowed_vesters: None,
                max_curve_complexity: 500,
            },
            &[],
            "vesting",
            Some(admin.to_string()),
        )
        .unwrap();

    // upload new code and migrate
    let new_id = app.store_code(contract_vesting());

    // passing any curve other then Picewise Linear will fail the migration
    let err = app
        .migrate_contract(
            admin.clone(),
            instance.clone(),
            &MigrateMsg {
                picewise_linear_curve: Curve::saturating_linear((0, 100), (100, 0)),
            },
            new_id,
        )
        .unwrap_err();
    assert_eq!(
        ContractError::MigrationIncorrectCurve {},
        err.downcast().unwrap()
    );

    let new_curve = Curve::PiecewiseLinear(PiecewiseLinear {
        steps: vec![
            (100_000, Uint128::new(3_000_000)),
            (200_000, Uint128::new(3_500_000)),
            (300_000, Uint128::new(275_000)),
        ],
    });
    app.migrate_contract(
        admin,
        instance.clone(),
        &MigrateMsg {
            picewise_linear_curve: new_curve.clone(),
        },
        new_id,
    )
    .unwrap();

    // confim the value
    let response = app
        .wrap()
        .query_wasm_smart::<MinterResponse>(instance, &QueryMsg::Minter {})
        .unwrap()
        .cap
        .unwrap();
    assert_eq!(response, new_curve);
}
