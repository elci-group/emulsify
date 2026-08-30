//! Codebase inventory extraction.
//!
//! Emulsify measures compatibility between *architectural states*, not
//! commit graphs, so extraction works on plain directory trees: clone
//! or export the host and each fork and point emulsify at them. All
//! extraction is deterministic line-scanning — no parsing fuzz, no
//! network, no process spawning.

use crate::config::Config;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Everything emulsify needs to know about one codebase.
#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub root: PathBuf,
    pub name: String,
    /// Relative POSIX-style paths of scanned files.
    pub paths: BTreeSet<String>,
    /// Source files only, with detected language.
    pub sources: BTreeMap<String, &'static str>,
    /// Named symbols (functions, types, traits, modules, classes...).
    pub symbols: BTreeSet<String>,
    /// struct/enum/class/interface-style type names.
    pub type_names: BTreeSet<String>,
    /// trait/interface-style abstraction names.
    pub abstractions: BTreeSet<String>,
    /// Function names only: the behavioural surface.
    pub functions: BTreeSet<String>,
    /// Declared dependency names from manifests.
    pub dependencies: BTreeSet<String>,
    /// Directory skeleton (depth <= 2) of source files.
    pub modules: BTreeSet<String>,
    /// Top-level source directories: ownership/domain proxy.
    pub domains: BTreeSet<String>,
    /// Relative paths of test files.
    pub test_paths: BTreeSet<String>,
    /// sha256 of every scanned source file, by relative path.
    pub content_hashes: BTreeMap<String, String>,
    pub languages: BTreeMap<String, usize>,
    pub file_count: usize,
}

impl Inventory {
    pub fn extract(root: &Path, config: &Config) -> Result<Inventory, String> {
        let root = root
            .canonicalize()
            .map_err(|e| format!("cannot read {}: {e}", root.display()))?;
        if !root.is_dir() {
            return Err(format!("{} is not a directory", root.display()));
        }
        let mut inv = Inventory {
            name: root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string()),
            root,
            ..Inventory::default()
        };

        let mut files = Vec::new();
        collect_files(&inv.root, &inv.root, config, &mut files, 0)?;
        files.sort();

        for rel in files {
            let abs = inv.root.join(&rel);
            inv.file_count += 1;
            inv.paths.insert(rel.clone());
            let language = detect_language(&rel);
            let Some(lang) = language else { continue };
            let is_test = is_test_path(&rel);
            let meta = fs::metadata(&abs).map_err(|e| format!("stat {}: {e}", abs.display()))?;
            if meta.len() > config.max_scan_bytes {
                *inv.languages.entry(lang.to_string()).or_insert(0) += 1;
                inv.sources.insert(rel, lang);
                continue;
            }
            let text = match fs::read_to_string(&abs) {
                Ok(t) => t,
                Err(_) => {
                    // Binary or non-UTF8 source file: hash it, skip symbols.
                    if let Some(bytes) = read_bytes(&abs) {
                        inv.content_hashes.insert(rel.clone(), hash_hex(&bytes));
                    }
                    continue;
                }
            };
            *inv.languages.entry(lang.to_string()).or_insert(0) += 1;
            inv.sources.insert(rel.clone(), lang);
            inv.content_hashes
                .insert(rel.clone(), hash_hex(text.as_bytes()));
            if is_test {
                inv.test_paths.insert(rel.clone());
            }
            scan_symbols(lang, &text, &mut inv);
            if let Some(manifest) = manifest_deps(&rel, &text) {
                inv.dependencies.extend(manifest);
            }
            if let Some(dir) = parent_dir(&rel) {
                inv.domains.insert(domain_segment(&dir));
                inv.modules.insert(depth_prefix(&dir, 2));
            }
        }
        Ok(inv)
    }

    /// Build an inventory from a `bound-snapshot/v1` produced by bound-core.
    /// Files without content are still recorded, but symbol extraction requires
    /// content to be present in the snapshot.
    pub fn from_snapshot(snapshot: &bound_core::Snapshot) -> Inventory {
        let root = snapshot.target.clone();
        let mut inv = Inventory {
            name: root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "snapshot".to_string()),
            root,
            ..Inventory::default()
        };

        for entry in &snapshot.files {
            let rel = entry.relative_path.replace('\\', "/");
            inv.file_count += 1;
            inv.paths.insert(rel.clone());
            let language = detect_language(&rel);
            let Some(lang) = language else { continue };
            let is_test = is_test_path(&rel);
            inv.sources.insert(rel.clone(), lang);
            *inv.languages.entry(lang.to_string()).or_insert(0) += 1;
            if is_test {
                inv.test_paths.insert(rel.clone());
            }
            if let Some(text) = &entry.content {
                inv.content_hashes
                    .insert(rel.clone(), hash_hex(text.as_bytes()));
                scan_symbols(lang, text, &mut inv);
                if let Some(manifest) = manifest_deps(&rel, text) {
                    inv.dependencies.extend(manifest);
                }
                if let Some(dir) = parent_dir(&rel) {
                    inv.domains.insert(domain_segment(&dir));
                    inv.modules.insert(depth_prefix(&dir, 2));
                }
            }
        }

        inv
    }
}

