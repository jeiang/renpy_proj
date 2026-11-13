use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

// An exception requires review of a script that emits no link directives.
const NON_LINKING_BUILD_SCRIPTS: &[&str] = &[];

#[test]
fn workspace_dependency_rules() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--all-features",
            "--locked",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).expect("cargo metadata JSON");
    let packages: HashMap<&str, &Value> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p))
        .collect();
    let nodes: HashMap<&str, &Value> = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p))
        .collect();
    let mut errors = Vec::new();
    for member in metadata["workspace_members"].as_array().unwrap() {
        let id = member.as_str().unwrap();
        let package = packages[id];
        let name = package["name"].as_str().unwrap();
        let allows_python = name.starts_with("py-") || matches!(name, "urp-pyhost" | "urp-host");
        if !allows_python {
            let mut pending = vec![id];
            let mut seen = HashSet::new();
            while let Some(dep) = pending.pop() {
                if !seen.insert(dep) {
                    continue;
                }
                let dep_name = packages[dep]["name"].as_str().unwrap();
                if dep_name == "pyo3" || dep_name.starts_with("pyo3-") {
                    errors.push(format!("{name} has forbidden dependency {dep_name}"));
                    break;
                }
                pending.extend(
                    nodes[dep]["dependencies"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap()),
                );
            }
        }
        let has_build_script = package["targets"].as_array().unwrap().iter().any(|t| {
            t["kind"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k == "custom-build")
        });
        if has_build_script && name != "urp-pyhost" && !NON_LINKING_BUILD_SCRIPTS.contains(&name) {
            errors.push(format!(
                "{name} has a forbidden build script; only urp-pyhost may emit link directives"
            ));
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
