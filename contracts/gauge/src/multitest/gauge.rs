use cosmwasm_std::{Decimal, Uint128};
use voting::Vote;

use super::suite::SuiteBuilder;

use crate::error::ContractError;
use crate::msg::GaugeResponse;

const EPOCH: u64 = 7 * 86_400;

#[test]
fn create_gauge() {
    let voter1 = "voter1";
    let voter2 = "voter2";
    let mut suite = SuiteBuilder::new()
        .with_voting_members(&[(voter1, 100), (voter2, 100)])
        .build();

    suite.next_block();
    suite
        .propose_update_proposal_module(voter1.to_string(), None)
        .unwrap();

    suite.next_block();
    let proposal = suite.list_proposals().unwrap()[0];
    suite
        .place_vote_single(voter1, proposal, Vote::Yes)
        .unwrap();
    suite
        .place_vote_single(voter2, proposal, Vote::Yes)
        .unwrap();

    suite.next_block();
    suite
        .execute_single_proposal(voter1.to_string(), proposal)
        .unwrap();
    let proposal_modules = suite.query_proposal_modules().unwrap();

    // Second proposal module is cw proposal single, first one is newly added gauge
    assert_eq!(proposal_modules.len(), 2);
    let gauge_contract = proposal_modules[0].clone();

    let gauge_adapter = suite
        .instantiate_adapter_and_create_gauge(
            gauge_contract.clone(),
            &[voter1, voter2],
            (1000, "ujuno"),
        )
        .unwrap();

    let response = suite.query_gauge(gauge_contract, 0).unwrap();
    assert_eq!(
        response,
        GaugeResponse {
            id: 0,
            title: "gauge".to_owned(),
            adapter: gauge_adapter.to_string(),
            epoch_size: EPOCH,
            min_percent_selected: Some(Decimal::percent(5)),
            max_options_selected: 10,
            is_stopped: false,
            next_epoch: suite.current_time() + 7 * 86400,
        }
    );
}

#[test]
fn gauge_can_upgrade_from_self() {
    let voter1 = "voter1";
    let mut suite = SuiteBuilder::new()
        .with_voting_members(&[(voter1, 100)])
        .build();

    suite.next_block();
    suite
        .propose_update_proposal_module(voter1.to_string(), None)
        .unwrap();
    suite.next_block();
    let proposal = suite.list_proposals().unwrap()[0];
    suite
        .place_vote_single(voter1, proposal, Vote::Yes)
        .unwrap();
    suite.next_block();
    suite
        .execute_single_proposal(voter1.to_string(), proposal)
        .unwrap();
    let proposal_modules = suite.query_proposal_modules().unwrap();

    // Second proposal module is cw proposal single, first one is newly added gauge
    assert_eq!(proposal_modules.len(), 2);
    let gauge_contract = proposal_modules[0].clone();

    let gauge_adapter = suite
        .instantiate_adapter_and_create_gauge(
            gauge_contract.clone(),
            &["option1", "option2"],
            (1000, "ujuno"),
        )
        .unwrap();

    // now let's migrate the gauge and make sure nothing breaks
    suite.auto_migrate_gauge(&gauge_contract, None).unwrap();

    let response = suite.query_gauge(gauge_contract, 0).unwrap();
    assert_eq!(
        response,
        GaugeResponse {
            id: 0,
            title: "gauge".to_owned(),
            adapter: gauge_adapter.to_string(),
            epoch_size: EPOCH,
            min_percent_selected: Some(Decimal::percent(5)),
            max_options_selected: 10,
            is_stopped: false,
            next_epoch: suite.current_time() + 7 * 86400,
        }
    );
}

