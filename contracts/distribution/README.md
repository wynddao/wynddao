# Distribution

This contract sends a specific amount of cw20 tokens per epoch to a predefined address that distributes it.
The distribution address has to support `wynd-stake::ExecuteMsg::DistributeRewards`, because it is sent by this contract.

## Instantiate

Here we set the basic values needed for the contract:
- epoch - number of seconds between payments
- payment - how much to pay out each epoch (Coin)
- recipient - the contract address to pay to - must handle ExecuteMsg::DistributeRewards{}
- admin - who can adjust the config
- cw20_contract - the contract of the token to send

## Execution

There is one method for the admin to update the config and one to cause the payout.
The payout message will be called regularly by an off-chain bot.

## Query

You can query the current config.
