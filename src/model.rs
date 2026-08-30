//! Core domain model: viscosity dimensions, fluid classes, and actions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The nine merge-compatibility dimensions from the Emulsify model.
///
/// Every dimension is scored in `[0, 1]` where 1 means "identical
/// material". A dimension that cannot be measured for a given pair
/// (for example, neither side has any tests) is reported as absent and
/// excluded from the weighted sum rather than silently scored zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionId {
    Api,
    Types,
    Dependencies,
    Architecture,
    Behaviour,
    Tests,
    Ownership,
    Lineage,
    Abstraction,
}

impl DimensionId {
    pub const ALL: [DimensionId; 9] = [
        DimensionId::Api,
        DimensionId::Types,
        DimensionId::Dependencies,
        DimensionId::Architecture,
        DimensionId::Behaviour,
        DimensionId::Tests,
        DimensionId::Ownership,
        DimensionId::Lineage,
        DimensionId::Abstraction,
    ];

    /// Stable key used in `emulsify.toml` and JSON output.
    pub fn key(self) -> &'static str {
        match self {
            DimensionId::Api => "api",
            DimensionId::Types => "types",
            DimensionId::Dependencies => "dependencies",
            DimensionId::Architecture => "architecture",
            DimensionId::Behaviour => "behaviour",
            DimensionId::Tests => "tests",
            DimensionId::Ownership => "ownership",
            DimensionId::Lineage => "lineage",
            DimensionId::Abstraction => "abstraction",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DimensionId::Api => "API similarity",
            DimensionId::Types => "type/schema similarity",
            DimensionId::Dependencies => "dependency similarity",
            DimensionId::Architecture => "architectural similarity",
            DimensionId::Behaviour => "behavioural similarity",
            DimensionId::Tests => "test compatibility",
            DimensionId::Ownership => "ownership/domain similarity",
            DimensionId::Lineage => "commit lineage",
            DimensionId::Abstraction => "abstraction similarity",
        }
    }
}

/// A per-dimension score. `None` means the dimension was not measurable
/// for this pair and was excluded from the weighted total.
pub type DimensionScores = BTreeMap<DimensionId, Option<f64>>;

/// Weighted compatibility between two codebases plus the effective
/// weight that was actually measurable.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Compatibility {
    /// Weighted mean over the measurable dimensions, in `[0, 1]`.
    pub overall: f64,
    pub dimensions: DimensionScores,
    /// Sum of weights of the measurable dimensions (<= 1.0).
    pub measured_weight: f64,
}

impl Compatibility {
    /// Viscosity is the inverse of compatibility: highly compatible
    /// material flows ("watery"), incompatible material resists ("oily").
    pub fn viscosity(&self) -> f64 {
        1.0 - self.overall
    }
}

/// The fluid character of a fork, derived from its compatibility band.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FluidClass {
    /// Low viscosity: directly incorporable.
    Watery,
    /// Meaningful divergence, but shared abstractions remain.
    Medium,
    /// Architecturally divergent; direct incorporation would destabilise the host.
    Oily,
    /// The fork should not be merged directly at all.
    Immiscible,
}

impl FluidClass {
    pub fn label(self) -> &'static str {
        match self {
            FluidClass::Watery => "watery",
            FluidClass::Medium => "medium",
            FluidClass::Oily => "oily",
            FluidClass::Immiscible => "immiscible",
        }
    }
}

/// The four consolidation states of the Emulsify model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// `fork -> host`: low-viscosity material can be directly incorporated.
    Mix,
    /// `fork -> adapter -> host`: compatible, but requires an intermediate
    /// consolidation transformation.
    Emulsify,
    /// `fork <=> host`: valuable but divergent; keep independently
    /// maintained behind an interface boundary.
    Suspend,
    /// `fork ⛔ host`: the architectures are effectively immiscible.
    Reject,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Mix => "mix",
            Action::Emulsify => "emulsify",
            Action::Suspend => "suspend",
            Action::Reject => "reject",
        }
    }
}
