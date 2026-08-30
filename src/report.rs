//! Human and JSON rendering of analyses and plans.

use crate::config::Config;
use crate::consolidate::{ConsolidationPlan, PairwiseMatrix};
use crate::extract::Inventory;
use crate::model::{Action, Compatibility, DimensionId, FluidClass};
use serde::Serialize;
use std::collections::BTreeMap;

fn fmt2(x: f64) -> String {
    format!("{x:.2}")
}

fn bar(score: f64) -> String {
    let width = 20;
    let filled = ((score * width as f64).round() as usize).min(width);
    format!("{}{}", "█".repeat(filled), "·".repeat(width - filled))
}

fn action_text(action: Action) -> &'static str {
    match action {
        Action::Mix => "mix",
        Action::Emulsify => "emulsify (adapter)",
        Action::Suspend => "suspend (boundary)",
        Action::Reject => "reject",
    }
}

/// One fork's result for the `analyze` command.
pub struct ForkAnalysis<'a> {
    pub inventory: &'a Inventory,
    pub compatibility: &'a Compatibility,
    pub class: FluidClass,
    pub action: Action,
}

/// `emulsify analyze`: summary table plus per-fork dimension breakdown.
pub fn analyze_text(host: &Inventory, forks: &[ForkAnalysis<'_>], config: &Config) -> String {
    let mut out = String::new();
    out.push_str("Emulsify viscosity analysis\n");
    out.push_str(&format!(
        "host: {} ({}) — {} files",
        host.name,
        host.root.display(),
        host.file_count
    ));
    if !host.languages.is_empty() {
        let langs: Vec<String> = host
            .languages
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        out.push_str(&format!(" [{}]", langs.join(", ")));
    }
    out.push_str("\n\n");

    let name_w = forks
        .iter()
        .map(|f| f.inventory.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    out.push_str(&format!(
        "{:<name_w$}  compat  visc   class       action\n",
        "fork",
        name_w = name_w
    ));
    for f in forks {
        out.push_str(&format!(
            "{:<name_w$}  {:>6}  {:>5}  {:<10}  {}\n",
            f.inventory.name,
            fmt2(f.compatibility.overall),
            fmt2(f.compatibility.viscosity()),
            f.class.label(),
            action_text(f.action),
            name_w = name_w
        ));
    }

    for f in forks {
        out.push_str(&format!(
            "\n{} — compatibility {} / viscosity {} vs {}\n",
            f.inventory.name,
            fmt2(f.compatibility.overall),
            fmt2(f.compatibility.viscosity()),
            host.name
        ));
        out.push_str(&dimension_text(f.compatibility, config));
    }
    out
}

/// Per-fork dimension breakdown lines.
pub fn dimension_text(detail: &Compatibility, config: &Config) -> String {
    let mut out = String::new();
    for id in DimensionId::ALL {
        match detail.dimensions[&id] {
            Some(score) => out.push_str(&format!(
                "  {:<24} {:>5}  {}  (weight {})\n",
                id.key(),
                fmt2(score),
                bar(score),
                fmt2(config.weight(id))
            )),
            None => out.push_str(&format!("  {:<24}   n/a  (not measurable)\n", id.key())),
        }
    }
    out
}

/// `emulsify plan`: ordered consolidation steps.
pub fn plan_text(plan: &ConsolidationPlan) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Consolidation plan for {} — nearest-neighbour bind order\n\n",
        plan.host
    ));
    for v in &plan.verdicts {
        let bind = match (v.action, &v.consolidation_point) {
            (Action::Mix, Some(point)) if point == &plan.host => {
                format!("{} -> {}", v.fork, plan.host)
            }
            (Action::Mix | Action::Emulsify, Some(point)) => {
                format!("{} -> {} via {}", v.fork, plan.host, point)
            }
            (Action::Suspend, Some(point)) => {
                format!("{} <=> {} boundary (nearest: {})", v.fork, plan.host, point)
            }
            _ => v.fork.clone(),
        };
        let note = match v.action {
            Action::Mix => "direct bind",
            Action::Emulsify => "consolidation adapter",
            Action::Suspend => "keep independent, establish interface",
            Action::Reject => "immiscible — do not merge directly",
        };
        let bind_compat = v
            .bind_compatibility
            .map(fmt2)
            .unwrap_or_else(|| "-".to_string());
        out.push_str(&format!(
            "{:>2}. {:<8} {:<46} compat {:>4}  {}\n",
            v.step,
            v.action.label(),
            bind,
            bind_compat,
            note
        ));
    }
    if !plan.suspension_groups.is_empty() {
        out.push_str("\nsuspension groups (consolidate together, apart from the host):\n");
        for group in &plan.suspension_groups {
            out.push_str(&format!("  [{}]\n", group.join(", ")));
        }
    }
    out
}

