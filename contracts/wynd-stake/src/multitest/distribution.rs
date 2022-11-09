use cosmwasm_std::Decimal;

use super::suite::SuiteBuilder;
use crate::ContractError;

#[test]
fn divisible_amount_distributed() {
    let members = vec![
        "member1".to_owned(),
        "member2".to_owned(),
        "member3".to_owned(),
        "member4".to_owned(),
    ];
    let bonds = vec![5_000u128, 10_000u128, 25_000u128];
    let delegated: u128 = bonds.iter().sum();
    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        .with_initial_balances(vec![
            (&members[0], bonds[0], None),
            (&members[1], bonds[1], None),
            (&members[2], bonds[2], None),
            (&members[3], 400u128, None),
        ])
        .build();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), 0);

    suite
        .delegate(&members[0], bonds[0], unbonding_period)
        .unwrap();
    suite
        .delegate(&members[1], bonds[1], unbonding_period)
        .unwrap();
    suite
        .delegate(&members[2], bonds[2], unbonding_period)
        .unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    let _resp = suite.distribute_funds(&members[3], None, 400).unwrap();

    // resp.assert_event(&distribution_event(&members[3], &denom, 400));

    assert_eq!(
        suite.query_balance_staking_contract().unwrap(),
        delegated + 400,
    );

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        0
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        0
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        0
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[3]).unwrap(),
        0
    );

    assert_eq!(suite.withdrawable_rewards(&members[0]).unwrap(), 50);
    assert_eq!(suite.withdrawable_rewards(&members[1]).unwrap(), 100);
    assert_eq!(suite.withdrawable_rewards(&members[2]).unwrap(), 250,);

    assert_eq!(suite.distributed_funds().unwrap(), 400);
    assert_eq!(suite.undistributed_funds().unwrap(), 0);

    suite.withdraw_funds(&members[0], None, None).unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();
    suite.withdraw_funds(&members[2], None, None).unwrap();

    // assert_eq!(
    //     suite
    //         .query_balance_vesting_contract(suite.stake_contract().as_str())
    //         .unwrap(),
    //     0
    // );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        50
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        100
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        250
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[3]).unwrap(),
        0
    );
}

#[test]
fn divisible_amount_distributed_twice() {
    let members = vec![
        "member1".to_owned(),
        "member2".to_owned(),
        "member3".to_owned(),
        "member4".to_owned(),
    ];

    let bonds = vec![5_000u128, 10_000u128, 25_000u128];
    let delegated: u128 = bonds.iter().sum();
    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        .with_initial_balances(vec![
            (&members[0], bonds[0], None),
            (&members[1], bonds[1], None),
            (&members[2], bonds[2], None),
            (&members[3], 1000u128, None),
        ])
        .build();

    suite
        .delegate(&members[0], bonds[0], unbonding_period)
        .unwrap();
    suite
        .delegate(&members[1], bonds[1], unbonding_period)
        .unwrap();
    suite
        .delegate(&members[2], bonds[2], unbonding_period)
        .unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    suite.distribute_funds(&members[3], None, 400).unwrap();

    assert_eq!(suite.distributed_funds().unwrap(), 400);
    assert_eq!(suite.undistributed_funds().unwrap(), 0);

    suite.withdraw_funds(&members[0], None, None).unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();
    suite.withdraw_funds(&members[2], None, None).unwrap();

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        50
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        100
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        250
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[3]).unwrap(),
        600
    );

    suite.distribute_funds(&members[3], None, 600).unwrap();

    assert_eq!(suite.distributed_funds().unwrap(), 1000);
    assert_eq!(suite.undistributed_funds().unwrap(), 0);

    suite.withdraw_funds(&members[0], None, None).unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();
    suite.withdraw_funds(&members[2], None, None).unwrap();

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        125
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        250
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        625
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[3]).unwrap(),
        0
    );
}

#[test]
fn divisible_amount_distributed_twice_accumulated() {
    let members = vec![
        "member1".to_owned(),
        "member2".to_owned(),
        "member3".to_owned(),
        "member4".to_owned(),
    ];

    let bonds = vec![5_000u128, 10_000u128, 25_000u128];
    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        .with_initial_balances(vec![
            (&members[0], bonds[0], None),
            (&members[1], bonds[1], None),
            (&members[2], bonds[2], None),
            (&members[3], 1000u128, None),
        ])
        .build();

    suite
        .delegate(&members[0], bonds[0], unbonding_period)
        .unwrap();
    suite
        .delegate(&members[1], bonds[1], unbonding_period)
        .unwrap();
    suite
        .delegate(&members[2], bonds[2], unbonding_period)
        .unwrap();

    suite.distribute_funds(&members[3], None, 400).unwrap();

    suite.distribute_funds(&members[3], None, 600).unwrap();

    assert_eq!(suite.distributed_funds().unwrap(), 1000);
    assert_eq!(suite.undistributed_funds().unwrap(), 0);

    suite.withdraw_funds(&members[0], None, None).unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();
    suite.withdraw_funds(&members[2], None, None).unwrap();

    assert_eq!(
        suite
            .query_balance_vesting_contract(suite.vesting_contract().as_str())
            .unwrap(),
        0
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        125
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        250
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        625
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[3]).unwrap(),
        0
    );
}

