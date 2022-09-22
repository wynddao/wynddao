// These are largely copied from
// https://github.com/DA0-DA0/dao-contracts/blob/v1.0.0/contracts/cw-proposal-single/src/tests.rs
// Being test code in a 3rd party repo, it was unclear how to extend them.
// Custom_code is in dao_bindings.rs. Ideally this can be largely replaced by imports with dao-dao v2

use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20Coin;
use cw_multi_test::{App, Executor};

use cw_proposal_single::msg::DepositInfo;
use cw_proposal_single::query::{ProposalResponse, VoteResponse};
use testing::{ShouldExecute, TestVote};
use voting::{Status, Threshold};

use crate::dao_bindings::{instantiate_with_wynd_stake, single_proposal_contract};

#[test]
fn test_vote_simple() {
    testing::test_simple_votes(do_votes_wynd_stake);
}

#[test]
fn test_simple_early_rejection() {
    testing::test_simple_early_rejection(do_votes_wynd_stake);
}

#[test]
fn test_vote_abstain_only() {
    testing::test_vote_abstain_only(do_votes_wynd_stake);
}

#[test]
fn test_votes_favor_yes() {
    testing::test_votes_favor_yes(do_votes_wynd_stake);
}

fn do_votes_wynd_stake(
    votes: Vec<TestVote>,
    threshold: Threshold,
    expected_status: Status,
    total_supply: Option<Uint128>,
) {
    do_test_votes(
        votes,
        threshold,
        expected_status,
        total_supply,
        None,
        instantiate_with_wynd_stake,
    );
}

fn do_test_votes<F>(
    votes: Vec<TestVote>,
    threshold: Threshold,
    expected_status: Status,
    total_supply: Option<Uint128>,
    deposit_info: Option<DepositInfo>,
    setup_governance: F,
) -> (App, Addr)
where
    F: Fn(&mut App, u64, cw_proposal_single::msg::InstantiateMsg, Option<Vec<Cw20Coin>>) -> Addr,
{
    let mut app = App::default();
    let govmod_id = app.store_code(single_proposal_contract());

    let mut initial_balances = votes
        .iter()
        .map(|TestVote { voter, weight, .. }| Cw20Coin {
            address: voter.to_string(),
            amount: *weight,
        })
        .collect::<Vec<Cw20Coin>>();
    let initial_balances_supply = votes.iter().fold(Uint128::zero(), |p, n| p + n.weight);
    let to_fill = total_supply.map(|total_supply| total_supply - initial_balances_supply);
    if let Some(fill) = to_fill {
        initial_balances.push(Cw20Coin {
            address: "filler".to_string(),
            amount: fill,
        })
    }

    let proposer = match votes.first() {
        Some(vote) => vote.voter.clone(),
        None => panic!("do_test_votes must have at least one vote."),
    };

    let max_voting_period = cw_utils::Duration::Height(6);
    let instantiate = cw_proposal_single::msg::InstantiateMsg {
        threshold,
        max_voting_period,
        min_voting_period: None,
        allow_revoting: false,
        deposit_info,
        executor: cw_proposal_single::state::Executor::Anyone,
    };

    let governance_addr =
        setup_governance(&mut app, govmod_id, instantiate, Some(initial_balances));

    let governance_modules: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(
            governance_addr.clone(),
            &cw_core::msg::QueryMsg::ProposalModules {
                start_at: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(governance_modules.len(), 1);
    let govmod_single = governance_modules.into_iter().next().unwrap();

    // Allow a proposal deposit as needed.
    let config: cw_proposal_single::state::Config = app
        .wrap()
        .query_wasm_smart(
            govmod_single.clone(),
            &cw_proposal_single::msg::QueryMsg::Config {},
        )
        .unwrap();
    if let Some(cw_proposal_single::state::CheckedDepositInfo {
        ref token, deposit, ..
    }) = config.deposit_info
    {
        app.execute_contract(
            Addr::unchecked(&proposer),
            token.clone(),
            &cw20_vesting::msg::ExecuteMsg::IncreaseAllowance {
                spender: govmod_single.to_string(),
                amount: deposit,
                expires: None,
            },
            &[],
        )
        .unwrap();
    }

    app.execute_contract(
        Addr::unchecked(&proposer),
        govmod_single.clone(),
        &cw_proposal_single::msg::ExecuteMsg::Propose {
            title: "A simple text proposal".to_string(),
            description: "This is a simple text proposal".to_string(),
            msgs: vec![],
        },
        &[],
    )
    .unwrap();

    // Cast votes.
    for vote in votes {
        let TestVote {
            voter,
            position,
            weight,
            should_execute,
        } = vote;
        // Vote on the proposal.
        let res = app.execute_contract(
            Addr::unchecked(voter.clone()),
            govmod_single.clone(),
            &cw_proposal_single::msg::ExecuteMsg::Vote {
                proposal_id: 1,
                vote: position,
            },
            &[],
        );
        match should_execute {
            ShouldExecute::Yes => {
                assert!(res.is_ok());
                // Check that the vote was recorded correctly.
                let vote: VoteResponse = app
                    .wrap()
                    .query_wasm_smart(
                        govmod_single.clone(),
                        &cw_proposal_single::msg::QueryMsg::Vote {
                            proposal_id: 1,
                            voter: voter.clone(),
                        },
                    )
                    .unwrap();
                let expected = VoteResponse {
                    vote: Some(cw_proposal_single::query::VoteInfo {
                        voter: Addr::unchecked(&voter),
                        vote: position,
                        power: match config.deposit_info {
                            Some(cw_proposal_single::state::CheckedDepositInfo {
                                deposit, ..
                            }) => {
                                if proposer == voter {
                                    weight - deposit
                                } else {
                                    weight
                                }
                            }
                            None => weight,
                        },
                    }),
                };
                assert_eq!(vote, expected)
            }
            ShouldExecute::No => assert!(res.is_err()),
            ShouldExecute::Meh => (),
        }
    }

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            govmod_single,
            &cw_proposal_single::msg::QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    assert_eq!(proposal.proposal.status, expected_status);

    (app, governance_addr)
}
