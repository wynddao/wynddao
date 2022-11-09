use std::ops::{Div, Mul};

use cosmwasm_std::{Decimal, OverflowError, OverflowOperation, StdError, Uint128};

use crate::error::ContractError;
use crate::msg::{AllStakedResponse, StakedResponse};
use crate::multitest::suite::SEVEN_DAYS;

use super::suite::SuiteBuilder;
use test_case::test_case;

#[test]
fn unbond_overflow() {
    let unbonding_period = 1000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![(unbonding_period, Decimal::one())])
        .build();

    let err = suite.unbond("user", 1u128, unbonding_period).unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::overflow(OverflowError::new(
            OverflowOperation::Sub,
            0,
            1
        ))),
        err.downcast().unwrap()
    );
}

#[test]
fn no_unbonding_period_found() {
    let user1 = "user1";
    let unbonding_period = 1000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        .with_initial_balances(vec![(user1, 100_000, None)])
        .build();

    let err = suite
        .delegate(user1, 12_000u128, unbonding_period + 1)
        .unwrap_err();
    assert_eq!(
        ContractError::NoUnbondingPeriodFound(unbonding_period + 1),
        err.downcast().unwrap()
    );

    suite.delegate(user1, 12_000u128, unbonding_period).unwrap();

    let err = suite
        .unbond(user1, 12_000u128, unbonding_period + 1)
        .unwrap_err();
    assert_eq!(
        ContractError::NoUnbondingPeriodFound(unbonding_period + 1),
        err.downcast().unwrap()
    );

    suite.unbond(user1, 12_000u128, unbonding_period).unwrap();
}

#[test]
fn bond_adds_voting_power() {
    let user1 = "user1";
    let user2 = "user2";
    let user3 = "user3";
    let unbonding_period = 1000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        .with_initial_balances(vec![
            (user1, 100_000, None),
            (user2, 100_000, None),
            (user3, 100_000, None),
        ])
        .build();

    suite.delegate(user1, 12_000u128, unbonding_period).unwrap();
    suite.delegate(user2, 7_500u128, unbonding_period).unwrap();
    suite.delegate(user3, 4_000u128, unbonding_period).unwrap();

    assert_eq!(suite.query_total_staked().unwrap(), 23_500u128);
    let periods = suite.query_staked_periods().unwrap();
    assert_eq!(periods.len(), 1);
    assert_eq!(periods[0].unbonding_period, unbonding_period);
    assert_eq!(periods[0].voting_multiplier, Decimal::one());
    assert_eq!(periods[0].total_staked.u128(), 23_500u128);

    assert_eq!(
        suite.query_staked(user1, unbonding_period).unwrap(),
        12_000u128
    );
    assert_eq!(
        suite.query_staked(user2, unbonding_period).unwrap(),
        7_500u128
    );
    assert_eq!(
        suite.query_staked(user3, unbonding_period).unwrap(),
        4_000u128
    );

    assert_eq!(suite.query_voting_power(user1, None).unwrap(), 12u128);
    assert_eq!(suite.query_voting_power(user2, None).unwrap(), 7u128);
    assert_eq!(suite.query_voting_power(user3, None).unwrap(), 0u128);

    suite.delegate(user2, 7_600u128, unbonding_period).unwrap();
    suite.delegate(user3, 1_200u128, unbonding_period).unwrap();

    assert_eq!(suite.query_total_staked().unwrap(), 32_300u128);

    assert_eq!(suite.query_voting_power(user2, None).unwrap(), 15u128);
    assert_eq!(suite.query_voting_power(user3, None).unwrap(), 5u128);
}

