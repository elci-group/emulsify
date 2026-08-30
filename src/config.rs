//! Configuration: dimension weights and compatibility thresholds.
//!
//! Loaded from `emulsify.toml` when present; every field has a default
//! so an absent file is fine. Weights are normalised at load time, so
//! users can express relative importance without summing to 1.0.

use crate::model::DimensionId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Thresholds {
    /// Compatibility at or above this binds directly (`mix`).
    pub mix: f64,
    /// Compatibility at or above this binds through a consolidation
    /// adapter (`emulsify`).
    pub emulsify: f64,
    /// Compatibility at or above this suspends the fork behind an
    /// interface boundary. Below it the fork is rejected as immiscible.
    pub suspend: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            mix: 0.75,
            emulsify: 0.50,
            suspend: 0.30,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub weights: BTreeMap<String, f64>,
    pub thresholds: Thresholds,
    /// Directory names never descended into.
    pub ignore_dirs: Vec<String>,
    /// Files larger than this many bytes are counted but not hashed or scanned.
    pub max_scan_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        let mut weights = BTreeMap::new();
        for (id, w) in DimensionId::ALL.iter().zip(DEFAULT_WEIGHTS) {
            weights.insert(id.key().to_string(), w);
        }
        Config {
            weights,
            thresholds: Thresholds::default(),
            ignore_dirs: DEFAULT_IGNORE_DIRS.iter().map(|s| s.to_string()).collect(),
            max_scan_bytes: 1_048_576,
        }
    }
}

/// Default relative importance of the nine dimensions, in the order of
/// `DimensionId::ALL`. Sums to 1.0.
const DEFAULT_WEIGHTS: [f64; 9] = [
    0.15, // api
    0.10, // types
    0.15, // dependencies
    0.15, // architecture
    0.10, // behaviour
    0.10, // tests
    0.05, // ownership
    0.10, // lineage
    0.10, // abstraction
];

const DEFAULT_IGNORE_DIRS: [&str; 12] = [
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
    ".next",
    "vendor",
];

impl Config {
    /// Load configuration from `path`, falling back to defaults for
    /// anything the file does not specify.
    pub fn load(path: &Path) -> Result<Config, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read config {}: {e}", path.display()))?;
        let file: ConfigFile = toml::from_str(&raw).map_err(|e| format!("parse error: {e}"))?;
        let mut config = Config::default();
        if let Some(w) = file.weights {
            for (k, v) in w {
                if DimensionId::ALL.iter().any(|d| d.key() == k) {
                    config.weights.insert(k, v);
                } else {
                    return Err(format!("unknown weight dimension `{k}`"));
                }
            }
        }
        if let Some(t) = file.thresholds {
            if let Some(v) = t.mix {
                config.thresholds.mix = v;
            }
            if let Some(v) = t.emulsify {
                config.thresholds.emulsify = v;
            }
            if let Some(v) = t.suspend {
                config.thresholds.suspend = v;
            }
        }
        if let Some(d) = file.ignore_dirs {
            config.ignore_dirs = d;
        }
        if let Some(v) = file.max_scan_bytes {
            config.max_scan_bytes = v;
        }
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        let t = &self.thresholds;
        if !(0.0..=1.0).contains(&t.mix)
            || !(0.0..=1.0).contains(&t.emulsify)
            || !(0.0..=1.0).contains(&t.suspend)
        {
            return Err("thresholds must lie in [0, 1]".into());
        }
        if !(t.suspend..=t.mix).contains(&t.emulsify)
            || t.emulsify > t.mix
            || t.suspend > t.emulsify
        {
            return Err("thresholds must satisfy suspend <= emulsify <= mix".into());
        }
        let total: f64 = self.weights.values().sum();
        if total <= 0.0 {
            return Err("dimension weights must sum to a positive value".into());
        }
        for (k, v) in &self.weights {
            if *v < 0.0 {
                return Err(format!("weight for `{k}` must be non-negative"));
            }
        }
        Ok(())
    }

    /// Weight for a dimension after normalising the total to 1.0.
    pub fn weight(&self, id: DimensionId) -> f64 {
        let total: f64 = self.weights.values().sum();
        if total <= 0.0 {
            return 0.0;
        }
        self.weights.get(id.key()).copied().unwrap_or(0.0) / total
    }

    /// Directory names to skip during extraction.
    pub fn ignored(&self, name: &str) -> bool {
        self.ignore_dirs.iter().any(|d| d == name)
    }
}

#[derive(Deserialize, Default)]
struct ConfigFile {
    weights: Option<BTreeMap<String, f64>>,
    thresholds: Option<ThresholdsFile>,
    ignore_dirs: Option<Vec<String>>,
    max_scan_bytes: Option<u64>,
}

#[derive(Deserialize, Default)]
struct ThresholdsFile {
    mix: Option<f64>,
    emulsify: Option<f64>,
    suspend: Option<f64>,
}

/// Example configuration written by `emulsify init`.
pub const EXAMPLE_CONFIG: &str = r#"# Emulsify configuration.
# Weights are relative and normalised automatically; only listed
# dimensions need values. Thresholds are compatibility bands:
#   >= mix       -> mix          (direct bind)
#   >= emulsify  -> emulsify     (bind via consolidation adapter)
#   >= suspend   -> suspend      (keep independent behind an interface)
#   <  suspend   -> reject       (immiscible)

[weights]
api = 0.15
types = 0.10
dependencies = 0.15
architecture = 0.15
behaviour = 0.10
tests = 0.10
ownership = 0.05
lineage = 0.10
abstraction = 0.10

[thresholds]
mix = 0.75
emulsify = 0.50
suspend = 0.30
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("emulsify.toml");
        std::fs::write(&path, EXAMPLE_CONFIG).unwrap();
        let config = Config::load(&path).unwrap();
        assert!((config.thresholds.mix - 0.75).abs() < 1e-9);
        assert!((config.thresholds.emulsify - 0.50).abs() < 1e-9);
        assert!((config.weight(DimensionId::Api) - 0.15).abs() < 1e-9);
    }

    #[test]
    fn weights_merge_with_defaults_and_normalise() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("emulsify.toml");
        // Listed dimensions override defaults; the rest keep theirs.
        std::fs::write(&path, "[weights]\napi = 2\ntypes = 1\n").unwrap();
        let config = Config::load(&path).unwrap();
        let total = 2.0 + 1.0 + 0.75; // remaining seven defaults sum to 0.75
        assert!((config.weight(DimensionId::Api) - 2.0 / total).abs() < 1e-9);
        assert!((config.weight(DimensionId::Types) - 1.0 / total).abs() < 1e-9);
        assert!(config.weight(DimensionId::Api) > 0.5);
        let sum: f64 = DimensionId::ALL.iter().map(|d| config.weight(*d)).sum();
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn misordered_thresholds_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("emulsify.toml");
        std::fs::write(&path, "[thresholds]\nmix = 0.5\nemulsify = 0.9\n").unwrap();
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn unknown_dimension_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("emulsify.toml");
        std::fs::write(&path, "[weights]\nvibes = 1.0\n").unwrap();
        assert!(Config::load(&path).is_err());
    }
}
