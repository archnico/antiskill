mod support;

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = support::unique_temp_path(&format!("antiskill-coverage-cli-{label}"));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn command(current: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(args)
        .current_dir(current)
        .output()
        .unwrap()
}

fn init_workspace(parent: &Path) -> PathBuf {
    let workspace = parent.join("workspace");
    let output = command(parent, &["init", workspace.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    workspace
}

fn store_annotations(parent: &Path, workspace: &Path, group_id: &str) -> String {
    let input = parent.join(format!("{group_id}.json"));
    fs::write(
        &input,
        format!(
            r#"{{"schema_version":1,"annotation_profile":"antiskill_annotation_group_v1","group_id":"{group_id}","source_id":"source:book","snapshot_id":"snapshot:abc","rights":"private_research","visibility":"private","items":[{{"item_id":"a1","target_kind":"position","target_id":"position:1","kind":"text","payload":"x"}},{{"item_id":"a2","target_kind":"game","target_id":"game:1","kind":"nag","payload":"!"}}]}}"#
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-annotations", "--root"])
        .arg(workspace)
        .arg("--input")
        .arg(input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("ANNOTATION_PATH\t"))
        .unwrap()
        .to_owned()
}

#[test]
fn coverage_counts_only_verified_attributed_annotation_items() {
    let directory = TestDirectory::new("annotations");
    let workspace = init_workspace(&directory.0);
    store_annotations(&directory.0, &workspace, "group:one");
    store_annotations(&directory.0, &workspace, "group:two");

    let output = command(&workspace, &["coverage", "--format", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let annotations = report["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dimension| dimension["dimension"] == "annotations")
        .unwrap();
    assert_eq!(annotations["status"], "observed_only");
    assert_eq!(annotations["observed"], 4);
    assert!(annotations["expected"].is_null());
    assert!(annotations["ratio"].is_null());
}

#[test]
fn coverage_fails_closed_on_corrupt_annotation_artifact() {
    let directory = TestDirectory::new("corrupt-annotations");
    let workspace = init_workspace(&directory.0);
    let path = store_annotations(&directory.0, &workspace, "group:corrupt");
    fs::write(workspace.join(path), b"not-xz").unwrap();

    let output = command(&workspace, &["coverage", "--format", "json"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ERRORE coverage:"));
}
