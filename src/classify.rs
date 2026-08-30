//! Compatibility-band classification: fluid class and consolidation action.

use crate::config::Thresholds;
use crate::model::{Action, FluidClass};

/// Map a compatibility score to its fluid character and the action the
/// planner should take. Bands:
///
/// - `>= mix`       → watery   / mix
/// - `>= emulsify`  → medium   / emulsify (bind through consolidation)
/// - `>= suspend`   → oily     / suspend (interface boundary)
/// - otherwise      → immiscible / reject
pub fn classify(compatibility: f64, thresholds: &Thresholds) -> (FluidClass, Action) {
    if compatibility >= thresholds.mix {
        (FluidClass::Watery, Action::Mix)
    } else if compatibility >= thresholds.emulsify {
        (FluidClass::Medium, Action::Emulsify)
    } else if compatibility >= thresholds.suspend {
        (FluidClass::Oily, Action::Suspend)
    } else {
        (FluidClass::Immiscible, Action::Reject)
    }
}
