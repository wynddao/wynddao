use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Env, OverflowError, Timestamp, Uint128};
use cw_controllers::{Admin, Claims, Hooks};
use cw_storage_plus::{Item, Map, SnapshotItem, SnapshotMap, Strategy};

use crate::msg::StakeConfig;

pub const CLAIMS: Claims = Claims::new("claims");

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct Config {
    /// address of cw20 contract token to stake
    pub cw20_contract: Addr,
    pub tokens_per_power: Uint128,
    pub min_bond: Uint128,
    /// configured unbonding periods in seconds
    pub unbonding_periods: Vec<UnbondingPeriod>,
}

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct BondingInfo {
    /// the amount of staked tokens which are not locked
    stake: Uint128,
    pub votes: Uint128,
    pub rewards: Uint128,
    /// Vec of locked_tokens sorted by expiry timestamp
    locked_tokens: Vec<(Timestamp, Uint128)>,
}

impl BondingInfo {
    /// Add an amount of tokens to the stake
    pub fn add_unlocked_tokens(&mut self, amount: Uint128) -> Uint128 {
        let tokens = self.stake.checked_add(amount).unwrap();

        self.stake = tokens;

        tokens
    }

    /// Inserts a new locked_tokens entry in its correct place with a provided expires Timestamp and an amount
    pub fn add_locked_tokens(&mut self, expires: Timestamp, amount: Uint128) {
        // Insert the new locked_tokens entry into its correct place using a binary search and an insert
        match self.locked_tokens.binary_search(&(expires, amount)) {
            Ok(pos) => self.locked_tokens[pos].1 += amount,
            Err(pos) => self.locked_tokens.insert(pos, (expires, amount)),
        }
    }

    /// Free any tokens which are now considered unlocked
    /// Split locked tokens based on which are expired and assign the remaining ones to locked_tokens
    /// For each unlocked one, add this amount to the stake
    pub fn free_unlocked_tokens(&mut self, env: &Env) {
        if self.locked_tokens.is_empty() {
            return;
        }
        let (unlocked, remaining): (Vec<_>, Vec<_>) = self
            .locked_tokens
            .iter()
            .partition(|(time, _)| time <= &env.block.time);
        self.locked_tokens = remaining;

        self.stake += unlocked.into_iter().map(|(_, v)| v).sum::<Uint128>();
    }

    /// Attempt to release an amount of stake. First releasing any already unlocked tokens
    /// and then subtracting the requested amount from stake.
    /// On success, returns total_unlocked() after reducing the stake by this amount.
    pub fn release_stake(&mut self, env: &Env, amount: Uint128) -> Result<Uint128, OverflowError> {
        self.free_unlocked_tokens(env);

        let new_stake = self.stake.checked_sub(amount)?;

        self.stake = new_stake;

        Ok(self.stake)
    }

    /// Return all locked tokens at a given block time that is all
    /// locked_tokens with a Timestamp > the block time passed in env as a param
    pub fn total_locked(&self, env: &Env) -> Uint128 {
        let locked_stake = self
            .locked_tokens
            .iter()
            .filter_map(|(t, v)| if t > &env.block.time { Some(v) } else { None })
            .sum::<Uint128>();
        locked_stake
    }

    /// Return all locked tokens at a given block time that is all
    /// locked_tokens with a Timestamp > the block time passed in env as a param
    pub fn total_unlocked(&self, env: &Env) -> Uint128 {
        let mut unlocked_stake: Uint128 = self.stake;
        unlocked_stake += self
            .locked_tokens
            .iter()
            .filter_map(|(t, v)| if t <= &env.block.time { Some(v) } else { None })
            .sum::<Uint128>();

        unlocked_stake
    }

    /// Return all stake for this BondingInfo, including locked_tokens
    pub fn total_stake(&self) -> Uint128 {
        let total_stake: Uint128 = self
            .stake
            .checked_add(self.locked_tokens.iter().map(|x| x.1).sum())
            .unwrap();
        total_stake
    }
}

pub const ADMIN: Admin = Admin::new("admin");
pub const HOOKS: Hooks = Hooks::new("cw4-hooks");
pub const CONFIG: Item<Config> = Item::new("config");

pub const MEMBERS: SnapshotMap<&Addr, Uint128> = SnapshotMap::new(
    cw4::MEMBERS_KEY,
    cw4::MEMBERS_CHECKPOINTS,
    cw4::MEMBERS_CHANGELOG,
    Strategy::EveryBlock,
);
/// Contains the total rewards per user
pub const REWARDS: Map<&Addr, Uint128> = Map::new("rewards");

pub const TOTAL_VOTES: SnapshotItem<Uint128> = SnapshotItem::new(
    "total",
    "total__checkpoints",
    "total__changelog",
    Strategy::EveryBlock,
);
/// Contains the sum of all rewards
pub const TOTAL_REWARDS: Item<Uint128> = Item::new("total_rewards");

#[derive(Default, Serialize, Deserialize)]
pub struct TokenInfo {
    // how many tokens are fully bonded
    pub staked: Uint128,
    // how many tokens are unbounded and awaiting claim
    pub unbonding: Uint128,
}

impl TokenInfo {
    pub fn total(&self) -> Uint128 {
        self.staked + self.unbonding
    }
}

pub const TOTAL_STAKED: Item<TokenInfo> = Item::new("total_staked");

pub const STAKE: Map<(&Addr, UnbondingPeriod), BondingInfo> = Map::new("stake");

