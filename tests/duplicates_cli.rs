mod support;

use serde::Serialize;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = support::unique_temp_path(&format!("antiskill-duplicates-cli-{label}"));
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

fn ingest(workspace: &Path, input: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .arg("ingest")
        .arg(input)
        .current_dir(workspace)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn duplicates(workspace: &Path) -> Output {
    command(workspace, &["duplicates"])
}

fn legacy_ingest(workspace: &Path, input: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root", ".", "--input"])
        .arg(input)
        .current_dir(workspace)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn logical_json(path: &Path) -> Vec<u8> {
    let bytes = fs::read(path).unwrap();
    let mut decoder = xz2::read::XzDecoder::new(bytes.as_slice());
    let mut logical = Vec::new();
    decoder.read_to_end(&mut logical).unwrap();
    logical
}

fn active_collection(workspace: &Path) -> PathBuf {
    let state: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("inspector/workspace-state.json")).unwrap(),
    )
    .unwrap();
    workspace.join(state["active_collection"]["path"].as_str().unwrap())
}

fn write_fixture(input: &Path) {
    let exact = "[Event \"Open\"]\n[Event \"Open Side\"]\n[White \"Alice\"]\n[White \"A. Example\"]\n[Black \"Bob\"]\n[Date \"2026.08.11\"]\n[Result \"1-0\"]\n\n1. e4 {same} e5 1-0\n";
    let variant = "[Event \"Variant\"]\n[White \"Alice\"]\n[Black \"Bob\"]\n[Date \"2026.08.12\"]\n[Result \"0-1\"]\n\n1. e4 {different bytes} e5 0-1\n";
    let exact_only = "[Event \"Exact only\"]\n[White \"Carol\"]\n[Black \"Dave\"]\n[Result \"*\"]\n\n1. d4 d5 *\n";
    let noncomparable = "[Result \"*\"]\n\n1. c4 BadSan *\n";
    fs::write(
        input.join("corpus.pgn"),
        format!(
            "{exact}\n{exact}\n{variant}\n{exact_only}\n{exact_only}\n{noncomparable}\n{noncomparable}"
        ),
    )
    .unwrap();
}

#[test]
fn help_and_arguments_are_product_facing() {
    let directory = TestDirectory::new("help");
    let top = command(&directory.0, &["--help"]);
    let top = stdout(&top);
    assert!(
        top.contains("duplicates   Mostra copie identiche e PGN con la stessa sequenza di mosse")
    );
    assert!(top.contains("antiskill duplicates --help"));

    for argument in ["help", "--help", "-h"] {
        let output = command(&directory.0, &["duplicates", argument]);
        assert!(output.status.success());
        let help = stdout(&output);
        for expected in [
            "workspace corrente",
            "Collection attiva",
            "copie byte-identiche",
            "PGN diversi",
            "solo diagnostico",
            "non cancella",
            "antiskill export",
        ] {
            assert!(help.contains(expected), "manca: {expected}");
        }
    }
    for args in [
        vec!["duplicates", "unexpected"],
        vec!["duplicates", "--format", "jsonl"],
    ] {
        let output = command(&directory.0, &args);
        assert!(!output.status.success());
        assert!(stdout(&output).is_empty());
        assert!(stderr(&output).contains("ERRORE duplicates: argomento inatteso"));
    }
}

