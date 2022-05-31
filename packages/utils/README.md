# Wynd Utils

This is a set of general utility functions to be used by mutliple contracts.

## Curves

We provide some curve types that can be used for vesting or otheer cases. They are designed to
generally be either monotonically increasing or monotonically decreasing withing a specified range.

While they are general functions in the form of `f(x) = y`, the main use case is with input of
block time, eg. `f(t) = y`. When using for other domains, please review the assumptions and make any
adjustments needed to generalize.