#[test]
fn gauge_migrate_with_next_epochs() {
    let voter1 = "voter1";
    let mut suite = SuiteBuilder::new()
        .with_voting_members(&[(voter1, 100)])
        .build();

    suite.next_block();
    suite
        .propose_update_proposal_module(voter1.to_string(), None)
        .unwrap();
    suite.next_block();
    let proposal = suite.list_proposals().unwrap()[0];
    suite
        .place_vote_single(voter1, proposal, Vote::Yes)
        .unwrap();
    suite.next_block();
    suite
        .execute_single_proposal(voter1.to_string(), proposal)
        .unwrap();
    let proposal_modules = suite.query_proposal_modules().unwrap();

    // Second proposal module is cw proposal single, first one is newly added gauge
    assert_eq!(proposal_modules.len(), 2);
    let gauge_contract = proposal_modules[0].clone();

    let gauge_adapter = suite
        .instantiate_adapter_and_create_gauge(
            gauge_contract.clone(),
            &["option1", "option2"],
            (1000, "ujuno"),
        )
        .unwrap();

    // previous settings
    let response = suite.query_gauge(gauge_contract.clone(), 0).unwrap();
    assert_eq!(
        response,
        GaugeResponse {
            id: 0,
            title: "gauge".to_owned(),
            adapter: gauge_adapter.to_string(),
            epoch_size: EPOCH,
            min_percent_selected: Some(Decimal::percent(5)),
            max_options_selected: 10,
            is_stopped: false,
            next_epoch: suite.current_time() + 7 * 86400,
        }
    );

    // now let's migrate the gauge and make sure nothing breaks
    let gauge_id = 0;
    // change next epoch from 7 to 14 days
    suite
        .auto_migrate_gauge(
            &gauge_contract,
            vec![(gauge_id, suite.current_time() + 14 * 86400)],
        )
        .unwrap();

    let response = suite.query_gauge(gauge_contract.clone(), 0).unwrap();
    assert_eq!(
        response,
        GaugeResponse {
            id: 0,
            title: "gauge".to_owned(),
            adapter: gauge_adapter.to_string(),
            epoch_size: EPOCH,
            min_percent_selected: Some(Decimal::percent(5)),
            max_options_selected: 10,
            is_stopped: false,
            next_epoch: suite.current_time() + 14 * 86400,
        }
    );

    // try to migrate updating next epoch on nonexisting gauge_id
    // actually generic error makes it more difficult to debug in presentable form, I think this is
    // enough
    let _err = suite
        .auto_migrate_gauge(
            &gauge_contract,
            vec![(420, suite.current_time() + 14 * 86400)],
        )
        .unwrap_err();
}

/// attach adaptor in instantiate
#[test]
fn execute_gauge() {
    let voter1 = "voter1";
    let voter2 = "voter2";
    let reward_to_distribute = (1000, "ujuno");
    let mut suite = SuiteBuilder::new()
        .with_voting_members(&[(voter1, 100), (voter2, 100)])
        .with_core_balance(reward_to_distribute)
        .build();

    suite.next_block();
    let gauge_config = suite
        .instantiate_adapter_and_return_config(&[voter1, voter2], reward_to_distribute)
        .unwrap();
    suite
        .propose_update_proposal_module(voter1.to_string(), vec![gauge_config])
        .unwrap();

    suite.next_block();
    let proposal = suite.list_proposals().unwrap()[0];
    suite
        .place_vote_single(voter1, proposal, Vote::Yes)
        .unwrap();
    suite
        .place_vote_single(voter2, proposal, Vote::Yes)
        .unwrap();

    suite.next_block();
    suite
        .execute_single_proposal(voter1.to_string(), proposal)
        .unwrap();
    let proposal_modules = suite.query_proposal_modules().unwrap();
    let gauge_contract = proposal_modules[0].clone();

    let gauge_id = 0;

    // vote for one of the options in gauge
    suite
        .place_vote(
            &gauge_contract,
            voter1.to_owned(),
            gauge_id,
            Some(voter1.to_owned()), // option to vote for
        )
        .unwrap();
    suite
        .place_vote(
            &gauge_contract,
            voter2.to_owned(),
            gauge_id,
            Some(voter1.to_owned()),
        )
        .unwrap();

    let selected_set = suite.query_selected_set(&gauge_contract, gauge_id).unwrap();
    // voter1 was option voted for with two 100 voting powers combined
    assert_eq!(selected_set, vec![("voter1".to_owned(), Uint128::new(200))]);

    // before advancing specified epoch tally won't get sampled
    suite.advance_time(EPOCH);

    suite
        .execute_options(&gauge_contract, voter1, gauge_id)
        .unwrap();

    assert_eq!(
        suite.query_balance(voter1, reward_to_distribute.1).unwrap(),
        1000u128
    );
}