/// Unbonding period in seconds
type UnbondingPeriod = u64;
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct StakeMultipliers {
    /// Voting multiplier - stake * voting_multiplier = voting_power
    pub voting: Decimal,
    /// Reward multiplier - stake * reward_multiplier = reward_power
    pub reward: Decimal,
    /// Total staked - not a multiplier, but a total amount of tokens staked to this UnbondingPeriod
    pub staked: Uint128,
}

impl From<StakeConfig> for StakeMultipliers {
    fn from(sc: StakeConfig) -> Self {
        Self {
            voting: sc.voting_multiplier,
            reward: sc.reward_multiplier,
            staked: Uint128::zero(),
        }
    }
}
pub const STAKE_CONFIG: Map<UnbondingPeriod, StakeMultipliers> = Map::new("stake_config");

/**** For distribution logic *****/

/// How much points is the worth of single token in rewards distribution.
/// The scaling is performed to have better precision of fixed point division.
/// This value is not actually the scaling itself, but how much bits value should be shifted
/// (for way more efficient division).
///
/// 32, to have those 32 bits, but it reduces how much tokens may be handled by this contract
/// (it is now 96-bit integer instead of 128). In original ERC2222 it is handled by 256-bit
/// calculations, but I256 is missing and it is required for this.
pub const SHARES_SHIFT: u8 = 32;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug, Default)]
pub struct Distribution {
    /// How many shares is single point worth
    pub shares_per_point: Uint128,
    /// Shares which were not fully distributed on previous distributions, and should be redistributed
    pub shares_leftover: u64,
    /// Total rewards distributed by this contract.
    pub distributed_total: Uint128,
    /// Total rewards not yet withdrawn.
    pub withdrawable_total: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct WithdrawAdjustment {
    /// How much points should be added/removed from calculated funds while withdrawal.
    pub shares_correction: i128,
    /// How much funds addresses already withdrawn.
    pub withdrawn_rewards: Uint128,
    /// User delegated for funds withdrawal
    pub delegated: Addr,
}

/// Rewards distribution data
pub const DISTRIBUTION: Item<Distribution> = Item::new("distribution");
/// Information how to exactly adjust rewards while withdrawal
pub const WITHDRAW_ADJUSTMENT: Map<&Addr, WithdrawAdjustment> = Map::new("withdraw_adjustment");

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::OverflowOperation;

    #[test]
    fn test_bonding_info_add() {
        let mut info = BondingInfo::default();
        let env = mock_env();

        info.stake = info.add_unlocked_tokens(Uint128::new(1000u128));

        assert_eq!(info.total_unlocked(&env), Uint128::new(1000u128));

        info.add_locked_tokens(env.block.time.plus_seconds(1000), Uint128::new(1000u128));
        assert_eq!(
            info.locked_tokens,
            [(env.block.time.plus_seconds(1000), Uint128::new(1000u128))]
        );
        assert_eq!(info.total_locked(&env), Uint128::new(1000u128))
    }
    #[test]
    fn test_bonding_info_add_then_release() {
        let mut info = BondingInfo::default();
        let env = mock_env();

        info.stake = info.add_unlocked_tokens(Uint128::new(1000u128));

        info.add_locked_tokens(env.block.time.plus_seconds(1000), Uint128::new(1000u128));
        // Trying to release both locked and unlocked tokens fails
        let err = info
            .release_stake(&env, Uint128::new(2000u128))
            .unwrap_err();
        assert_eq!(
            err,
            OverflowError {
                operation: OverflowOperation::Sub,
                operand1: "1000".to_string(),
                operand2: "2000".to_string()
            }
        );
        // But releasing the unlocked tokens passes
        info.release_stake(&env, Uint128::new(1000u128)).unwrap();
    }

    #[test]
    fn test_bonding_info_queries() {
        let mut info = BondingInfo::default();
        let env = mock_env();

        info.stake = info.add_unlocked_tokens(Uint128::new(1000u128));
        info.add_locked_tokens(env.block.time.plus_seconds(10), Uint128::new(1000u128));

        info.stake = info.add_unlocked_tokens(Uint128::new(500u128));
        info.add_locked_tokens(env.block.time.plus_seconds(20), Uint128::new(500u128));

        info.stake = info.add_unlocked_tokens(Uint128::new(100u128));
        info.add_locked_tokens(env.block.time.plus_seconds(30), Uint128::new(100u128));

        assert_eq!(info.total_locked(&env), Uint128::new(1600u128));
        assert_eq!(info.total_unlocked(&env), Uint128::new(1600u128));
        assert_eq!(info.total_stake(), Uint128::new(3200u128));
    }

    #[test]
    fn test_free_tokens() {
        let mut info = BondingInfo::default();
        let env = mock_env();

        info.stake = info.add_unlocked_tokens(Uint128::new(1000u128));

        assert_eq!(info.total_unlocked(&env), Uint128::new(1000u128));

        info.add_locked_tokens(env.block.time.minus_seconds(1000), Uint128::new(1000u128));
        assert_eq!(
            info.locked_tokens,
            [(env.block.time.minus_seconds(1000), Uint128::new(1000u128))]
        );

        info.add_locked_tokens(env.block.time.plus_seconds(1000), Uint128::new(1000u128));

        assert_eq!(info.total_unlocked(&env), Uint128::new(2000u128));
        assert_eq!(
            info.release_stake(&env, Uint128::new(1500u128)).unwrap(),
            Uint128::new(500u128)
        );
        assert_eq!(info.total_stake(), Uint128::new(1500));
        assert_eq!(info.total_locked(&env), Uint128::new(1000u128));
    }
}
