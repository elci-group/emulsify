//! Command-line interface and command orchestration.

use crate::classify::classify;
use crate::config::{Config, EXAMPLE_CONFIG};
use crate::consolidate::{self, ConsolidationPlan};
use crate::extract::Inventory;
use crate::model::Compatibility;
use crate::report::{self, ForkAnalysis};
use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "emulsify",
    version,
    about = "Viscosity-based merge compatibility analysis and nearest-neighbour consolidation planning",
    long_about = "Emulsify treats forks as fluids ranked by viscosity and consolidates them at \
their nearest compatible architectural state — preventing an aggressive mix \
of watery and oily code.\n\n\
A fork is scored across nine dimensions (API, types, dependencies, \
architecture, behaviour, tests, ownership, lineage, abstraction) and \
classified into one of four states:\n  \
mix       fork -> host (direct bind)\n  \
emulsify  fork -> adapter -> host (consolidation)\n  \
suspend   fork <=> host (interface boundary)\n  \
reject    fork ⛔ host (immiscible)"
)]
struct Cli {
    /// Emit machine-readable JSON instead of text
    #[arg(long, global = true)]
    json: bool,

    /// Path to emulsify.toml (defaults are used when absent)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Measure the viscosity of each fork against the host
    Analyze {
        /// Host codebase (the merge target)
        host: PathBuf,
        /// Fork codebases to measure
        #[arg(required = true)]
        forks: Vec<PathBuf>,
    },
    /// Produce an ordered nearest-neighbour consolidation plan
    Plan {
        /// Host codebase (the merge target)
        host: PathBuf,
        /// Fork codebases to consolidate
        #[arg(required = true)]
        forks: Vec<PathBuf>,
    },
    /// Print the full pairwise compatibility matrix
    Graph {
        /// Host codebase (the merge target)
        host: PathBuf,
        /// Fork codebases to compare
        #[arg(required = true)]
        forks: Vec<PathBuf>,
    },
    /// Analyze bound-snapshot/v1 JSON files instead of directories
    AnalyzeSnapshots {
        /// Host snapshot (the merge target)
        host: PathBuf,
        /// Fork snapshots to measure
        #[arg(required = true)]
        forks: Vec<PathBuf>,
    },
    /// Plan consolidation from bound-snapshot/v1 JSON files
    PlanSnapshots {
        /// Host snapshot (the merge target)
        host: PathBuf,
        /// Fork snapshots to consolidate
        #[arg(required = true)]
        forks: Vec<PathBuf>,
    },
    /// Print compatibility matrix from bound-snapshot/v1 JSON files
    GraphSnapshots {
        /// Host snapshot (the merge target)
        host: PathBuf,
        /// Fork snapshots to compare
        #[arg(required = true)]
        forks: Vec<PathBuf>,
    },
    /// Write an example emulsify.toml in the current directory
    Init {
        /// Overwrite an existing emulsify.toml
        #[arg(long)]
        force: bool,
    },
}

pub fn run(argv: Vec<String>) -> ExitCode {
    let cli = Cli::parse_from(argv);
    match execute(&cli) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("emulsify: {err}");
            ExitCode::FAILURE
        }
    }
}

fn execute(cli: &Cli) -> Result<String, String> {
    match &cli.command {
        Command::Init { force } => init(*force),
        Command::Analyze { host, forks }
        | Command::Plan { host, forks }
        | Command::Graph { host, forks } => {
            let config = load_config(cli.config.as_deref())?;
            let (host_inv, fork_invs) = extract_all(host, forks, &config)?;
            run_matrix_command(&cli.command, &host_inv, &fork_invs, &config, cli.json)
        }
        Command::AnalyzeSnapshots { host, forks }
        | Command::PlanSnapshots { host, forks }
        | Command::GraphSnapshots { host, forks } => {
            let config = load_config(cli.config.as_deref())?;
            let (host_inv, fork_invs) = extract_all_snapshots(host, forks)?;
            run_matrix_command(&cli.command, &host_inv, &fork_invs, &config, cli.json)
        }
    }
}

