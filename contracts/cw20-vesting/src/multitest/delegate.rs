use cosmwasm_std::Addr;

use super::suite::SuiteBuilder;

use crate::error::ContractError;
use wynd_utils::Curve;

const START: u64 = 1571797419;
const END: u64 = START + 10_000;

mod staking_address {
    use super::*;

    #[test]
    fn staking_address_not_set() {
        let user = "user";
        let mut suite = SuiteBuilder::new().build();

        let err = suite.delegate(user, 75_000u128).unwrap_err();
        assert_eq!(
            ContractError::StakingAddressNotSet {},
            err.downcast().unwrap()
        );
    }

    #[test]
    fn minter_address_not_set() {
        let user = "user";
        let mut suite = SuiteBuilder::new().build();

        let err = suite
            .update_staking_address(user, "random_address")
            .unwrap_err();
        assert_eq!(
            ContractError::MinterAddressNotSet {},
            err.downcast().unwrap()
        );
    }

    #[test]
    fn minter_is_not_an_admin() {
        let user = "user";
        let mut suite = SuiteBuilder::new().with_minter("random_user", None).build();

        let err = suite
            .update_staking_address(user, "random_address")
            .unwrap_err();
        assert_eq!(
            ContractError::UnauthorizedUpdateStakingAddress {},
            err.downcast().unwrap()
        );
    }

    #[test]
    fn update_works() {
        let mut suite = SuiteBuilder::new().with_minter("admin", None).build();

        assert_eq!(suite.query_staking_address().unwrap(), None);

        suite
            .update_staking_address("admin", "random_address")
            .unwrap();
        assert_eq!(
            suite.query_staking_address().unwrap(),
            Some(Addr::unchecked("random_address"))
        );
    }

    #[test]
    fn update_twice_is_not_allowed() {
        let mut suite = SuiteBuilder::new().with_minter("admin", None).build();

        // first correct update
        suite
            .update_staking_address("admin", "random_address")
            .unwrap();

        // second unallowed update
        let err = suite
            .update_staking_address("admin", "random_addressx2")
            .unwrap_err();
        assert_eq!(
            ContractError::StakingAddressAlreadyUpdated {},
            err.downcast().unwrap()
        );
    }
}

mod delegates {
    use super::*;

    #[test]
    fn invalid_zero_amount() {
        let mut suite = SuiteBuilder::new().build();

        let err = suite.delegate("user", 0u128).unwrap_err();
        assert_eq!(ContractError::InvalidZeroAmount {}, err.downcast().unwrap());
    }

    #[test]
    fn simple_working_scenario() {
        let user = "user";
        let mut suite = SuiteBuilder::new()
            .with_initial_balances(vec![(user, 100_000, None)])
            .with_minter("admin", None)
            .build();

        assert_eq!(suite.query_balance(user).unwrap(), 100_000u128);
        assert_eq!(suite.query_delegated(user).unwrap(), 0u128);
        assert_eq!(suite.query_staking_contract().unwrap(), 0u128);

        // update staking address to staking contract
        let staking_contract = suite.staking_contract();
        suite
            .update_staking_address("admin", &staking_contract)
            .unwrap();

        suite.delegate(user, 75_000u128).unwrap();
        assert_eq!(suite.query_balance(user).unwrap(), 25_000u128);
        assert_eq!(suite.query_delegated(user).unwrap(), 75_000u128);
        assert_eq!(suite.query_staking_contract().unwrap(), 75_000u128);
    }

    #[test]
    fn delegating_vested_tokens_works() {
        let user = "user";
        let mut suite = SuiteBuilder::new()
            .with_initial_balances(vec![(
                user,
                100_000,
                // All initial tokens are locked
                Curve::saturating_linear((START, 100_000), (END, 0)),
            )])
            .with_minter("admin", None)
            .build();

        assert_eq!(suite.query_balance(user).unwrap(), 100_000u128);
        assert_eq!(suite.query_vested(user).unwrap(), 100_000u128);

        let staking_contract = suite.staking_contract();
        suite
            .update_staking_address("admin", &staking_contract)
            .unwrap();

        // delegating all vested tokens is possible
        suite.delegate(user, 100_000u128).unwrap();
        assert_eq!(suite.query_balance(user).unwrap(), 0u128);
        assert_eq!(suite.query_vested(user).unwrap(), 100_000u128);
        assert_eq!(suite.query_delegated(user).unwrap(), 100_000u128);

        let err = suite.delegate(user, 1u128).unwrap_err();
        assert_eq!(ContractError::NotEnoughToDelegate, err.downcast().unwrap());
    }

