mod support;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = support::unique_temp_path(&format!("antiskill-export-cli-{label}"));
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
    command(workspace, &["ingest", input.to_str().unwrap()])
}

fn legacy_ingest(workspace: &Path, input: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root", ".", "--input"])
        .arg(input)
        .current_dir(workspace)
        .output()
        .unwrap()
}

fn search(workspace: &Path, filters: &[&str]) -> Output {
    let mut args = vec!["search"];
    args.extend(filters);
    command(workspace, &args)
}

fn export(workspace: &Path, game_id: &str, output: &Path) -> Output {
    command(workspace, &["export", game_id, output.to_str().unwrap()])
}

fn batch_export(workspace: &Path, ids_file: &Path, output_dir: &Path) -> Output {
    command(
        workspace,
        &[
            "export",
            "--batch",
            ids_file.to_str().unwrap(),
            output_dir.to_str().unwrap(),
        ],
    )
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn search_rows(workspace: &Path, filters: &[&str]) -> Vec<Vec<String>> {
    let output = search(workspace, filters);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines[2], "ID\tDate\tWhite\tBlack\tResult\tEvent");
    lines[3..]
        .iter()
        .map(|line| line.split('\t').map(str::to_owned).collect())
        .collect()
}

fn id_for_player(workspace: &Path, player: &str) -> String {
    let rows = search_rows(workspace, &["--player", player]);
    assert_eq!(rows.len(), 1);
    rows[0][0].clone()
}

fn active_collection(workspace: &Path) -> PathBuf {
    let state: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("inspector/workspace-state.json")).unwrap(),
    )
    .unwrap();
    workspace.join(state["active_collection"]["path"].as_str().unwrap())
}

fn active_collection_manifest(workspace: &Path) -> serde_json::Value {
    let path = active_collection(workspace);
    let stored = fs::read(&path).unwrap();
    let mut logical = Vec::new();
    if path.extension().and_then(|value| value.to_str()) == Some("xz") {
        xz2::read::XzDecoder::new(stored.as_slice())
            .read_to_end(&mut logical)
            .unwrap();
    } else {
        logical = stored;
    }
    serde_json::from_slice(&logical).unwrap()
}

fn simple_pgn(player: &str, event: &str) -> Vec<u8> {
    format!(
        "[Event \"{event}\"]\n[White \"{player}\"]\n[Black \"Opponent\"]\n[Date \"2026.08.11\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n"
    )
    .into_bytes()
}

#[derive(serde::Serialize)]
struct LegacyCollection {
    schema_version: u32,
    collection_profile: &'static str,
    sources: Vec<LegacyCollectionSource>,
}

#[derive(serde::Serialize)]
struct LegacyCollectionSource {
    id: u64,
    source_blake3: String,
    source_bytes: u64,
    catalog: String,
    catalog_blake3: String,
    catalog_bytes: u64,
    games: u64,
    observations: serde_json::Value,
}

#[derive(serde::Serialize)]
struct TestWorkspaceState {
    schema_version: u32,
    workspace_state_profile: &'static str,
    active_collection: TestActiveCollection,
}

#[derive(serde::Serialize)]
struct TestActiveCollection {
    path: String,
    blake3: String,
    bytes: u64,
}

fn activate_legacy_collection(workspace: &Path) {
    let state_path = workspace.join("inspector/workspace-state.json");
    let state: serde_json::Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    let collection_path = workspace.join(state["active_collection"]["path"].as_str().unwrap());
    let stored = fs::read(&collection_path).unwrap();
    let mut logical = Vec::new();
    if collection_path.extension().and_then(|value| value.to_str()) == Some("xz") {
        xz2::read::XzDecoder::new(stored.as_slice())
            .read_to_end(&mut logical)
            .unwrap();
    } else {
        logical = stored;
    }
    let collection: serde_json::Value = serde_json::from_slice(&logical).unwrap();
    let sources = collection["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| LegacyCollectionSource {
            id: source["id"].as_u64().unwrap(),
            source_blake3: source["source_blake3"].as_str().unwrap().into(),
            source_bytes: source["source_bytes"].as_u64().unwrap(),
            catalog: source["catalog"].as_str().unwrap().into(),
            catalog_blake3: source["catalog_blake3"].as_str().unwrap().into(),
            catalog_bytes: source["catalog_bytes"].as_u64().unwrap(),
            games: source["games"].as_u64().unwrap(),
            observations: source["observations"].clone(),
        })
        .collect();
    let legacy = LegacyCollection {
        schema_version: 1,
        collection_profile: "antiskill_workspace_collection_v1",
        sources,
    };
    let mut bytes = serde_json::to_vec_pretty(&legacy).unwrap();
    bytes.push(b'\n');
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let relative = format!("inspector/collections/{hash}.json");
    fs::write(workspace.join(&relative), &bytes).unwrap();
    let state = TestWorkspaceState {
        schema_version: 1,
        workspace_state_profile: "antiskill_workspace_state_v1",
        active_collection: TestActiveCollection {
            path: relative,
            blake3: hash,
            bytes: u64::try_from(bytes.len()).unwrap(),
        },
    };
    let mut state_bytes = serde_json::to_vec_pretty(&state).unwrap();
    state_bytes.push(b'\n');
    fs::write(state_path, state_bytes).unwrap();
}

