//! End-to-end tests: run the compiled `emulsify` binary against
//! fixture codebases spanning the full viscosity range.

use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_emulsify"))
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A small Rust service: the host and the seeds of its forks.
fn seed_service(root: &Path, name: &str) {
    write(
        &root.join("Cargo.toml"),
        &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1\"\nclap = \"4\"\n"),
    );
    write(
        &root.join("src/main.rs"),
        "mod api;\nmod storage;\n\nfn main() {\n    let store = storage::Store::new();\n    api::serve(store);\n}\n",
    );
    write(
        &root.join("src/api/mod.rs"),
        "pub fn serve(store: crate::storage::Store) {\n    handle(&store);\n}\n\nfn handle(s: &crate::storage::Store) {\n    s.load();\n}\n",
    );
    write(
        &root.join("src/storage/mod.rs"),
        "pub struct Store { path: String }\n\nimpl Store {\n    pub fn new() -> Store { Store { path: \"/tmp/db\".into() } }\n    pub fn load(&self) -> Option<String> { None }\n    pub fn save(&self, _v: &str) {}\n}\n",
    );
    write(&root.join("tests/api.rs"), "#[test]\nfn serves() {}\n");
}

fn fixtures() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let host = root.join("host");
    seed_service(&host, "host");

    // Watery fork: the host plus one compatible addition.
    let watery = root.join("fork-watery");
    seed_service(&watery, "fork-watery");
    write(&root.join("fork-watery/Cargo.toml"), "[package]\nname = \"fork-watery\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1\"\nclap = \"4\"\nsha2 = \"0.10\"\n");
    write(
        &watery.join("src/storage/hash.rs"),
        "impl crate::storage::Store {\n    pub fn digest(&self, v: &str) -> String { v.to_string() }\n}\n",
    );

    // Oily fork: same dependencies, wholly different architecture.
    let oily = root.join("fork-oily");
    write(
        &oily.join("Cargo.toml"),
        "[package]\nname = \"fork-oily\"\nversion = \"0.2.0\"\n\n[dependencies]\nserde = \"1\"\nclap = \"4\"\n",
    );
    write(
        &oily.join("src/main.rs"),
        "mod commands;\nmod persistence;\n\nfn main() {\n    let repo = persistence::Repository::open();\n    commands::dispatch(repo);\n}\n",
    );
    write(
        &oily.join("src/commands/mod.rs"),
        "pub trait Command { fn run(&self); }\n\npub struct Serve;\n\nimpl Command for Serve {\n    fn run(&self) { dispatch_inner(); }\n}\n\nfn dispatch_inner() {}\npub fn dispatch(r: crate::persistence::Repository) { r.fetch(); }\n",
    );
    write(
        &oily.join("src/persistence/mod.rs"),
        "pub trait RepositoryBackend { fn fetch(&self) -> String; }\n\npub struct Repository;\n\nimpl Repository {\n    pub fn open() -> Repository { Repository }\n    pub fn fetch(&self) -> String { String::new() }\n}\n",
    );
    write(
        &oily.join("tests/commands.rs"),
        "#[test]\nfn dispatches() {}\n",
    );

    (dir, host, watery, oily)
}

