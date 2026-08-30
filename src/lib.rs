//! Emulsify — viscosity-based merge compatibility and nearest-neighbour
//! consolidation.
//!
//! Conventional merging treats adjacency in git history as
//! mergeability. Emulsify instead measures the *material character* of
//! each fork — how compatible its architecture, abstractions,
//! dependencies and behaviour are with the host — and consolidates
//! forks at their nearest compatible architectural state:
//!
//! measure → classify → find neighbours → consolidate → validate → bind

pub mod classify;
pub mod cli;
pub mod config;
pub mod consolidate;
pub mod extract;
pub mod model;
pub mod report;
pub mod similarity;
