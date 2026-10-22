mod support;

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn test_root() -> PathBuf {
    support::unique_temp_path("antiskill-dedup-cli")
}

fn output_field<'a>(stdout: &'a str, field: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{field}\t")))
        .unwrap_or_else(|| panic!("campo {field} assente dall'output"))
}

fn run_success(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(args)
        .output()
        .expect("esecuzione CLI");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout UTF-8")
}

fn logical_json(path: &Path) -> Vec<u8> {
    let bytes = fs::read(path).unwrap();
    if path.to_string_lossy().ends_with(".json.xz") {
        let mut decoder = xz2::read::XzDecoder::new(bytes.as_slice());
        let mut logical = Vec::new();
        decoder.read_to_end(&mut logical).unwrap();
        logical
    } else {
        bytes
    }
}

fn create_mainlines(workspace: &Path, source: &Path) -> String {
    let root = workspace.to_str().unwrap();
    let source = source.to_str().unwrap();

    let imported = run_success(&["inspector", "import", "--root", root, source]);
    let segmented = run_success(&[
        "inspector",
        "segment",
        "--root",
        root,
        "--manifest",
        output_field(&imported, "MANIFEST"),
    ]);
    let tokenized = run_success(&[
        "inspector",
        "tokenize",
        "--root",
        root,
        "--segmentation",
        output_field(&segmented, "SEGMENTATION"),
    ]);
    let parsed = run_success(&[
        "inspector",
        "parse",
        "--root",
        root,
        "--tokenization",
        output_field(&tokenized, "TOKENIZATION"),
    ]);
    let interpreted = run_success(&[
        "inspector",
        "interpret",
        "--root",
        root,
        "--parse",
        output_field(&parsed, "PARSE"),
    ]);
    let catalog = run_success(&[
        "inspector",
        "catalog",
        "--root",
        root,
        "--interpretation",
        output_field(&interpreted, "INTERPRETATION"),
    ]);
    let occurrences = run_success(&[
        "inspector",
        "create-occurrences",
        "--root",
        root,
        "--catalog",
        output_field(&catalog, "CATALOG"),
    ]);
    let mainlines = run_success(&[
        "inspector",
        "create-mainlines",
        "--root",
        root,
        "--occurrences",
        output_field(&occurrences, "OCCURRENCES"),
    ]);

    output_field(&mainlines, "MAINLINES_ARTIFACT").to_owned()
}

fn dedup(workspace: &Path, mainlines: &[&str], format: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_antiskill"));
    command
        .args(["dedup", "canonical-mainline", "--root"])
        .arg(workspace);

    for mainline in mainlines {
        command.args(["--mainlines", mainline]);
    }

    if let Some(format) = format {
        command.args(["--format", format]);
    }

    command.output().expect("esecuzione dedup")
}

fn dedup_success(workspace: &Path, mainlines: &[&str], format: Option<&str>) -> String {
    let output = dedup(workspace, mainlines, format);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).expect("stdout UTF-8")
}