#[test]
fn search_id_to_export_is_byte_exact_for_distinct_duplicate_occurrences() {
    let directory = TestDirectory::new("e2e");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    let outside = directory.0.join("outside");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::create_dir(&outside).unwrap();
    let game = b"[Event \"Odd spacing\"]\r\n[White \"Esempio\"]\r\n[Black \"Eve\"]\r\n[Date \"2026.08.11\"]\r\n[Result \"1-0\"]\r\n\r\n1. e4  {kept} e5  1-0\r\n";
    let mut source = game.to_vec();
    source.push(b'\n');
    source.extend_from_slice(game);
    fs::write(input.join("duplicates.pgn"), &source).unwrap();

    let ingested = ingest(&workspace, &input);
    assert!(ingested.status.success(), "{}", stderr(&ingested));
    let rows = search_rows(&workspace, &["--player", "esempio"]);
    assert_eq!(rows.len(), 2);
    let ids = rows.iter().map(|row| row[0].clone()).collect::<Vec<_>>();
    assert_ne!(ids[0], ids[1]);
    for id in &ids {
        assert_eq!(id.len(), 67);
        assert!(id.starts_with("g1-"));
        assert!(id[3..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
    }

    let relative = Path::new("first.pgn");
    let first = export(&workspace, &ids[0], relative);
    assert!(first.status.success(), "{}", stderr(&first));
    assert_eq!(fs::read(workspace.join(relative)).unwrap(), game);
    let absolute = outside.join("second.pgn");
    let second = export(&workspace, &ids[1], &absolute);
    assert!(second.status.success(), "{}", stderr(&second));
    assert_eq!(fs::read(&absolute).unwrap(), game);

    for output in [stdout(&first), stdout(&second)] {
        assert!(output.contains("Partita esportata."));
        for forbidden in [
            "source_id",
            "game_id",
            "source hash",
            "game hash",
            "inspector/",
            "source_start",
            "source_end",
            "schema",
        ] {
            assert!(!output.to_lowercase().contains(forbidden));
        }
    }

    let repeated = ingest(&workspace, &input);
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    let repeated_ids = search_rows(&workspace, &["--player", "esempio"])
        .into_iter()
        .map(|row| row[0].clone())
        .collect::<Vec<_>>();
    assert_eq!(repeated_ids, ids);
}

#[test]
fn single_export_accepts_batch_as_literal_output_filename() {
    let directory = TestDirectory::new("batch-filename");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    let game = simple_pgn("Literal Output", "Single export compatibility");
    fs::write(input.join("game.pgn"), &game).unwrap();

    let ingested = ingest(&workspace, &input);
    assert!(ingested.status.success(), "{}", stderr(&ingested));
    let id = id_for_player(&workspace, "Literal Output");
    assert!(id.starts_with("g1-"));

    let result = export(&workspace, &id, Path::new("--batch"));
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(fs::read(workspace.join("--batch")).unwrap(), game);
}

#[test]
fn public_id_is_independent_from_source_id_order_and_observation_path() {
    let directory = TestDirectory::new("stable");
    let workspace_one = directory.0.join("workspace-one");
    let workspace_two = directory.0.join("workspace-two");
    let input_one = directory.0.join("input-one");
    let input_two = directory.0.join("input-two");
    for path in [&workspace_one, &workspace_two, &input_one, &input_two] {
        fs::create_dir(path).unwrap();
    }
    let target = simple_pgn("Target Player", "Target");
    fs::write(input_one.join("renamed-one.pgn"), &target).unwrap();
    fs::write(input_two.join("renamed-two.pgn"), &target).unwrap();
    let target_hash = blake3::hash(&target).to_hex().to_string();
    let filler = (0_u64..10_000)
        .map(|index| simple_pgn("Filler", &format!("Filler {index}")))
        .find(|bytes| blake3::hash(bytes).to_hex().as_str() < target_hash.as_str())
        .expect("fixture con hash precedente");
    fs::write(input_two.join("filler.pgn"), filler).unwrap();
    assert!(ingest(&workspace_one, &input_one).status.success());
    assert!(ingest(&workspace_two, &input_two).status.success());

    let first_id = id_for_player(&workspace_one, "Target Player");
    let second_id = id_for_player(&workspace_two, "Target Player");
    assert_eq!(first_id, second_id);

    let technical_source_id = |workspace: &Path| {
        let output = command(
            workspace,
            &[
                "inspector",
                "query-collection",
                "--root",
                workspace.to_str().unwrap(),
                "--player",
                "Target Player",
            ],
        );
        assert!(output.status.success(), "{}", stderr(&output));
        stdout(&output)
            .lines()
            .find(|line| line.starts_with("GAME\t"))
            .unwrap()
            .split('\t')
            .nth(1)
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    assert_eq!(technical_source_id(&workspace_one), 0);
    assert_eq!(technical_source_id(&workspace_two), 1);
}

#[test]
fn batch_export_writes_multiple_exact_original_occurrences_and_ignores_blank_lines() {
    let directory = TestDirectory::new("batch-success");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    let first = b"[Event \"First\"]\r\n[White \"Alice\"]\r\n[Black \"Bob\"]\r\n[Date \"2026.08.11\"]\r\n[Result \"1-0\"]\r\n\r\n1. e4  {exact} e5 1-0\r\n";
    let second = b"[Event \"Second\"]\n[White \"Carol\"]\n[Black \"Dave\"]\n[Date \"2026.08.12\"]\n[Result \"0-1\"]\n\n1. d4 d5 0-1\n";
    fs::write(input.join("first.pgn"), first).unwrap();
    fs::write(input.join("second.pgn"), second).unwrap();
    assert!(ingest(&workspace, &input).status.success());
    let first_id = id_for_player(&workspace, "Alice");
    let second_id = id_for_player(&workspace, "Carol");
    let ids_file = workspace.join("ids.txt");
    fs::write(
        &ids_file,
        format!("  {first_id}  \n\n\t{second_id}\t\n   \n"),
    )
    .unwrap();

    let result = batch_export(&workspace, Path::new("ids.txt"), Path::new("exported"));

    assert!(result.status.success(), "{}", stderr(&result));
    let output = workspace.join("exported");
    assert_eq!(
        fs::read(output.join(format!("{first_id}.pgn"))).unwrap(),
        first
    );
    assert_eq!(
        fs::read(output.join(format!("{second_id}.pgn"))).unwrap(),
        second
    );
    assert_eq!(fs::read_dir(output).unwrap().count(), 2);
}

#[test]
fn batch_export_rejects_invalid_inputs_before_final_publication() {
    let directory = TestDirectory::new("batch-input-errors");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("game.pgn"), simple_pgn("Alice", "Open")).unwrap();
    assert!(ingest(&workspace, &input).status.success());
    let id = id_for_player(&workspace, "Alice");
    let cases = [
        ("duplicates", format!("{id}\n{id}\n"), "duplicato"),
        (
            "malformed",
            "not-a-game-id\n".into(),
            "ID partita non valido",
        ),
        (
            "missing",
            format!("g1-{}\n", "0".repeat(64)),
            "Partita non presente",
        ),
        ("empty", "\n \t\n".into(), "non contiene Game ID"),
    ];
    for (label, contents, expected) in cases {
        let ids_file = workspace.join(format!("{label}.txt"));
        let output = workspace.join(format!("output-{label}"));
        fs::write(&ids_file, contents).unwrap();
        let result = batch_export(&workspace, &ids_file, &output);
        assert!(!result.status.success(), "{label}");
        assert!(
            stderr(&result).contains(expected),
            "{label}: {}",
            stderr(&result)
        );
        assert!(!output.exists(), "{label}");
    }

    let unreadable_as_file = workspace.join("ids-directory");
    fs::create_dir(&unreadable_as_file).unwrap();
    let output = workspace.join("output-read-failure");
    let result = batch_export(&workspace, &unreadable_as_file, &output);
    assert!(!result.status.success());
    assert!(!output.exists());
}

#[test]
fn batch_export_rejects_existing_output_and_unsafe_parents_without_changes() {
    let directory = TestDirectory::new("batch-destinations");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("game.pgn"), simple_pgn("Alice", "Open")).unwrap();
    assert!(ingest(&workspace, &input).status.success());
    let id = id_for_player(&workspace, "Alice");
    let ids_file = workspace.join("ids.txt");
    fs::write(&ids_file, format!("{id}\n")).unwrap();

    let existing = workspace.join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("keep"), b"untouched").unwrap();
    let result = batch_export(&workspace, &ids_file, &existing);
    assert!(!result.status.success());
    assert_eq!(fs::read(existing.join("keep")).unwrap(), b"untouched");

    let missing_parent_output = workspace.join("missing-parent/output");
    assert!(!batch_export(&workspace, &ids_file, &missing_parent_output)
        .status
        .success());
    assert!(!missing_parent_output.exists());

    let file_parent = workspace.join("file-parent");
    fs::write(&file_parent, b"untouched").unwrap();
    let file_parent_output = file_parent.join("output");
    assert!(!batch_export(&workspace, &ids_file, &file_parent_output)
        .status
        .success());
    assert_eq!(fs::read(file_parent).unwrap(), b"untouched");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let real_parent = workspace.join("real-parent");
        let linked_parent = workspace.join("linked-parent");
        fs::create_dir(&real_parent).unwrap();
        symlink(&real_parent, &linked_parent).unwrap();
        let linked_output = linked_parent.join("output");
        assert!(!batch_export(&workspace, &ids_file, &linked_output)
            .status
            .success());
        assert!(!real_parent.join("output").exists());
    }
}