#[test]
fn cwd_active_collection_reports_overlap_metadata_noncomparables_and_conflict() {
    let directory = TestDirectory::new("diagnosis");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    write_fixture(&input);
    ingest(&workspace, &input);

    let first = duplicates(&workspace);
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(first.stderr.is_empty());
    let text = stdout(&first);
    assert_eq!(text, stdout(&duplicates(&workspace)));
    assert!(text.starts_with("Duplicati nel corpus attivo\n"));
    assert!(text.contains("Copie byte-identiche: 3 gruppi"));
    assert!(text.contains("Stessa sequenza di mosse, PGN diversi: 1 gruppo"));
    assert!(text.contains("3 occurrence · 2 varianti byte"));
    assert!(text.contains("Non confrontabili per mainline: 2"));
    assert!(text.contains("Risultati discordanti"));
    assert!(text.contains("2026.08.11"));
    assert!(text.contains("Alice | A. Example – Bob"));
    assert!(text.contains("1-0"));
    assert!(text.contains("Open | Open Side"));
    assert!(text.contains("— – —"));

    let ids = text
        .split_whitespace()
        .filter(|value| value.starts_with("g1-"))
        .collect::<Vec<_>>();
    assert_eq!(
        ids.len(),
        9,
        "overlap exact/canonical deve restare visibile"
    );
    assert!(ids.iter().all(|id| {
        id.len() == 67
            && id[3..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }));
    let exact_ids = text
        .split("Stessa sequenza di mosse, PGN diversi:")
        .next()
        .unwrap()
        .split_whitespace()
        .filter(|value| value.starts_with("g1-"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(exact_ids.len(), 6);
    let canonical_ids = text
        .split("Stessa sequenza di mosse, PGN diversi:")
        .nth(1)
        .unwrap()
        .split("Non confrontabili per mainline:")
        .next()
        .unwrap()
        .split_whitespace()
        .filter(|value| value.starts_with("g1-"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        canonical_ids.len(),
        3,
        "il gruppo exact con una sola variante byte non deve comparire qui"
    );

    let source_hash = blake3::hash(&fs::read(input.join("corpus.pgn")).unwrap())
        .to_hex()
        .to_string();
    for forbidden in [
        "source_id",
        "game_id",
        "source_blake3",
        "source_game_blake3",
        "canonical_mainline_id",
        "group_id",
        "catalog",
        "source_start",
        "source_end",
        "segment_ordinal",
        "schema",
        "profile",
        "inspector/",
        &source_hash,
    ] {
        assert!(!text.contains(forbidden), "esposto: {forbidden}");
    }
}

#[test]
fn no_duplicates_is_success() {
    let directory = TestDirectory::new("none");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(
        input.join("one.pgn"),
        b"[Event \"Only\"]\n[White \"Alice\"]\n[Black \"Bob\"]\n[Result \"*\"]\n\n1. d4 d5 *\n",
    )
    .unwrap();
    ingest(&workspace, &input);
    let output = duplicates(&workspace);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "Duplicati nel corpus attivo\n\nCopie byte-identiche: 0 gruppi\n\nStessa sequenza di mosse, PGN diversi: 0 gruppi\n\nNon confrontabili per mainline: 0\n"
    );
}

#[derive(Serialize)]
struct LegacyObservation {
    canonical_path: String,
    input_relative_path: String,
}

#[derive(Serialize)]
struct LegacySource {
    id: u64,
    source_blake3: String,
    source_bytes: u64,
    catalog: String,
    catalog_blake3: String,
    catalog_bytes: u64,
    games: u64,
    observations: Vec<LegacyObservation>,
}

#[derive(Serialize)]
struct LegacyCollection {
    schema_version: u32,
    collection_profile: String,
    sources: Vec<LegacySource>,
}

#[derive(Serialize)]
struct ActiveCollection {
    path: String,
    blake3: String,
    bytes: u64,
}

#[derive(Serialize)]
struct WorkspaceState {
    schema_version: u32,
    workspace_state_profile: String,
    active_collection: ActiveCollection,
}

fn serialize_pretty<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    bytes
}

fn activate_legacy_v1(workspace: &Path) {
    let v2: serde_json::Value =
        serde_json::from_slice(&logical_json(&active_collection(workspace))).unwrap();
    let sources = v2["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| LegacySource {
            id: source["id"].as_u64().unwrap(),
            source_blake3: source["source_blake3"].as_str().unwrap().into(),
            source_bytes: source["source_bytes"].as_u64().unwrap(),
            catalog: source["catalog"].as_str().unwrap().into(),
            catalog_blake3: source["catalog_blake3"].as_str().unwrap().into(),
            catalog_bytes: source["catalog_bytes"].as_u64().unwrap(),
            games: source["games"].as_u64().unwrap(),
            observations: source["observations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|observation| LegacyObservation {
                    canonical_path: observation["canonical_path"].as_str().unwrap().into(),
                    input_relative_path: observation["input_relative_path"]
                        .as_str()
                        .unwrap()
                        .into(),
                })
                .collect(),
        })
        .collect();
    let logical = serialize_pretty(&LegacyCollection {
        schema_version: 1,
        collection_profile: "antiskill_workspace_collection_v1".into(),
        sources,
    });
    let hash = blake3::hash(&logical).to_hex().to_string();
    let relative = format!("inspector/collections/{hash}.json.xz");
    let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
    encoder.write_all(&logical).unwrap();
    fs::write(workspace.join(&relative), encoder.finish().unwrap()).unwrap();
    let state = WorkspaceState {
        schema_version: 1,
        workspace_state_profile: "antiskill_workspace_state_v1".into(),
        active_collection: ActiveCollection {
            path: relative,
            blake3: hash,
            bytes: logical.len() as u64,
        },
    };
    fs::write(
        workspace.join("inspector/workspace-state.json"),
        serialize_pretty(&state),
    )
    .unwrap();
}

#[test]
fn workspace_state_and_collection_fail_closed_including_legacy_v1() {
    let directory = TestDirectory::new("errors");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();

    let missing_state = duplicates(&workspace);
    assert!(!missing_state.status.success());
    assert!(stdout(&missing_state).is_empty());
    assert!(stderr(&missing_state).contains("Stato workspace mancante"));
    assert!(!stderr(&missing_state).to_lowercase().contains("panic"));

    fs::write(
        input.join("one.pgn"),
        b"[Event \"One\"]\n[Result \"*\"]\n\n1. e4 e5 *\n",
    )
    .unwrap();
    ingest(&workspace, &input);
    let state_path = workspace.join("inspector/workspace-state.json");
    let valid_state = fs::read(&state_path).unwrap();
    let collection_path = active_collection(&workspace);
    let valid_collection = fs::read(&collection_path).unwrap();

    fs::write(&state_path, b"{ broken\n").unwrap();
    let corrupt_state = duplicates(&workspace);
    assert!(!corrupt_state.status.success());
    assert!(stderr(&corrupt_state).contains("Stato workspace corrotto"));
    fs::write(&state_path, &valid_state).unwrap();

    fs::remove_file(&collection_path).unwrap();
    let missing_collection = duplicates(&workspace);
    assert!(!missing_collection.status.success());
    assert!(stderr(&missing_collection).contains("Active Collection non verificabile"));
    fs::write(&collection_path, &valid_collection).unwrap();

    fs::write(&collection_path, b"corrupt").unwrap();
    let corrupt_collection = duplicates(&workspace);
    assert!(!corrupt_collection.status.success());
    assert!(stderr(&corrupt_collection).contains("Active Collection non verificabile"));
    fs::write(&collection_path, valid_collection).unwrap();

    legacy_ingest(&workspace, &input);
    activate_legacy_v1(&workspace);
    let legacy = duplicates(&workspace);
    assert!(!legacy.status.success());
    assert!(stdout(&legacy).is_empty());
    assert!(stderr(&legacy).contains("legacy v1"));
    assert!(stderr(&legacy).contains("non supporta l'analisi duplicati prodotto"));
}