#[test]
fn unbond_updates_voting_power() {
    let user1 = "user1";
    let user2 = "user2";
    let user3 = "user3";
    let unbonding_period = 1000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            // one unbonding_period with power 1.0
            (unbonding_period, Decimal::one()),
        ])
        .with_min_bond(5_000)
        .with_initial_balances(vec![
            (user1, 100_000, None),
            (user2, 100_000, None),
            (user3, 100_000, None),
        ])
        .build();

    suite.delegate(user1, 12_000u128, unbonding_period).unwrap();
    suite.delegate(user2, 7_500u128, unbonding_period).unwrap();
    suite.delegate(user3, 4_000u128, unbonding_period).unwrap();

    assert_eq!(suite.query_total_staked().unwrap(), 23_500u128);

    suite.unbond(user1, 4_500u128, unbonding_period).unwrap();
    suite.unbond(user2, 2_600u128, unbonding_period).unwrap();
    suite.unbond(user3, 1_111u128, unbonding_period).unwrap();

    assert_eq!(suite.query_total_staked().unwrap(), 15289u128);

    assert_eq!(
        suite.query_staked(user1, unbonding_period).unwrap(),
        7_500u128
    );
    assert_eq!(
        suite.query_staked(user2, unbonding_period).unwrap(),
        4_900u128
    );
    assert_eq!(
        suite.query_staked(user3, unbonding_period).unwrap(),
        2_889u128
    );

    assert_eq!(suite.query_voting_power(user1, None).unwrap(), 7u128);
    assert_eq!(suite.query_voting_power(user2, None).unwrap(), 0u128);
    assert_eq!(suite.query_voting_power(user3, None).unwrap(), 0u128);

    suite.delegate(user1, 600u128, unbonding_period).unwrap();
    suite.delegate(user2, 100u128, unbonding_period).unwrap();
    suite.delegate(user3, 2_222u128, unbonding_period).unwrap();

    assert_eq!(
        suite.query_staked(user1, unbonding_period).unwrap(),
        8_100u128
    );
    assert_eq!(
        suite.query_staked(user2, unbonding_period).unwrap(),
        5_000u128
    );
    assert_eq!(
        suite.query_staked(user3, unbonding_period).unwrap(),
        5111u128
    );

    assert_eq!(suite.query_voting_power(user1, None).unwrap(), 8u128);
    assert_eq!(suite.query_voting_power(user2, None).unwrap(), 5u128);
    assert_eq!(suite.query_voting_power(user3, None).unwrap(), 5u128);
}