fn dedup_collection(workspace: &Path, collection: &str, format: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_antiskill"));
    command
        .args(["dedup", "canonical-mainline", "--root"])
        .arg(workspace)
        .args(["--collection", collection]);
    if let Some(format) = format {
        command.args(["--format", format]);
    }
    command.output().expect("esecuzione dedup collection")
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let mut entries = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let path = entry.path();
            let file_type = entry.file_type().unwrap();

            if file_type.is_dir() {
                visit(root, &path, files);
            } else if file_type.is_file() {
                files.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

struct Fixture {
    directory: PathBuf,
    workspace: PathBuf,
    first: String,
    second: String,
}

fn fixture() -> Fixture {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();

    let exact_one = "[Event \"Exact One\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n";
    let exact_two = "[Event \"Exact Two\"]\n[Result \"0-1\"]\n\n1. e4 e5 0-1\n";

    let first_source = format!(
        "{exact_one}\n\
         {exact_one}\n\
         [Event \"Comments and RAV\"]\n[Result \"1-0\"]\n\n1. e4 {{kept}} e5 (1... c5) 1-0\n\n\
         {exact_two}\n\
         {exact_two}\n\
         [Event \"Singleton\"]\n[Result \"*\"]\n\n1. d4 d5 *\n\n\
         [Event \"Non comparable\"]\n[Result \"*\"]\n\n1. c4 BadSan *\n"
    );

    let second_source =
        b"[Event \"Second Source\"]\n[Site \"Elsewhere\"]\n[Result \"1/2-1/2\"]\n\n1. e4 e5 1/2-1/2\n";

    let first_path = input.join("first.pgn");
    let second_path = input.join("second.pgn");
    fs::write(&first_path, first_source).unwrap();
    fs::write(&second_path, second_source).unwrap();

    let first = create_mainlines(&workspace, &first_path);
    let second = create_mainlines(&workspace, &second_path);

    Fixture {
        directory,
        workspace,
        first,
        second,
    }
}

#[test]
fn canonical_mainline_report_is_deterministic_and_preserves_evidence() {
    let fixture = fixture();
    let before = snapshot(&fixture.workspace);

    let text = dedup_success(&fixture.workspace, &[&fixture.first, &fixture.second], None);
    let reversed = dedup_success(&fixture.workspace, &[&fixture.second, &fixture.first], None);

    assert_eq!(text, reversed);
    assert_eq!(
        output_field(&text, "DEDUP_PROFILE"),
        "same_canonical_mainline_v1"
    );
    assert!(output_field(&text, "INPUT_ARTIFACTS").starts_with("2\t"));
    assert_eq!(output_field(&text, "COMPARABLE_OCCURRENCES"), "7");
    assert_eq!(output_field(&text, "NONCOMPARABLE_OCCURRENCES"), "1");
    assert_eq!(output_field(&text, "GROUPS"), "1");
    assert_eq!(output_field(&text, "GROUPED_OCCURRENCES"), "6");
    assert_eq!(output_field(&text, "GROUP_SIZE_DISTRIBUTION"), "6:1");
    assert_eq!(text.matches("\nEXACT_BYTES\t").count(), 2);
    assert_eq!(text.matches("\nOCCURRENCE\t").count(), 6);
    assert_eq!(text.matches("\nNONCOMPARABLE\t").count(), 1);
    assert!(text.contains("same_mainline_different_bytes=true"));
    assert!(text.contains(r#"distinct_results=[["0-1"],["1-0"],["1/2-1/2"]]"#));
    assert!(text.contains("result_conflict=true"));
    assert!(text.contains("reasons=non_applied_move") || text.contains("non_applied_move"));
    assert!(text.ends_with("DEDUP_REPORT\tPASS\n"));

    let jsonl = dedup_success(
        &fixture.workspace,
        &[&fixture.first, &fixture.second],
        Some("jsonl"),
    );
    let reversed_jsonl = dedup_success(
        &fixture.workspace,
        &[&fixture.second, &fixture.first],
        Some("jsonl"),
    );
    assert_eq!(jsonl, reversed_jsonl);
    assert!(jsonl.ends_with('\n'));

    let records = jsonl
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(records.len(), 4);
    assert_eq!(records[0]["record_type"], "summary");
    assert_eq!(records[0]["schema_version"], 1);
    assert_eq!(records[0]["dedup_profile"], "same_canonical_mainline_v1");
    assert_eq!(
        records[0]["canonical_mainline_profile"],
        "antiskill_canonical_mainline_v1"
    );
    assert_eq!(records[0]["input_artifacts"].as_array().unwrap().len(), 2);
    assert_eq!(records[0]["comparable_occurrences"], 7);
    assert_eq!(records[0]["noncomparable_occurrences"], 1);
    assert_eq!(records[0]["groups"], 1);
    assert_eq!(records[0]["grouped_occurrences"], 6);

    assert_eq!(records[1]["record_type"], "group_size");
    assert_eq!(records[1]["size"], 6);
    assert_eq!(records[1]["groups"], 1);

    let group = &records[2];
    assert_eq!(group["record_type"], "group");
    assert_eq!(group["occurrence_count"], 6);
    assert_eq!(group["logical_source_count"], 2);
    assert_eq!(group["byte_variant_count"], 4);
    assert_eq!(group["same_mainline_different_bytes"], true);
    assert_eq!(group["result_conflict"], true);
    assert_eq!(group["members"].as_array().unwrap().len(), 6);
    assert_eq!(group["exact_byte_subgroups"].as_array().unwrap().len(), 2);
    assert!(group["exact_byte_subgroups"]
        .as_array()
        .unwrap()
        .iter()
        .all(|subgroup| subgroup["occurrence_ids"].as_array().unwrap().len() == 2));
    assert_eq!(group["distinct_results"].as_array().unwrap().len(), 3);

    let canonical_id = group["canonical_mainline_id"].as_str().unwrap();
    let mut expected = blake3::Hasher::new();
    expected.update(b"antiskill_same_canonical_mainline_v1\0antiskill_canonical_mainline_v1\0");
    expected.update(canonical_id.as_bytes());
    expected.update(b"\n");
    assert_eq!(
        group["group_id"].as_str().unwrap(),
        expected.finalize().to_hex().as_str()
    );

    let members = group["members"].as_array().unwrap();
    assert!(members.windows(2).all(|pair| {
        pair[0]["occurrence_id"].as_str().unwrap() < pair[1]["occurrence_id"].as_str().unwrap()
    }));
    assert!(members.iter().all(|member| {
        member["mainline_artifact"]["blake3"]
            .as_str()
            .unwrap()
            .len()
            == 64
            && member["provenance"]["occurrences_manifest"]["blake3"]
                .as_str()
                .unwrap()
                .len()
                == 64
    }));

    assert_eq!(records[3]["record_type"], "non_comparable");
    assert!(records[3]["canonical_mainline_id"].is_null());
    assert!(records[3]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "non_applied_move"));

    assert_eq!(before, snapshot(&fixture.workspace));
    fs::remove_dir_all(fixture.directory).unwrap();
}

#[test]
fn canonical_mainline_rejects_duplicate_missing_and_tampered_inputs_without_stdout() {
    let fixture = fixture();

    let duplicate = dedup(&fixture.workspace, &[&fixture.first, &fixture.first], None);
    assert!(!duplicate.status.success());
    assert!(duplicate.stdout.is_empty());
    assert!(String::from_utf8_lossy(&duplicate.stderr)
        .contains("Artefatto canonical mainline duplicato"));

    let missing = dedup(
        &fixture.workspace,
        &[&fixture.first, "inspector/mainlines/missing.json.xz"],
        None,
    );
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());

    let artifact = fixture.workspace.join(&fixture.second);
    fs::write(&artifact, b"tampered").unwrap();

    let tampered = dedup(
        &fixture.workspace,
        &[&fixture.first, &fixture.second],
        Some("jsonl"),
    );
    assert!(!tampered.status.success());
    assert!(tampered.stdout.is_empty());

    fs::remove_dir_all(fixture.directory).unwrap();
}

#[test]
fn canonical_mainline_rejects_repeated_occurrence_id_across_distinct_artifacts() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let first_dir = directory.join("first");
    let second_dir = directory.join("second");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&first_dir).unwrap();
    fs::create_dir_all(&second_dir).unwrap();

    let bytes = b"[Event \"Same\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n";
    let first_source = first_dir.join("same.pgn");
    let second_source = second_dir.join("same.pgn");
    fs::write(&first_source, bytes).unwrap();
    fs::write(&second_source, bytes).unwrap();

    let first = create_mainlines(&workspace, &first_source);
    let second = create_mainlines(&workspace, &second_source);

    assert_ne!(
        first, second,
        "la fixture deve produrre due artefatti mainline distinti"
    );

    let output = dedup(&workspace, &[&first, &second], Some("jsonl"));

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("occurrence_id presente in più artefatti canonical mainline"));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn collection_input_is_equivalent_to_the_declared_mainlines_and_exclusive() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(
        input.join("one.pgn"),
        b"[Event \"One\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n[Event \"Bad\"]\n[Result \"*\"]\n\n1. c4 BadSan *\n",
    )
    .unwrap();
    fs::write(
        input.join("two.pgn"),
        b"[Event \"Two\"]\n[Result \"0-1\"]\n\n1. e4 e5 0-1\n",
    )
    .unwrap();

    let ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
    ]);
    let collection = output_field(&ingest, "COLLECTION");
    let manifest: serde_json::Value =
        serde_json::from_slice(&logical_json(&workspace.join(collection))).unwrap();
    assert_eq!(manifest["schema_version"], 2);
    let mainlines = manifest["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["mainlines"].as_str().unwrap())
        .collect::<Vec<_>>();

    for format in [None, Some("jsonl")] {
        let manual = dedup_success(&workspace, &mainlines, format);
        let via_collection = dedup_collection(&workspace, collection, format);
        assert!(via_collection.status.success());
        assert!(via_collection.stderr.is_empty());
        assert_eq!(via_collection.stdout, manual.as_bytes());
    }

    let mixed = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["dedup", "canonical-mainline", "--root"])
        .arg(&workspace)
        .args(["--collection", collection, "--mainlines", mainlines[0]])
        .output()
        .unwrap();
    assert!(!mixed.status.success());
    assert!(mixed.stdout.is_empty());
    assert!(String::from_utf8_lossy(&mixed.stderr).contains("mutuamente esclusivi"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "pilot manuale: richiede ANTISKILL_REAL_PILOT_PGN"]
fn pilot_reale_canonical_mainline_dedup() {
    let source = PathBuf::from(
        std::env::var_os("ANTISKILL_REAL_PILOT_PGN").expect("impostare ANTISKILL_REAL_PILOT_PGN"),
    );
    assert!(source.is_file(), "ANTISKILL_REAL_PILOT_PGN non è un file");

    let original = fs::read(&source).unwrap();
    let original_hash = blake3::hash(&original);

    let directory = test_root();
    let workspace = directory.join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let mainlines = create_mainlines(&workspace, &source);

    let text = dedup_success(&workspace, &[&mainlines], None);
    assert_eq!(
        output_field(&text, "DEDUP_PROFILE"),
        "same_canonical_mainline_v1"
    );
    assert!(text.ends_with("DEDUP_REPORT\tPASS\n"));

    let comparable = output_field(&text, "COMPARABLE_OCCURRENCES")
        .parse::<u64>()
        .unwrap();
    let grouped = output_field(&text, "GROUPED_OCCURRENCES")
        .parse::<u64>()
        .unwrap();
    assert!(grouped <= comparable);

    let jsonl = dedup_success(&workspace, &[&mainlines], Some("jsonl"));
    let summary: serde_json::Value = serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();
    assert_eq!(summary["record_type"], "summary");
    assert_eq!(summary["dedup_profile"], "same_canonical_mainline_v1");
    assert_eq!(summary["comparable_occurrences"], comparable);
    assert_eq!(summary["grouped_occurrences"], grouped);

    assert_eq!(
        blake3::hash(&fs::read(&source).unwrap()),
        original_hash,
        "il pilot non deve modificare il PGN sorgente"
    );

    fs::remove_dir_all(directory).unwrap();
}