#[test]
fn batch_export_fails_closed_for_legacy_collection_without_public_ids() {
    let directory = TestDirectory::new("batch-legacy");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("game.pgn"), simple_pgn("Alice", "Open")).unwrap();
    assert!(ingest(&workspace, &input).status.success());
    let id = id_for_player(&workspace, "Alice");
    let legacy = legacy_ingest(&workspace, &input);
    assert!(legacy.status.success(), "{}", stderr(&legacy));
    activate_legacy_collection(&workspace);
    let ids_file = workspace.join("ids.txt");
    fs::write(&ids_file, format!("{id}\n")).unwrap();
    let output = workspace.join("legacy-output");

    let result = batch_export(&workspace, &ids_file, &output);

    assert!(!result.status.success());
    assert!(stderr(&result).contains("legacy v1"), "{}", stderr(&result));
    assert!(!output.exists());
}

#[test]
fn export_rejects_ids_state_collection_and_unsafe_destinations_fail_closed() {
    let directory = TestDirectory::new("errors");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    let empty_workspace = directory.0.join("empty-workspace");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::create_dir(&empty_workspace).unwrap();
    fs::write(input.join("game.pgn"), simple_pgn("Alice", "Open")).unwrap();
    assert!(ingest(&workspace, &input).status.success());
    let id = id_for_player(&workspace, "Alice");

    let malformed = [
        format!("x1-{}", "a".repeat(64)),
        format!("g1-{}", "a".repeat(63)),
        format!("g1-{}", "a".repeat(65)),
        format!("g1-{}A", "a".repeat(63)),
        format!("g1-{}g", "a".repeat(63)),
        "a".repeat(64),
    ];
    for (index, malformed) in malformed.iter().enumerate() {
        let output = workspace.join(format!("invalid-{index}.pgn"));
        let result = export(&workspace, malformed, &output);
        assert!(!result.status.success());
        assert!(stderr(&result).contains("ID partita non valido"));
        assert!(!output.exists());
    }

    let absent_output = workspace.join("absent.pgn");
    let absent = export(
        &workspace,
        &format!("g1-{}", "0".repeat(64)),
        &absent_output,
    );
    assert!(!absent.status.success());
    assert!(stderr(&absent).contains("Partita non presente nel corpus attivo"));
    assert!(!absent_output.exists());

    let no_state_output = empty_workspace.join("none.pgn");
    let no_state = export(&empty_workspace, &id, &no_state_output);
    assert!(!no_state.status.success());
    assert!(stderr(&no_state).contains("Stato workspace mancante"));
    assert!(!no_state_output.exists());

    let state_path = workspace.join("inspector/workspace-state.json");
    let valid_state = fs::read(&state_path).unwrap();
    fs::write(&state_path, b"{ broken\n").unwrap();
    let corrupt_state = export(&workspace, &id, &workspace.join("corrupt-state.pgn"));
    assert!(!corrupt_state.status.success());
    assert!(stderr(&corrupt_state).contains("Stato workspace corrotto"));
    fs::write(&state_path, &valid_state).unwrap();

    let collection_path = active_collection(&workspace);
    let valid_collection = fs::read(&collection_path).unwrap();
    fs::remove_file(&collection_path).unwrap();
    let missing_collection = export(&workspace, &id, &workspace.join("missing.pgn"));
    assert!(!missing_collection.status.success());
    assert!(stderr(&missing_collection).contains("Active Collection non verificabile"));
    fs::write(&collection_path, &valid_collection).unwrap();
    fs::write(&collection_path, b"corrupt").unwrap();
    let corrupt_collection = export(&workspace, &id, &workspace.join("corrupt.pgn"));
    assert!(!corrupt_collection.status.success());
    assert!(stderr(&corrupt_collection).contains("Active Collection non verificabile"));
    fs::write(&collection_path, valid_collection).unwrap();

    let missing_parent = workspace.join("missing-parent");
    let missing_parent_result = export(&workspace, &id, &missing_parent.join("game.pgn"));
    assert!(!missing_parent_result.status.success());
    assert!(stderr(&missing_parent_result).contains("Parent della destinazione inesistente"));
    assert!(!missing_parent.exists());

    let existing_file = workspace.join("existing.pgn");
    fs::write(&existing_file, b"keep me").unwrap();
    let existing_file_result = export(&workspace, &id, &existing_file);
    assert!(!existing_file_result.status.success());
    assert_eq!(fs::read(&existing_file).unwrap(), b"keep me");

    let existing_directory = workspace.join("existing-directory");
    fs::create_dir(&existing_directory).unwrap();
    assert!(!export(&workspace, &id, &existing_directory)
        .status
        .success());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let victim = directory.0.join("victim.pgn");
        let link = workspace.join("link.pgn");
        fs::write(&victim, b"untouched").unwrap();
        symlink(&victim, &link).unwrap();
        assert!(!export(&workspace, &id, &link).status.success());
        assert_eq!(fs::read_link(&link).unwrap(), victim);
        assert_eq!(fs::read(&victim).unwrap(), b"untouched");
    }
}