#[test]
fn points_changed_after_distribution() {
    let members = vec![
        "member0".to_owned(),
        "member1".to_owned(),
        "member2".to_owned(),
        "member3".to_owned(),
    ];

    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one(), Decimal::percent(200)),
        ])
        .with_min_bond(1000)
        .with_initial_balances(vec![
            (&members[0], 6_000u128, None),
            (&members[1], 2_000u128, None),
            (&members[2], 5_000u128, None),
            (&members[3], 1500u128, None),
        ])
        .build();

    suite.delegate(&members[0], 1000, unbonding_period).unwrap();
    suite.delegate(&members[1], 2000, unbonding_period).unwrap();
    suite.delegate(&members[2], 5000, unbonding_period).unwrap();

    assert_eq!(suite.query_rewards(&members[0]).unwrap(), 2u128);
    assert_eq!(suite.query_rewards(&members[1]).unwrap(), 4u128);
    assert_eq!(suite.query_rewards(&members[2]).unwrap(), 10u128);
    assert_eq!(suite.query_total_rewards().unwrap(), 16u128);

    suite.distribute_funds(&members[3], None, 400).unwrap();
    assert_eq!(suite.undistributed_funds().unwrap(), 0u128);
    assert_eq!(suite.withdrawable_funds().unwrap(), 400u128);
    // TODO: add distributed / withdrawable tests

    // Modifying power to:
    // member[0] => 6
    // member[1] => 0 (removed)
    // member[2] => 5
    suite.delegate(&members[0], 5000, unbonding_period).unwrap();
    suite.unbond(&members[1], 2000, unbonding_period).unwrap();
    // BUG: unbonding tokens are considered rewards to be paid out
    assert_eq!(suite.undistributed_funds().unwrap(), 0u128);
    assert_eq!(suite.withdrawable_funds().unwrap(), 400u128);

    assert_eq!(suite.query_rewards(&members[0]).unwrap(), 12u128);
    assert_eq!(suite.query_rewards(&members[1]).unwrap(), 0u128);
    assert_eq!(suite.query_rewards(&members[2]).unwrap(), 10u128);
    assert_eq!(suite.query_total_rewards().unwrap(), 22u128);

    // Ensure funds are withdrawn properly, considering old points
    suite.withdraw_funds(&members[0], None, None).unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();
    suite.withdraw_funds(&members[2], None, None).unwrap();
    assert_eq!(suite.distributed_funds().unwrap(), 400u128);
    assert_eq!(suite.undistributed_funds().unwrap(), 0u128);
    assert_eq!(suite.withdrawable_funds().unwrap(), 0u128);

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        50
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        100
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        250
    );

    // Distribute tokens again to ensure distribution considers new points
    // 600 -> member0 and 500 -> member2
    suite.distribute_funds(&members[3], None, 1100).unwrap();
    assert_eq!(suite.distributed_funds().unwrap(), 1500u128);
    assert_eq!(suite.withdrawable_funds().unwrap(), 1100u128);

    suite.withdraw_funds(&members[0], None, None).unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();
    suite.withdraw_funds(&members[2], None, None).unwrap();
    assert_eq!(suite.withdrawable_funds().unwrap(), 0u128);

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        650
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        100
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        750
    );
}

#[test]
fn points_changed_after_distribution_accumulated() {
    let members = vec![
        "member1".to_owned(),
        "member2".to_owned(),
        "member3".to_owned(),
        "member4".to_owned(),
    ];

    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one(), Decimal::percent(200)),
        ])
        .with_min_bond(1000)
        .with_initial_balances(vec![
            (&members[0], 6_000u128, None),
            (&members[1], 2_000u128, None),
            (&members[2], 5_000u128, None),
            (&members[3], 1500u128, None),
        ])
        .build();

    suite.delegate(&members[0], 1000, unbonding_period).unwrap();
    suite.delegate(&members[1], 2000, unbonding_period).unwrap();
    suite.delegate(&members[2], 5000, unbonding_period).unwrap();

    suite.distribute_funds(&members[3], None, 400).unwrap();
    // Modifying wights to:
    // member[0] => 6
    // member[1] => 0 (removed)
    // member[2] => 5
    // total_points => 11
    suite.delegate(&members[0], 5000, unbonding_period).unwrap();
    suite.unbond(&members[1], 2000, unbonding_period).unwrap();

    // Distribute tokens again to ensure distribution considers new points
    suite.distribute_funds(&members[3], None, 1100).unwrap();

    // Withdraws sums of both distributions, so it works when they were using different points
    suite.withdraw_funds(&members[0], None, None).unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();
    suite.withdraw_funds(&members[2], None, None).unwrap();

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        650
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        100
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        750
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[3]).unwrap(),
        0
    );
}