#[test]
fn one_user_multiple_unbonding_periods() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            (unbonding_period1, Decimal::percent(25)),
            (unbonding_period2, Decimal::percent(50)),
            (unbonding_period3, Decimal::percent(75)),
        ])
        .with_initial_balances(vec![(user, 100_000, None)])
        .build();

    let bonds = vec![20_000u128, 30_000u128, 10_000u128];
    let delegated: u128 = bonds.iter().sum();

    suite.delegate(user, bonds[0], unbonding_period1).unwrap();
    suite.delegate(user, bonds[1], unbonding_period2).unwrap();
    suite.delegate(user, bonds[2], unbonding_period3).unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // unbond on second unbonding period
    suite.unbond(user, 20_000u128, unbonding_period2).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        10_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 17u128); // 0.25 * 20_000 + 0.5 * 10_000 + 0.75 * 10_000

    // top some more on first unbonding period
    suite.delegate(user, 5_000u128, unbonding_period1).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        25_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 18u128); // 0.25 * 25_000 + 0.5 * 10_000 + 0.75 * 10_000
    assert_eq!(suite.query_total_power(None).unwrap(), 18u128);

    assert_eq!(
        suite.query_all_staked(user).unwrap(),
        AllStakedResponse {
            stakes: vec![
                StakedResponse {
                    stake: Uint128::new(25_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 1000,
                    cw20_contract: suite.vesting_contract(),
                },
                StakedResponse {
                    stake: Uint128::new(10_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 4000,
                    cw20_contract: suite.vesting_contract(),
                },
                StakedResponse {
                    stake: Uint128::new(10_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 8000,
                    cw20_contract: suite.vesting_contract(),
                },
            ]
        }
    );

    let periods = suite.query_staked_periods().unwrap();
    assert_eq!(periods.len(), 3);
    assert_eq!(periods[0].unbonding_period, unbonding_period1);
    assert_eq!(periods[0].voting_multiplier, Decimal::percent(25));
    assert_eq!(periods[0].total_staked.u128(), 25_000);
    assert_eq!(periods[1].unbonding_period, unbonding_period2);
    assert_eq!(periods[1].voting_multiplier, Decimal::percent(50));
    assert_eq!(periods[1].total_staked.u128(), 10_000);
    assert_eq!(periods[2].unbonding_period, unbonding_period3);
    assert_eq!(periods[2].voting_multiplier, Decimal::percent(75));
    assert_eq!(periods[2].total_staked.u128(), 10_000);
}

#[test]
fn one_user_multiple_periods_rebond_then_bond() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config(vec![
            (
                unbonding_period1,
                Decimal::percent(25),
                Decimal::percent(25),
            ),
            (
                unbonding_period2,
                Decimal::percent(50),
                Decimal::percent(60),
            ),
            (
                unbonding_period3,
                Decimal::percent(75),
                Decimal::percent(80),
            ),
        ])
        .with_initial_balances(vec![(user, 100_000, None)])
        .build();

    let bonds = vec![20_000u128, 30_000u128, 10_000u128];
    let delegated: u128 = bonds.iter().sum();

    suite.delegate(user, bonds[0], unbonding_period1).unwrap();
    suite.delegate(user, bonds[1], unbonding_period2).unwrap();
    suite.delegate(user, bonds[2], unbonding_period3).unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // rebond all tokens from bonding period 1 to period 2
    suite
        .rebond(user, 20_000u128, unbonding_period1, unbonding_period2)
        .unwrap();
    assert_eq!(suite.query_staked(user, unbonding_period1).unwrap(), 0u128);

    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        50_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 32u128);
    assert_eq!(suite.query_total_power(None).unwrap(), 32u128); // 0.25 * 0 + 0.5 * 50_000 + 0.75 * 10_000

    assert_eq!(suite.query_rewards(user).unwrap(), 38u128);
    assert_eq!(suite.query_total_rewards().unwrap(), 38u128); // 0.25 * 0 + 0.6 * 50_000 + 0.8 * 10_000

    // top some more on first unbonding period but not more than we originally topped up
    suite.delegate(user, 25_000u128, unbonding_period1).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        25_000u128
    );
    assert_eq!(
        suite.query_all_staked(user).unwrap(),
        AllStakedResponse {
            stakes: vec![
                StakedResponse {
                    stake: Uint128::new(25_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 1000,
                    cw20_contract: suite.vesting_contract(),
                },
                StakedResponse {
                    stake: Uint128::new(50_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 4000,
                    cw20_contract: suite.vesting_contract(),
                },
                StakedResponse {
                    stake: Uint128::new(10_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 8000,
                    cw20_contract: suite.vesting_contract(),
                },
            ]
        }
    );
    // NOTE: The power at this point should be 0.25 * 25_000 + 0.5 * 50_000 + 0.75 * 10_000 = 38
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 38u128); // 0.25 * 25_000 + 0.5 * 50_000 + 0.75 * 10_000
    assert_eq!(suite.query_total_power(None).unwrap(), 38u128);

    assert_eq!(suite.query_rewards(user).unwrap(), 44u128);
    assert_eq!(suite.query_total_rewards().unwrap(), 44u128); // 0.25 * 25_000 + 0.6 * 50_000 + 0.8 * 10_000
}

#[test]
fn one_user_multiple_periods_rebond_increase() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            (unbonding_period1, Decimal::percent(25)),
            (unbonding_period2, Decimal::percent(50)),
            (unbonding_period3, Decimal::percent(75)),
        ])
        .with_min_bond(5000)
        .with_initial_balances(vec![(user, 100_000, None)])
        .build();

    let bonds = vec![20_000u128, 30_000u128, 10_000u128];
    let delegated: u128 = bonds.iter().sum();

    suite.delegate(user, bonds[0], unbonding_period1).unwrap();
    suite.delegate(user, bonds[1], unbonding_period2).unwrap();
    suite.delegate(user, bonds[2], unbonding_period3).unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // rebond all tokens from bonding period 1 to period 2
    suite
        .rebond(user, 20_000u128, unbonding_period1, unbonding_period2)
        .unwrap();
    assert_eq!(suite.query_staked(user, unbonding_period1).unwrap(), 0u128);
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        50_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 32u128); // 0.25 * 0 + 0.5 * 50_000 + 0.75 * 10_000

    // top some more on first unbonding period, more than the original deposit
    suite.delegate(user, 25_000u128, unbonding_period1).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        25_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 38u128); // 0.25 * 25_000 + 0.5 * 50_000 + 0.75 * 10_000

    suite
        .rebond(user, 20_000u128, unbonding_period1, unbonding_period3)
        .unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        5_000u128
    );
    assert_eq!(
        suite.query_staked(user, unbonding_period3).unwrap(),
        30_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 48u128); // 0.25 * 5_000 + 0.5 * 50_000 + 0.75 * 30_000

    suite
        .rebond(user, 10_000u128, unbonding_period2, unbonding_period3)
        .unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        5_000u128
    );
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        40_000u128
    );
    assert_eq!(
        suite.query_staked(user, unbonding_period3).unwrap(),
        40_000u128
    );

    // NOTE: The power at this point should be 0.25 * 5_000 + 0.5 * 40_000 + 0.75 * 40_000 = 51
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 51u128);
    assert_eq!(suite.query_total_power(None).unwrap(), 51u128);
}