#[test]
fn execute_gauge_twice_same_epoch() {
    let voter1 = "voter1";
    let voter2 = "voter2";
    let reward_to_distribute = (2000, "ujuno");
    let mut suite = SuiteBuilder::new()
        .with_voting_members(&[(voter1, 100), (voter2, 100)])
        .with_core_balance(reward_to_distribute)
        .build();

    suite.next_block();
    let gauge_config = suite
        .instantiate_adapter_and_return_config(&[voter1, voter2], (1000, "ujuno")) // reward per
        // epoch
        .unwrap();
    suite
        .propose_update_proposal_module(voter1.to_string(), vec![gauge_config])
        .unwrap();

    suite.next_block();
    let proposal = suite.list_proposals().unwrap()[0];
    suite
        .place_vote_single(voter1, proposal, Vote::Yes)
        .unwrap();
    suite
        .place_vote_single(voter2, proposal, Vote::Yes)
        .unwrap();

    suite.next_block();
    suite
        .execute_single_proposal(voter1.to_string(), proposal)
        .unwrap();
    let proposal_modules = suite.query_proposal_modules().unwrap();
    let gauge_contract = proposal_modules[0].clone();

    let gauge_id = 0;

    // vote for one of the options in gauge
    suite
        .place_vote(
            &gauge_contract,
            voter1.to_owned(),
            gauge_id,
            Some(voter1.to_owned()), // option to vote for
        )
        .unwrap();
    suite
        .place_vote(
            &gauge_contract,
            voter2.to_owned(),
            gauge_id,
            Some(voter1.to_owned()),
        )
        .unwrap();

    let selected_set = suite.query_selected_set(&gauge_contract, gauge_id).unwrap();
    // voter1 was option voted for with two 100 voting powers combined
    assert_eq!(selected_set, vec![("voter1".to_owned(), Uint128::new(200))]);

    // before advancing specified epoch tally won't get sampled
    suite.advance_time(EPOCH);

    suite
        .execute_options(&gauge_contract, voter1, gauge_id)
        .unwrap();

    assert_eq!(
        suite.query_balance(voter1, reward_to_distribute.1).unwrap(),
        1000u128
    );

    // execution twice same time won't work
    let err = suite
        .execute_options(&gauge_contract, voter1, gauge_id)
        .unwrap_err();
    let next_epoch = suite.current_time() + EPOCH;
    assert_eq!(
        ContractError::EpochNotReached {
            gauge_id,
            current_epoch: suite.current_time(),
            next_epoch
        },
        err.downcast().unwrap()
    );

    // just before next epoch fails as well
    suite.advance_time(EPOCH - 1);
    let err = suite
        .execute_options(&gauge_contract, voter1, gauge_id)
        .unwrap_err();
    assert_eq!(
        ContractError::EpochNotReached {
            gauge_id,
            current_epoch: suite.current_time(),
            next_epoch
        },
        err.downcast().unwrap()
    );

    // another epoch is fine
    suite.advance_time(EPOCH);
    suite
        .execute_options(&gauge_contract, voter1, gauge_id)
        .unwrap();

    assert_eq!(
        suite.query_balance(voter1, reward_to_distribute.1).unwrap(),
        2000u128
    );
}