#[test]
fn distribution_with_leftover() {
    let members = vec![
        "member1".to_owned(),
        "member2".to_owned(),
        "member3".to_owned(),
        "member4".to_owned(),
    ];

    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        // points are set to be prime numbers, difficult to distribute over. All are mutually prime
        // with distributed amount
        .with_initial_balances(vec![
            (&members[0], 7_000u128, None),
            (&members[1], 11_000u128, None),
            (&members[2], 13_000u128, None),
            (&members[3], 3100u128, None),
        ])
        .build();

    suite
        .delegate(&members[0], 7_000, unbonding_period)
        .unwrap();
    suite
        .delegate(&members[1], 11_000, unbonding_period)
        .unwrap();
    suite
        .delegate(&members[2], 13_000, unbonding_period)
        .unwrap();

    suite.distribute_funds(&members[3], None, 100).unwrap();

    suite.withdraw_funds(&members[0], None, None).unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();
    suite.withdraw_funds(&members[2], None, None).unwrap();

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        22
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        35
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        41
    );

    // Second distribution adding to the first one would actually make it properly divisible,
    // all shares should be properly split
    suite.distribute_funds(&members[3], None, 3000).unwrap();

    suite.withdraw_funds(&members[0], None, None).unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();
    suite.withdraw_funds(&members[2], None, None).unwrap();

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        700
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        1100
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        1300
    );
}

#[test]
fn distribution_with_leftover_accumulated() {
    let members = vec![
        "member1".to_owned(),
        "member2".to_owned(),
        "member3".to_owned(),
        "member4".to_owned(),
    ];

    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        // points are set to be prime numbers, difficult to distribute over. All are mutually prime
        // with distributed amount
        .with_initial_balances(vec![
            (&members[0], 7_000u128, None),
            (&members[1], 11_000u128, None),
            (&members[2], 13_000u128, None),
            (&members[3], 3100u128, None),
        ])
        .build();

    suite
        .delegate(&members[0], 7_000, unbonding_period)
        .unwrap();
    suite
        .delegate(&members[1], 11_000, unbonding_period)
        .unwrap();
    suite
        .delegate(&members[2], 13_000, unbonding_period)
        .unwrap();

    suite.distribute_funds(&members[3], None, 100).unwrap();

    // Second distribution adding to the first one would actually make it properly divisible,
    // all shares should be properly split
    suite.distribute_funds(&members[3], None, 3000).unwrap();

    suite.withdraw_funds(&members[0], None, None).unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();
    suite.withdraw_funds(&members[2], None, None).unwrap();

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        700
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        1100
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        1300
    );
}

#[test]
fn redirecting_withdrawn_funds() {
    let members = vec![
        "member1".to_owned(),
        "member2".to_owned(),
        "member3".to_owned(),
        "member4".to_owned(),
    ];

    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        .with_min_bond(1000)
        // points are set to be prime numbers, difficult to distribute over. All are mutually prime
        // with distributed amount
        .with_initial_balances(vec![
            (&members[0], 4_000u128, None),
            (&members[1], 6_000u128, None),
            (&members[3], 100u128, None),
        ])
        .build();

    suite
        .delegate(&members[0], 4_000, unbonding_period)
        .unwrap();
    suite
        .delegate(&members[1], 6_000, unbonding_period)
        .unwrap();

    suite.distribute_funds(&members[3], None, 100).unwrap();

    suite
        .withdraw_funds(&members[0], None, members[2].as_str())
        .unwrap();
    suite.withdraw_funds(&members[1], None, None).unwrap();

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        0
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        60
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        40
    );
}

#[test]
fn cannot_withdraw_others_funds() {
    let members = vec![
        "member1".to_owned(),
        "member2".to_owned(),
        "member3".to_owned(),
    ];
    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        .with_min_bond(1000)
        .with_initial_balances(vec![
            (&members[0], 4_000u128, None),
            (&members[1], 6_000u128, None),
            (&members[2], 100u128, None),
        ])
        .build();

    suite
        .delegate(&members[0], 4_000u128, unbonding_period)
        .unwrap();
    suite
        .delegate(&members[1], 6_000u128, unbonding_period)
        .unwrap();

    suite.distribute_funds(&members[2], None, 100).unwrap();
    assert_eq!(suite.query_balance_staking_contract().unwrap(), 10100);

    let err = suite
        .withdraw_funds(&members[0], members[1].as_str(), None)
        .unwrap_err();

    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    suite
        .withdraw_funds(&members[1], members[1].as_str(), None)
        .unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), 10040);
    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        0
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        60
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        0
    );
}