/// `emulsify graph`: compatibility matrix.
pub fn graph_text(matrix: &PairwiseMatrix) -> String {
    let mut out = String::from("compatibility matrix\n");
    let first_w = matrix
        .names
        .iter()
        .map(|n| n.len())
        .max()
        .unwrap_or(4)
        .max(4)
        + 2;
    out.push_str(&format!("{:<width$}", "", width = first_w));
    for name in &matrix.names {
        out.push_str(&format!("{:>8}", truncate(name, 8)));
    }
    out.push('\n');
    for (i, name) in matrix.names.iter().enumerate() {
        out.push_str(&format!(
            "{:<width$}",
            truncate(name, first_w - 1),
            width = first_w
        ));
        for j in 0..matrix.names.len() {
            if i == j {
                out.push_str(&format!("{:>8}", "-"));
            } else {
                out.push_str(&format!("{:>8}", fmt2(matrix.scores[i][j])));
            }
        }
        out.push('\n');
    }
    out
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

// ---- JSON views ----

#[derive(Serialize)]
pub struct AnalysisJson<'a> {
    pub host: CodebaseJson<'a>,
    pub forks: Vec<ForkJson<'a>>,
}

#[derive(Serialize)]
pub struct CodebaseJson<'a> {
    pub name: &'a str,
    pub path: String,
    pub files: usize,
    pub languages: &'a BTreeMap<String, usize>,
}

#[derive(Serialize)]
pub struct ForkJson<'a> {
    pub name: &'a str,
    pub path: String,
    pub compatibility: f64,
    pub viscosity: f64,
    pub fluid_class: &'static str,
    pub action: &'static str,
    pub dimensions: Vec<DimensionJson>,
}

#[derive(Serialize)]
pub struct DimensionJson {
    pub dimension: &'static str,
    pub score: Option<f64>,
    pub weight: f64,
}

pub fn analyze_json<'a>(
    host: &'a Inventory,
    forks: &'a [ForkAnalysis<'a>],
    config: &Config,
) -> AnalysisJson<'a> {
    AnalysisJson {
        host: CodebaseJson {
            name: &host.name,
            path: host.root.display().to_string(),
            files: host.file_count,
            languages: &host.languages,
        },
        forks: forks
            .iter()
            .map(|f| ForkJson {
                name: &f.inventory.name,
                path: f.inventory.root.display().to_string(),
                compatibility: f.compatibility.overall,
                viscosity: f.compatibility.viscosity(),
                fluid_class: f.class.label(),
                action: f.action.label(),
                dimensions: DimensionId::ALL
                    .iter()
                    .map(|&id| DimensionJson {
                        dimension: id.key(),
                        score: f.compatibility.dimensions.get(&id).copied().flatten(),
                        weight: config.weight(id),
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[derive(Serialize)]
pub struct PlanJson<'a> {
    pub host: &'a str,
    pub steps: Vec<StepJson<'a>>,
    pub suspension_groups: &'a [Vec<String>],
}

#[derive(Serialize)]
pub struct StepJson<'a> {
    pub step: usize,
    pub action: &'static str,
    pub fork: &'a str,
    pub binds_to: Option<&'a str>,
    pub consolidation_point: Option<&'a str>,
    pub host_compatibility: f64,
    pub bind_compatibility: Option<f64>,
}

pub fn plan_json(plan: &ConsolidationPlan) -> PlanJson<'_> {
    PlanJson {
        host: &plan.host,
        steps: plan
            .verdicts
            .iter()
            .map(|v| StepJson {
                step: v.step,
                action: v.action.label(),
                fork: &v.fork,
                binds_to: v.binds_to.as_deref(),
                consolidation_point: v.consolidation_point.as_deref(),
                host_compatibility: v.compatibility,
                bind_compatibility: v.bind_compatibility,
            })
            .collect(),
        suspension_groups: &plan.suspension_groups,
    }
}

#[derive(Serialize)]
pub struct GraphJson<'a> {
    pub names: &'a [String],
    pub matrix: Vec<Vec<f64>>,
}

pub fn graph_json(matrix: &PairwiseMatrix) -> GraphJson<'_> {
    GraphJson {
        names: &matrix.names,
        matrix: matrix.scores.clone(),
    }
}
