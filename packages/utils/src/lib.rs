#![warn(missing_docs)]
#![doc(html_logo_url = "../../../uml/logo.png")]
//! # WYND Curve
//!
//! ## Description
//!
//! We need a project that defines how we are going to schedule the distribution tokens.
//!
//! ## Objectives
//!
//! The main goal of the **WYND curve** is to:
//!   - Define Linear, piecewise and constant curves
//!

/// Main Curve Module
mod curve;

/// Scalable Curves
mod scalable_curve;

pub use curve::{Curve, CurveError, PiecewiseLinear, SaturatingLinear};
pub use scalable_curve::{ScalableCurve, ScalableLinear, ScalablePiecewise};
