use cosmwasm_std::Uint128;
use voting::Vote;

use super::suite::SuiteBuilder;
use crate::error::ContractError;
use crate::msg::VoteInfo;

#[test]
fn add_option() {
    let voter1 = "voter1";
    let voter2 = "voter2";
    let mut suite = SuiteBuilder::new()
        .with_voting_members(&[(voter1, 100), (voter2, 200)])
        .build();

    suite.next_block();
    suite
        .propose_update_proposal_module(voter1.to_string())
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

    suite
        .instantiate_adapter_and_create_gauge(
            gauge_contract.clone(),
            &[voter1, voter2],
            (1000, "ujuno"),
        )
        .unwrap();

    let gauge_id = 0; // first created gauge

    // gauge returns list all options; it does query adapter at initialization
    let options = suite.query_list_options(&gauge_contract, gauge_id).unwrap();
    assert_eq!(options.len(), 2);

    // Voting members can add options
    suite
        .add_option(&gauge_contract, voter1, gauge_id, "addedoption1")
        .unwrap();
    suite
        .add_option(&gauge_contract, voter2, gauge_id, "addedoption2")
        .unwrap();
    let options = suite.query_list_options(&gauge_contract, gauge_id).unwrap();
    // added options are automatically voted for by creators
    assert_eq!(
        options,
        vec![
            ("addedoption1".to_owned(), Uint128::new(100)),
            ("addedoption2".to_owned(), Uint128::new(200)),
            ("voter1".to_owned(), Uint128::zero()),
            ("voter2".to_owned(), Uint128::zero())
        ]
    );

    // Non-voting members cannot add options
    let err = suite
        .add_option(&gauge_contract, "random_voter", gauge_id, "addedoption3")
        .unwrap_err();
    assert_eq!(
        ContractError::NoVotingPower("random_voter".to_owned()),
        err.downcast().unwrap()
    );
}

#[test]
fn vote_for_option() {
    let voter1 = "voter1";
    let voter2 = "voter2";
    let mut suite = SuiteBuilder::new()
        .with_voting_members(&[(voter1, 100), (voter2, 200)])
        .build();

    suite.next_block();
    suite
        .propose_update_proposal_module(voter1.to_string())
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

    suite
        .instantiate_adapter_and_create_gauge(
            gauge_contract.clone(),
            &[voter1, voter2],
            (1000, "ujuno"),
        )
        .unwrap();

    let gauge_id = 0; // first created gauge

    // vote for option from adapter (voting members are by default
    // options in adapter in this test suite)
    suite
        .place_vote(
            &gauge_contract,
            voter1.to_owned(),
            gauge_id,
            Some(voter1.to_owned()),
        )
        .unwrap();
    assert_eq!(
        Some(VoteInfo {
            voter: voter1.to_owned(),
            option: voter1.to_owned(),
            power: Uint128::new(100)
        }),
        suite.query_vote(&gauge_contract, gauge_id, voter1).unwrap()
    );

    // change vote for option added through gauge
    suite
        .add_option(&gauge_contract, voter1, gauge_id, "option1")
        .unwrap();
    // voter2 drops vote as well
    suite
        .place_vote(
            &gauge_contract,
            voter2.to_owned(),
            gauge_id,
            Some("option1".to_owned()),
        )
        .unwrap();
    assert_eq!(
        vec![
            VoteInfo {
                voter: voter1.to_owned(),
                option: "option1".to_owned(),
                power: Uint128::new(100)
            },
            VoteInfo {
                voter: voter2.to_owned(),
                option: "option1".to_owned(),
                power: Uint128::new(200),
            }
        ],
        suite.query_list_votes(&gauge_contract, gauge_id).unwrap()
    );

    // vote for non-existing option
    let err = suite
        .place_vote(
            &gauge_contract,
            voter1.to_owned(),
            gauge_id,
            Some("random option".to_owned()),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::OptionDoesNotExists {
            option: "random option".to_owned(),
            gauge_id
        },
        err.downcast().unwrap()
    );
}

#[test]
fn remove_vote() {
    let voter1 = "voter1";
    let voter2 = "voter2";
    let mut suite = SuiteBuilder::new()
        .with_voting_members(&[(voter1, 100), (voter2, 200)])
        .build();

    suite.next_block();
    suite
        .propose_update_proposal_module(voter1.to_string())
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

    suite
        .instantiate_adapter_and_create_gauge(
            gauge_contract.clone(),
            &[voter1, voter2],
            (1000, "ujuno"),
        )
        .unwrap();

    let gauge_id = 0; // first created gauge

    // vote for option from adapter (voting members are by default
    // options in adapter in this test suite)
    suite
        .place_vote(
            &gauge_contract,
            voter1.to_owned(),
            gauge_id,
            Some(voter1.to_owned()),
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
    assert_eq!(
        vec![
            VoteInfo {
                voter: voter1.to_owned(),
                option: voter1.to_owned(),
                power: Uint128::new(100)
            },
            VoteInfo {
                voter: voter2.to_owned(),
                option: voter1.to_owned(),
                power: Uint128::new(200),
            },
        ],
        suite.query_list_votes(&gauge_contract, gauge_id).unwrap()
    );

    // remove vote
    suite
        .place_vote(&gauge_contract, voter1.to_owned(), gauge_id, None)
        .unwrap();
    assert_eq!(
        vec![VoteInfo {
            voter: voter2.to_owned(),
            option: voter1.to_owned(),
            power: Uint128::new(200),
        }],
        suite.query_list_votes(&gauge_contract, gauge_id).unwrap()
    );
    assert_eq!(
        None,
        suite.query_vote(&gauge_contract, gauge_id, voter1).unwrap()
    );

    // remove nonexisting vote
    let err = suite
        .place_vote(&gauge_contract, voter1.to_owned(), gauge_id, None)
        .unwrap_err();
    assert_eq!(
        ContractError::CannotRemoveNonexistingVote {},
        err.downcast().unwrap()
    );
}