const MAX_DEPTH: usize = 16;

fn collect_files(
    base: &Path,
    dir: &Path,
    config: &Config,
    out: &mut Vec<String>,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read {}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("stat {}: {e}", path.display()))?;
        if file_type.is_dir() {
            if config.ignored(&name) || name.starts_with('.') {
                continue;
            }
            collect_files(base, &path, config, out, depth + 1)?;
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(base)
                .expect("path is under base")
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

fn detect_language(rel: &str) -> Option<&'static str> {
    let ext = rel.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "tsx" => Some("typescript"),
        "go" => Some("go"),
        "java" => Some("java"),
        "c" | "h" => Some("c"),
        "cc" | "cpp" | "hpp" => Some("cpp"),
        "rb" => Some("ruby"),
        "sh" | "bash" => Some("shell"),
        "toml" => Some("toml"),
        "json" => Some("json"),
        _ => None,
    }
}

fn is_test_path(rel: &str) -> bool {
    let lower = rel.to_lowercase();
    lower.contains("test") || lower.contains("/tests/") || lower.contains("spec")
}

/// Line-scan a source file for named declarations. Deliberately
/// heuristic: emulsify compares *surfaces*, it does not need to bind
/// every name perfectly — it needs the same file to always produce the
/// same surface.
fn scan_symbols(lang: &str, text: &str, inv: &mut Inventory) {
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }
        match lang {
            "rust" => scan_rust(&tokens, inv),
            "python" => scan_python(&tokens, inv),
            "javascript" | "typescript" => scan_js(&tokens, inv),
            "go" => scan_go(&tokens, inv),
            "java" | "cpp" | "c" => scan_c_family(&tokens, inv),
            _ => {}
        }
    }
}

fn clean_name(token: &str) -> Option<String> {
    let name: String = token
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty()
        || name
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(true)
        || matches!(
            name.as_str(),
            "self" | "Self" | "super" | "crate" | "new" | "main" | "impl"
        )
    {
        None
    } else {
        Some(name)
    }
}

fn scan_rust(tokens: &[&str], inv: &mut Inventory) {
    for (i, tok) in tokens.iter().enumerate() {
        let kind = match *tok {
            "fn" => Some(Kind::Function),
            "struct" | "enum" | "union" => Some(Kind::Type),
            "trait" => Some(Kind::Trait),
            "type" => Some(Kind::Type),
            "mod" => Some(Kind::Module),
            "const" | "static" => Some(Kind::Value),
            _ => None,
        };
        let Some(kind) = kind else { continue };
        // `type` and `const` double as ordinary words inside expressions;
        // only trust them in declaration position (statement start, or
        // after visibility/qualifier tokens).
        if matches!(*tok, "type" | "const" | "static") && i > 2 {
            continue;
        }
        let Some(name) = tokens.get(i + 1).and_then(|t| clean_name(t)) else {
            continue;
        };
        inv.symbols.insert(name.clone());
        match kind {
            Kind::Function => {
                inv.functions.insert(name);
            }
            Kind::Type => {
                inv.type_names.insert(name);
            }
            Kind::Trait => {
                inv.type_names.insert(name.clone());
                inv.abstractions.insert(name);
            }
            _ => {}
        }
    }
}

fn scan_python(tokens: &[&str], inv: &mut Inventory) {
    let head = tokens[0];
    let kind = match head {
        "def" | "async" => Kind::Function,
        "class" => Kind::Type,
        _ => return,
    };
    let idx = if head == "async" { 1 } else { 0 };
    if head == "async" && tokens.get(1).map(|t| *t != "def").unwrap_or(true) {
        return;
    }
    let Some(name) = tokens.get(idx + 1).and_then(|t| clean_name(t)) else {
        return;
    };
    inv.symbols.insert(name.clone());
    match kind {
        Kind::Function => {
            inv.functions.insert(name);
        }
        Kind::Type => {
            inv.type_names.insert(name);
        }
        _ => {}
    }
}

