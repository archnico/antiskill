mod support;

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = support::unique_temp_path("antiskill-players-cli");
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

fn sample_pgn() -> &'static str {
    "[Event \"AuthoritativeA\"]\n[White \"Esempio, Auròra\"]\n[Black \"Opponent A\"]\n[WhiteFideId \"900000001\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n\
[Event \"AuthoritativeB\"]\n[White \"Aurora' Esempio\"]\n[Black \"Opponent B\"]\n[WhiteFideId \"00900000001\"]\n[Result \"1-0\"]\n\n1. d4 d5 1-0\n\n\
[Event \"Candidate\"]\n[White \"Esempio, Aurora`\"]\n[Black \"Opponent C\"]\n[Result \"1-0\"]\n\n1. c4 c5 1-0\n\n\
[Event \"DifferentId\"]\n[White \"Esempio, Auròra\"]\n[Black \"Opponent D\"]\n[WhiteFideId \"900000002\"]\n[Result \"1-0\"]\n\n1. Nf3 Nf6 1-0\n\n\
[Event \"Conflict\"]\n[White \"Esempio, Auròra\"]\n[Black \"Opponent E\"]\n[WhiteFideId \"900000001\"]\n[WhiteFideId \"900000002\"]\n[Result \"1-0\"]\n\n1. g3 g6 1-0\n"
}

fn setup() -> TestDirectory {
    let directory = TestDirectory::new();
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    fs::write(input.join("sample.pgn"), sample_pgn()).unwrap();
    let ingest = command(&workspace, &["ingest", input.to_str().unwrap()]);
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    directory
}

#[test]
fn players_groups_only_unambiguous_fide_id_claims() {
    let directory = setup();
    let workspace = directory.0.join("workspace");
    let output = command(
        &workspace,
        &["players", "--fide-id", "900000001", "--format", "json"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["fide_id"], "900000001");
    assert!(view["player_entity_id"]
        .as_str()
        .unwrap()
        .starts_with("p1-"));
    assert_eq!(view["claims"].as_array().unwrap().len(), 2);
    assert!(view["claims"]
        .as_array()
        .unwrap()
        .iter()
        .all(|claim| claim["occurrence_id"].as_str().unwrap().starts_with("g1-")));
    assert_eq!(view["conflicts"].as_array().unwrap().len(), 1);
    assert!(view["conflicts"][0]["occurrence_id"]
        .as_str()
        .unwrap()
        .starts_with("g1-"));
    assert_eq!(view["candidates"].as_array().unwrap().len(), 0);
    assert_eq!(view["candidates_scanned"], false);
    let aliases = view["aliases"].as_array().unwrap();
    assert!(aliases.iter().any(|value| value == "Esempio, Auròra"));
    assert!(aliases.iter().any(|value| value == "Aurora' Esempio"));
}

#[test]
fn players_reports_name_only_candidates_without_merging_different_ids() {
    let directory = setup();
    let workspace = directory.0.join("workspace");
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "900000001",
            "--candidates",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["claims"].as_array().unwrap().len(), 2);
    assert_eq!(view["candidates"].as_array().unwrap().len(), 1);
    assert!(view["candidates"][0]["occurrence_id"]
        .as_str()
        .unwrap()
        .starts_with("g1-"));
    assert_eq!(view["candidates"][0]["names"][0], "Esempio, Aurora`");
    assert_eq!(view["candidates_scanned"], true);
}

#[test]
fn players_export_is_conservative_unless_candidates_are_explicit() {
    let directory = setup();
    let workspace = directory.0.join("workspace");
    let authoritative = directory.0.join("authoritative.pgn");
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "900000001",
            "--export",
            authoritative.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(&authoritative).unwrap();
    assert!(text.contains("AuthoritativeA"));
    assert!(text.contains("AuthoritativeB"));
    assert!(!text.contains("Candidate"));
    assert!(!text.contains("DifferentId"));
    assert!(!text.contains("Conflict"));

    let with_candidates = directory.0.join("with-candidates.pgn");
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "900000001",
            "--candidates",
            "--export",
            with_candidates.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(&with_candidates).unwrap();
    assert!(text.contains("Candidate"));
    assert!(!text.contains("DifferentId"));
    assert!(!text.contains("Conflict"));
}

