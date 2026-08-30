//! Pairwise similarity and weighted compatibility computation.

use crate::config::Config;
use crate::extract::Inventory;
use crate::model::{Compatibility, DimensionId, DimensionScores};
use std::collections::BTreeSet;

/// Jaccard similarity of two sets: |A ∩ B| / |A ∪ B|.
pub fn jaccard<T: Ord>(a: &BTreeSet<T>, b: &BTreeSet<T>) -> f64 {
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    let inter = a.intersection(b).count();
    inter as f64 / union as f64
}

/// Jaccard similarity that reports "unmeasurable" when neither side
/// exhibits the property at all: absence of signal on both sides says
/// nothing about compatibility. One-sided absence is a real signal
/// (the fork shares none of this material) and scores zero.
fn jaccard_option<T: Ord>(a: &BTreeSet<T>, b: &BTreeSet<T>) -> Option<f64> {
    if a.is_empty() && b.is_empty() {
        None
    } else {
        Some(jaccard(a, b))
    }
}

/// Weighted compatibility of `fork` against `host`.
///
/// Dimensions that cannot be measured for this pair are excluded and
/// the remaining weights renormalised, so a fork without tests is not
/// punished for the absence.
pub fn compatibility(host: &Inventory, fork: &Inventory, config: &Config) -> Compatibility {
    let mut scores: DimensionScores = DimensionScores::new();

    scores.insert(
        DimensionId::Api,
        jaccard_option(&host.symbols, &fork.symbols),
    );
    scores.insert(
        DimensionId::Types,
        jaccard_option(&host.type_names, &fork.type_names),
    );
    scores.insert(
        DimensionId::Dependencies,
        jaccard_option(&host.dependencies, &fork.dependencies),
    );
    scores.insert(
        DimensionId::Architecture,
        jaccard_option(&host.modules, &fork.modules),
    );
    scores.insert(
        DimensionId::Behaviour,
        jaccard_option(&host.functions, &fork.functions),
    );
    scores.insert(DimensionId::Tests, test_similarity(host, fork));
    scores.insert(
        DimensionId::Ownership,
        jaccard_option(&host.domains, &fork.domains),
    );
    scores.insert(DimensionId::Lineage, lineage_similarity(host, fork));
    scores.insert(DimensionId::Abstraction, abstraction_similarity(host, fork));

    let mut weighted = 0.0;
    let mut measured = 0.0;
    for id in DimensionId::ALL {
        if let Some(score) = scores[&id] {
            let w = config.weight(id);
            weighted += score * w;
            measured += w;
        }
    }
    let overall = if measured > 0.0 {
        weighted / measured
    } else {
        0.0
    };
    Compatibility {
        overall,
        dimensions: scores,
        measured_weight: measured,
    }
}

fn test_similarity(host: &Inventory, fork: &Inventory) -> Option<f64> {
    jaccard_option(&host.test_paths, &fork.test_paths)
}

/// Shared-ancestry proxy without consulting git: files that exist at
/// the same relative path with identical content. Unchanged overlap
/// over the union of both file sets.
fn lineage_similarity(host: &Inventory, fork: &Inventory) -> Option<f64> {
    if host.paths.is_empty() && fork.paths.is_empty() {
        return None;
    }
    let shared: usize = host
        .content_hashes
        .iter()
        .filter(|(p, h)| fork.content_hashes.get(*p) == Some(h))
        .count();
    let union = host.paths.union(&fork.paths).count();
    if union == 0 {
        return None;
    }
    Some(shared as f64 / union as f64)
}

fn abstraction_similarity(host: &Inventory, fork: &Inventory) -> Option<f64> {
    jaccard_option(&host.abstractions, &fork.abstractions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DimensionId;

    #[test]
    fn jaccard_basics() {
        let a: BTreeSet<u32> = [1, 2, 3].into_iter().collect();
        let b: BTreeSet<u32> = [1, 2, 4].into_iter().collect();
        assert!((jaccard(&a, &b) - 0.5).abs() < 1e-9);
        let empty: BTreeSet<u32> = BTreeSet::new();
        assert_eq!(jaccard(&empty, &empty), 0.0);
        assert_eq!(jaccard(&a, &a), 1.0);
    }

    fn inv_with(fns: &[&str], types: &[&str], deps: &[&str]) -> Inventory {
        Inventory {
            symbols: fns.iter().chain(types).map(|s| s.to_string()).collect(),
            functions: fns.iter().map(|s| s.to_string()).collect(),
            type_names: types.iter().map(|s| s.to_string()).collect(),
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            modules: ["src/core", "tests"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            domains: ["core", "tests"].iter().map(|s| s.to_string()).collect(),
            paths: ["src/core/lib.rs", "tests/lib.rs"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            content_hashes: [
                ("src/core/lib.rs".to_string(), "h1".to_string()),
                ("tests/lib.rs".to_string(), "h2".to_string()),
            ]
            .into_iter()
            .collect(),
            ..Inventory::default()
        }
    }

    #[test]
    fn unmeasurable_dimensions_are_renormalised() {
        let host = inv_with(&["load", "save"], &["Store"], &["serde"]);
        let fork = inv_with(&["load"], &["Store"], &["serde"]);
        let config = Config::default();
        let c = compatibility(&host, &fork, &config);
        // tests + abstraction absent on both sides -> excluded
        assert!(c.dimensions[&DimensionId::Tests].is_none());
        assert!(c.dimensions[&DimensionId::Abstraction].is_none());
        assert!((c.measured_weight - 0.80).abs() < 1e-9);
        // api: {load,save,Store} vs {load,Store} -> 2/3
        assert!((c.dimensions[&DimensionId::Api].unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert!(c.overall > 0.0 && c.overall <= 1.0);
    }

    #[test]
    fn identical_material_is_fully_compatible() {
        let host = inv_with(&["a"], &["T"], &["d"]);
        let fork = inv_with(&["a"], &["T"], &["d"]);
        let c = compatibility(&host, &fork, &Config::default());
        assert!((c.overall - 1.0).abs() < 1e-9);
        assert!((c.viscosity() - 0.0).abs() < 1e-9);
    }
}
