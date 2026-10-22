mod support;

use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = support::unique_temp_path(&format!("antiskill-unique-games-cli-{label}"));
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

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn sample_pgn() -> &'static str {
    "[Event \"A\"]\n[White \"Alpha\"]\n[Black \"Beta\"]\n[Date \"2026.09.11\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n[Event \"B\"]\n[White \"Gamma\"]\n[Black \"Delta\"]\n[Date \"2026.09.11\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n[Event \"Empty\"]\n[White \"Epsilon\"]\n[Black \"Zeta\"]\n[Date \"2026.09.11\"]\n[Result \"*\"]\n\n*\n"
}

fn read_xz_json(path: &Path) -> Value {
    let file = fs::File::open(path).unwrap();
    let mut decoder = xz2::read::XzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
#[test]
fn unique_games_collapses_only_comparable_canonical_mainlines() {
    let directory = TestDirectory::new("canonical");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("sample.pgn"), sample_pgn()).unwrap();

    let ingest = command(&workspace, &["ingest", input.to_str().unwrap()]);
    assert!(ingest.status.success(), "{}", stderr(&ingest));

    let first = command(&workspace, &["unique-games", "--format", "json"]);
    assert!(first.status.success(), "{}", stderr(&first));
    let first_stdout = stdout(&first);
    let summary: Value = serde_json::from_str(first_stdout.trim()).unwrap();
    assert_eq!(summary["schema"], "antiskill.unique_games.v1");
    assert_eq!(summary["source_occurrences"], 3);
    assert_eq!(summary["comparable_occurrences"], 2);
    assert_eq!(summary["noncomparable_singletons"], 1);
    assert_eq!(summary["logical_games"], 2);
    assert_eq!(summary["collapsed_occurrences"], 1);
    assert_eq!(summary["duplicate_logical_games"], 1);
    assert_eq!(summary["max_occurrences_per_logical_game"], 2);
    let groups_relative = summary["groups_path"].as_str().unwrap();
    let groups = read_xz_json(&workspace.join(groups_relative));
    let group = &groups["groups"][0];
    assert_eq!(group["occurrence_count"], 2);
    let occurrence_ids = group["occurrence_ids"].as_array().unwrap();
    assert_eq!(occurrence_ids.len(), 2);
    assert!(occurrence_ids[0].as_str().unwrap() < occurrence_ids[1].as_str().unwrap());
    assert_eq!(group["representative_occurrence_id"], occurrence_ids[0]);

    let second = command(&workspace, &["unique-games", "--format", "json"]);
    assert!(second.status.success(), "{}", stderr(&second));
    assert_eq!(stdout(&second), first_stdout);

    let search = command(&workspace, &["search"]);
    assert!(search.status.success(), "{}", stderr(&search));
    assert!(stdout(&search).starts_with("3 partite trovate."));

    let scratch = workspace.join("raw/tmp");
    assert!(scratch.is_dir());
    assert_eq!(fs::read_dir(scratch).unwrap().count(), 0);
}

#[test]
fn unique_games_prefers_metadata_rich_exact_occurrence_as_representative() {
    let directory = TestDirectory::new("representative-quality");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    let pgn = "[Event \"\"]\n[Site \"\"]\n[Date \"????.??.??\"]\n[Round \"\"]\n[White \"\"]\n[Black \"\"]\n[Result \"0-1\"]\n[GameURL \"https://example.invalid/poor\"]\n\n1. d4 d5 2. c4 e6 0-1\n\n[Event \"Rich Event\"]\n[Site \"Florence ITA\"]\n[Date \"2026.09.13\"]\n[Round \"1\"]\n[White \"Rich, Alice\"]\n[Black \"Named, Bob\"]\n[Result \"0-1\"]\n[WhiteElo \"1900\"]\n[BlackElo \"2000\"]\n\n1. d4 d5 2. c4 e6 0-1\n";
    fs::write(input.join("sample.pgn"), pgn).unwrap();

    let ingest = command(&workspace, &["ingest", input.to_str().unwrap()]);
    assert!(ingest.status.success(), "{}", stderr(&ingest));

    let rich_search = command(
        &workspace,
        &["search", "--format", "json", "--player", "Rich, Alice"],
    );
    assert!(rich_search.status.success(), "{}", stderr(&rich_search));
    let search_json: Value = serde_json::from_slice(&rich_search.stdout).unwrap();
    let rich_occurrence = search_json["games"][0]["id"].as_str().unwrap();

    let summary_output = command(&workspace, &["unique-games", "--format", "json"]);
    assert!(
        summary_output.status.success(),
        "{}",
        stderr(&summary_output)
    );
    let summary: Value = serde_json::from_slice(&summary_output.stdout).unwrap();
    assert_eq!(summary["source_occurrences"], 2);
    assert_eq!(summary["logical_games"], 1);
    let groups = read_xz_json(&workspace.join(summary["groups_path"].as_str().unwrap()));
    assert_eq!(groups["profile"], "antiskill_unique_game_groups_v2");
    assert_eq!(
        groups["representative_policy"],
        "core_metadata_completeness_v1__lowest_public_occurrence_id_tiebreak"
    );
    let group = &groups["groups"][0];
    assert_eq!(group["occurrence_count"], 2);
    assert_eq!(group["representative_occurrence_id"], rich_occurrence);

    let listing = command(&workspace, &["unique-games", "--list"]);
    assert!(listing.status.success(), "{}", stderr(&listing));
    let records = stdout(&listing)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        records[0]["representative_policy"],
        groups["representative_policy"]
    );
    assert_eq!(records[1]["representative_occurrence_id"], rich_occurrence);
}

