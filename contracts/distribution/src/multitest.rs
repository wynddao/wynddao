pub(crate) mod mock_recipient;
mod suite;

use self::suite::SuiteBuilder;

const ADMIN: &str = "admin";
const USER: &str = "user";

#[test]
fn transfer_should_work() {
    // 1 token every 10 seconds
    let mut suite = SuiteBuilder::new(10, 1, ADMIN)
        .with_initial_balances(vec![(ADMIN, 1000)])
        .build();

    // give some money to distribution contract
    suite
        .transfer_token(ADMIN, suite.distribution_contract(), 1000)
        .unwrap();

    // go forward 1 epoch
    suite.fast_forward(1, 10);
    suite.trigger_payout(USER).unwrap();
    assert_eq!(
        1,
        suite
            .query_token_balance(suite.recipient_contract())
            .unwrap(),
        "payout should pay out 1 token"
    );
    // clean up
    suite.burn_token(suite.recipient_contract(), 1).unwrap();

    // go forward 1.5 epochs
    suite.fast_forward(1, 15);
    suite.trigger_payout(ADMIN).unwrap();
    assert_eq!(
        1,
        suite
            .query_token_balance(suite.recipient_contract())
            .unwrap(),
        "payout should only pay out after a full epoch"
    );
    // go forward the other half epoch
    suite.fast_forward(1, 5);
    suite.trigger_payout(USER).unwrap();
    assert_eq!(
        2,
        suite
            .query_token_balance(suite.recipient_contract())
            .unwrap(),
        "payout should only pay out after a full epoch"
    );
    // clean up
    suite.burn_token(suite.recipient_contract(), 2).unwrap();

    // go forward 10 epochs
    suite.fast_forward(10, 100);
    suite.trigger_payout(ADMIN).unwrap();
    assert_eq!(
        10,
        suite
            .query_token_balance(suite.recipient_contract())
            .unwrap(),
        "payout should pay out all elapsed epochs"
    );
    // clean up
    suite.burn_token(suite.recipient_contract(), 10).unwrap();

    // go forward half epoch
    suite.fast_forward(1, 5);
    suite.trigger_payout(USER).unwrap_err();
    assert_eq!(
        0,
        suite
            .query_token_balance(suite.recipient_contract())
            .unwrap()
    );

    // go forward 987, drain distribution contract
    suite.fast_forward(100, 9870);
    suite.trigger_payout(ADMIN).unwrap();
    assert_eq!(
        987,
        suite
            .query_token_balance(suite.recipient_contract())
            .unwrap(),
        "payout should pay out all elapsed epochs"
    );
    // clean up
    suite.burn_token(suite.recipient_contract(), 987).unwrap();

    // distribution contract is drained, so payout should fail
    suite.fast_forward(1, 10);
    suite.trigger_payout(ADMIN).unwrap_err();
}