#[test]
fn rebond_then_rebond_again() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            (unbonding_period1, Decimal::percent(25)),
            (unbonding_period2, Decimal::percent(50)),
            (unbonding_period3, Decimal::percent(75)),
        ])
        .with_initial_balances(vec![(user, 100_000, None)])
        .build();

    // delegate on first unbonding period
    suite
        .delegate(user, 100_000u128, unbonding_period1)
        .unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        100_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 25u128); // 0.25 * 20_000

    // rebond 40% of tokens to bucket 2
    suite
        .rebond(user, 40_000u128, unbonding_period1, unbonding_period2)
        .unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        60_000u128
    );

    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        40_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 35u128); // 0.25 * 60_000 + 0.5 * 40_000

    // rebond half of bucket 2 tokens to bucket 3
    suite
        .rebond(user, 20_000u128, unbonding_period2, unbonding_period3)
        .unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        20_000u128
    );

    assert_eq!(
        suite.query_staked(user, unbonding_period3).unwrap(),
        20_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 40u128); // 0.25 * 60_000 + 0.5 * 20_000 + 0.75 * 20_000

    assert_eq!(
        suite.query_all_staked(user).unwrap(),
        AllStakedResponse {
            stakes: vec![
                StakedResponse {
                    stake: Uint128::new(60_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 1000,
                    cw20_contract: suite.vesting_contract(),
                },
                StakedResponse {
                    stake: Uint128::new(20_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 4000,
                    cw20_contract: suite.vesting_contract(),
                },
                StakedResponse {
                    stake: Uint128::new(20_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 8000,
                    cw20_contract: suite.vesting_contract(),
                },
            ]
        }
    );
    // NOTE: The power at this point should be 0.25 * 60_000 + 0.5 * 20_000 + 0.75 * 20_000 = 40
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 40u128); // 0.25 * 60_000 + 0.5 * 20_000 + 0.75 * 20_000
    assert_eq!(suite.query_total_power(None).unwrap(), 40u128);
}

#[test]
fn one_user_multiple_periods_rebond_fail() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            (unbonding_period1, Decimal::percent(25)),
            (unbonding_period2, Decimal::percent(50)),
            (unbonding_period3, Decimal::percent(75)),
        ])
        .with_initial_balances(vec![(user, 100_000, None)])
        .build();

    let bonds = vec![20_000u128, 30_000u128, 10_000u128];
    let delegated: u128 = bonds.iter().sum();

    suite.delegate(user, bonds[0], unbonding_period1).unwrap();
    suite.delegate(user, bonds[1], unbonding_period2).unwrap();
    suite.delegate(user, bonds[2], unbonding_period3).unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // Fail case, rebonding 50_000 from a bucket with 20_000
    let err = suite
        .rebond(user, 50_000u128, unbonding_period1, unbonding_period2)
        .unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::overflow(OverflowError::new(
            OverflowOperation::Sub,
            20000u128,
            50000u128
        ))),
        err.downcast().unwrap()
    );

    // Fail case, rebonding to a non-existent bucket
    let err = suite
        .rebond(user, 10_000u128, unbonding_period1, 12000)
        .unwrap_err();
    assert_eq!(
        ContractError::NoUnbondingPeriodFound(12000),
        err.downcast().unwrap()
    );

    // Fail case, rebonding from a non-existent bucket
    let err = suite
        .rebond(user, 50_000u128, 2000, unbonding_period2)
        .unwrap_err();
    assert_eq!(
        ContractError::NoUnbondingPeriodFound(2000),
        err.downcast().unwrap()
    );
}