#[test]
fn execute_stopped_gauge() {
    let voter1 = "voter1";
    let voter2 = "voter2";
    let reward_to_distribute = (1000, "ujuno");
    let mut suite = SuiteBuilder::new()
        .with_voting_members(&[(voter1, 100), (voter2, 100)])
        .with_core_balance(reward_to_distribute)
        .build();

    suite.next_block();
    let gauge_config = suite
        .instantiate_adapter_and_return_config(&[voter1, voter2], reward_to_distribute)
        .unwrap();
    suite
        .propose_update_proposal_module(voter1.to_string(), vec![gauge_config])
        .unwrap();

    suite.next_block();
    let proposal = suite.list_proposals().unwrap()[0];
    suite
        .place_vote_single(voter1, proposal, Vote::Yes)
        .unwrap();
    suite
        .place_vote_single(voter2, proposal, Vote::Yes)
        .unwrap();

    suite.next_block();
    suite
        .execute_single_proposal(voter1.to_string(), proposal)
        .unwrap();
    let proposal_modules = suite.query_proposal_modules().unwrap();
    let gauge_contract = proposal_modules[0].clone();

    let gauge_id = 0;

    // stop the gauge by not-owner
    let err = suite
        .stop_gauge(&gauge_contract, voter1, gauge_id)
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    // stop the gauge by owner
    suite
        .stop_gauge(&gauge_contract, suite.owner.clone(), gauge_id)
        .unwrap();

    // vote for one of the options in gauge
    suite
        .place_vote(
            &gauge_contract,
            voter1.to_owned(),
            gauge_id,
            Some(voter1.to_owned()), // option to vote for
        )
        .unwrap();
    suite
        .place_vote(
            &gauge_contract,
            voter2.to_owned(),
            gauge_id,
            Some(voter1.to_owned()),
        )
        .unwrap();

    // Despite gauge being stopped, user
    let selected_set = suite.query_selected_set(&gauge_contract, gauge_id).unwrap();
    assert_eq!(selected_set, vec![("voter1".to_owned(), Uint128::new(200))]);

    // before advancing specified epoch tally won't get sampled
    suite.advance_time(EPOCH);

    let err = suite
        .execute_options(&gauge_contract, voter1, gauge_id)
        .unwrap_err();
    assert_eq!(
        ContractError::GaugeStopped(gauge_id),
        err.downcast().unwrap()
    );
}

