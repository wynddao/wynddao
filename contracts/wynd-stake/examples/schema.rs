use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, export_schema_with_title, remove_schemas, schema_for};

use cw4::{AdminResponse, HooksResponse};
use cw_core_interface::voting::{
    InfoResponse, TotalPowerAtHeightResponse, VotingPowerAtHeightResponse,
};
use wynd_stake::msg::{
    AllStakedResponse, BondingInfoResponse, ClaimsResponse, DelegatedResponse,
    DistributedRewardsResponse, DistributionDataResponse, ExecuteMsg, InstantiateMsg, QueryMsg,
    ReceiveDelegationMsg, RewardsResponse, StakedResponse, TokenContractResponse,
    TotalRewardsResponse, TotalStakedResponse, UndistributedRewardsResponse,
    WithdrawAdjustmentDataResponse, WithdrawableRewardsResponse,
};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(ReceiveDelegationMsg), &out_dir);

    export_schema(&schema_for!(AdminResponse), &out_dir);
    export_schema(&schema_for!(HooksResponse), &out_dir);
    export_schema(&schema_for!(ClaimsResponse), &out_dir);
    export_schema(&schema_for!(StakedResponse), &out_dir);
    export_schema(&schema_for!(AllStakedResponse), &out_dir);
    export_schema(&schema_for!(TotalStakedResponse), &out_dir);
    export_schema(&schema_for!(BondingInfoResponse), &out_dir);

    export_schema(&schema_for!(InfoResponse), &out_dir);
    export_schema(&schema_for!(TotalPowerAtHeightResponse), &out_dir);
    export_schema(&schema_for!(VotingPowerAtHeightResponse), &out_dir);
    export_schema(&schema_for!(TokenContractResponse), &out_dir);
    export_schema(&schema_for!(TotalRewardsResponse), &out_dir);
    export_schema(&schema_for!(RewardsResponse), &out_dir);

    export_schema(&schema_for!(WithdrawableRewardsResponse), &out_dir);
    export_schema(&schema_for!(DelegatedResponse), &out_dir);
    export_schema_with_title(
        &schema_for!(UndistributedRewardsResponse),
        &out_dir,
        "UndistributedRewardsResponse",
    );
    export_schema_with_title(
        &schema_for!(DistributedRewardsResponse),
        &out_dir,
        "DistributedRewardsResponse",
    );
    export_schema_with_title(
        &schema_for!(DistributionDataResponse),
        &out_dir,
        "DistributionDataResponse",
    );
    export_schema_with_title(
        &schema_for!(WithdrawAdjustmentDataResponse),
        &out_dir,
        "WithdrawAdjustmentDataResponse",
    );
}
