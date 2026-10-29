//! The rules that keep modules independent of each other and of the master (docs/modules.md, "Rules").

use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// The names of the crates a `Cargo.toml` depends on (all sections, as it is written one dependency per line).
fn dependencies(manifest: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(manifest).unwrap();
    let mut in_dependencies = false;
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_dependencies = line.contains("dependencies");
        } else if in_dependencies {
            if let Some((name, _)) = line.split_once('=') {
                names.push(name.trim().to_string());
            }
        }
    }
    names
}

fn module_services() -> Vec<PathBuf> {
    let modules = workspace().join("modules");
    let found: Vec<PathBuf> = std::fs::read_dir(modules)
        .unwrap()
        .flatten()
        .map(|m| m.path().join("service/Cargo.toml"))
        .filter(|p| p.is_file())
        .collect();
    assert!(!found.is_empty(), "there is at least the video module");
    found
}

#[test]
fn a_module_depends_on_the_contract_and_the_shared_helpers_only() {
    const ALLOWED: [&str; 2] = ["fafnirx-module-api", "fafnirx-common"];
    for manifest in module_services() {
        for name in dependencies(&manifest).into_iter().filter(|n| n.starts_with("fafnirx-")) {
            assert!(
                ALLOWED.contains(&name.as_str()),
                "{} depends on {name}: a module may use {ALLOWED:?} only, never the master or another module",
                manifest.display()
            );
        }
    }
}

#[test]
fn the_master_reaches_modules_over_http_not_through_a_crate() {
    const ALLOWED: [&str; 3] = ["fafnirx-module-api", "fafnirx-common", "fafnirx-db"];
    let master = workspace().join("crates/master/Cargo.toml");
    for name in dependencies(&master).into_iter().filter(|n| n.starts_with("fafnirx-")) {
        assert!(ALLOWED.contains(&name.as_str()), "the master depends on {name}");
    }
}

#[test]
fn the_web_app_and_the_contract_name_the_same_sdk_version() {
    let runtime =
        std::fs::read_to_string(workspace().join("frontend/src/sdk/runtime.ts")).expect("the web app's runtime");
    let line = runtime.lines().find(|l| l.starts_with("export const SDK = ")).expect("runtime.ts says which SDK it is");
    let version: u32 = line.trim_start_matches("export const SDK = ").trim().parse().expect("a number");
    assert_eq!(version, fafnirx_module_api::WEB_SDK, "frontend/src/sdk/runtime.ts and module-api's WEB_SDK differ");
}