fn write_federation_index(directory: &TestDirectory) -> PathBuf {
    let path = directory.0.join("federation-index.jsonl");
    let record = json!({
        "profile": "antiskill_torneionline_allin_identity_bridge_v1",
        "source_blake3": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "source_bytes": 100,
        "source_line": 43566,
        "raw_name": "ESEMPIO AURORA '",
        "name_signature": "aurora esempio",
        "signature_unique": true,
        "fsi_id": "900001",
        "fide_id": "900000001",
        "birth_year": 1990,
        "birth_date": "01-01-1990",
        "federation": "ITA"
    });
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&record).unwrap()),
    )
    .unwrap();
    path
}

fn setup_federated_without_homonym() -> TestDirectory {
    let directory = TestDirectory::new();
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    let pgn = "[Event \"Direct\"]\n[White \"Esempio, Auròra\"]\n[Black \"Opponent A\"]\n[WhiteFideId \"900000001\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n\
[Event \"MalformedAlias\"]\n[White \"Aurora, Esempio\"]\n[Black \"Opponent B\"]\n[Result \"1-0\"]\n\n1. d4 d5 1-0\n";
    fs::write(input.join("sample.pgn"), pgn).unwrap();
    let ingest = command(&workspace, &["ingest", input.to_str().unwrap()]);
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    directory
}

#[test]
fn federation_v2_promotes_unique_token_equivalent_alias_and_exports_it() {
    let directory = setup_federated_without_homonym();
    let workspace = directory.0.join("workspace");
    let index = write_federation_index(&directory);
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "900000001",
            "--federation-index",
            index.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["profile"], "antiskill_player_identity_view_v2");
    assert_eq!(view["direct_count"], 1);
    assert_eq!(view["promoted_count"], 1);
    assert_eq!(view["verified_occurrences"].as_array().unwrap().len(), 2);
    assert_eq!(view["unresolved_candidates"].as_array().unwrap().len(), 0);
    assert!(view["verified_occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |row| row["evidence_class"] == "verified_alias" && row["names"][0] == "Aurora, Esempio"
        ));
    assert!(view["external_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| id["namespace"] == "fsi" && id["value"] == "900001"));

    let export = directory.0.join("verified.pgn");
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "900000001",
            "--federation-index",
            index.to_str().unwrap(),
            "--export",
            export.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(export).unwrap();
    assert!(text.contains("Direct"));
    assert!(text.contains("MalformedAlias"));
}

#[test]
fn federation_v2_blocks_alias_when_same_signature_has_other_fide_id() {
    let directory = setup();
    let workspace = directory.0.join("workspace");
    let index = write_federation_index(&directory);
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "900000001",
            "--federation-index",
            index.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["direct_count"], 2);
    assert_eq!(view["promoted_count"], 0);
    assert_eq!(view["unresolved_candidates"].as_array().unwrap().len(), 1);
    assert_eq!(view["blocked_signatures"][0], "aurora esempio");
}

fn synthetic_context_id() -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"antiskill_tpol_official_context_v1");
    hasher.update(b"\0");
    hasher.update(b"SYN2025A");
    hasher.update(b"\0");
    hasher.update(b"1");
    hasher.update(b"\0");
    hasher.update(b"1");
    hasher.update(b"\0");
    hasher.update(b"19");
    hasher.update(b"\0");
    hasher.update(b"1-0");
    format!("tc1-{}", hasher.finalize().to_hex())
}