#[test]
fn multiple_users_multiple_unbonding_periods() {
    let user1 = "user1";
    let user2 = "user2";
    let user3 = "user3";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;

    let bonds = vec![20_000u128, 30_000u128, 10_000u128, 16_000u128, 6_000u128];
    let delegated: u128 = bonds.iter().sum();
    let members = ["user1", "user2", "user3"];

    let mut suite = SuiteBuilder::new()
        .with_stake_config(vec![
            (unbonding_period1, Decimal::percent(30), Decimal::percent(1)),
            (
                unbonding_period2,
                Decimal::percent(60),
                Decimal::percent(40),
            ),
            (
                unbonding_period3,
                Decimal::percent(80),
                Decimal::percent(60),
            ),
        ])
        .with_min_bond(4_500)
        .with_initial_balances(vec![
            (user1, 100_000, None),
            (user2, 100_000, None),
            (user3, 100_000, None),
        ])
        .build();

    suite
        .delegate(members[0], bonds[0], unbonding_period1)
        .unwrap();
    suite
        .delegate(members[1], bonds[1], unbonding_period2)
        .unwrap();
    suite
        .delegate(members[0], bonds[2], unbonding_period3)
        .unwrap();
    suite
        .delegate(members[2], bonds[3], unbonding_period2)
        .unwrap();
    suite
        .delegate(members[2], bonds[4], unbonding_period3)
        .unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // first user unbonds on second unbonding period
    suite.unbond(user1, 20_000u128, unbonding_period1).unwrap();
    assert_eq!(suite.query_staked(user1, unbonding_period1).unwrap(), 0u128);
    assert_eq!(
        suite.query_staked(user1, unbonding_period3).unwrap(),
        10_000u128
    );
    assert_eq!(suite.query_voting_power(user1, None).unwrap(), 8u128); // 0.8 * 10_000

    assert_eq!(suite.query_total_power(None).unwrap(), 39u128); // 8 + 18 + 13

    assert_eq!(suite.query_rewards(user1).unwrap(), 6u128); // same as before

    assert_eq!(suite.query_total_rewards().unwrap(), 27u128); // same as before
}

