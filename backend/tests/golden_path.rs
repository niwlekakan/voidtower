//! Static acceptance test for the dedicated golden-path integration CI job.
//!
//! The real Docker/App Vault/restic harness lives in
//! `examples/golden_path.rs`. Keeping infrastructure-mutating checks outside
//! Cargo's auto-discovered test targets means ordinary `cargo test` runs stay
//! hermetic without ignoring or silently skipping tests.

use std::path::{Path, PathBuf};

fn ci_yml_path_from(manifest_dir: &Path, github_workspace: Option<&Path>) -> PathBuf {
    let repository_root = github_workspace.unwrap_or_else(|| {
        manifest_dir
            .parent()
            .expect("backend manifest directory has a repository parent")
    });
    repository_root.join(".github/workflows/ci.yml")
}

#[test]
fn golden_path_job_runs_on_pull_request_and_push_to_main() {
    let workspace = std::env::var_os("GITHUB_WORKSPACE");
    let ci_yml_path = ci_yml_path_from(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        workspace.as_deref().map(Path::new),
    );
    let content = std::fs::read_to_string(&ci_yml_path).expect("read ci.yml");
    let doc: serde_yaml::Value = serde_yaml::from_str(&content).expect("parse ci.yml");
    let mapping = doc.as_mapping().expect("ci.yml top level is a mapping");

    // YAML 1.1 parsers may resolve a bare `on:` key to boolean true.
    let on = mapping
        .iter()
        .find_map(|(key, value)| {
            let is_on_key = matches!(key, serde_yaml::Value::String(s) if s == "on")
                || matches!(key, serde_yaml::Value::Bool(true));
            is_on_key.then_some(value)
        })
        .expect("workflow has an `on:` trigger block");
    for trigger in ["push", "pull_request"] {
        let branches = on
            .get(trigger)
            .and_then(|value| value.get("branches"))
            .and_then(serde_yaml::Value::as_sequence)
            .unwrap_or_else(|| panic!("{trigger} trigger missing branches"));
        assert!(
            branches.iter().any(|branch| branch.as_str() == Some("main")),
            "{trigger} trigger must include main"
        );
    }

    let jobs = doc
        .get("jobs")
        .and_then(serde_yaml::Value::as_mapping)
        .expect("workflow has a jobs mapping");
    let golden_path_job = jobs
        .get(serde_yaml::Value::String("golden-path".into()))
        .expect("a `golden-path` job exists in ci.yml");

    assert!(
        golden_path_job.get("if").is_none(),
        "golden-path job must run for every push and pull request"
    );

    let steps = golden_path_job
        .get("steps")
        .and_then(serde_yaml::Value::as_sequence)
        .expect("golden-path job has steps");
    let run_commands = steps
        .iter()
        .filter_map(|step| step.get("run").and_then(serde_yaml::Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        run_commands.contains("docker info") && run_commands.contains("docker compose version"),
        "golden-path job must fail loudly when Docker or Compose is unavailable"
    );
    assert!(
        run_commands.contains("restic version"),
        "golden-path job must fail loudly when restic is unavailable"
    );
    assert!(
        run_commands.contains("cargo run --example golden_path"),
        "golden-path job must invoke the standalone real-infrastructure harness"
    );
}

#[test]
fn github_workspace_wins_over_a_crate_copy_manifest_path() {
    let manifest_dir = Path::new("mutants-copy/backend");
    let workspace = Path::new("checkout");

    assert_eq!(
        ci_yml_path_from(manifest_dir, Some(workspace)),
        workspace.join(".github/workflows/ci.yml")
    );
}