fn scan_js(tokens: &[&str], inv: &mut Inventory) {
    for (i, tok) in tokens.iter().enumerate() {
        let kind = match *tok {
            "function" => Some(Kind::Function),
            "class" => Some(Kind::Type),
            "interface" => Some(Kind::Trait),
            "enum" => Some(Kind::Type),
            _ => None,
        };
        let Some(kind) = kind else { continue };
        let Some(name) = tokens.get(i + 1).and_then(|t| clean_name(t)) else {
            continue;
        };
        inv.symbols.insert(name.clone());
        match kind {
            Kind::Function => {
                inv.functions.insert(name);
            }
            Kind::Type => {
                inv.type_names.insert(name);
            }
            Kind::Trait => {
                inv.type_names.insert(name.clone());
                inv.abstractions.insert(name);
            }
            _ => {}
        }
    }
}

fn scan_go(tokens: &[&str], inv: &mut Inventory) {
    // func NAME(...) | func (r T) NAME(...)
    if tokens[0] != "func" {
        scan_go_types(tokens, inv);
        return;
    }
    if let Some(name) = tokens.get(1).and_then(|t| clean_name(t)) {
        inv.symbols.insert(name.clone());
        inv.functions.insert(name);
    }
}

fn scan_go_types(tokens: &[&str], inv: &mut Inventory) {
    if tokens.first() != Some(&"type") {
        return;
    }
    let Some(name) = tokens.get(1).and_then(|t| clean_name(t)) else {
        return;
    };
    inv.symbols.insert(name.clone());
    if tokens.get(2) == Some(&"struct") {
        inv.type_names.insert(name);
    } else if tokens.get(2) == Some(&"interface") {
        inv.type_names.insert(name.clone());
        inv.abstractions.insert(name);
    }
}

fn scan_c_family(tokens: &[&str], inv: &mut Inventory) {
    for (i, tok) in tokens.iter().enumerate() {
        let kind = match *tok {
            "class" => Some(Kind::Type),
            "struct" => Some(Kind::Type),
            "interface" => Some(Kind::Trait),
            "enum" => Some(Kind::Type),
            _ => None,
        };
        let Some(kind) = kind else { continue };
        let Some(name) = tokens.get(i + 1).and_then(|t| clean_name(t)) else {
            continue;
        };
        inv.symbols.insert(name.clone());
        match kind {
            Kind::Type => {
                inv.type_names.insert(name);
            }
            Kind::Trait => {
                inv.type_names.insert(name.clone());
                inv.abstractions.insert(name);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Function,
    Type,
    Trait,
    Module,
    Value,
}

/// Dependency names for manifest files, by relative path.
fn manifest_deps(rel: &str, text: &str) -> Option<Vec<String>> {
    match rel {
        "Cargo.toml" => Some(cargo_deps(text)),
        "package.json" => Some(npm_deps(text)),
        "pyproject.toml" => Some(pyproject_deps(text)),
        "go.mod" => Some(gomod_deps(text)),
        _ => None,
    }
}

fn cargo_deps(text: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == "[dependencies]"
                || line.starts_with("[dependencies.")
                || line == "[dev-dependencies]"
                || line.starts_with("[dev-dependencies.");
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(eq) = line.find('=') {
            let name = line[..eq].trim();
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                deps.push(name.to_string());
            }
        }
    }
    deps
}

fn npm_deps(text: &str) -> Vec<String> {
    // Lightweight scan: take string keys in the dependencies blocks.
    let mut deps = Vec::new();
    let mut section = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.contains("\"dependencies\"") {
            section = 1;
        } else if line.contains("\"devDependencies\"") {
            section = 1;
        } else if line.starts_with('}') || line.starts_with(']') {
            section = 0;
        } else if section == 1 {
            if let Some(rest) = line.strip_prefix('"') {
                if let Some(end) = rest.find("\":") {
                    deps.push(rest[..end].to_string());
                }
            }
        }
    }
    deps
}