fn run_matrix_command(
    command: &Command,
    host_inv: &Inventory,
    fork_invs: &[Inventory],
    config: &Config,
    json: bool,
) -> Result<String, String> {
    let matrix = consolidate::pairwise(host_inv, fork_invs, config);
    match command {
        Command::Analyze { .. } | Command::AnalyzeSnapshots { .. } => {
            let analyses = fork_analyses(fork_invs, &matrix.host_detail, config);
            if json {
                Ok(
                    serde_json::to_string_pretty(&report::analyze_json(
                        host_inv, &analyses, config,
                    ))
                    .map_err(|e| e.to_string())?,
                )
            } else {
                Ok(report::analyze_text(host_inv, &analyses, config))
            }
        }
        Command::Plan { .. } | Command::PlanSnapshots { .. } => {
            let plan: ConsolidationPlan = consolidate::plan(&matrix, config);
            if json {
                Ok(serde_json::to_string_pretty(&report::plan_json(&plan))
                    .map_err(|e| e.to_string())?)
            } else {
                Ok(report::plan_text(&plan))
            }
        }
        Command::Graph { .. } | Command::GraphSnapshots { .. } => {
            if json {
                Ok(serde_json::to_string_pretty(&report::graph_json(&matrix))
                    .map_err(|e| e.to_string())?)
            } else {
                Ok(report::graph_text(&matrix))
            }
        }
        Command::Init { .. } => unreachable!(),
    }
}

fn init(force: bool) -> Result<String, String> {
    let path = Path::new("emulsify.toml");
    if path.exists() && !force {
        return Err("emulsify.toml already exists (use --force to overwrite)".into());
    }
    std::fs::write(path, EXAMPLE_CONFIG).map_err(|e| format!("write emulsify.toml: {e}"))?;
    Ok("wrote emulsify.toml\n".to_string())
}

fn load_config(explicit: Option<&Path>) -> Result<Config, String> {
    match explicit {
        Some(path) => Config::load(path),
        None => {
            let default = Path::new("emulsify.toml");
            if default.exists() {
                Config::load(default)
            } else {
                Ok(Config::default())
            }
        }
    }
}

/// Extract host and forks, giving every participant a unique display name.
fn extract_all(
    host: &Path,
    forks: &[PathBuf],
    config: &Config,
) -> Result<(Inventory, Vec<Inventory>), String> {
    let mut host_inv = Inventory::extract(host, config)?;
    host_inv.name = "host".to_string();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    seen.insert(host_inv.name.clone(), 1);
    let mut fork_invs = Vec::with_capacity(forks.len());
    for fork in forks {
        let mut inv = Inventory::extract(fork, config)?;
        dedupe_name(&mut inv, &mut seen);
        fork_invs.push(inv);
    }
    Ok((host_inv, fork_invs))
}

fn load_snapshot(path: &Path) -> Result<bound_core::Snapshot, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("cannot open snapshot {}: {e}", path.display()))?;
    serde_json::from_reader(file)
        .map_err(|e| format!("invalid snapshot JSON {}: {e}", path.display()))
}

/// Build host and fork inventories from bound-snapshot/v1 JSON files.
fn extract_all_snapshots(
    host: &Path,
    forks: &[PathBuf],
) -> Result<(Inventory, Vec<Inventory>), String> {
    let host_snapshot = load_snapshot(host)?;
    let mut host_inv = Inventory::from_snapshot(&host_snapshot);
    host_inv.name = "host".to_string();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    seen.insert(host_inv.name.clone(), 1);
    let mut fork_invs = Vec::with_capacity(forks.len());
    for fork in forks {
        let snapshot = load_snapshot(fork)?;
        let mut inv = Inventory::from_snapshot(&snapshot);
        dedupe_name(&mut inv, &mut seen);
        fork_invs.push(inv);
    }
    Ok((host_inv, fork_invs))
}

fn dedupe_name(inv: &mut Inventory, seen: &mut BTreeMap<String, usize>) {
    let base = inv.name.clone();
    let count = seen.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count > 1 {
        inv.name = format!("{base}-{count}");
    }
}

fn fork_analyses<'a>(
    forks: &'a [Inventory],
    details: &'a [Compatibility],
    config: &Config,
) -> Vec<ForkAnalysis<'a>> {
    forks
        .iter()
        .zip(details)
        .map(|(inv, compat)| {
            let (class, action) = classify(compat.overall, &config.thresholds);
            ForkAnalysis {
                inventory: inv,
                compatibility: compat,
                class,
                action,
            }
        })
        .collect()
}