    #[test]
    fn delegating_liquid_and_vested() {
        let user = "user";
        let mut suite = SuiteBuilder::new()
            .with_initial_balances(vec![(
                user,
                100_000,
                // 50% initial tokens are locked
                Curve::saturating_linear((START, 50_000), (END, 0)),
            )])
            .with_minter("admin", None)
            .build();

        assert_eq!(suite.query_balance(user).unwrap(), 100_000u128);
        assert_eq!(suite.query_vested(user).unwrap(), 50_000u128);
        assert_eq!(suite.query_delegated(user).unwrap(), 0u128);

        let staking_contract = suite.staking_contract();
        suite
            .update_staking_address("admin", &staking_contract)
            .unwrap();

        suite.delegate(user, 20_000u128).unwrap();
        assert_eq!(suite.query_balance(user).unwrap(), 80_000u128);
        assert_eq!(suite.query_vested(user).unwrap(), 50_000u128);
        assert_eq!(suite.query_delegated(user).unwrap(), 20_000u128);

        // delegate second time
        suite.delegate(user, 30_000u128).unwrap();
        assert_eq!(suite.query_balance(user).unwrap(), 50_000u128);
        assert_eq!(suite.query_vested(user).unwrap(), 50_000u128);
        assert_eq!(suite.query_delegated(user).unwrap(), 50_000u128);

        // delegate third time
        suite.delegate(user, 50_000u128).unwrap();
        assert_eq!(suite.query_balance(user).unwrap(), 0u128);
        assert_eq!(suite.query_vested(user).unwrap(), 50_000u128);
        assert_eq!(suite.query_delegated(user).unwrap(), 100_000u128);

        let err = suite.delegate(user, 1u128).unwrap_err();
        assert_eq!(
            ContractError::NotEnoughToDelegate {},
            err.downcast().unwrap()
        );
    }
}

mod undelegates {
    use super::*;

    #[test]
    fn invalid_zero_amount() {
        let mut suite = SuiteBuilder::new().build();

        let err = suite.undelegate("user", "user", 0u128).unwrap_err();
        assert_eq!(ContractError::InvalidZeroAmount {}, err.downcast().unwrap());
    }

    #[test]
    fn staking_address_not_set() {
        let mut suite = SuiteBuilder::new().build();

        let err = suite.undelegate("user", "user", 1u128).unwrap_err();
        assert_eq!(
            ContractError::StakingAddressNotSet {},
            err.downcast().unwrap()
        );
    }

    #[test]
    fn unauthorized_undelegate() {
        let user = "user";
        let mut suite = SuiteBuilder::new()
            .with_initial_balances(vec![(user, 100_000, None)])
            .with_minter("admin", None)
            .build();

        let staking_contract = suite.staking_contract();
        suite
            .update_staking_address("admin", &staking_contract)
            .unwrap();

        let err = suite
            .undelegate("random user", user, 35_000u128)
            .unwrap_err();
        assert_eq!(
            ContractError::UnauthorizedUndelegate {},
            err.downcast().unwrap()
        );
    }

    #[test]
    fn no_tokens_delegated() {
        let user = "user";
        let mut suite = SuiteBuilder::new()
            .with_initial_balances(vec![(user, 100_000, None)])
            .with_minter("admin", None)
            .build();

        let staking_contract = suite.staking_contract();
        suite
            .update_staking_address("admin", &staking_contract)
            .unwrap();

        let err = suite
            .undelegate(&staking_contract, user, 35_000u128)
            .unwrap_err();
        assert_eq!(ContractError::NoTokensDelegated {}, err.downcast().unwrap());
    }