#[test]
fn one_user_rebond_decrease() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;

    let mut suite = SuiteBuilder::new()
        .with_stake_config(vec![
            (unbonding_period1, Decimal::percent(25), Decimal::percent(1)),
            (
                unbonding_period2,
                Decimal::percent(50),
                Decimal::percent(40),
            ),
            (
                unbonding_period3,
                Decimal::percent(75),
                Decimal::percent(60),
            ),
        ])
        .with_initial_balances(vec![(user, 100_000, None)])
        .build();

    let bonds = vec![20_000u128, 30_000u128, 10_000u128];
    let delegated: u128 = bonds.iter().sum();

    suite.delegate(user, bonds[0], unbonding_period1).unwrap();
    suite.delegate(user, bonds[1], unbonding_period2).unwrap();
    suite.delegate(user, bonds[2], unbonding_period3).unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // Rebond downwards from period 3 to 1 introducing a lockup for those tokens
    suite
        .rebond(user, 10_000u128, unbonding_period3, unbonding_period1)
        .unwrap();

    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        30_000u128
    );

    assert_eq!(suite.query_staked(user, unbonding_period3).unwrap(), 0u128);

    assert_eq!(suite.query_voting_power(user, None).unwrap(), 22u128); // 0.25 * 30_000 + 0.5 * 30_000 + 0.75 * 0

    // Unbond 20k which is not locked. Only 10k are locked from the downwards rebond
    suite.unbond(user, 20_000u128, unbonding_period1).unwrap();

    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        10_000u128
    );

    // Unbond is unsuccessful as the final 10k tokens are 'locked'
    let err = suite
        .unbond(user, 10_000u128, unbonding_period1)
        .unwrap_err();

    assert_eq!(
        ContractError::Std(StdError::overflow(OverflowError::new(
            OverflowOperation::Sub,
            0u128,
            10000u128
        ))),
        err.downcast().unwrap()
    );

    // Rebond is also unsuccessful as the final 10k tokens are 'locked'
    let err = suite
        .rebond(user, 10_000u128, unbonding_period1, unbonding_period3)
        .unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::overflow(OverflowError::new(
            OverflowOperation::Sub,
            0u128,
            10000u128
        ))),
        err.downcast().unwrap()
    );

    // Before we advance time, ensure the locked_tokens are accounted as such in the query
    // Verify the locked and unlocked stakes via the query
    assert_eq!(
        suite.query_all_staked(user).unwrap(),
        AllStakedResponse {
            stakes: vec![
                StakedResponse {
                    stake: Uint128::new(10_000),
                    total_locked: Uint128::new(10_000),
                    unbonding_period: 1000,
                    cw20_contract: suite.vesting_contract(),
                },
                StakedResponse {
                    stake: Uint128::new(30_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 4000,
                    cw20_contract: suite.vesting_contract(),
                },
                StakedResponse {
                    stake: Uint128::zero(),
                    total_locked: Uint128::zero(),
                    unbonding_period: 8000,
                    cw20_contract: suite.vesting_contract(),
                },
            ]
        }
    );

    // Advance time such that we can use those 10k again
    suite.update_time(unbonding_period3 - unbonding_period1 + 1);

    // Unbond is successful, the 'locked' tokens are unbonded
    suite.unbond(user, 5_000u128, unbonding_period1).unwrap();

    // Rebond is also successful
    suite
        .rebond(user, 5_000u128, unbonding_period1, unbonding_period3)
        .unwrap();

    // Verify again the locked and unlocked stakes via the query
    assert_eq!(
        suite.query_all_staked(user).unwrap(),
        AllStakedResponse {
            stakes: vec![
                StakedResponse {
                    stake: Uint128::zero(),
                    total_locked: Uint128::zero(),
                    unbonding_period: 1000,
                    cw20_contract: suite.vesting_contract(),
                },
                StakedResponse {
                    stake: Uint128::new(30_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 4000,
                    cw20_contract: suite.vesting_contract(),
                },
                StakedResponse {
                    stake: Uint128::new(5_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 8000,
                    cw20_contract: suite.vesting_contract(),
                },
            ]
        }
    );

    let periods = suite.query_staked_periods().unwrap();
    assert_eq!(periods.len(), 3);
    assert_eq!(periods[0].unbonding_period, unbonding_period1);
    assert_eq!(periods[0].total_staked.u128(), 0);
    assert_eq!(periods[1].unbonding_period, unbonding_period2);
    assert_eq!(periods[1].total_staked.u128(), 30_000);
    assert_eq!(periods[2].unbonding_period, unbonding_period3);
    assert_eq!(periods[2].total_staked.u128(), 5_000);
}

#[test]
fn one_user_rebond_decrease_then_rebond_again() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config(vec![
            (
                unbonding_period1,
                Decimal::percent(25),
                Decimal::percent(25),
            ),
            (
                unbonding_period2,
                Decimal::percent(50),
                Decimal::percent(50),
            ),
            (
                unbonding_period3,
                Decimal::percent(75),
                Decimal::percent(75),
            ),
        ])
        .with_initial_balances(vec![(user, 100_000, None)])
        .build();

    let bonds = vec![20_000u128, 30_000u128, 10_000u128];
    let delegated: u128 = bonds.iter().sum();

    suite.delegate(user, bonds[0], unbonding_period1).unwrap();
    suite.delegate(user, bonds[1], unbonding_period2).unwrap();
    suite.delegate(user, bonds[2], unbonding_period3).unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // Rebond downwards from period 3 to 1 introducing a lockup for those tokens
    suite
        .rebond(user, 10_000u128, unbonding_period3, unbonding_period1)
        .unwrap();

    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        30_000u128
    );

    assert_eq!(suite.query_staked(user, unbonding_period3).unwrap(), 0u128);

    assert_eq!(suite.query_voting_power(user, None).unwrap(), 22u128); // 0.25 * 30_000 + 0.5 * 30_000 + 0.75 * 0

    // Unbond 20k which is not locked. Only 10k are locked from the downwards rebond
    suite.unbond(user, 20_000u128, unbonding_period1).unwrap();

    // Unbond is unsuccessful as the final 10k tokens are 'locked'
    let err = suite
        .unbond(user, 10_000u128, unbonding_period1)
        .unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::overflow(OverflowError::new(
            OverflowOperation::Sub,
            0u128,
            10000u128
        ))),
        err.downcast().unwrap()
    );

    // Rebond is also unsuccessful as the final 20k tokens are 'locked'
    let err = suite
        .rebond(user, 10_000u128, unbonding_period1, unbonding_period3)
        .unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::overflow(OverflowError::new(
            OverflowOperation::Sub,
            0u128,
            10000u128
        ))),
        err.downcast().unwrap()
    );

    // Advance time such that we can use those 10k again
    suite.update_time(SEVEN_DAYS * 2);

    // Rebond is also successful as the final 10k tokens are no longer 'locked'
    suite
        .rebond(user, 10_000u128, unbonding_period1, unbonding_period3)
        .unwrap();

    // Try another rebond for a smaller amount
    suite
        .rebond(user, 5_000u128, unbonding_period3, unbonding_period1)
        .unwrap();

    // Advance time such that we can use those 5k again
    suite.update_time(SEVEN_DAYS * 2);

    // Unbond is successful, the 'locked' tokens are unbonded
    suite.unbond(user, 5_000u128, unbonding_period1).unwrap();

    // Add more to bonding period 2
    suite.delegate(user, 20_000u128, unbonding_period2).unwrap();

    // Try another rebond for a large amount from another period
    suite
        .rebond(user, 20_000u128, unbonding_period2, unbonding_period1)
        .unwrap();

    // Advance time such that we can use those 5k again
    suite.update_time(SEVEN_DAYS * 2);

    // Unbond is successful, the 'locked' tokens are unbonded
    suite.unbond(user, 10_000u128, unbonding_period1).unwrap();

    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        10_000u128
    );

    // Try another rebond for a large amount for a third time
    suite
        .rebond(user, 20_000u128, unbonding_period2, unbonding_period1)
        .unwrap();

    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        30_000u128
    );

    // delegate on first unbonding period
    suite.delegate(user, 20_000u128, unbonding_period1).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        50_000u128
    );

    // Advance time such that we can use those 5k again
    suite.update_time(SEVEN_DAYS * 2);

    // Unbond is successful, the 'locked' tokens are unbonded
    suite.unbond(user, 20_000u128, unbonding_period1).unwrap();
}

