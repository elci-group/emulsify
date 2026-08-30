//! Nearest-neighbour consolidation planning.
//!
//! The planner never asks "can git resolve this?" It asks "what is the
//! nearest *compatible* state each fork can bind to?" Compatibility is
//! single-linkage: a fork binds to a consolidated cluster through its
//! most compatible member, so a later fork may consolidate onto an
//! earlier fork's state rather than onto the raw host.
//!
//! Procedure (measure → classify → find neighbours → consolidate):
//!
//! 1. Bind rounds: repeatedly take the globally most-compatible
//!    (unbound fork, cluster) pair. If that compatibility clears the
//!    emulsify threshold, bind the fork into the cluster (directly if
//!    it clears the mix threshold, through a consolidation adapter
//!    otherwise) and continue.
//! 2. Whatever remains cannot bind: each fork is classified against its
//!    nearest cluster — oily forks suspend behind an interface
//!    boundary, immiscible forks are rejected.
//! 3. Suspended forks that are highly compatible with each other are
//!    grouped: they should be consolidated with each other, just not
//!    with the host.

use crate::classify::classify;
use crate::config::Config;
use crate::extract::Inventory;
use crate::model::{Action, Compatibility, FluidClass};
use crate::similarity::compatibility;
use serde::Serialize;

/// Overall pairwise compatibility between every participating codebase,
/// `names[i]` is the host when `i == 0`.
pub struct PairwiseMatrix {
    pub names: Vec<String>,
    pub scores: Vec<Vec<f64>>,
    /// Full dimension breakdown of each fork against the host.
    pub host_detail: Vec<Compatibility>,
}

pub fn pairwise(host: &Inventory, forks: &[Inventory], config: &Config) -> PairwiseMatrix {
    let mut names = vec![host.name.clone()];
    names.extend(forks.iter().map(|f| f.name.clone()));
    let n = names.len();
    let mut scores = vec![vec![0.0; n]; n];
    let mut host_detail = Vec::with_capacity(forks.len());
    for i in 0..n {
        for j in (i + 1)..n {
            let (a, b) = if i == 0 {
                (host, &forks[j - 1])
            } else {
                (&forks[i - 1], &forks[j - 1])
            };
            let c = compatibility(a, b, config).overall;
            scores[i][j] = c;
            scores[j][i] = c;
        }
    }
    for f in forks {
        host_detail.push(compatibility(host, f, config));
    }
    PairwiseMatrix {
        names,
        scores,
        host_detail,
    }
}

/// What the planner decided for one fork, in bind order.
#[derive(Clone, Debug, Serialize)]
pub struct Verdict {
    pub fork: String,
    /// Overall compatibility against the host root.
    pub compatibility: f64,
    pub viscosity: f64,
    pub fluid_class: FluidClass,
    pub action: Action,
    /// Cluster the fork binds into (the host cluster grows monotonically).
    pub binds_to: Option<String>,
    /// The member of that cluster the fork is actually nearest to:
    /// its consolidation point.
    pub consolidation_point: Option<String>,
    /// Compatibility at the consolidation point (may exceed the
    /// host-root compatibility).
    pub bind_compatibility: Option<f64>,
    /// 1-based position in the consolidation order.
    pub step: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConsolidationPlan {
    pub host: String,
    /// Verdicts in the order they should be consolidated.
    pub verdicts: Vec<Verdict>,
    /// Groups of suspended forks that are compatible with each other
    /// and should be consolidated together, away from the host.
    pub suspension_groups: Vec<Vec<String>>,
}

pub fn plan(matrix: &PairwiseMatrix, config: &Config) -> ConsolidationPlan {
    let n = matrix.names.len();
    let host_name = matrix.names[0].clone();
    // Cluster membership: node 0 (host) plus bound forks, in bind order.
    let mut cluster: Vec<usize> = vec![0];
    let mut bound: Vec<usize> = Vec::new();
    let mut unassigned: Vec<usize> = (1..n).collect();
    let mut verdicts: Vec<Verdict> = Vec::new();

    // Phase 1: bind the globally best (fork, cluster) pair each round.
    loop {
        let mut best: Option<(usize, usize, f64)> = None; // (fork, member, compat)
        for &f in &unassigned {
            for &m in &cluster {
                let c = matrix.scores[f][m];
                let better = match best {
                    None => true,
                    Some((_, _, bc)) => {
                        c > bc + f64::EPSILON
                            || (c > bc - f64::EPSILON
                                && matrix.names[f] < matrix.names[best.unwrap().0])
                    }
                };
                if better {
                    best = Some((f, m, c));
                }
            }
        }
        let Some((f, member, c)) = best else { break };
        if c < config.thresholds.emulsify {
            break;
        }
        let (_, action) = classify(c, &config.thresholds);
        verdicts.push(Verdict {
            fork: matrix.names[f].clone(),
            compatibility: matrix.scores[f][0],
            viscosity: 1.0 - matrix.scores[f][0],
            fluid_class: classify(matrix.scores[f][0], &config.thresholds).0,
            action,
            binds_to: Some(host_name.clone()),
            consolidation_point: Some(matrix.names[member].clone()),
            bind_compatibility: Some(c),
            step: verdicts.len() + 1,
        });
        cluster.push(f);
        bound.push(f);
        unassigned.retain(|&x| x != f);
    }

    // Phase 2: classify the remainder against its nearest cluster member.
    let mut suspended: Vec<usize> = Vec::new();
    for &f in &unassigned {
        let (member, c) = cluster
            .iter()
            .copied()
            .map(|m| (m, matrix.scores[f][m]))
            .max_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| matrix.names[b.0].cmp(&matrix.names[a.0]))
            })
            .expect("cluster always contains the host");
        let (_, action) = classify(c, &config.thresholds);
        let is_suspend = action == Action::Suspend;
        if is_suspend {
            suspended.push(f);
        }
        verdicts.push(Verdict {
            fork: matrix.names[f].clone(),
            compatibility: matrix.scores[f][0],
            viscosity: 1.0 - matrix.scores[f][0],
            fluid_class: classify(matrix.scores[f][0], &config.thresholds).0,
            action,
            binds_to: if is_suspend {
                Some(host_name.clone())
            } else {
                None
            },
            consolidation_point: Some(matrix.names[member].clone()),
            bind_compatibility: Some(c),
            step: verdicts.len() + 1,
        });
    }

    // Phase 3: co-suspend suspended forks that mix well with each other.
    let suspension_groups = group_suspended(&suspended, matrix, config);

    ConsolidationPlan {
        host: host_name,
        verdicts,
        suspension_groups,
    }
}