fn write_context_index(directory: &TestDirectory) -> PathBuf {
    let path = directory.0.join("context-index.jsonl");
    let record = json!({
        "profile": "antiskill_tpol_official_context_v1",
        "context_id": synthetic_context_id(),
        "event_code": "SYN2025A",
        "event_name": "TORNEO AURORA 2025",
        "event_start_date": "2025.02.15",
        "event_end_date": "2025.02.16",
        "round": 1,
        "board": 1,
        "white_slot": 1,
        "black_slot": 19,
        "white_name": "Alfa, Aurora",
        "black_name": "Beta, Bruno",
        "white_fide_id": "910000001",
        "black_fide_id": "910000002",
        "white_fsi_id": "910001",
        "black_fsi_id": "910002",
        "result": "1-0",
        "roster_source_blake3": "b".repeat(64),
        "results_source_blake3": "c".repeat(64),
        "crosstable_source_blake3": "d".repeat(64),
        "evidence_class": "official_context_join"
    });
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&record).unwrap()),
    )
    .unwrap();
    path
}

fn setup_context_workspace(duplicate: bool) -> TestDirectory {
    let directory = TestDirectory::new();
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    let first = "[Event \"TORNEO AURORA 2025\"]\n[Date \"2025.02.15\"]\n[Round \"1\"]\n[White \"TARGET NAME COMPLETELY WRONG\"]\n[Black \"Beta, Bruno\"]\n[BlackFideId \"910000002\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n";
    let mut pgn = first.to_owned();
    if duplicate {
        pgn.push_str("\n[Event \"TORNEO AURORA 2025\"]\n[Date \"2025.02.15\"]\n[Round \"1\"]\n[White \"ANOTHER WRONG TARGET NAME\"]\n[Black \"Beta, Bruno\"]\n[BlackFideId \"910000002\"]\n[Result \"1-0\"]\n\n1. d4 d5 1-0\n");
    }
    fs::write(input.join("context.pgn"), pgn).unwrap();
    let ingest = command(&workspace, &["ingest", input.to_str().unwrap()]);
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    directory
}

#[test]
fn official_context_v3_identifies_deliberately_destroyed_target_name() {
    let directory = setup_context_workspace(false);
    let workspace = directory.0.join("workspace");
    let index = write_context_index(&directory);
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "00910000001",
            "--context-index",
            index.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["profile"], "antiskill_player_identity_view_v3");
    assert_eq!(view["fide_id"], "910000001");
    assert_eq!(view["direct_count"], 0);
    assert_eq!(view["promoted_count"], 0);
    assert_eq!(view["context_count"], 1);
    assert_eq!(view["verified_occurrences"].as_array().unwrap().len(), 1);
    assert_eq!(
        view["verified_occurrences"][0]["evidence_class"],
        "official_context_join"
    );
    assert_eq!(
        view["verified_occurrences"][0]["names"][0],
        "TARGET NAME COMPLETELY WRONG"
    );
    assert_eq!(view["verified_occurrences"][0]["raw_fide_ids"], json!([]));
    assert_eq!(view["context_evidence"][0]["opponent_fide_id"], "910000002");
    assert_eq!(view["unresolved_context_candidates"], json!([]));

    let export = directory.0.join("context-verified.pgn");
    let exported = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "910000001",
            "--context-index",
            index.to_str().unwrap(),
            "--export",
            export.to_str().unwrap(),
        ],
    );
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let exported_bytes = fs::read(&export).unwrap();
    let original_bytes = fs::read(directory.0.join("input/context.pgn")).unwrap();
    assert_eq!(exported_bytes, original_bytes);
}

#[test]
fn official_context_v3_fails_closed_when_two_occurrences_fit_one_context() {
    let directory = setup_context_workspace(true);
    let workspace = directory.0.join("workspace");
    let index = write_context_index(&directory);
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "910000001",
            "--context-index",
            index.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["context_count"], 0);
    assert_eq!(view["verified_occurrences"], json!([]));
    assert_eq!(
        view["unresolved_context_candidates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(view["unresolved_context_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|candidate| candidate["matching_context_ids"].as_array().unwrap().len() == 1));
}