fn run(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(bin())
        .args(args)
        .output()
        .expect("spawn emulsify");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn analyze_ranks_watery_above_oily() {
    let (_dir, host, watery, oily) = fixtures();
    let (ok, out, err) = run(&[
        "analyze",
        host.to_str().unwrap(),
        watery.to_str().unwrap(),
        oily.to_str().unwrap(),
    ]);
    assert!(ok, "emulsify failed: {err}");
    let watery_line = out.lines().find(|l| l.starts_with("fork-watery")).unwrap();
    let oily_line = out.lines().find(|l| l.starts_with("fork-oily")).unwrap();
    assert!(watery_line.contains("watery"), "{watery_line}");
    assert!(watery_line.contains("mix"), "{watery_line}");
    assert!(
        oily_line.contains("immiscible") || oily_line.contains("oily"),
        "{oily_line}"
    );
}

#[test]
fn plan_binds_watery_fork_first() {
    let (_dir, host, watery, oily) = fixtures();
    let (ok, out, err) = run(&[
        "plan",
        host.to_str().unwrap(),
        watery.to_str().unwrap(),
        oily.to_str().unwrap(),
    ]);
    assert!(ok, "emulsify failed: {err}");
    let first_step = out
        .lines()
        .find(|l| l.trim_start().starts_with("1."))
        .expect("plan has a first step");
    assert!(first_step.contains("fork-watery"), "{first_step}");
    assert!(first_step.contains("mix"), "{first_step}");
    assert!(out.contains("-> host"), "{out}");
}

#[test]
fn plan_json_is_well_formed() {
    let (_dir, host, watery, oily) = fixtures();
    let (ok, out, err) = run(&[
        "--json",
        "plan",
        host.to_str().unwrap(),
        watery.to_str().unwrap(),
        oily.to_str().unwrap(),
    ]);
    assert!(ok, "emulsify failed: {err}");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    let steps = value["steps"].as_array().unwrap();
    assert_eq!(steps[0]["fork"].as_str().unwrap(), "fork-watery");
    assert_eq!(steps[0]["action"].as_str().unwrap(), "mix");
    assert!(value["host"].is_string());
}

#[test]
fn analyze_json_carries_dimensions() {
    let (_dir, host, watery, _oily) = fixtures();
    let (ok, out, err) = run(&[
        "--json",
        "analyze",
        host.to_str().unwrap(),
        watery.to_str().unwrap(),
    ]);
    assert!(ok, "emulsify failed: {err}");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    let fork = &value["forks"][0];
    let dims = fork["dimensions"].as_array().unwrap();
    assert_eq!(dims.len(), 9);
    assert!(fork["viscosity"].as_f64().unwrap() < 0.5);
}

#[test]
fn graph_prints_a_matrix() {
    let (_dir, host, watery, oily) = fixtures();
    let (ok, out, err) = run(&[
        "graph",
        host.to_str().unwrap(),
        watery.to_str().unwrap(),
        oily.to_str().unwrap(),
    ]);
    assert!(ok, "emulsify failed: {err}");
    assert!(out.contains("compatibility matrix"));
    assert!(out.contains("host"));
}

#[test]
fn thresholds_from_config_change_the_verdict() {
    let (_dir, host, watery, _oily) = fixtures();
    // Raise the mix bar above the watery fork: it must now bind
    // through a consolidation adapter instead of directly.
    let cfg_dir = TempDir::new().unwrap();
    let cfg = cfg_dir.path().join("emulsify.toml");
    std::fs::write(
        &cfg,
        "[thresholds]\nmix = 0.99\nemulsify = 0.5\nsuspend = 0.3\n",
    )
    .unwrap();
    let (ok, out, err) = run(&[
        "--config",
        cfg.to_str().unwrap(),
        "plan",
        host.to_str().unwrap(),
        watery.to_str().unwrap(),
    ]);
    assert!(ok, "emulsify failed: {err}");
    assert!(out.contains("emulsify"), "{out}");
    assert!(!out.contains("mix      fork-watery"), "{out}");
}

#[test]
fn init_writes_config_and_refuses_to_clobber() {
    let dir = TempDir::new().unwrap();
    let output = Command::new(bin())
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(dir.path().join("emulsify.toml").exists());
    let again = Command::new(bin())
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!again.status.success());
    let forced = Command::new(bin())
        .args(["init", "--force"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(forced.status.success());
}

#[test]
fn missing_host_fails_cleanly() {
    let (ok, _out, err) = run(&["analyze", "/nonexistent/path", "/also/missing"]);
    assert!(!ok);
    assert!(err.contains("emulsify:"), "{err}");
}
