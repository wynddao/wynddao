use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, BlockInfo, Env, Storage, Uint128};
use cw_storage_plus::{Item, Map};

use crate::ContractError;
use cw20::{AllowanceResponse, Logo, MarketingInfoResponse};
use wynd_utils::Curve;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
    pub mint: Option<MinterData>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct MinterData {
    pub minter: Addr,
    /// cap is how many more tokens can be issued by the minter
    pub cap: Option<Curve>,
}

impl TokenInfo {
    pub fn get_cap(&self, block: &BlockInfo) -> Option<Uint128> {
        self.mint
            .as_ref()
            .and_then(|v| v.cap.as_ref().map(|v| v.value(block.time.seconds())))
    }
}

pub const ALLOWLIST: Item<Vec<Addr>> = Item::new("allowlist");
pub const TOKEN_INFO: Item<TokenInfo> = Item::new("token_info");
pub const MARKETING_INFO: Item<MarketingInfoResponse> = Item::new("marketing_info");
pub const LOGO: Item<Logo> = Item::new("logo");
pub const BALANCES: Map<&Addr, Uint128> = Map::new("balance");
pub const ALLOWANCES: Map<(&Addr, &Addr), AllowanceResponse> = Map::new("allowance");
/// existing vesting schedules for each account
pub const VESTING: Map<&Addr, Curve> = Map::new("vesting");

/// This reduces the account by the given amount, but it also checks the vesting schedule to
/// ensure there is enough liquidity to do the transfer.
/// (Always use this to enforce the vesting schedule)
pub fn deduct_coins(
    storage: &mut dyn Storage,
    env: &Env,
    sender: &Addr,
    amount: Uint128,
) -> Result<Uint128, ContractError> {
    // vesting is how much is currently vesting
    let vesting = VESTING
        .may_load(storage, sender)?
        .map(|v| v.value(env.block.time.seconds()));

    // this occurs when there is a curve defined, but it is now at 0 (eg. fully vested)
    // in this case, we can safely delete it (as it will remain 0 forever)
    if vesting == Some(Uint128::zero()) {
        VESTING.remove(storage, sender);
    }

    BALANCES.update(storage, sender, |balance: Option<Uint128>| {
        let remainder = balance.unwrap_or_default().checked_sub(amount)?;
        // enforce vesting (must have at least this much available)
        if let Some(vest) = vesting {
            if vest > remainder {
                return Err(ContractError::CantMoveVestingTokens);
            }
        }
        Ok(remainder)
    })
}