/// Union suspended forks whose mutual compatibility clears the mix
/// threshold: same material, kept away from the host.
fn group_suspended(
    suspended: &[usize],
    matrix: &PairwiseMatrix,
    config: &Config,
) -> Vec<Vec<String>> {
    let mut parent: Vec<usize> = (0..suspended.len()).collect();

    fn find(parent: &mut Vec<usize>, i: usize) -> usize {
        if parent[i] != i {
            let root = find(parent, parent[i]);
            parent[i] = root;
        }
        parent[i]
    }

    for i in 0..suspended.len() {
        for j in (i + 1)..suspended.len() {
            let c = matrix.scores[suspended[i]][suspended[j]];
            if c >= config.thresholds.mix {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut roots: std::collections::BTreeMap<usize, Vec<String>> = Default::default();
    for (i, &node) in suspended.iter().enumerate() {
        if matrix.names[node].is_empty() {
            continue;
        }
        roots
            .entry(find(&mut parent, i))
            .or_default()
            .push(matrix.names[node].clone());
    }
    for (_, mut members) in roots {
        if members.len() > 1 {
            members.sort();
            groups.push(members);
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::model::Action;

    fn matrix(names: &[&str], scores: Vec<Vec<f64>>) -> PairwiseMatrix {
        PairwiseMatrix {
            names: names.iter().map(|s| s.to_string()).collect(),
            scores,
            host_detail: Vec::new(),
        }
    }

    #[test]
    fn binds_most_compatible_first() {
        //        host  a     b
        let m = matrix(
            &["host", "a", "b"],
            vec![
                vec![0.0, 0.90, 0.80],
                vec![0.90, 0.0, 0.40],
                vec![0.80, 0.40, 0.0],
            ],
        );
        let plan = plan(&m, &Config::default());
        assert_eq!(plan.verdicts[0].fork, "a");
        assert_eq!(plan.verdicts[0].action, Action::Mix);
        assert_eq!(plan.verdicts[1].fork, "b");
        assert_eq!(plan.verdicts[1].action, Action::Mix);
    }

    #[test]
    fn later_fork_binds_through_nearest_earlier_fork() {
        // c is more compatible with a's consolidated state than with raw host
        let m = matrix(
            &["host", "a", "c"],
            vec![
                vec![0.0, 0.90, 0.55],
                vec![0.90, 0.0, 0.95],
                vec![0.55, 0.95, 0.0],
            ],
        );
        let plan = plan(&m, &Config::default());
        let c = &plan.verdicts[1];
        assert_eq!(c.fork, "c");
        assert_eq!(c.action, Action::Mix);
        assert_eq!(c.consolidation_point.as_deref(), Some("a"));
        assert!((c.bind_compatibility.unwrap() - 0.95).abs() < 1e-9);
    }

    #[test]
    fn oily_forks_suspend_and_group_together() {
        let m = matrix(
            &["host", "d", "e"],
            vec![
                vec![0.0, 0.40, 0.40],
                vec![0.40, 0.0, 0.90],
                vec![0.40, 0.90, 0.0],
            ],
        );
        let plan = plan(&m, &Config::default());
        assert_eq!(plan.verdicts.len(), 2);
        assert!(plan.verdicts.iter().all(|v| v.action == Action::Suspend));
        assert_eq!(
            plan.suspension_groups,
            vec![vec!["d".to_string(), "e".to_string()]]
        );
    }

    #[test]
    fn immiscible_forks_are_rejected() {
        let m = matrix(&["host", "f"], vec![vec![0.0, 0.10], vec![0.10, 0.0]]);
        let plan = plan(&m, &Config::default());
        assert_eq!(plan.verdicts[0].action, Action::Reject);
        assert!(plan.verdicts[0].binds_to.is_none());
    }
}