#[test]
fn unique_games_help_and_format_errors_are_product_facing() {
    let directory = TestDirectory::new("help");
    for argument in ["help", "--help", "-h"] {
        let output = command(&directory.0, &["unique-games", argument]);
        assert!(output.status.success());
        let help = stdout(&output);
        assert!(help.contains("antiskill unique-games [--format human|json]"));
        assert!(help.contains("non distruttiva"));
    }
    let invalid = command(&directory.0, &["unique-games", "--format", "yaml"]);
    assert!(!invalid.status.success());
    assert!(stderr(&invalid).contains("Formato unique-games non supportato: yaml"));
}

#[test]
fn unique_games_fails_closed_on_corrupted_source_index() {
    let directory = TestDirectory::new("corrupt-index");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("sample.pgn"), sample_pgn()).unwrap();

    let ingest = command(&workspace, &["ingest", input.to_str().unwrap()]);
    assert!(ingest.status.success(), "{}", stderr(&ingest));

    let indexes = workspace.join("inspector/source-indexes");
    let index = fs::read_dir(indexes)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "bin"))
        .unwrap();
    let mut bytes = fs::read(&index).unwrap();
    let last = bytes.last_mut().unwrap();
    *last ^= 1;
    fs::write(&index, bytes).unwrap();

    let output = command(&workspace, &["unique-games", "--format", "json"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("SourceIndex"));
    assert!(!workspace.join("inspector/unique_game_views").exists());
    let scratch = workspace.join("raw/tmp");
    assert!(scratch.is_dir());
    assert_eq!(fs::read_dir(scratch).unwrap().count(), 0);
}

#[test]
fn unique_games_list_streams_complete_logical_game_records() {
    let directory = TestDirectory::new("listing");
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("sample.pgn"), sample_pgn()).unwrap();
    let ingest = command(&workspace, &["ingest", input.to_str().unwrap()]);
    assert!(ingest.status.success(), "{}", stderr(&ingest));

    let output = command(&workspace, &["unique-games", "--list"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let records = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 4);
    assert_eq!(records[0]["record"], "header");
    assert_eq!(records[0]["source_occurrences"], 3);
    assert_eq!(records[0]["logical_games"], 2);
    assert_eq!(records[0]["complete"], false);
    let games = &records[1..3];
    assert!(games
        .iter()
        .all(|record| record["record"] == "logical_game"));
    assert!(games.iter().all(|record| record["logical_game_id"]
        .as_str()
        .unwrap()
        .starts_with("u1-")));
    assert_eq!(
        games
            .iter()
            .map(|record| record["occurrence_count"].as_u64().unwrap())
            .sum::<u64>(),
        3
    );
    let comparable = games
        .iter()
        .find(|record| !record["canonical_mainline_id"].is_null())
        .unwrap();
    assert_eq!(comparable["occurrence_count"], 2);
    assert_eq!(comparable["occurrence_ids"].as_array().unwrap().len(), 2);
    let singleton = games
        .iter()
        .find(|record| record["canonical_mainline_id"].is_null())
        .unwrap();
    assert_eq!(singleton["occurrence_count"], 1);
    for game in games {
        let occurrences = game["occurrence_ids"].as_array().unwrap();
        assert_eq!(game["representative_occurrence_id"], occurrences[0]);
        assert!(occurrences
            .iter()
            .all(|id| id.as_str().unwrap().starts_with("g1-")));
    }
    assert_eq!(records[3]["record"], "end");
    assert_eq!(records[3]["source_occurrences"], 3);
    assert_eq!(records[3]["logical_games"], 2);
    assert_eq!(records[3]["complete"], true);

    let scratch = workspace.join("raw/tmp");
    assert!(scratch.is_dir());
    assert_eq!(fs::read_dir(scratch).unwrap().count(), 0);
}