#[test]
fn update_gauge() {
    let voter1 = "voter1";
    let voter2 = "voter2";
    let mut suite = SuiteBuilder::new()
        .with_voting_members(&[(voter1, 100), (voter2, 100)])
        .build();

    suite.next_block();
    suite
        .propose_update_proposal_module(voter1.to_string(), None)
        .unwrap();

    suite.next_block();
    let proposal = suite.list_proposals().unwrap()[0];
    suite
        .place_vote_single(voter1, proposal, Vote::Yes)
        .unwrap();
    suite
        .place_vote_single(voter2, proposal, Vote::Yes)
        .unwrap();

    suite.next_block();
    suite
        .execute_single_proposal(voter1.to_string(), proposal)
        .unwrap();
    let proposal_modules = suite.query_proposal_modules().unwrap();

    // Second proposal module is cw proposal single, first one is newly added gauge
    assert_eq!(proposal_modules.len(), 2);
    let gauge_contract = proposal_modules[0].clone();

    let gauge_adapter = suite
        .instantiate_adapter_and_create_gauge(
            gauge_contract.clone(),
            &[voter1, voter2],
            (1000, "ujuno"),
        )
        .unwrap();

    let second_gauge_adapter = suite
        .instantiate_adapter_and_create_gauge(
            gauge_contract.clone(),
            &[voter1, voter2],
            (1000, "uusdc"),
        )
        .unwrap();

    let response = suite.query_gauges(gauge_contract.clone()).unwrap();
    assert_eq!(
        response,
        vec![
            GaugeResponse {
                id: 0,
                title: "gauge".to_owned(),
                adapter: gauge_adapter.to_string(),
                epoch_size: EPOCH,
                min_percent_selected: Some(Decimal::percent(5)),
                max_options_selected: 10,
                is_stopped: false,
                next_epoch: suite.current_time() + 7 * 86400,
            },
            GaugeResponse {
                id: 1,
                title: "gauge".to_owned(),
                adapter: second_gauge_adapter.to_string(),
                epoch_size: EPOCH,
                min_percent_selected: Some(Decimal::percent(5)),
                max_options_selected: 10,
                is_stopped: false,
                next_epoch: suite.current_time() + 7 * 86400,
            }
        ]
    );

    // update parameters on the first gauge
    let owner = suite.owner.clone();
    let new_epoch = EPOCH * 2;
    let new_min_percent = Some(Decimal::percent(10));
    let new_max_options = 15;
    suite
        .update_gauge(
            &owner,
            gauge_contract.clone(),
            0,
            new_epoch,
            new_min_percent,
            new_max_options,
        )
        .unwrap();

    let response = suite.query_gauges(gauge_contract.clone()).unwrap();
    assert_eq!(
        response,
        vec![
            GaugeResponse {
                id: 0,
                title: "gauge".to_owned(),
                adapter: gauge_adapter.to_string(),
                epoch_size: new_epoch,
                min_percent_selected: new_min_percent,
                max_options_selected: new_max_options,
                is_stopped: false,
                next_epoch: suite.current_time() + 7 * 86400,
            },
            GaugeResponse {
                id: 1,
                title: "gauge".to_owned(),
                adapter: second_gauge_adapter.to_string(),
                epoch_size: EPOCH,
                min_percent_selected: Some(Decimal::percent(5)),
                max_options_selected: 10,
                is_stopped: false,
                next_epoch: suite.current_time() + 7 * 86400,
            }
        ]
    );

    // clean setting of min_percent_selected on second gauge
    suite
        .update_gauge(
            &owner,
            gauge_contract.clone(),
            1,
            None,
            Some(Decimal::zero()),
            None,
        )
        .unwrap();

    let response = suite.query_gauges(gauge_contract.clone()).unwrap();
    assert_eq!(
        response,
        vec![
            GaugeResponse {
                id: 0,
                title: "gauge".to_owned(),
                adapter: gauge_adapter.to_string(),
                epoch_size: new_epoch,
                min_percent_selected: new_min_percent,
                max_options_selected: new_max_options,
                is_stopped: false,
                next_epoch: suite.current_time() + 7 * 86400,
            },
            GaugeResponse {
                id: 1,
                title: "gauge".to_owned(),
                adapter: second_gauge_adapter.to_string(),
                epoch_size: EPOCH,
                min_percent_selected: None,
                max_options_selected: 10,
                is_stopped: false,
                next_epoch: suite.current_time() + 7 * 86400,
            }
        ]
    );

    // Not owner cannot update gauges
    let err = suite
        .update_gauge(
            "notowner",
            gauge_contract.clone(),
            0,
            new_epoch,
            new_min_percent,
            new_max_options,
        )
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let err = suite
        .update_gauge(
            &owner,
            gauge_contract.clone(),
            0,
            50,
            new_min_percent,
            new_max_options,
        )
        .unwrap_err();
    assert_eq!(ContractError::EpochSizeTooShort {}, err.downcast().unwrap());

    let err = suite
        .update_gauge(
            &owner,
            gauge_contract.clone(),
            0,
            new_epoch,
            Some(Decimal::one()),
            new_max_options,
        )
        .unwrap_err();
    assert_eq!(
        ContractError::MinPercentSelectedTooBig {},
        err.downcast().unwrap()
    );

    let err = suite
        .update_gauge(&owner, gauge_contract, 0, new_epoch, new_min_percent, 0)
        .unwrap_err();
    assert_eq!(
        ContractError::MaxOptionsSelectedTooSmall {},
        err.downcast().unwrap()
    );
}
