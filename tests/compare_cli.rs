mod support;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
        let path = support::unique_temp_path(&format!("antiskill-compare-cli-{label}"));
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

fn ingest(workspace: &Path, input: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .arg("ingest")
        .arg(input)
        .current_dir(workspace)
        .output()
        .unwrap()
}

fn compare(workspace: &Path, input: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .arg("compare")
        .arg(input)
        .current_dir(workspace)
        .output()
        .unwrap()
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

fn setup(label: &str, files: &[(&str, &[u8])]) -> (TestDirectory, PathBuf, PathBuf) {
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
    let output = ingest(&workspace, &input);
    assert!(output.status.success(), "{}", stderr(&output));
    (temporary, workspace, input)
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

fn active_manifest(workspace: &Path) -> (PathBuf, Value) {
    let state: Value = serde_json::from_slice(
        &fs::read(workspace.join("inspector/workspace-state.json")).unwrap(),
    )
    .unwrap();
    let relative = state["active_collection"]["path"].as_str().unwrap();
    let path = workspace.join(relative);
    let mut logical = Vec::new();
    if relative.ends_with(".xz") {
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

#[derive(Deserialize, Serialize)]
struct SourceV2 {
    id: u64,
    source_blake3: String,
    source_bytes: u64,
    catalog: String,
    catalog_blake3: String,
    catalog_bytes: u64,
    occurrences: String,
    occurrences_blake3: String,
    occurrences_bytes: u64,
    mainlines: String,
    mainlines_blake3: String,
    mainlines_bytes: u64,
    games: u64,
    observations: Vec<Observation>,
}

#[derive(Deserialize, Serialize)]
struct ManifestV2 {
    schema_version: u32,
    collection_profile: String,
    sources: Vec<SourceV2>,
}

#[derive(Deserialize, Serialize)]
struct SourceV3 {
    id: u64,
    source_blake3: String,
    source_bytes: u64,
    source_index: String,
    source_index_blake3: String,
    source_index_bytes: u64,
    games: u64,
    observations: Vec<Observation>,
}

#[derive(Deserialize, Serialize)]
struct ManifestV3 {
    schema_version: u32,
    collection_profile: String,
    sources: Vec<SourceV3>,
}

#[derive(Serialize)]
struct WorkspaceState<'a> {
    schema_version: u32,
    workspace_state_profile: &'a str,
    active_collection: ActiveCollection<'a>,
}

#[derive(Serialize)]
struct ActiveCollection<'a> {
    path: &'a str,
    blake3: &'a str,
    bytes: u64,
}

fn install_active_manifest(workspace: &Path, value: &Value) {
    let mut logical = match value["schema_version"].as_u64() {
        Some(1) => {
            serde_json::to_vec_pretty(&serde_json::from_value::<ManifestV1>(value.clone()).unwrap())
                .unwrap()
        }
        Some(2) => {
            serde_json::to_vec_pretty(&serde_json::from_value::<ManifestV2>(value.clone()).unwrap())
                .unwrap()
        }
        Some(3) => {
            serde_json::to_vec_pretty(&serde_json::from_value::<ManifestV3>(value.clone()).unwrap())
                .unwrap()
        }
        other => panic!("unsupported test Collection schema: {other:?}"),
    };
    logical.push(b'\n');
    let hash = blake3::hash(&logical).to_hex().to_string();
    let relative = format!("inspector/collections/{hash}.json.xz");
    let target = workspace.join(&relative);
    let mut encoder = XzEncoder::new(Vec::new(), 6);
    encoder.write_all(&logical).unwrap();
    fs::write(&target, encoder.finish().unwrap()).unwrap();
    let state = WorkspaceState {
        schema_version: 1,
        workspace_state_profile: "antiskill_workspace_state_v1",
        active_collection: ActiveCollection {
            path: &relative,
            blake3: &hash,
            bytes: logical.len() as u64,
        },
    };
    let mut state_bytes = serde_json::to_vec_pretty(&state).unwrap();
    state_bytes.push(b'\n');
    fs::write(
        workspace.join("inspector/workspace-state.json"),
        state_bytes,
    )
    .unwrap();
}

#[test]
fn help_and_argument_contract_are_product_facing() {
    let temporary = TestDirectory::new("help");
    let top = command(&temporary.0, &["--help"]);
    assert!(top.status.success());
    let top = stdout(&top);
    let ingest = top.find("    ingest       ").unwrap();
    let compare = top.find("    compare      ").unwrap();
    let search = top.find("    search       ").unwrap();
    assert!(ingest < compare && compare < search);
    assert!(top.contains("antiskill compare --help"));

    for help in ["--help", "-h", "help"] {
        let output = command(&temporary.0, &["compare", help]);
        assert!(output.status.success());
        let text = stdout(&output);
        assert!(text.contains("antiskill compare DIR"));
        assert!(text.contains("read-only"));
    }
    for args in [
        vec!["compare"],
        vec!["compare", "one", "two"],
        vec!["compare", "--unexpected"],
    ] {
        let output = command(&temporary.0, &args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(stderr(&output).contains("ERRORE compare:"));
        assert!(stderr(&output).contains("antiskill compare DIR"));
    }
}

#[test]
fn identical_snapshot_works_with_absolute_and_relative_directory() {
    let bytes = pgn("same");
    let (_temporary, workspace, input) = setup("identical", &[("same.PgN", &bytes)]);
    let absolute = compare(&workspace, &input);
    assert!(absolute.status.success(), "{}", stderr(&absolute));
    assert_eq!(
        stdout(&absolute),
        "Nessuna differenza: 1 file PGN invariato.\n"
    );
    let relative = command(&workspace, &["compare", "../input"]);
    assert!(relative.status.success(), "{}", stderr(&relative));
    assert_eq!(stdout(&relative), stdout(&absolute));
}

#[test]
fn mixed_diff_is_path_keyed_deterministic_and_has_no_technical_details() {
    let unchanged = pgn("unchanged");
    let modified = pgn("before");
    let moved = pgn("moved");
    let duplicate = pgn("duplicate");
    let (_temporary, workspace, input) = setup(
        "mixed",
        &[
            ("same.pgn", &unchanged),
            ("z-modified.pgn", &modified),
            ("old/moved.pgn", &moved),
            ("duplicates/a.pgn", &duplicate),
            ("duplicates/b.pgn", &duplicate),
            ("removed-z.pgn", &pgn("removed-z")),
            ("removed-a.pgn", &pgn("removed-a")),
        ],
    );
    fs::write(input.join("z-modified.pgn"), pgn("after")).unwrap();
    fs::create_dir(input.join("new")).unwrap();
    fs::rename(input.join("old/moved.pgn"), input.join("new/moved.pgn")).unwrap();
    fs::remove_file(input.join("removed-z.pgn")).unwrap();
    fs::remove_file(input.join("removed-a.pgn")).unwrap();
    fs::write(input.join("added-z.pgn"), pgn("added-z")).unwrap();
    fs::write(input.join("added-a.pgn"), pgn("added-a")).unwrap();

    let output = compare(&workspace, &input);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(output.stderr.is_empty());
    let text = stdout(&output);
    assert!(text
        .starts_with("Confronto snapshot:\n1 modificato · 3 aggiunti · 3 rimossi · 3 invariati\n"));
    assert!(text.contains("Modificati\n- z-modified.pgn\n"));
    assert!(text.contains("Aggiunti\n- added-a.pgn\n- added-z.pgn\n- new/moved.pgn\n"));
    assert!(text.contains("Rimossi\n- old/moved.pgn\n- removed-a.pgn\n- removed-z.pgn\n"));
    for forbidden in [
        "blake3",
        "inspector/",
        "collection",
        "profile",
        "source id",
        "offset",
    ] {
        assert!(!text.to_lowercase().contains(forbidden));
    }
}

#[test]
fn empty_current_directory_reports_every_active_path_removed() {
    let a = pgn("a");
    let b = pgn("b");
    let (_temporary, workspace, input) = setup("empty", &[("a.pgn", &a), ("b.pgn", &b)]);
    fs::remove_file(input.join("a.pgn")).unwrap();
    fs::remove_file(input.join("b.pgn")).unwrap();
    let output = compare(&workspace, &input);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("0 modificati · 0 aggiunti · 2 rimossi · 0 invariati"));
    assert!(stdout(&output).contains("Rimossi\n- a.pgn\n- b.pgn\n"));
}

#[test]
fn root_symlink_missing_and_file_inputs_fail_nonzero() {
    let bytes = pgn("active");
    let (temporary, workspace, input) = setup("invalid-input", &[("a.pgn", &bytes)]);
    let file = temporary.0.join("file.pgn");
    fs::write(&file, pgn("file")).unwrap();
    for (path, message) in [
        (temporary.0.join("missing"), "Directory input non valida"),
        (file, "Input non è una directory"),
    ] {
        let output = compare(&workspace, &path);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(stderr(&output).contains(message));
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&input, temporary.0.join("input-link")).unwrap();
        let output = compare(&workspace, &temporary.0.join("input-link"));
        assert!(!output.status.success());
        assert!(stderr(&output).contains("Input è un symlink"));
    }
}

#[test]
fn internal_symlinks_are_ignored_like_ingest() {
    let bytes = pgn("active");
    let (temporary, workspace, input) = setup("internal-link", &[("a.pgn", &bytes)]);
    #[cfg(unix)]
    {
        fs::write(temporary.0.join("outside.pgn"), pgn("outside")).unwrap();
        std::os::unix::fs::symlink(temporary.0.join("outside.pgn"), input.join("ignored.pgn"))
            .unwrap();
        let output = compare(&workspace, &input);
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(
            stdout(&output),
            "Nessuna differenza: 1 file PGN invariato.\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn scan_and_read_failures_do_not_print_a_partial_comparison() {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::PermissionsExt;

    let bytes = pgn("active");
    let (temporary, workspace, input) = setup("failures", &[("a.pgn", &bytes)]);
    let invalid_name = std::ffi::OsString::from_vec(b"bad-\xff.pgn".to_vec());
    fs::write(input.join(invalid_name), pgn("bad-name")).unwrap();
    let scan_failure = compare(&workspace, &input);
    assert!(!scan_failure.status.success());
    assert!(scan_failure.stdout.is_empty());
    assert!(stderr(&scan_failure).contains("Scansione PGN non completata"));

    let clean = temporary.0.join("read-failure");
    fs::create_dir(&clean).unwrap();
    let unreadable = clean.join("a.pgn");
    fs::write(&unreadable, &bytes).unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
    let read_failure = compare(&workspace, &clean);
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!read_failure.status.success());
    assert!(read_failure.stdout.is_empty());
    assert!(stderr(&read_failure).contains("Impossibile leggere il PGN a.pgn"));
}

#[test]
fn workspace_state_and_active_collection_fail_closed() {
    let bytes = pgn("active");

    let (_missing_temp, missing_workspace, missing_input) =
        setup("missing-state", &[("a.pgn", &bytes)]);
    fs::remove_file(missing_workspace.join("inspector/workspace-state.json")).unwrap();
    let missing_state = compare(&missing_workspace, &missing_input);
    assert!(!missing_state.status.success());
    assert!(stderr(&missing_state).contains("Stato workspace mancante"));

    let (_corrupt_temp, corrupt_workspace, corrupt_input) =
        setup("corrupt-state", &[("a.pgn", &bytes)]);
    fs::write(
        corrupt_workspace.join("inspector/workspace-state.json"),
        b"not json",
    )
    .unwrap();
    let corrupt_state = compare(&corrupt_workspace, &corrupt_input);
    assert!(!corrupt_state.status.success());
    assert!(stderr(&corrupt_state).contains("Stato workspace corrotto"));

    let (_absent_temp, absent_workspace, absent_input) =
        setup("missing-collection", &[("a.pgn", &bytes)]);
    let (active, _) = active_manifest(&absent_workspace);
    fs::remove_file(active).unwrap();
    let missing_collection = compare(&absent_workspace, &absent_input);
    assert!(!missing_collection.status.success());
    assert!(stderr(&missing_collection).contains("Active Collection non verificabile"));

    let (_bad_temp, bad_workspace, bad_input) = setup("corrupt-collection", &[("a.pgn", &bytes)]);
    let (active, _) = active_manifest(&bad_workspace);
    fs::write(active, b"corrupt").unwrap();
    let corrupt_collection = compare(&bad_workspace, &bad_input);
    assert!(!corrupt_collection.status.success());
    assert!(stderr(&corrupt_collection).contains("Active Collection non verificabile"));
}

#[test]
fn verified_collection_v1_and_v2_are_supported() {
    let bytes = pgn("active");
    let temporary = TestDirectory::new("versions");
    let workspace = temporary.0.join("workspace");
    let input = temporary.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("a.pgn"), &bytes).unwrap();

    let legacy = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root", ".", "--input"])
        .arg(&input)
        .current_dir(&workspace)
        .output()
        .unwrap();
    assert!(legacy.status.success(), "{}", stderr(&legacy));

    let v2 = compare(&workspace, &input);
    assert!(v2.status.success(), "{}", stderr(&v2));

    let (_old_path, mut manifest) = active_manifest(&workspace);
    assert_eq!(manifest["schema_version"], json!(2));
    manifest["schema_version"] = json!(1);
    manifest["collection_profile"] = json!("antiskill_workspace_collection_v1");
    for source in manifest["sources"].as_array_mut().unwrap() {
        let object = source.as_object_mut().unwrap();
        for field in [
            "occurrences",
            "occurrences_blake3",
            "occurrences_bytes",
            "mainlines",
            "mainlines_blake3",
            "mainlines_bytes",
        ] {
            object.remove(field);
        }
    }
    install_active_manifest(&workspace, &manifest);
    let v1 = compare(&workspace, &input);
    assert!(v1.status.success(), "{}", stderr(&v1));
    assert_eq!(stdout(&v1), "Nessuna differenza: 1 file PGN invariato.\n");
}

#[test]
fn conflicting_historical_observation_path_fails_closed() {
    let a = pgn("a");
    let b = pgn("b");
    let (_temporary, workspace, input) =
        setup("historical-conflict", &[("a.pgn", &a), ("b.pgn", &b)]);
    let (_old_path, mut manifest) = active_manifest(&workspace);
    let sources = manifest["sources"].as_array_mut().unwrap();
    let path = sources[0]["observations"][0]["input_relative_path"].clone();
    sources[1]["observations"][0]["input_relative_path"] = path;
    install_active_manifest(&workspace, &manifest);
    let output = compare(&workspace, &input);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("identità sorgente confliggenti"));
}

#[test]
fn compare_preserves_workspace_and_input_tree_byte_for_byte() {
    let a = pgn("a");
    let b = pgn("b");
    let (_temporary, workspace, input) =
        setup("readonly", &[("same.pgn", &a), ("changed.pgn", &b)]);
    fs::write(input.join("changed.pgn"), pgn("changed")).unwrap();
    fs::write(input.join("added.pgn"), pgn("added")).unwrap();
    let workspace_before = tree(&workspace);
    let input_before = tree(&input);
    let output = compare(&workspace, &input);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(tree(&workspace), workspace_before);
    assert_eq!(tree(&input), input_before);
}