fn pyproject_deps(text: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_deps = false;
            continue;
        }
        if trimmed.starts_with("dependencies") && trimmed.contains('[') {
            in_deps = true;
            continue;
        }
        if in_deps {
            if trimmed.starts_with(']') {
                in_deps = false;
            } else if let Some(stripped) = trimmed.strip_prefix('"') {
                let name = stripped
                    .split(|c: char| {
                        c == '"'
                            || c == ' '
                            || c == '['
                            || c == ';'
                            || c == '<'
                            || c == '>'
                            || c == '='
                            || c == '!'
                            || c == '~'
                    })
                    .next()
                    .unwrap_or("");
                if !name.is_empty() {
                    deps.push(name.to_string());
                }
            }
        }
    }
    deps
}

fn gomod_deps(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("require "))
        .filter_map(|l| l.split_whitespace().next())
        .map(|s| s.to_string())
        .collect()
}

fn read_bytes(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

fn hash_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn parent_dir(rel: &str) -> Option<String> {
    rel.rfind('/').map(|i| rel[..i].to_string())
}

/// Ownership/domain proxy: the meaningful top-level division of a
/// source tree. Conventional roots (`src/`, `lib/`, `app/`, `source/`)
/// are transparent — `src/api/mod.rs` belongs to the `api` domain,
/// while a file directly under `src/` is root-level surface area.
fn domain_segment(dir: &str) -> String {
    let segments: Vec<&str> = dir.split('/').collect();
    const ROOTS: [&str; 4] = ["src", "lib", "app", "source"];
    if segments.len() >= 2 && ROOTS.contains(&segments[0]) {
        return segments[1].to_string();
    }
    if segments.len() == 1 && ROOTS.contains(&segments[0]) {
        return "(root)".to_string();
    }
    segments[0].to_string()
}

fn depth_prefix(dir: &str, depth: usize) -> String {
    dir.split('/').take(depth).collect::<Vec<_>>().join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_symbols_types_and_abstractions() {
        let code = "\
pub struct Store { path: String }
pub trait Repo { fn load(&self) -> Option<String>; }
impl Store {
    pub fn new() -> Store { Store { path: String::new() } }
}
enum Mode { Fast }
fn main() {}
";
        let mut inv = Inventory::default();
        scan_symbols("rust", code, &mut inv);
        assert!(inv.symbols.contains("Store"));
        assert!(inv.symbols.contains("Repo"));
        assert!(inv.symbols.contains("Mode"));
        assert!(inv.symbols.contains("load"));
        assert!(inv.functions.contains("load"));
        assert!(inv.type_names.contains("Store"));
        assert!(inv.type_names.contains("Mode"));
        assert!(inv.abstractions.contains("Repo"));
        // `main` carries no compatibility signal: every crate has one.
        assert!(!inv.symbols.contains("main"));
    }

    #[test]
    fn python_symbols() {
        let code = "\
def index():
    return 1

async def fetch_all():
    pass

class View:
    pass
";
        let mut inv = Inventory::default();
        scan_symbols("python", code, &mut inv);
        assert!(inv.functions.contains("index"));
        assert!(inv.functions.contains("fetch_all"));
        assert!(inv.type_names.contains("View"));
    }

    #[test]
    fn cargo_dependency_parsing() {
        let manifest = "\
[package]
name = \"x\"

[dependencies]
serde = \"1\"
clap = { version = \"4\" }

[dev-dependencies]
tempfile = \"3\"

[build-dependencies]
ignored = \"1\"
";
        assert_eq!(cargo_deps(manifest), vec!["serde", "clap", "tempfile"]);
    }

    #[test]
    fn domains_see_through_conventional_roots() {
        assert_eq!(domain_segment("src/api"), "api");
        assert_eq!(domain_segment("src/api/v1"), "api");
        assert_eq!(domain_segment("src"), "(root)");
        assert_eq!(domain_segment("tests"), "tests");
        assert_eq!(domain_segment("app/views"), "views");
    }

    #[test]
    fn extract_walks_and_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/core")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn run() {}\n").unwrap();
        std::fs::write(root.join("src/core/eng.rs"), "pub struct Engine;\n").unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("target/ignored.rs"), "fn nope() {}\n").unwrap();
        let config = Config::default();
        let inv = Inventory::extract(root, &config).unwrap();
        assert_eq!(inv.file_count, 2);
        assert!(inv.functions.contains("run"));
        assert!(inv.type_names.contains("Engine"));
        assert!(inv.domains.contains("core"));
        assert!(!inv.paths.iter().any(|p| p.contains("target")));
        let h = inv.content_hashes.get("src/main.rs").unwrap();
        assert_eq!(h.len(), 64);
    }
}