#[test]
fn funds_withdrawal_delegation() {
    let members = vec![
        "member1".to_owned(),
        "member2".to_owned(),
        "member3".to_owned(),
    ];

    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        .with_min_bond(1000)
        .with_initial_balances(vec![
            (&members[0], 4_000u128, None),
            (&members[1], 6_000u128, None),
            (&members[2], 100u128, None),
        ])
        .build();
    suite
        .delegate(&members[0], 4_000u128, unbonding_period)
        .unwrap();
    suite
        .delegate(&members[1], 6_000u128, unbonding_period)
        .unwrap();

    assert_eq!(
        suite.delegated(&members[0]).unwrap().as_str(),
        members[0].as_str()
    );
    assert_eq!(
        suite.delegated(&members[1]).unwrap().as_str(),
        members[1].as_str()
    );

    suite.distribute_funds(&members[2], None, 100).unwrap();

    suite.delegate_withdrawal(&members[1], &members[0]).unwrap();

    suite
        .withdraw_funds(&members[0], members[1].as_str(), None)
        .unwrap();
    suite
        .withdraw_funds(&members[0], members[0].as_str(), None)
        .unwrap();

    assert_eq!(
        suite.delegated(&members[0]).unwrap().as_str(),
        members[0].as_str()
    );
    assert_eq!(
        suite.delegated(&members[1]).unwrap().as_str(),
        members[0].as_str()
    );

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        100
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        0
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[2]).unwrap(),
        0
    );
}

#[test]
fn querying_unknown_address() {
    let suite = SuiteBuilder::new().build();

    let resp = suite.withdrawable_rewards("unknown").unwrap();
    assert_eq!(resp, 0);
}

#[test]
fn rebond_works() {
    let members = vec!["member0".to_owned(), "member1".to_owned()];
    let executor = "executor";

    let unbonding_period = 1000u64;
    let unbonding_period2 = 2000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config(vec![
            // one unbonding_period with rewards power 1.0
            (unbonding_period, Decimal::one(), Decimal::one()),
            // later unbonding_period with rewards power 2.0
            (unbonding_period2, Decimal::one(), Decimal::percent(200)),
        ])
        .with_min_bond(1000)
        .with_initial_balances(vec![
            (&members[0], 1_000u128, None),
            (&members[1], 2_000u128, None),
            (executor, 450 + 300, None),
        ])
        .build();

    // delegate
    suite
        .delegate(&members[0], 1_000u128, unbonding_period)
        .unwrap();
    suite
        .delegate(&members[1], 2_000u128, unbonding_period)
        .unwrap();

    // rebond member1 up to unbonding_period2
    suite
        .rebond(&members[1], 2_000u128, unbonding_period, unbonding_period2)
        .unwrap();
    // rewards power breakdown:
    // member0: 1000 * 1 / 1000 = 1
    // member1: 2000 * 2 / 1000 = 4
    // total: 5

    // distribute
    suite.distribute_funds(executor, None, 450).unwrap();

    // withdraw
    suite
        .withdraw_funds(&members[0], members[0].as_str(), None)
        .unwrap();
    suite
        .withdraw_funds(&members[1], members[1].as_str(), None)
        .unwrap();

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        90,
        "member0 should have received 450 * 1 / 5 = 90"
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        360,
        "member1 should have received 450 * 4 / 5 = 360"
    );

    // rebond member1 down again to unbonding_period
    suite
        .rebond(&members[1], 2_000u128, unbonding_period2, unbonding_period)
        .unwrap();
    // rewards power breakdown:
    // member0: 1000 * 1 / 1000 = 1
    // member1: 2000 * 1 / 1000 = 2
    // total: 3

    // distribute
    suite.distribute_funds(executor, None, 300).unwrap();

    // withdraw
    suite
        .withdraw_funds(&members[0], members[0].as_str(), None)
        .unwrap();
    suite
        .withdraw_funds(&members[1], members[1].as_str(), None)
        .unwrap();

    assert_eq!(
        suite.query_balance_vesting_contract(&members[0]).unwrap(),
        90 + 100,
        "member0 should have received 300 * 1 / 3 = 100"
    );
    assert_eq!(
        suite.query_balance_vesting_contract(&members[1]).unwrap(),
        360 + 200,
        "member1 should have received 300 * 2 / 3 = 200"
    );
}
