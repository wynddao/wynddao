use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal, Uint128};

use crate::{Curve, CurveError, PiecewiseLinear, SaturatingLinear};

/// Scalable Curve types
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScalableCurve {
    /// constant over time
    Constant {
        /// ratio
        ratio: Decimal,
    },
    /// Linear increasing or decreasing function
    ScalableLinear(ScalableLinear),
    /// Linear increasing or decreasing function by parts
    ScalablePiecewise(ScalablePiecewise),
}

impl ScalableCurve {
    /// apply f(x) using amount according to the type
    pub fn scale(self, amount: Uint128) -> Curve {
        match self {
            ScalableCurve::Constant { ratio } => Curve::Constant { y: amount * ratio },
            ScalableCurve::ScalableLinear(s) => s.scale(amount),
            ScalableCurve::ScalablePiecewise(p) => p.scale(amount),
        }
    }

    /// sanity check of monotonic increasing (always grow in value, never decrease)
    pub fn validate_monotonic_increasing(&self) -> Result<(), CurveError> {
        self.clone()
            .scale(Uint128::new(1_000_000_000))
            .validate_monotonic_increasing()
    }

    /// sanity check of monotonic decreasing (always decrease from an inital value, never increase)
    pub fn validate_monotonic_decreasing(&self) -> Result<(), CurveError> {
        self.clone()
            .scale(Uint128::new(1_000_000_000))
            .validate_monotonic_decreasing()
    }

    /// create a linear scalable function based on 2 points where x is time and y is value
    pub fn linear((min_x, min_percent): (u64, u64), (max_x, max_percent): (u64, u64)) -> Self {
        ScalableCurve::ScalableLinear(ScalableLinear {
            min_x,
            min_y: Decimal::percent(min_percent),
            max_x,
            max_y: Decimal::percent(max_percent),
        })
    }
}

/// Scalable Linear
/// $$f(x)=\begin{cases}
/// [y * amount],  & \text{if $x_1$ >= x <= $x_2$ }
/// \end{cases}$$
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Eq, PartialEq)]
pub struct ScalableLinear {
    /// start x time
    pub min_x: u64,
    /// start y rate
    pub min_y: Decimal,
    /// end x time
    pub max_x: u64,
    /// end y rate
    pub max_y: Decimal,
}

impl ScalableLinear {
    /// scale aplly the f(x) for the amount specified
    pub fn scale(self, amount: Uint128) -> Curve {
        Curve::SaturatingLinear(SaturatingLinear {
            min_x: self.min_x,
            min_y: amount * self.min_y,
            max_x: self.max_x,
            max_y: amount * self.max_y,
        })
    }
}

/// Scalable Piece Wise
/// $$f(x)=\begin{cases}
/// \[x_n, (a_n*y_n)\],  & \text{if $x_n$ > $x_{n-1}$}
/// \end{cases}$$
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Eq, PartialEq)]
pub struct ScalablePiecewise {
    /// steps where x [`u64`] is time and y [`Decimal`](cosmwasm_std::Decimal)
    pub steps: Vec<(u64, Decimal)>,
}

impl ScalablePiecewise {
    /// scale aplly the f(x) for the amount specified
    pub fn scale(self, amount: Uint128) -> Curve {
        let steps = self
            .steps
            .into_iter()
            .map(|(x, y)| (x, amount * y))
            .collect();
        Curve::PiecewiseLinear(PiecewiseLinear { steps })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_case::test_case;

    // TODO: Test percent values greater that 100. Also zero/really small percent
    #[test_case(50,Uint128::new(444); "scale constant, should return percent * amount")]
    fn scale_constant(percent: u64, amount: Uint128) {
        let flex = ScalableCurve::Constant {
            ratio: Decimal::percent(percent),
        };
        let curve = flex.scale(amount);
        assert_eq!(curve, Curve::constant(222));
    }

    #[test_case(10000, 20000, 80, 20 ; "scale linear, should not fail")]
    fn scale_linear(min_x: u64, max_x: u64, p1: u64, p2: u64) {
        let (min_x, max_x) = (min_x, max_x);
        let flex = ScalableCurve::ScalableLinear(ScalableLinear {
            min_x,
            min_y: Decimal::percent(p1),
            max_x,
            max_y: Decimal::percent(p2),
        });
        let curve = flex.scale(Uint128::new(1100));
        assert_eq!(curve, Curve::saturating_linear((min_x, 880), (max_x, 220)));
    }

    #[test_case(vec![10000, 20000, 25000],vec![100, 70, 0],vec![8000,5600,0]; "scale piecewise, should not fail")]
    fn scale_piecewise(x: Vec<u64>, p: Vec<u64>, expected: Vec<u128>) {
        let (x1, x2, x3) = (x[0], x[1], x[2]);
        let flex = ScalableCurve::ScalablePiecewise(ScalablePiecewise {
            steps: vec![
                (x1, Decimal::percent(p[0])),
                (x2, Decimal::percent(p[1])),
                (x3, Decimal::percent(p[2])),
            ],
        });
        let curve = flex.scale(Uint128::new(expected[0]));
        assert_eq!(
            curve,
            Curve::PiecewiseLinear(PiecewiseLinear {
                steps: vec![
                    (x1, Uint128::new(expected[0])),
                    (x2, Uint128::new(expected[1])),
                    (x3, Uint128::new(expected[2])),
                ]
            })
        );
    }
}