    #[test]
    fn simple_working_scenario() {
        let user = "user";
        let mut suite = SuiteBuilder::new()
            .with_initial_balances(vec![(user, 100_000, None)])
            .with_minter("admin", None)
            .build();

        let staking_contract = suite.staking_contract();
        suite
            .update_staking_address("admin", &staking_contract)
            .unwrap();

        suite.delegate(user, 75_000u128).unwrap();
        assert_eq!(suite.query_balance(user).unwrap(), 25_000u128);
        assert_eq!(suite.query_delegated(user).unwrap(), 75_000u128);
        assert_eq!(suite.query_balance(&staking_contract).unwrap(), 75_000u128);

        suite
            .undelegate(&staking_contract, user, 35_000u128)
            .unwrap();
        assert_eq!(suite.query_balance(user).unwrap(), 60_000u128);
        assert_eq!(suite.query_delegated(user).unwrap(), 40_000u128);
        assert_eq!(suite.query_balance(&staking_contract).unwrap(), 40_000u128);
    }
}

#[test]
fn delegate_undelegate_multiple_users() {
    let user1 = "user1";
    let user1_balance = (
        user1,
        100_000,
        // 50% initial tokens are locked
        Curve::saturating_linear((START, 50_000), (END, 0)),
    );
    let user2 = "user2";
    let user2_balance = (
        user2,
        500_000,
        // 20% initial tokens are locked
        Curve::saturating_linear((START, 100_000), (END, 0)),
    );
    let user3 = "user3";
    let user3_balance = (
        user3,
        10_000_000,
        // 70% initial tokens are locked
        Curve::saturating_linear((START, 7_000_000), (END, 0)),
    );
    let mut suite = SuiteBuilder::new()
        .with_initial_balances(vec![user1_balance, user2_balance, user3_balance])
        .with_minter("admin", None)
        .build();

    let staking_contract = suite.staking_contract();
    suite
        .update_staking_address("admin", &staking_contract)
        .unwrap();

    // user1 has 100_000, vested 50_000
    // delegates 25_000, undelegates 10_000, delegates 30_000
    // left balance is 55_000, vested 50_000, delegated 45_000
    //
    // user 2 has 500_000, vested 100_000
    // delegates 450_000, delegates 400_000 with error, undelegates 200_000
    // left balance is 250_000, vested 100_000, delegated 250_000
    //
    // user 3 has 10_000_000, vested 7_000_000
    // delegates 7_000_000, delegates 3_000_000, delegates 2_000_000 with error
    // left balance is 0, vested 7_000_000, delegated 10_000_000

    suite.delegate(user1, 25_000u128).unwrap();
    suite
        .undelegate(&staking_contract, user1, 10_000u128)
        .unwrap();
    suite.delegate(user1, 30_000u128).unwrap();

    suite.delegate(user2, 450_000u128).unwrap();
    suite.delegate(user2, 400_000u128).unwrap_err();
    suite
        .undelegate(&staking_contract, user2, 200_000u128)
        .unwrap();

    suite.delegate(user3, 7_000_000u128).unwrap();
    suite.delegate(user3, 3_000_000u128).unwrap();
    suite.delegate(user3, 2_000_000u128).unwrap_err();

    assert_eq!(suite.query_balance(user1).unwrap(), 55_000u128);
    assert_eq!(suite.query_vested(user1).unwrap(), 50_000u128);
    assert_eq!(suite.query_delegated(user1).unwrap(), 45_000u128);

    assert_eq!(suite.query_balance(user2).unwrap(), 250_000u128);
    assert_eq!(suite.query_vested(user2).unwrap(), 100_000u128);
    assert_eq!(suite.query_delegated(user2).unwrap(), 250_000u128);

    assert_eq!(suite.query_balance(user3).unwrap(), 0u128);
    assert_eq!(suite.query_vested(user3).unwrap(), 7_000_000u128);
    assert_eq!(suite.query_delegated(user3).unwrap(), 10_000_000u128);
}