#[test_case(vec![(1000,25),(4000,50), (8000,75)],vec![20000,30000,20000] => Some(38); "should success")]
fn query_all_staked(stake_config: Vec<(u64, u64)>, amount: Vec<u128>) -> Option<u64> {
    let user = "user";

    let sc: Vec<(u64, Decimal)> = stake_config
        .iter()
        .map(|item| (item.0, Decimal::percent(item.1)))
        .collect();

    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(sc.clone())
        .with_initial_balances(vec![(user, 100_000, None)])
        .build();

    let mut previous_stake_power: u128 = 0u128;

    for i in 0..=(sc.len() - 1) {
        // delegate unbonding period
        suite.delegate(user, amount[i], stake_config[i].0).unwrap();
        // This works
        suite.query_staked(user, stake_config[i].0).unwrap();
        // This works
        suite.query_all_staked(user).unwrap();

        assert_eq!(
            suite.query_staked(user, stake_config[i].0).unwrap(),
            amount[i]
        );

        let actual: u128 = suite.query_voting_power(user, None).unwrap();
        previous_stake_power += sc[i]
            .1 // 0.25
            .mul(Decimal::raw(amount[i])) // 20_000
            .div(Uint128::new(1000)) // 1000 why this?
            .atomics()
            .u128();
        assert_eq!(actual, previous_stake_power); // 0.25 * 20_000 + 0.5 * 30_000 + 0.75 * 10_000
    }

    // This works
    suite.query_all_staked(user).unwrap();

    Some(38u64)
}

#[test]
fn delegate_unbond_under_min_bond() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let mut suite = SuiteBuilder::new()
        .with_stake_config_voting(vec![
            (unbonding_period1, Decimal::percent(40)),
            (unbonding_period2, Decimal::percent(80)),
        ])
        .with_min_bond(2_000)
        .with_initial_balances(vec![(user, 100_000, None)])
        .build();

    // delegating first amount works (5_000 * 0.4 = 2_000)
    suite.delegate(user, 5_000u128, unbonding_period1).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        5_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 2u128);

    // delegating another amount under min bond doesn't increase voting power
    // 1_800 < 2_000
    suite.delegate(user, 1_800u128, unbonding_period2).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        1_800u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 2u128);

    // once the stake hits min_bond (2_000), count it, even if voting power (2_000 * 0.8 = 1_600) is still under min_bond
    suite.delegate(user, 200u128, unbonding_period2).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        2_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 3u128);

    suite.delegate(user, 5_000u128, unbonding_period2).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        7_000u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 7u128); // 2 + 5

    // undelegate tokens from first pool so that delegation goes under min_bond
    suite.unbond(user, 3_500u128, unbonding_period1).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        1_500u128
    );
    assert_eq!(suite.query_voting_power(user, None).unwrap(), 5u128); // only points from second unbonding_period counts now
}
