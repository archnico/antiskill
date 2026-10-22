mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = support::unique_temp_path(&format!("antiskill-ingest-cli-{label}"));
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

fn search(workspace: &Path, player: &str) -> Output {
    command(workspace, &["search", "--player", player])
}

fn pgn(event: &str, white: &str) -> Vec<u8> {
    format!(
        "[Event \"{event}\"]\n[White \"{white}\"]\n[Black \"Opponent\"]\n[Date \"2026.08.11\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n"
    )
    .into_bytes()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn assert_search_count(workspace: &Path, player: &str, expected: usize) {
    let result = search(workspace, player);
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(
        stdout(&result).starts_with(&format!("{expected} partit")),
        "{}",
        stdout(&result)
    );
}

#[test]
fn user_ingest_uses_cwd_activates_collection_and_searches_without_inspector_options() {
    let directory = TestDirectory::new("success");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("pgn");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("game.pgn"), pgn("Open", "Esempio")).unwrap();

    let result = ingest(&workspace, &input);
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(result.stderr.is_empty());
    let human = stdout(&result);
    assert!(human.contains("Ingest completato."));
    assert!(human.contains("1 file PGN · 1 sorgente · 1 partita · 0 errori"));
    assert!(workspace.join("inspector/workspace-state.json").is_file());
    assert_search_count(&workspace, "Esempio", 1);

    let technical_markers = [
        "inspector/",
        "COLLECTION",
        "BLAKE3",
        "schema",
        "catalog",
        "occurrences",
        "mainlines",
        "reused_workspace",
    ];
    for marker in technical_markers {
        assert!(!human.to_lowercase().contains(&marker.to_lowercase()));
    }
}

#[test]
fn repeated_snapshot_reuses_workspace_and_removed_source_leaves_active_corpus() {
    let directory = TestDirectory::new("remove");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("pgn");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("a.pgn"), pgn("A", "Alice")).unwrap();
    fs::write(input.join("b.pgn"), pgn("B", "Bob")).unwrap();

    let first = ingest(&workspace, &input);
    assert!(first.status.success(), "{}", stderr(&first));
    assert_search_count(&workspace, "Alice", 1);
    assert_search_count(&workspace, "Bob", 1);

    let repeated = ingest(&workspace, &input);
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert!(stdout(&repeated).contains("2 sorgenti riusate dal workspace"));
    assert_search_count(&workspace, "Alice", 1);
    assert_search_count(&workspace, "Bob", 1);

    fs::remove_file(input.join("b.pgn")).unwrap();
    let removed = ingest(&workspace, &input);
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert!(stdout(&removed).contains("1 file PGN · 1 sorgente · 1 partita"));
    assert_search_count(&workspace, "Alice", 1);
    assert_search_count(&workspace, "Bob", 0);
}

#[test]
fn changed_pgn_replaces_old_source_in_active_snapshot() {
    let directory = TestDirectory::new("replace");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("pgn");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    let path = input.join("a.pgn");
    fs::write(&path, pgn("Old", "Old Player")).unwrap();
    assert!(ingest(&workspace, &input).status.success());
    assert_search_count(&workspace, "Old Player", 1);

    fs::write(&path, pgn("New", "New Player")).unwrap();
    let changed = ingest(&workspace, &input);
    assert!(changed.status.success(), "{}", stderr(&changed));
    assert!(stdout(&changed).contains("1 file PGN · 1 sorgente · 1 partita"));
    assert_search_count(&workspace, "Old Player", 0);
    assert_search_count(&workspace, "New Player", 1);
}

#[test]
fn partial_ingest_preserves_previous_state_and_reports_file_and_cause() {
    let directory = TestDirectory::new("partial-with-state");
    let workspace = directory.0.join("workspace");
    let initial = directory.0.join("initial");
    let partial = directory.0.join("partial");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&initial).unwrap();
    fs::create_dir(&partial).unwrap();
    fs::write(initial.join("active.pgn"), pgn("Active", "Active Player")).unwrap();
    assert!(ingest(&workspace, &initial).status.success());
    let state_path = workspace.join("inspector/workspace-state.json");
    let before = fs::read(&state_path).unwrap();

    fs::write(partial.join("valid.pgn"), pgn("Partial", "Partial Player")).unwrap();
    fs::write(partial.join("broken.pgn"), b"").unwrap();
    let result = ingest(&workspace, &partial);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let error = stderr(&result);
    assert!(error.contains("Ingest non completato."));
    assert!(error.contains("broken.pgn"));
    assert!(error.contains("vuoto"));
    assert!(!error.to_lowercase().contains("panic"));
    assert_eq!(fs::read(state_path).unwrap(), before);
    assert_search_count(&workspace, "Active Player", 1);
    assert_search_count(&workspace, "Partial Player", 0);
}

#[test]
fn partial_ingest_without_previous_active_state_does_not_create_state() {
    let directory = TestDirectory::new("partial-no-state");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("pgn");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("valid.pgn"), pgn("Partial", "Player")).unwrap();
    fs::write(input.join("broken.pgn"), b"").unwrap();

    let result = ingest(&workspace, &input);
    assert!(!result.status.success());
    assert!(!workspace.join("inspector/workspace-state.json").exists());
}