fn synthetic_context_id_for(
    event: &str,
    round: u32,
    white_slot: u32,
    black_slot: u32,
    result: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"antiskill_tpol_official_context_v1");
    hasher.update(b"\0");
    hasher.update(event.as_bytes());
    hasher.update(b"\0");
    hasher.update(round.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(white_slot.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(black_slot.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(result.as_bytes());
    format!("tc1-{}", hasher.finalize().to_hex())
}

fn write_second_context_index(directory: &TestDirectory) -> PathBuf {
    let path = directory.0.join("context-index-second.jsonl");
    let record = json!({
        "profile": "antiskill_tpol_official_context_v1",
        "context_id": synthetic_context_id_for("SYN2025B", 1, 5, 7, "1-0"),
        "event_code": "SYN2025B",
        "event_name": "SECONDO TORNEO 2025",
        "event_start_date": "2025.03.01",
        "event_end_date": "2025.03.02",
        "round": 1,
        "board": 2,
        "white_slot": 5,
        "black_slot": 7,
        "white_name": "Alfa, Aurora",
        "black_name": "Opponent Two",
        "white_fide_id": "910000001",
        "black_fide_id": "910000003",
        "white_fsi_id": "910001",
        "black_fsi_id": null,
        "result": "1-0",
        "roster_source_blake3": "e".repeat(64),
        "results_source_blake3": "f".repeat(64),
        "crosstable_source_blake3": "1".repeat(64),
        "evidence_class": "official_context_join"
    });
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&record).unwrap()),
    )
    .unwrap();
    path
}

fn setup_multi_context_workspace() -> TestDirectory {
    let directory = TestDirectory::new();
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    let pgn = "[Event \"TORNEO AURORA 2025\"]\n[Date \"2025.02.15\"]\n[Round \"1\"]\n[White \"FIRST DESTROYED TARGET\"]\n[Black \"Beta, Bruno\"]\n[BlackFideId \"910000002\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n\
[Event \"SECONDO TORNEO 2025\"]\n[Date \"2025.03.01\"]\n[Round \"1\"]\n[White \"SECOND DESTROYED TARGET\"]\n[Black \"Opponent Two\"]\n[BlackFideId \"910000003\"]\n[Result \"1-0\"]\n\n1. d4 d5 1-0\n";
    fs::write(input.join("multi-context.pgn"), pgn).unwrap();
    let ingest = command(&workspace, &["ingest", input.to_str().unwrap()]);
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    directory
}

#[test]
fn official_context_registry_spans_multiple_events_in_one_player_query() {
    let directory = setup_multi_context_workspace();
    let workspace = directory.0.join("workspace");
    let first = write_context_index(&directory);
    let second = write_second_context_index(&directory);
    let registry = directory.0.join("context-registry");
    let build = command(
        &workspace,
        &[
            "sources",
            "registry",
            "torneionline-tpol",
            "--root",
            registry.to_str().unwrap(),
            "--index",
            second.to_str().unwrap(),
            "--index",
            first.to_str().unwrap(),
        ],
    );
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "910000001",
            "--context-registry",
            registry.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["profile"], "antiskill_player_identity_view_v3");
    assert_eq!(view["context_count"], 2);
    assert_eq!(view["verified_occurrences"].as_array().unwrap().len(), 2);
    let aliases = view["aliases"].as_array().unwrap();
    assert!(aliases
        .iter()
        .any(|value| value == "FIRST DESTROYED TARGET"));
    assert!(aliases
        .iter()
        .any(|value| value == "SECOND DESTROYED TARGET"));
    let evidence = view["context_evidence"].as_array().unwrap();
    assert_eq!(evidence.len(), 2);
    assert_eq!(evidence[0]["event_code"], "SYN2025A");
    assert_eq!(evidence[1]["event_code"], "SYN2025B");
}