#[test]
fn active_search_and_single_export_still_verify_v3_artifacts_fail_closed() {
    let directory = TestDirectory::new("active-artifact-verification");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("game.pgn"), simple_pgn("Alice", "Open")).unwrap();
    assert!(ingest(&workspace, &input).status.success());
    let id = id_for_player(&workspace, "Alice");
    let manifest = active_collection_manifest(&workspace);
    let source = &manifest["sources"][0];

    let index_path = workspace.join(source["source_index"].as_str().unwrap());
    let index_bytes = fs::read(&index_path).unwrap();
    fs::write(&index_path, b"corrupt source index").unwrap();
    let broken_search = search(&workspace, &["--player", "Alice"]);
    assert!(!broken_search.status.success());
    assert!(broken_search.stdout.is_empty());
    fs::write(&index_path, index_bytes).unwrap();

    let source_hash = source["source_blake3"].as_str().unwrap();
    let object_path = workspace
        .join("raw/objects/blake3")
        .join(&source_hash[..2])
        .join(source_hash);
    let object_bytes = fs::read(&object_path).unwrap();
    fs::write(&object_path, b"corrupt source blob").unwrap();
    let output = workspace.join("must-not-exist.pgn");
    let broken_export = export(&workspace, &id, &output);
    assert!(!broken_export.status.success());
    assert!(broken_export.stdout.is_empty());
    assert!(!output.exists());
    fs::write(object_path, object_bytes).unwrap();
}

#[test]
fn export_help_and_arity_are_user_facing() {
    let directory = TestDirectory::new("help");
    let help = command(&directory.0, &["export", "--help"]);
    assert!(help.status.success());
    let text = stdout(&help);
    for expected in [
        "antiskill export GAME_ID OUTPUT",
        "antiskill export --batch IDS_FILE OUTPUT_DIR",
        "antiskill search",
        "byte-esatta",
        "directory corrente",
        "assoluto",
        "non vengono mai sovrascritti",
    ] {
        assert!(text.contains(expected), "manca: {expected}");
    }
    let top = command(&directory.0, &["--help"]);
    assert!(stdout(&top).contains("antiskill export --help"));
    for args in [
        vec!["export"],
        vec!["export", "one"],
        vec!["export", "one", "two", "three"],
        vec!["export", "--batch", "ids.txt"],
        vec!["export", "--batch", "ids.txt", "output", "extra"],
    ] {
        let result = command(&directory.0, &args);
        assert!(!result.status.success());
        assert!(stderr(&result).contains("esattamente GAME_ID e OUTPUT"));
    }
    let invalid_id_with_batch_output = command(&directory.0, &["export", "one", "--batch"]);
    assert!(!invalid_id_with_batch_output.status.success());
}