#[test]
fn zero_pgn_is_rejected_before_state_activation_with_and_without_previous_state() {
    let directory = TestDirectory::new("empty");
    let workspace = directory.0.join("workspace");
    let initial = directory.0.join("initial");
    let empty = directory.0.join("empty");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&initial).unwrap();
    fs::create_dir(&empty).unwrap();

    let no_state = ingest(&workspace, &empty);
    assert!(!no_state.status.success());
    assert!(stderr(&no_state).contains("non contiene file PGN"));
    assert!(!workspace.join("inspector/workspace-state.json").exists());
    assert!(!workspace.join("inspector").exists());

    fs::write(initial.join("active.pgn"), pgn("Active", "Active Player")).unwrap();
    assert!(ingest(&workspace, &initial).status.success());
    let state_path = workspace.join("inspector/workspace-state.json");
    let before = fs::read(&state_path).unwrap();
    let with_state = ingest(&workspace, &empty);
    assert!(!with_state.status.success());
    assert!(stderr(&with_state).contains("non contiene file PGN"));
    assert_eq!(fs::read(state_path).unwrap(), before);
}

#[test]
fn missing_argument_missing_path_and_file_input_fail_without_changing_state() {
    let directory = TestDirectory::new("invalid-input");
    let workspace = directory.0.join("workspace");
    let initial = directory.0.join("initial");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&initial).unwrap();
    fs::write(initial.join("active.pgn"), pgn("Active", "Active Player")).unwrap();
    assert!(ingest(&workspace, &initial).status.success());
    let state_path = workspace.join("inspector/workspace-state.json");
    let before = fs::read(&state_path).unwrap();

    let missing_argument = command(&workspace, &["ingest"]);
    assert!(!missing_argument.status.success());
    assert!(stderr(&missing_argument).contains("antiskill ingest DIR"));

    let missing_path = ingest(&workspace, &directory.0.join("absent"));
    assert!(!missing_path.status.success());
    assert!(stderr(&missing_path).contains("Directory input non valida"));

    let file = directory.0.join("file.pgn");
    fs::write(&file, pgn("File", "Player")).unwrap();
    let not_directory = ingest(&workspace, &file);
    assert!(!not_directory.status.success());
    assert!(stderr(&not_directory).contains("Input non è una directory"));

    let extra = command(&workspace, &["ingest", "one", "two"]);
    assert!(!extra.status.success());
    assert!(stderr(&extra).contains("esattamente un argomento DIR"));
    assert_eq!(fs::read(state_path).unwrap(), before);
}

#[test]
fn ingest_help_describes_source_snapshot_and_current_workspace() {
    let directory = TestDirectory::new("help");
    let top = command(&directory.0, &["--help"]);
    assert!(top.status.success());
    assert!(stdout(&top).contains("ingest"));
    assert!(stdout(&top).contains("antiskill ingest --help"));

    let help = command(&directory.0, &["ingest", "--help"]);
    assert!(help.status.success());
    let text = stdout(&help);
    assert!(text.contains("antiskill ingest DIR"));
    assert!(text.contains("directory sorgente PGN"));
    assert!(text.contains("workspace è la\ndirectory corrente"));
    assert!(text.contains("snapshot corrente"));
}

#[test]
fn inspector_ingest_directory_keeps_technical_output_contract() {
    let directory = TestDirectory::new("technical-regression");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("pgn");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("game.pgn"), pgn("Technical", "Player")).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root"])
        .arg(&workspace)
        .args(["--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", stderr(&result));
    let text = stdout(&result);
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 16);
    assert!(lines[0].starts_with("FILE\t\"game.pgn\"\tingested\t\""));
    assert_eq!(lines[0].split('\t').count(), 6);
    let labels = lines[1..]
        .iter()
        .map(|line| line.split_once('\t').unwrap().0)
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [
            "SCANNED_FILES",
            "PGN_FILES",
            "INGESTED",
            "REUSED_PREVIOUS",
            "REUSED_WORKSPACE",
            "DUPLICATE_SOURCES",
            "FAILED",
            "SKIPPED_SYMLINKS",
            "SOURCES",
            "GAMES",
            "COLLECTION_BLAKE3",
            "COLLECTION_BYTES",
            "NEW_MANIFEST",
            "COLLECTION",
            "INGEST_RESULT",
        ]
    );
    assert!(text.ends_with("INGEST_RESULT\tPASS\n"));

    let empty = directory.0.join("empty");
    fs::create_dir(&empty).unwrap();
    let technical_empty = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root"])
        .arg(&workspace)
        .args(["--input"])
        .arg(&empty)
        .output()
        .unwrap();
    assert!(
        technical_empty.status.success(),
        "{}",
        stderr(&technical_empty)
    );
    assert!(stdout(&technical_empty).contains("PGN_FILES\t0"));
    assert!(stdout(&technical_empty).ends_with("INGEST_RESULT\tPASS\n"));
}