fn synthetic_vesus_context_id() -> String {
    let mut hasher = blake3::Hasher::new();
    for part in [
        "antiskill_vesus_official_context_v1",
        "SYNV0001",
        "SYNT0001",
        "1",
        "1",
        "1",
        "31",
        "1-0",
    ] {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("vc1-{}", hasher.finalize().to_hex())
}

fn write_vesus_context_index(directory: &TestDirectory) -> PathBuf {
    let path = directory.0.join("vesus-context-index.jsonl");
    let record = json!({
        "profile": "antiskill_vesus_official_context_v1",
        "context_id": synthetic_vesus_context_id(),
        "event_code": "SYNV0001",
        "event_name": "4° Rapid Città Sintetica",
        "event_start_date": "2026.09.13",
        "event_end_date": "2026.09.13",
        "tournament_key": "SYNT0001",
        "tournament_name": "Open Fide",
        "round": 1,
        "board": 1,
        "white_slot": 1,
        "black_slot": 31,
        "white_name": "Alfa, Aurora",
        "black_name": "Beta, Bruno",
        "white_fide_id": "920000001",
        "black_fide_id": "920000002",
        "white_fsi_id": "920001",
        "black_fsi_id": "920002",
        "result": "1-0",
        "roster_source_blake3": "a".repeat(64),
        "results_source_blake3": "b".repeat(64),
        "crosstable_source_blake3": "c".repeat(64),
        "evidence_class": "official_context_join"
    });
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&record).unwrap()),
    )
    .unwrap();
    path
}

fn setup_vesus_context_workspace(
    duplicate: bool,
    contradictory_target_fide: bool,
) -> TestDirectory {
    let directory = TestDirectory::new();
    let workspace = directory.0.join("workspace");
    let input = directory.0.join("input");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&input).unwrap();
    let target_id = if contradictory_target_fide {
        "[WhiteFideId \"9999999\"]\n"
    } else {
        ""
    };
    let first = format!("[Event \"4° Rapid Città Sintetica\"]\n[Date \"2026.09.13\"]\n[Round \"1\"]\n[White \"TARGET NAME COMPLETELY WRONG\"]\n{target_id}[Black \"Beta, Bruno\"]\n[BlackFideId \"920000002\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n");
    let mut pgn = first;
    if duplicate {
        pgn.push_str("\n[Event \"4° Rapid Città Sintetica\"]\n[Date \"2026.09.13\"]\n[Round \"1\"]\n[White \"ANOTHER WRONG TARGET NAME\"]\n[Black \"Beta, Bruno\"]\n[BlackFideId \"920000002\"]\n[Result \"1-0\"]\n\n1. d4 d5 1-0\n");
    }
    fs::write(input.join("vesus.pgn"), pgn).unwrap();
    let ingest = command(&workspace, &["ingest", input.to_str().unwrap()]);
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    directory
}

#[test]
fn vesus_context_v3_identifies_destroyed_target_name_with_utf8_event() {
    let directory = setup_vesus_context_workspace(false, false);
    let workspace = directory.0.join("workspace");
    let index = write_vesus_context_index(&directory);
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "920000001",
            "--context-index",
            index.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["context_count"], 1);
    assert_eq!(
        view["verified_occurrences"][0]["names"][0],
        "TARGET NAME COMPLETELY WRONG"
    );
    assert!(view["context_evidence"][0]["context_id"]
        .as_str()
        .unwrap()
        .starts_with("vc1-"));
}

#[test]
fn vesus_context_v3_fails_closed_on_ambiguous_occurrences() {
    let directory = setup_vesus_context_workspace(true, false);
    let workspace = directory.0.join("workspace");
    let index = write_vesus_context_index(&directory);
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "920000001",
            "--context-index",
            index.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["context_count"], 0);
    assert_eq!(view["verified_occurrences"], json!([]));
    assert_eq!(
        view["unresolved_context_candidates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn vesus_context_v3_does_not_override_contradictory_target_fide() {
    let directory = setup_vesus_context_workspace(false, true);
    let workspace = directory.0.join("workspace");
    let index = write_vesus_context_index(&directory);
    let output = command(
        &workspace,
        &[
            "players",
            "--fide-id",
            "920000001",
            "--context-index",
            index.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(view["context_count"], 0);
    assert_eq!(view["verified_occurrences"], json!([]));
}
