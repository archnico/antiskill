mod support;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = support::unique_temp_path(&format!("antiskill-doctor-cli-{label}"));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn command(workspace: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(args)
        .current_dir(workspace)
        .output()
        .unwrap()
}

fn doctor(workspace: &Path) -> Output {
    command(workspace, &["doctor"])
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn pgn(name: &str) -> Vec<u8> {
    format!(
        "[Event \"{name}\"]\n[White \"Alice\"]\n[Black \"Bob\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n"
    )
    .into_bytes()
}

fn initialized(label: &str) -> (TestDirectory, PathBuf) {
    let temporary = TestDirectory::new(label);
    let workspace = temporary.0.join("workspace");
    let output = command(&temporary.0, &["init", workspace.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    (temporary, workspace)
}

fn ingested(label: &str, files: &[(&str, &[u8])]) -> (TestDirectory, PathBuf) {
    let temporary = TestDirectory::new(label);
    let workspace = temporary.0.join("workspace");
    let input = temporary.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    for (relative, bytes) in files {
        let path = input.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .arg("ingest")
        .arg(&input)
        .current_dir(&workspace)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    (temporary, workspace)
}

fn legacy_ingested(label: &str, files: &[(&str, &[u8])]) -> (TestDirectory, PathBuf) {
    let temporary = TestDirectory::new(label);
    let workspace = temporary.0.join("workspace");
    let input = temporary.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    for (relative, bytes) in files {
        let path = input.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root", ".", "--input"])
        .arg(&input)
        .current_dir(&workspace)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    (temporary, workspace)
}

#[derive(Debug, PartialEq, Eq)]
enum TreeEntry {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
}

fn tree(root: &Path) -> BTreeMap<String, TreeEntry> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<String, TreeEntry>) {
        let mut entries = fs::read_dir(directory)
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            let kind = entry.file_type().unwrap();
            if kind.is_symlink() {
                result.insert(relative, TreeEntry::Symlink(fs::read_link(path).unwrap()));
            } else if kind.is_dir() {
                result.insert(relative, TreeEntry::Directory);
                visit(root, &path, result);
            } else if kind.is_file() {
                result.insert(relative, TreeEntry::File(fs::read(path).unwrap()));
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[derive(Deserialize, Serialize)]
struct WorkspaceState {
    schema_version: u32,
    workspace_state_profile: String,
    active_collection: ActiveCollection,
}

#[derive(Deserialize, Serialize)]
struct ActiveCollection {
    path: String,
    blake3: String,
    bytes: u64,
}

fn state_path(workspace: &Path) -> PathBuf {
    workspace.join("inspector/workspace-state.json")
}

fn read_state(workspace: &Path) -> WorkspaceState {
    serde_json::from_slice(&fs::read(state_path(workspace)).unwrap()).unwrap()
}

fn write_state(workspace: &Path, state: &WorkspaceState) {
    let mut bytes = serde_json::to_vec_pretty(state).unwrap();
    bytes.push(b'\n');
    fs::write(state_path(workspace), bytes).unwrap();
}

fn active_manifest(workspace: &Path) -> (PathBuf, Value) {
    let state = read_state(workspace);
    let path = workspace.join(&state.active_collection.path);
    let mut logical = Vec::new();
    if state.active_collection.path.ends_with(".xz") {
        XzDecoder::new(fs::File::open(&path).unwrap())
            .read_to_end(&mut logical)
            .unwrap();
    } else {
        logical = fs::read(&path).unwrap();
    }
    (path, serde_json::from_slice(&logical).unwrap())
}

#[derive(Deserialize, Serialize)]
struct Observation {
    canonical_path: String,
    input_relative_path: String,
}

#[derive(Deserialize, Serialize)]
struct SourceV1 {
    id: u64,
    source_blake3: String,
    source_bytes: u64,
    catalog: String,
    catalog_blake3: String,
    catalog_bytes: u64,
    games: u64,
    observations: Vec<Observation>,
}

#[derive(Deserialize, Serialize)]
struct ManifestV1 {
    schema_version: u32,
    collection_profile: String,
    sources: Vec<SourceV1>,
}

fn install_v1(workspace: &Path, v2: Value) {
    let sources = v2["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| SourceV1 {
            id: source["id"].as_u64().unwrap(),
            source_blake3: source["source_blake3"].as_str().unwrap().into(),
            source_bytes: source["source_bytes"].as_u64().unwrap(),
            catalog: source["catalog"].as_str().unwrap().into(),
            catalog_blake3: source["catalog_blake3"].as_str().unwrap().into(),
            catalog_bytes: source["catalog_bytes"].as_u64().unwrap(),
            games: source["games"].as_u64().unwrap(),
            observations: serde_json::from_value(source["observations"].clone()).unwrap(),
        })
        .collect();
    let manifest = ManifestV1 {
        schema_version: 1,
        collection_profile: "antiskill_workspace_collection_v1".into(),
        sources,
    };
    let mut logical = serde_json::to_vec_pretty(&manifest).unwrap();
    logical.push(b'\n');
    let hash = blake3::hash(&logical).to_hex().to_string();
    let relative = format!("inspector/collections/{hash}.json.xz");
    let mut encoder = XzEncoder::new(Vec::new(), 6);
    encoder.write_all(&logical).unwrap();
    fs::write(workspace.join(&relative), encoder.finish().unwrap()).unwrap();
    write_state(
        workspace,
        &WorkspaceState {
            schema_version: 1,
            workspace_state_profile: "antiskill_workspace_state_v1".into(),
            active_collection: ActiveCollection {
                path: relative,
                blake3: hash,
                bytes: logical.len() as u64,
            },
        },
    );
}

#[test]
fn help_and_argument_contract_are_product_facing() {
    let temporary = TestDirectory::new("help");
    let top = command(&temporary.0, &["--help"]);
    assert!(top.status.success());
    let top = stdout(&top);
    let init = top.find("    init         ").unwrap();
    let doctor = top.find("    doctor       ").unwrap();
    let ingest = top.find("    ingest       ").unwrap();
    assert!(init < doctor && doctor < ingest);
    assert!(top.contains("antiskill doctor --help"));

    for argument in ["--help", "-h", "help"] {
        let output = command(&temporary.0, &["doctor", argument]);
        assert!(output.status.success());
        assert!(stdout(&output).contains("antiskill doctor"));
        assert!(stdout(&output).contains("read-only"));
    }
    for args in [vec!["doctor", "unexpected"], vec!["doctor", "one", "two"]] {
        let output = command(&temporary.0, &args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(stderr(&output).contains("ERRORE doctor:"));
        assert!(stderr(&output).contains("antiskill doctor"));
    }
}

#[test]
fn fresh_init_and_nonempty_legacy_workspace_without_state_are_healthy() {
    let (_temporary, workspace) = initialized("fresh");
    let fresh = doctor(&workspace);
    assert!(fresh.status.success(), "{}", stderr(&fresh));
    assert_eq!(
        stdout(&fresh),
        "Workspace OK.\nNessuno snapshot attivo.\nProssimo passo: antiskill ingest DIR\n"
    );

    fs::create_dir(workspace.join("inspector/collections")).unwrap();
    fs::write(
        workspace.join("inspector/collections/orphan-corrupt.json"),
        b"not a collection",
    )
    .unwrap();
    fs::write(workspace.join("inspector/legacy-artifact"), b"legacy").unwrap();
    let legacy = doctor(&workspace);
    assert!(legacy.status.success(), "{}", stderr(&legacy));
    assert_eq!(stdout(&legacy), stdout(&fresh));
}

#[test]
fn invalid_inspector_shapes_fail_without_creating_a_workspace() {
    let missing = TestDirectory::new("missing-inspector");
    let missing_result = doctor(&missing.0);
    assert!(!missing_result.status.success());
    assert!(missing_result.stdout.is_empty());
    assert!(stderr(&missing_result).contains("antiskill init DIR"));
    assert!(!missing.0.join("inspector").exists());

    let file = TestDirectory::new("file-inspector");
    fs::write(file.0.join("inspector"), b"file").unwrap();
    let file_result = doctor(&file.0);
    assert!(!file_result.status.success());
    assert!(stderr(&file_result).contains("non è una directory reale"));

    #[cfg(unix)]
    {
        let link = TestDirectory::new("link-inspector");
        fs::create_dir(link.0.join("real-inspector")).unwrap();
        std::os::unix::fs::symlink("real-inspector", link.0.join("inspector")).unwrap();
        let link_result = doctor(&link.0);
        assert!(!link_result.status.success());
        assert!(stderr(&link_result).contains("non è una directory reale"));
    }
}

#[test]
fn workspace_state_symlink_malformed_schema_and_profile_fail_closed() {
    #[cfg(unix)]
    {
        let (_temporary, workspace) = initialized("state-link");
        fs::write(workspace.join("target-state"), b"keep").unwrap();
        std::os::unix::fs::symlink("../target-state", state_path(&workspace)).unwrap();
        let output = doctor(&workspace);
        assert!(!output.status.success());
        assert!(stderr(&output).contains("Stato workspace corrotto"));
    }

    let (_malformed_temp, malformed) = initialized("state-malformed");
    fs::write(state_path(&malformed), b"not json").unwrap();
    let malformed_result = doctor(&malformed);
    assert!(!malformed_result.status.success());
    assert!(stderr(&malformed_result).contains("Stato workspace corrotto"));

    for (label, schema, profile, expected) in [
        (
            "state-schema",
            2,
            "antiskill_workspace_state_v1",
            "Versione stato workspace non supportata",
        ),
        (
            "state-profile",
            1,
            "unsupported",
            "Profilo stato workspace non supportato",
        ),
    ] {
        let (_temporary, workspace) = initialized(label);
        write_state(
            &workspace,
            &WorkspaceState {
                schema_version: schema,
                workspace_state_profile: profile.into(),
                active_collection: ActiveCollection {
                    path: format!("inspector/collections/{}.json.xz", "a".repeat(64)),
                    blake3: "a".repeat(64),
                    bytes: 1,
                },
            },
        );
        let output = doctor(&workspace);
        assert!(!output.status.success());
        assert!(stderr(&output).contains(expected));
    }
}

#[test]
fn missing_corrupt_and_state_mismatched_collection_fail_closed() {
    let bytes = pgn("active");

    let (_missing_temp, missing) = ingested("collection-missing", &[("a.pgn", &bytes)]);
    let (active, _) = active_manifest(&missing);
    fs::remove_file(active).unwrap();
    let missing_result = doctor(&missing);
    assert!(!missing_result.status.success());
    assert!(stderr(&missing_result).contains("Active Collection non verificabile"));

    let (_corrupt_temp, corrupt) = ingested("collection-corrupt", &[("a.pgn", &bytes)]);
    let (active, _) = active_manifest(&corrupt);
    fs::write(active, b"corrupt").unwrap();
    let corrupt_result = doctor(&corrupt);
    assert!(!corrupt_result.status.success());
    assert!(stderr(&corrupt_result).contains("Active Collection non verificabile"));

    let (_mismatch_temp, mismatch) = ingested("collection-mismatch", &[("a.pgn", &bytes)]);
    let mut state = read_state(&mismatch);
    state.active_collection.bytes += 1;
    write_state(&mismatch, &state);
    let mismatch_result = doctor(&mismatch);
    assert!(!mismatch_result.status.success());
    assert!(stderr(&mismatch_result).contains("identità diversa dallo stato"));
}

#[test]
fn broken_dependency_in_active_chain_fails_closed() {
    let bytes = pgn("active");
    let (_temporary, workspace) = legacy_ingested("broken-chain", &[("a.pgn", &bytes)]);
    let (_, manifest) = active_manifest(&workspace);
    for field in ["catalog", "occurrences", "mainlines"] {
        let relative = manifest["sources"][0][field].as_str().unwrap();
        let path = workspace.join(relative);
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"corrupt dependency").unwrap();
        let output = doctor(&workspace);
        assert!(!output.status.success(), "field {field}");
        assert!(output.stdout.is_empty(), "field {field}");
        assert!(
            stderr(&output).contains("Active Collection non verificabile"),
            "field {field}"
        );
        fs::write(path, original).unwrap();
    }
}

#[test]
fn v3_source_index_and_source_blob_corruption_fail_closed() {
    let bytes = pgn("active-v3");
    let (_temporary, workspace) = ingested("broken-v3", &[("a.pgn", &bytes)]);
    let (_, manifest) = active_manifest(&workspace);
    assert_eq!(manifest["schema_version"].as_u64(), Some(3));
    let source = &manifest["sources"][0];

    let index_path = workspace.join(source["source_index"].as_str().unwrap());
    let index_bytes = fs::read(&index_path).unwrap();
    fs::write(&index_path, b"corrupt source index").unwrap();
    let broken_index = doctor(&workspace);
    assert!(!broken_index.status.success());
    assert!(broken_index.stdout.is_empty());
    assert!(stderr(&broken_index).contains("Active Collection non verificabile"));
    fs::write(&index_path, index_bytes).unwrap();

    let hash = source["source_blake3"].as_str().unwrap();
    let object_path = workspace
        .join("raw/objects/blake3")
        .join(&hash[..2])
        .join(hash);
    let object_bytes = fs::read(&object_path).unwrap();
    fs::write(&object_path, b"corrupt source blob").unwrap();
    let broken_blob = doctor(&workspace);
    assert!(!broken_blob.status.success());
    assert!(broken_blob.stdout.is_empty());
    assert!(stderr(&broken_blob).contains("Active Collection non verificabile"));
    fs::write(object_path, object_bytes).unwrap();
}

#[test]
fn healthy_v1_and_v2_report_verified_counts_with_correct_pluralization() {
    let two_games = b"[Event \"A\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n[Event \"B\"]\n[Result \"0-1\"]\n\n1. d4 d5 0-1\n";
    let (_v2_temp, v2) = legacy_ingested("healthy-v2", &[("two-games.pgn", two_games)]);
    let v2_result = doctor(&v2);
    assert!(v2_result.status.success(), "{}", stderr(&v2_result));
    assert_eq!(
        stdout(&v2_result),
        "Workspace OK.\nSnapshot attivo verificato: 1 sorgente · 2 partite.\n"
    );

    let second = pgn("second-source");
    let (_v1_temp, v1) = legacy_ingested(
        "healthy-v1",
        &[("two-games.pgn", two_games), ("second.pgn", &second)],
    );
    let (_, manifest) = active_manifest(&v1);
    install_v1(&v1, manifest);
    let v1_result = doctor(&v1);
    assert!(v1_result.status.success(), "{}", stderr(&v1_result));
    assert_eq!(
        stdout(&v1_result),
        "Workspace OK.\nSnapshot attivo verificato: 2 sorgenti · 3 partite.\n"
    );
    let success = stdout(&v1_result).to_lowercase();
    for forbidden in ["blake3", "profile", "inspector/", "catalog", "bytes"] {
        assert!(!success.contains(forbidden));
    }
}

#[test]
fn doctor_is_workspace_tree_and_byte_exact_read_only() {
    let a = pgn("a");
    let b = pgn("b");
    let (_temporary, workspace) = ingested("readonly", &[("a.pgn", &a), ("nested/b.pgn", &b)]);
    #[cfg(unix)]
    std::os::unix::fs::symlink("collections", workspace.join("inspector/kept-link")).unwrap();
    let before = tree(&workspace);
    let output = doctor(&workspace);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(tree(&workspace), before);
}
