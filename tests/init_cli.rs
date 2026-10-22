mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = support::unique_temp_path(&format!("antiskill-init-cli-{label}"));
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

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn entry_names(path: &Path) -> Vec<String> {
    let mut names = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn assert_exact_initialized(workspace: &Path) {
    assert_eq!(entry_names(workspace), ["inspector"]);
    let inspector = workspace.join("inspector");
    let metadata = fs::symlink_metadata(&inspector).unwrap();
    assert!(metadata.is_dir());
    assert!(!metadata.file_type().is_symlink());
    assert!(entry_names(&inspector).is_empty());
    assert!(!inspector.join("workspace-state.json").exists());
}

#[test]
fn help_and_argument_contract_are_product_facing() {
    let directory = TestDirectory::new("help");
    let top = command(&directory.0, &["--help"]);
    assert!(top.status.success());
    let top = stdout(&top);
    let init = top
        .find("    init         Crea un nuovo workspace")
        .unwrap();
    let ingest = top.find("    ingest       Attiva lo snapshot PGN").unwrap();
    assert!(init < ingest);
    assert!(top.contains("antiskill init --help"));

    for argument in ["help", "--help", "-h"] {
        let output = command(&directory.0, &["init", argument]);
        assert!(output.status.success());
        let help = stdout(&output);
        assert!(help.contains("antiskill init DIR"));
        assert!(help.contains("relativo o assoluto"));
        assert!(help.contains("parent deve esistere"));
        assert!(help.contains("antiskill ingest DIR"));
    }

    for args in [
        vec!["init"],
        vec!["init", "one", "two"],
        vec!["init", "--root"],
    ] {
        let output = command(&directory.0, &args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(stderr(&output).contains("ERRORE init:"));
        assert!(stderr(&output).contains("antiskill init DIR"));
    }
}

#[test]
fn relative_and_absolute_missing_targets_create_only_workspace_and_inspector() {
    let directory = TestDirectory::new("paths");

    let relative = command(&directory.0, &["init", "relative-workspace"]);
    assert!(relative.status.success(), "{}", stderr(&relative));
    assert!(relative.stderr.is_empty());
    let relative_path = directory.0.join("relative-workspace");
    assert_exact_initialized(&relative_path);
    let relative_canonical = fs::canonicalize(&relative_path).unwrap();
    let relative_output = stdout(&relative);
    assert!(relative_output.contains(&format!(
        "Workspace creato: {}",
        relative_canonical.display()
    )));
    assert!(relative_output.contains("Prossimo passo: cd "));
    assert!(relative_output.contains("antiskill ingest DIR"));
    for forbidden in ["inspector/", "Collection", "schema", "hash"] {
        assert!(!relative_output.contains(forbidden));
    }

    let absolute_path = directory.0.join("absolute-workspace");
    let absolute = command(&directory.0, &["init", absolute_path.to_str().unwrap()]);
    assert!(absolute.status.success(), "{}", stderr(&absolute));
    assert_exact_initialized(&absolute_path);
    assert!(stdout(&absolute).contains(&format!(
        "Workspace creato: {}",
        fs::canonicalize(&absolute_path).unwrap().display()
    )));
}

#[test]
fn relative_path_with_current_directory_component_is_accepted() {
    let directory = TestDirectory::new("dot-path");
    let output = command(&directory.0, &["init", "./dot-workspace"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_exact_initialized(&directory.0.join("dot-workspace"));
}

#[test]
fn existing_empty_target_is_initialized_but_files_and_nonempty_targets_are_unchanged() {
    let directory = TestDirectory::new("existing");

    let empty = directory.0.join("empty");
    fs::create_dir(&empty).unwrap();
    let initialized = command(&directory.0, &["init", "empty"]);
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    assert!(stdout(&initialized).contains("Workspace inizializzato:"));
    assert_exact_initialized(&empty);

    let file = directory.0.join("file");
    fs::write(&file, b"do not replace").unwrap();
    let rejected_file = command(&directory.0, &["init", "file"]);
    assert!(!rejected_file.status.success());
    assert_eq!(fs::read(&file).unwrap(), b"do not replace");

    let nonempty = directory.0.join("nonempty");
    fs::create_dir(&nonempty).unwrap();
    fs::create_dir(nonempty.join("nested")).unwrap();
    fs::write(nonempty.join("nested/keep.bin"), b"preserve exactly").unwrap();
    let before_names = entry_names(&nonempty);
    let before_bytes = fs::read(nonempty.join("nested/keep.bin")).unwrap();
    let rejected_nonempty = command(&directory.0, &["init", "nonempty"]);
    assert!(!rejected_nonempty.status.success());
    assert!(stderr(&rejected_nonempty).contains("non è vuota"));
    assert_eq!(entry_names(&nonempty), before_names);
    assert_eq!(
        fs::read(nonempty.join("nested/keep.bin")).unwrap(),
        before_bytes
    );
    assert!(!nonempty.join("inspector").exists());
}

#[test]
fn missing_file_and_ambiguous_parents_fail_without_creating_a_chain() {
    let directory = TestDirectory::new("parents");

    let missing = command(&directory.0, &["init", "missing/workspace"]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("Parent del workspace inesistente"));
    assert!(!directory.0.join("missing").exists());

    let parent_file = directory.0.join("parent-file");
    fs::write(&parent_file, b"parent bytes").unwrap();
    let file_parent = command(&directory.0, &["init", "parent-file/workspace"]);
    assert!(!file_parent.status.success());
    assert!(stderr(&file_parent).contains("non è una directory reale"));
    assert_eq!(fs::read(&parent_file).unwrap(), b"parent bytes");

    let real = directory.0.join("real");
    fs::create_dir(&real).unwrap();
    let traversal = command(&directory.0, &["init", "real/../escaped"]);
    assert!(!traversal.status.success());
    assert!(stderr(&traversal).contains("Percorso workspace non sicuro"));
    assert!(!directory.0.join("escaped").exists());
}

#[cfg(unix)]
#[test]
fn target_and_parent_symlinks_are_rejected_without_touching_destinations() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new("symlinks");
    let outside = directory.0.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), b"outside bytes").unwrap();

    let target_link = directory.0.join("target-link");
    symlink(&outside, &target_link).unwrap();
    let target = command(&directory.0, &["init", "target-link"]);
    assert!(!target.status.success());
    assert!(stderr(&target).contains("target workspace è un symlink"));
    assert!(fs::symlink_metadata(&target_link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read(outside.join("keep")).unwrap(), b"outside bytes");
    assert!(!outside.join("inspector").exists());

    let parent_link = directory.0.join("parent-link");
    symlink(&outside, &parent_link).unwrap();
    let parent = command(&directory.0, &["init", "parent-link/workspace"]);
    assert!(!parent.status.success());
    assert!(stderr(&parent).contains("non è una directory reale"));
    assert!(!outside.join("workspace").exists());
    assert_eq!(fs::read(outside.join("keep")).unwrap(), b"outside bytes");

    fs::create_dir(outside.join("nested")).unwrap();
    let nested = command(&directory.0, &["init", "parent-link/nested/workspace"]);
    assert!(!nested.status.success());
    assert!(stderr(&nested).contains("symlink o un path ambiguo"));
    assert!(!outside.join("nested/workspace").exists());
}

#[test]
fn initialized_workspace_has_no_active_collection_until_normal_ingest() {
    let directory = TestDirectory::new("e2e");
    let input = directory.0.join("input");
    fs::create_dir(&input).unwrap();
    fs::write(
        input.join("game.pgn"),
        b"[Event \"Init E2E\"]\n[White \"Kasparov\"]\n[Black \"Opponent\"]\n[Date \"2026.08.11\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n",
    )
    .unwrap();

    let initialized = command(&directory.0, &["init", "workspace"]);
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    let workspace = directory.0.join("workspace");
    assert_exact_initialized(&workspace);

    let before_ingest = command(&workspace, &["search", "--player", "Kasparov"]);
    assert!(!before_ingest.status.success());
    assert!(stderr(&before_ingest).contains("Stato workspace mancante"));
    assert_exact_initialized(&workspace);

    let ingested = command(&workspace, &["ingest", input.to_str().unwrap()]);
    assert!(ingested.status.success(), "{}", stderr(&ingested));
    assert!(workspace.join("inspector/workspace-state.json").is_file());
    let search = command(&workspace, &["search", "--player", "Kasparov"]);
    assert!(search.status.success(), "{}", stderr(&search));
    assert!(stdout(&search).starts_with("1 partita trovata."));
}
