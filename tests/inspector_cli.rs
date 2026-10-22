mod support;

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;

fn logical_json(path: impl AsRef<std::path::Path>) -> Vec<u8> {
    let path = path.as_ref();
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

fn publish_test_collection(root: &std::path::Path, logical: &[u8]) -> String {
    publish_test_json(root, "collections", logical)
}

fn publish_test_json(root: &std::path::Path, directory: &str, logical: &[u8]) -> String {
    let hash = blake3::hash(logical).to_hex().to_string();
    let relative = format!("inspector/{directory}/{hash}.json.xz");
    let path = root.join(&relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
    encoder.write_all(logical).unwrap();
    fs::write(path, encoder.finish().unwrap()).unwrap();
    relative
}

fn test_root() -> PathBuf {
    support::unique_temp_path("antiskill-inspector-cli")
}

fn real_pilot_pgn() -> PathBuf {
    let source = std::env::var_os("ANTISKILL_REAL_PILOT_PGN")
        .map(PathBuf::from)
        .expect("ANTISKILL_REAL_PILOT_PGN must point to the real pilot PGN");
    assert!(
        source.is_file(),
        "ANTISKILL_REAL_PILOT_PGN does not point to a file"
    );
    source
}

fn copy_directory(source: &std::path::Path, target: &std::path::Path) {
    fs::create_dir(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        let destination = target.join(entry.file_name());
        if kind.is_dir() {
            copy_directory(&entry.path(), &destination);
        } else if kind.is_file() {
            fs::copy(entry.path(), destination).unwrap();
        } else {
            panic!("fixture workspace contiene un tipo di file inatteso");
        }
    }
}

fn search(workspace: &std::path::Path, filters: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .arg("search")
        .args(filters)
        .current_dir(workspace)
        .output()
        .unwrap()
}

fn technical_query(workspace: &std::path::Path, filters: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "query-collection", "--root"])
        .arg(workspace)
        .args(filters)
        .output()
        .unwrap()
}

#[test]
fn cli_search_uses_cwd_active_collection_and_existing_filter_semantics() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = workspace.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    let alpha = b"[Event \"Summer Open\"]\n[White \"Alice\"]\n[Black \"Esempio\"]\n[Date \"2026.08.01\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n";
    let beta = b"[Event \"Winter Cup\"]\n[White \"Carol\"]\n[Black \"Dave\"]\n[Date \"2026.08.02\"]\n[Result \"0-1\"]\n\n1. d4 d5 0-1\n";
    let gamma = b"[Event \"Summer Side\"]\n[White \"Esempio\"]\n[Black \"Eve\"]\n[Date \"2026.08.03\"]\n[Result \"1/2-1/2\"]\n\n1. c4 c5 1/2-1/2\n";
    fs::write(input.join("alpha.pgn"), alpha).unwrap();
    fs::write(input.join("beta.pgn"), beta).unwrap();
    fs::write(input.join("gamma.pgn"), gamma).unwrap();
    let ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
    ]);
    let collection = output_field(&ingest, "COLLECTION").to_owned();

    let player = search(&workspace, &["--player", "eSeMpIo"]);
    assert!(
        player.status.success(),
        "{}",
        String::from_utf8_lossy(&player.stderr)
    );
    let player_stdout = String::from_utf8(player.stdout).unwrap();
    let player_lines = player_stdout.lines().collect::<Vec<_>>();
    assert_eq!(player_lines[0], "2 partite trovate.");
    assert_eq!(player_lines[2], "ID\tDate\tWhite\tBlack\tResult\tEvent");
    let rows = player_lines[3..]
        .iter()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.len(), 6);
        assert!(row[0].starts_with("g1-"));
        assert_eq!(row[0].len(), 67);
        assert!(row[0][3..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
    }
    assert_eq!(
        rows.iter().map(|row| &row[1..]).collect::<Vec<_>>(),
        [
            ["2026.08.03", "Esempio", "Eve", "1/2-1/2", "Summer Side"].as_slice(),
            ["2026.08.01", "Alice", "Esempio", "1-0", "Summer Open"].as_slice(),
        ]
    );
    let alpha_hash = blake3::hash(alpha).to_hex().to_string();
    for forbidden in [
        "inspector/",
        "alpha.pgn",
        "source_blake3",
        "source_start",
        "segment_ordinal",
        &alpha_hash,
    ] {
        assert!(!player_stdout.contains(forbidden), "esposto: {forbidden}");
    }

    let explicit_human = search(&workspace, &["--player", "eSeMpIo", "--format", "human"]);
    assert!(explicit_human.status.success());
    assert_eq!(explicit_human.stdout, player_stdout.as_bytes());

    let player_json = search(&workspace, &["--format", "json", "--player", "eSeMpIo"]);
    assert!(
        player_json.status.success(),
        "{}",
        String::from_utf8_lossy(&player_json.stderr)
    );
    assert!(player_json.stdout.ends_with(b"\n"));
    assert_eq!(
        player_json
            .stdout
            .iter()
            .filter(|&&byte| byte == b'\n')
            .count(),
        1
    );
    let player_document: serde_json::Value = serde_json::from_slice(&player_json.stdout).unwrap();
    assert_eq!(player_document["schema"], "antiskill.search.v1");
    let json_games = player_document["games"].as_array().unwrap();
    assert_eq!(json_games.len(), rows.len());
    assert_eq!(
        json_games
            .iter()
            .map(|game| game["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        rows.iter().map(|row| row[0]).collect::<Vec<_>>()
    );
    for (game, row) in json_games.iter().zip(&rows) {
        let keys = game
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(keys.len(), 6);
        for expected in ["id", "date", "white", "black", "result", "event"] {
            assert!(
                keys.contains(&expected),
                "campo pubblico mancante: {expected}"
            );
        }
        assert_eq!(game["date"], serde_json::json!([row[1]]));
        assert_eq!(game["white"], serde_json::json!([row[2]]));
        assert_eq!(game["black"], serde_json::json!([row[3]]));
        assert_eq!(game["result"], serde_json::json!([row[4]]));
        assert_eq!(game["event"], serde_json::json!([row[5]]));
    }
    let repeated_player_json = search(&workspace, &["--player", "eSeMpIo", "--format", "json"]);
    assert_eq!(repeated_player_json.stdout, player_json.stdout);

    for (filters, expected) in [
        (vec!["--player", "eSeMpIo"], 2),
        (vec!["--event", "summer"], 2),
        (vec!["--date", "2026-08-02"], 1),
        (vec!["--date-from", "2026-08-02"], 2),
        (vec!["--date-to", "2026-08-02"], 2),
        (
            vec!["--date-from", "2026-08-02", "--date-to", "2026-08-03"],
            2,
        ),
    ] {
        let human = search(&workspace, &filters);
        let mut json_filters = vec!["--format", "json"];
        json_filters.extend(filters.iter().copied());
        let json = search(&workspace, &json_filters);
        let technical = technical_query(&workspace, &filters);
        assert!(human.status.success(), "filtri: {filters:?}");
        assert!(json.status.success(), "filtri: {filters:?}");
        assert!(technical.status.success(), "filtri: {filters:?}");
        let human_output = String::from_utf8(human.stdout).unwrap();
        let human_count = human_output
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let human_ids = human_output
            .lines()
            .skip(3)
            .map(|line| line.split('\t').next().unwrap())
            .collect::<Vec<_>>();
        let json_document: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
        let json_games = json_document["games"].as_array().unwrap();
        let technical_count =
            output_field(&String::from_utf8(technical.stdout).unwrap(), "MATCHES")
                .parse::<usize>()
                .unwrap();
        assert_eq!(human_count, expected, "filtri: {filters:?}");
        assert_eq!(json_games.len(), human_count, "filtri: {filters:?}");
        assert_eq!(
            json_games
                .iter()
                .map(|game| game["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            human_ids,
            "filtri: {filters:?}"
        );
        assert_eq!(human_count, technical_count, "filtri: {filters:?}");
    }

    let zero = search(&workspace, &["--player", "Nobody"]);
    assert!(zero.status.success());
    assert_eq!(
        String::from_utf8(zero.stdout).unwrap(),
        "0 partite trovate.\n"
    );
    assert!(zero.stderr.is_empty());

    let zero_json = search(&workspace, &["--player", "Nobody", "--format", "json"]);
    assert!(zero_json.status.success());
    assert_eq!(
        String::from_utf8(zero_json.stdout).unwrap(),
        "{\"schema\":\"antiskill.search.v1\",\"games\":[]}\n"
    );
    assert!(zero_json.stderr.is_empty());

    let technical = technical_query(
        &workspace,
        &["--collection", &collection, "--player", "Alice"],
    );
    assert!(technical.status.success());
    let alpha_observations =
        serde_json::to_string(&vec![input.join("alpha.pgn").to_str().unwrap()]).unwrap();
    assert_eq!(
        String::from_utf8(technical.stdout).unwrap(),
        format!(
            "MATCHES\t1\nGAME\t2\t0\t{}\t{}\t0\t0\t{}\t[\"Alice\"]\t[\"Esempio\"]\t[\"Summer Open\"]\t[\"2026.08.01\"]\t[\"1-0\"]\n",
            alpha_hash,
            alpha_observations,
            alpha.len()
        )
    );

    for invalid in [
        vec!["--format"],
        vec!["--format", "xml"],
        vec!["--format", "json", "--unknown", "value"],
        vec!["--format", "json", "--player"],
        vec!["--format", "json", "--format", "human"],
    ] {
        let output = search(&workspace, &invalid);
        assert!(!output.status.success(), "argomenti: {invalid:?}");
        assert!(output.stdout.is_empty(), "argomenti: {invalid:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("ERRORE search"),
            "argomenti: {invalid:?}"
        );
    }

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_search_json_preserves_multiple_tag_values_as_arrays() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(
        input.join("multiple-tags.pgn"),
        b"[Event \"Main Open\"]\n[Event \"Side Event\"]\n[White \"Alice\"]\n[White \"A. Example\"]\n[Black \"Bob\"]\n[Date \"2026.08.11\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n",
    )
    .unwrap();
    run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
    ]);

    let human = search(&workspace, &["--player", "Alice"]);
    assert!(human.status.success());
    let human_output = String::from_utf8(human.stdout).unwrap();
    let human_id = human_output
        .lines()
        .nth(3)
        .unwrap()
        .split('\t')
        .next()
        .unwrap();
    assert!(human_output.contains("Alice | A. Example"));
    assert!(human_output.contains("Main Open | Side Event"));

    let json = search(&workspace, &["--player", "Alice", "--format", "json"]);
    assert!(json.status.success());
    let document: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let game = &document["games"][0];
    assert_eq!(game["id"], human_id);
    assert_eq!(game["white"], serde_json::json!(["Alice", "A. Example"]));
    assert_eq!(
        game["event"],
        serde_json::json!(["Main Open", "Side Event"])
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_search_fails_closed_for_workspace_state_and_active_collection() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();

    let missing_state = search(&workspace, &["--player", "Alice"]);
    assert!(!missing_state.status.success());
    assert!(missing_state.stdout.is_empty());
    let error = String::from_utf8(missing_state.stderr).unwrap();
    assert!(error.contains("ERRORE search"));
    assert!(error.contains("Stato workspace mancante"));
    assert!(!error.to_lowercase().contains("panic"));

    fs::write(
        input.join("game.pgn"),
        b"[Event \"Open\"]\n[White \"Alice\"]\n[Black \"Bob\"]\n[Result \"*\"]\n\n1. e4 e5 *\n",
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
    let collection = output_field(&ingest, "COLLECTION").to_owned();
    let state_path = workspace.join("inspector/workspace-state.json");
    let valid_state = fs::read(&state_path).unwrap();

    let collection_bytes = output_field(&ingest, "COLLECTION_BYTES")
        .parse::<u64>()
        .unwrap();
    let inconsistent_state = String::from_utf8(valid_state.clone()).unwrap().replace(
        &format!("\"bytes\": {collection_bytes}"),
        &format!("\"bytes\": {}", collection_bytes + 1),
    );
    fs::write(&state_path, inconsistent_state).unwrap();
    let inconsistent_collection = search(&workspace, &["--player", "Alice"]);
    assert!(!inconsistent_collection.status.success());
    assert!(inconsistent_collection.stdout.is_empty());
    assert!(String::from_utf8_lossy(&inconsistent_collection.stderr)
        .contains("Active Collection non verificabile: identità diversa dallo stato"));
    fs::write(&state_path, &valid_state).unwrap();

    fs::write(&state_path, b"{ broken\n").unwrap();
    let corrupt_state = search(&workspace, &["--player", "Alice"]);
    assert!(!corrupt_state.status.success());
    assert!(corrupt_state.stdout.is_empty());
    assert!(String::from_utf8_lossy(&corrupt_state.stderr).contains("Stato workspace corrotto"));
    fs::write(&state_path, valid_state).unwrap();

    let collection_path = workspace.join(&collection);
    let valid_collection = fs::read(&collection_path).unwrap();
    fs::remove_file(&collection_path).unwrap();
    let missing_collection = search(&workspace, &["--player", "Alice"]);
    assert!(!missing_collection.status.success());
    assert!(missing_collection.stdout.is_empty());
    assert!(String::from_utf8_lossy(&missing_collection.stderr)
        .contains("Active Collection non verificabile"));
    fs::write(&collection_path, &valid_collection).unwrap();

    fs::write(&collection_path, b"corrupt").unwrap();
    let corrupt_collection = search(&workspace, &["--player", "Alice"]);
    assert!(!corrupt_collection.status.success());
    assert!(corrupt_collection.stdout.is_empty());
    assert!(String::from_utf8_lossy(&corrupt_collection.stderr)
        .contains("Active Collection non verificabile"));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_help_exposes_user_search() {
    let top = run_success(&["--help"]);
    assert!(top.contains("search"));
    assert!(top.contains("antiskill search --help"));
    let search = run_success(&["search", "--help"]);
    assert!(search.contains("antiskill search"));
    for filter in ["--player", "--event", "--date", "--date-from", "--date-to"] {
        assert!(search.contains(filter));
    }
    assert!(search.contains("--format human|json"));
}

#[test]
fn cli_active_workspace_state_v1_is_portable_verified_and_fail_closed() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let first_input = directory.join("first-input");
    let second_input = directory.join("second-input");
    let failed_input = directory.join("failed-input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&first_input).unwrap();
    fs::create_dir_all(&second_input).unwrap();
    fs::create_dir_all(&failed_input).unwrap();

    let query = |root: &std::path::Path, extra: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_antiskill"));
        command
            .args(["inspector", "query-collection", "--root"])
            .arg(root)
            .args(extra)
            .output()
            .unwrap()
    };

    let missing_state = query(&workspace, &["--player", "Alice"]);
    assert!(!missing_state.status.success());
    assert!(missing_state.stdout.is_empty());
    assert!(String::from_utf8_lossy(&missing_state.stderr).contains("Stato workspace mancante"));

    fs::write(
        first_input.join("alice.pgn"),
        b"[Event \"First\"]\n[White \"Alice\"]\n[Black \"Bob\"]\n[Result \"*\"]\n\n1. e4 e5 *\n",
    )
    .unwrap();
    let first_ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        first_input.to_str().unwrap(),
    ]);
    let first_collection = output_field(&first_ingest, "COLLECTION").to_owned();
    let first_collection_path = workspace.join(&first_collection);
    assert!(first_collection_path.is_file());

    let state_path = workspace.join("inspector/workspace-state.json");
    let first_state_bytes = fs::read(&state_path).unwrap();
    let first_state: serde_json::Value = serde_json::from_slice(&first_state_bytes).unwrap();
    assert_eq!(first_state["schema_version"], 1);
    assert_eq!(
        first_state["workspace_state_profile"],
        "antiskill_workspace_state_v1"
    );
    assert_eq!(first_state["active_collection"]["path"], first_collection);
    assert_eq!(
        first_state["active_collection"]["blake3"],
        output_field(&first_ingest, "COLLECTION_BLAKE3")
    );
    assert_eq!(
        first_state["active_collection"]["bytes"],
        output_field(&first_ingest, "COLLECTION_BYTES")
            .parse::<u64>()
            .unwrap()
    );
    assert!(
        !std::path::Path::new(first_state["active_collection"]["path"].as_str().unwrap())
            .is_absolute()
    );
    assert!(!String::from_utf8(first_state_bytes.clone())
        .unwrap()
        .contains(workspace.to_str().unwrap()));

    let active_first = query(&workspace, &["--player", "Alice"]);
    assert!(active_first.status.success());
    assert_eq!(
        output_field(&String::from_utf8(active_first.stdout).unwrap(), "MATCHES"),
        "1"
    );

    fs::write(
        second_input.join("carol.pgn"),
        b"[Event \"Second\"]\n[White \"Carol\"]\n[Black \"Dave\"]\n[Result \"*\"]\n\n1. d4 d5 *\n",
    )
    .unwrap();
    let second_ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        second_input.to_str().unwrap(),
        "--previous",
        &first_collection,
    ]);
    let second_collection = output_field(&second_ingest, "COLLECTION").to_owned();
    assert_ne!(first_collection, second_collection);
    let valid_state_bytes = fs::read(&state_path).unwrap();
    let valid_state: serde_json::Value = serde_json::from_slice(&valid_state_bytes).unwrap();
    assert_eq!(valid_state["active_collection"]["path"], second_collection);
    assert!(first_collection_path.is_file());
    let verify_previous = run_success(&[
        "inspector",
        "verify-collection",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        &first_collection,
    ]);
    assert_eq!(verify_previous, "COLLECTION_VERIFY\tPASS\n");

    let active_second = query(&workspace, &["--player", "Carol"]);
    assert!(active_second.status.success());
    assert_eq!(
        output_field(&String::from_utf8(active_second.stdout).unwrap(), "MATCHES"),
        "1"
    );
    let explicit_previous = query(
        &workspace,
        &["--collection", &first_collection, "--player", "Carol"],
    );
    assert!(explicit_previous.status.success());
    assert_eq!(
        output_field(
            &String::from_utf8(explicit_previous.stdout).unwrap(),
            "MATCHES"
        ),
        "0"
    );

    fs::write(&state_path, b"{ broken\n").unwrap();
    let corrupt_state = query(&workspace, &["--player", "Carol"]);
    assert!(!corrupt_state.status.success());
    assert!(corrupt_state.stdout.is_empty());
    assert!(String::from_utf8_lossy(&corrupt_state.stderr).contains("Stato workspace corrotto"));
    let override_corrupt_state = query(
        &workspace,
        &["--collection", &first_collection, "--player", "Alice"],
    );
    assert!(override_corrupt_state.status.success());
    fs::write(&state_path, &valid_state_bytes).unwrap();

    let second_collection_path = workspace.join(&second_collection);
    let second_collection_bytes = fs::read(&second_collection_path).unwrap();
    fs::remove_file(&second_collection_path).unwrap();
    let missing_collection = query(&workspace, &["--player", "Carol"]);
    assert!(!missing_collection.status.success());
    assert!(missing_collection.stdout.is_empty());
    assert!(String::from_utf8_lossy(&missing_collection.stderr)
        .contains("Active Collection non verificabile"));
    fs::write(&second_collection_path, &second_collection_bytes).unwrap();

    fs::write(&second_collection_path, b"corrupt").unwrap();
    let corrupt_collection = query(&workspace, &["--player", "Carol"]);
    assert!(!corrupt_collection.status.success());
    assert!(corrupt_collection.stdout.is_empty());
    assert!(String::from_utf8_lossy(&corrupt_collection.stderr)
        .contains("Active Collection non verificabile"));
    fs::write(&second_collection_path, &second_collection_bytes).unwrap();

    fs::write(
        failed_input.join("valid.pgn"),
        b"[Event \"Partial\"]\n[White \"Eve\"]\n[Black \"Frank\"]\n[Result \"*\"]\n\n1. c4 c5 *\n",
    )
    .unwrap();
    fs::write(failed_input.join("empty.pgn"), b"").unwrap();
    let failed_ingest = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root"])
        .arg(&workspace)
        .args(["--input"])
        .arg(&failed_input)
        .output()
        .unwrap();
    assert!(!failed_ingest.status.success());
    let partial_stdout = String::from_utf8(failed_ingest.stdout).unwrap();
    assert_eq!(output_field(&partial_stdout, "INGEST_RESULT"), "PARTIAL");
    assert_eq!(output_field(&partial_stdout, "FAILED"), "1");
    assert_eq!(output_field(&partial_stdout, "SOURCES"), "1");
    let partial_collection = output_field(&partial_stdout, "COLLECTION");
    assert!(workspace.join(partial_collection).is_file());
    assert_eq!(fs::read(&state_path).unwrap(), valid_state_bytes);

    let moved_workspace = directory.join("moved-workspace");
    copy_directory(&workspace, &moved_workspace);
    fs::remove_dir_all(&workspace).unwrap();
    let moved_query = query(&moved_workspace, &["--player", "Carol"]);
    assert!(
        moved_query.status.success(),
        "{}",
        String::from_utf8_lossy(&moved_query.stderr)
    );
    assert_eq!(
        output_field(&String::from_utf8(moved_query.stdout).unwrap(), "MATCHES"),
        "1"
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_collection_ingest_verify_query_and_export() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(input.join("nested")).unwrap();
    fs::create_dir_all(workspace.join("out")).unwrap();
    let first=b"[Event \"Cup\"]\r\n[White \"Alice\"]\r\n[Black \"Bob\"]\r\n[Date \"2026.08.01\"]\r\n[Result \"1-0\"]\r\n[White \"Alice\"]\r\n\r\n1. e4 $1 {kept} e5 (1... c5) 1-0\r\n";
    let second=b"[Event \"Other\"]\n[White \"Carol\"]\n[Black \"Dave\"]\n[Date \"2026.08.02\"]\n[Result \"*\"]\n\n1. d4 d5 *\n";
    fs::write(input.join("nested/a.PGN"), first).unwrap();
    fs::write(input.join("copy.pgn"), first).unwrap();
    fs::write(input.join("z.pgn"), second).unwrap();
    let mut mirrored = b"preamble\n".to_vec();
    mirrored.extend_from_slice(first);
    fs::write(input.join("mirrored.pgn"), mirrored).unwrap();
    fs::write(input.join("ignore.txt"), b"ignored").unwrap();
    let ingest = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root"])
        .arg(&workspace)
        .args(["--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    let stdout = String::from_utf8(ingest.stdout).unwrap();
    assert_eq!(output_field(&stdout, "PGN_FILES"), "4");
    assert_eq!(output_field(&stdout, "SOURCES"), "3");
    assert_eq!(output_field(&stdout, "DUPLICATE_SOURCES"), "1");
    assert_eq!(output_field(&stdout, "INGEST_RESULT"), "PASS");
    let collection = output_field(&stdout, "COLLECTION");
    let repeated = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root"])
        .arg(&workspace)
        .args(["--input"])
        .arg(&input)
        .args(["--previous", collection])
        .output()
        .unwrap();
    assert!(
        repeated.status.success(),
        "{}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    let repeated_stdout = String::from_utf8(repeated.stdout).unwrap();
    assert_eq!(output_field(&repeated_stdout, "REUSED_PREVIOUS"), "3");
    assert_eq!(output_field(&repeated_stdout, "DUPLICATE_SOURCES"), "1");
    let verify = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-collection", "--root"])
        .arg(&workspace)
        .args(["--collection", collection])
        .output()
        .unwrap();
    assert!(verify.status.success());
    assert_eq!(
        String::from_utf8(verify.stdout).unwrap(),
        "COLLECTION_VERIFY\tPASS\n"
    );
    let query = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "query-collection", "--root"])
        .arg(&workspace)
        .args([
            "--collection",
            collection,
            "--player",
            "alice",
            "--event",
            "cup",
            "--date",
            "2026-08-01",
        ])
        .output()
        .unwrap();
    assert!(
        query.status.success(),
        "{}",
        String::from_utf8_lossy(&query.stderr)
    );
    let query = String::from_utf8(query.stdout).unwrap();
    assert_eq!(output_field(&query, "MATCHES"), "2");
    assert!(query.contains("[\"Alice\",\"Alice\"]"));
    assert!(query.contains("copy.pgn"));
    assert!(query.contains("nested/a.PGN"));
    let source_id = query
        .lines()
        .find(|line| line.starts_with("GAME\t"))
        .unwrap()
        .split('\t')
        .nth(1)
        .unwrap();
    let output = PathBuf::from("out/game.pgn");
    let export = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-occurrence", "--root"])
        .arg(&workspace)
        .args([
            "--collection",
            collection,
            "--source-id",
            source_id,
            "--game-id",
            "0",
            "--output",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert_eq!(fs::read(workspace.join(output)).unwrap(), first);
    for (filters, expected) in [
        (vec![], 3),
        (vec!["--player", "alice"], 2),
        (vec!["--player", "Nobody", "--player", "alice"], 2),
        (vec!["--event", "other"], 1),
        (vec!["--date", "2026-08-02"], 1),
        (vec!["--date-from", "2026-08-02"], 1),
        (vec!["--date-to", "2026-08-01"], 2),
        (
            vec!["--date-from", "2026-08-01", "--date-to", "2026-08-02"],
            3,
        ),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_antiskill"));
        command
            .args(["inspector", "query-collection", "--root"])
            .arg(&workspace)
            .args(["--collection", collection])
            .args(filters);
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = String::from_utf8(result.stdout).unwrap();
        assert_eq!(
            output_field(&result, "MATCHES").parse::<u64>().unwrap(),
            expected
        );
    }
    let existing = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-occurrence", "--root"])
        .arg(&workspace)
        .args([
            "--collection",
            collection,
            "--source-id",
            source_id,
            "--game-id",
            "0",
            "--output",
            "out/game.pgn",
        ])
        .output()
        .unwrap();
    assert!(!existing.status.success());
    for (sid, gid) in [("999", "0"), (source_id, "999")] {
        let result = Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(["inspector", "export-occurrence", "--root"])
            .arg(&workspace)
            .args([
                "--collection",
                collection,
                "--source-id",
                sid,
                "--game-id",
                gid,
                "--output",
                "out/missing.pgn",
            ])
            .output()
            .unwrap();
        assert!(!result.status.success());
    }
    let traversal = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-occurrence", "--root"])
        .arg(&workspace)
        .args([
            "--collection",
            collection,
            "--source-id",
            source_id,
            "--game-id",
            "0",
            "--output",
            "../escape.pgn",
        ])
        .output()
        .unwrap();
    assert!(!traversal.status.success());
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        fs::write(workspace.join("out/target.pgn"), b"unchanged").unwrap();
        symlink(
            workspace.join("out/target.pgn"),
            workspace.join("out/link.pgn"),
        )
        .unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(["inspector", "export-occurrence", "--root"])
            .arg(&workspace)
            .args([
                "--collection",
                collection,
                "--source-id",
                source_id,
                "--game-id",
                "0",
                "--output",
                "out/link.pgn",
            ])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert_eq!(
            fs::read(workspace.join("out/target.pgn")).unwrap(),
            b"unchanged"
        );
    }
    assert!(fs::read_dir(workspace.join("out"))
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("occurrence-export")));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "pilot manuale Collection con un PGN reale"]
fn pilot_collection_end_to_end() {
    let real = real_pilot_pgn();
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(workspace.join("out")).unwrap();
    fs::copy(&real, input.join("pilot-real.pgn")).unwrap();
    let original_before = fs::read(&real).unwrap();
    let original_hash = blake3::hash(&original_before);
    let mut prefixed = b"unclassified pilot prefix\n".to_vec();
    prefixed.extend_from_slice(&original_before);
    fs::write(input.join("pilot-real-prefixed.PGN"), prefixed).unwrap();
    let synthetic=b"[Event \"Synthetic\"]\n[White \"Pilot\"]\n[Black \"Other\"]\n[Date \"2026.08.01\"]\n[Result \"*\"]\n\n1. d4 d5 *\n";
    fs::write(input.join("synthetic.pgn"), synthetic).unwrap();
    let ingest = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root"])
        .arg(&workspace)
        .args(["--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    let stdout = String::from_utf8(ingest.stdout).unwrap();
    assert_eq!(output_field(&stdout, "SOURCES"), "3");
    assert!(output_field(&stdout, "GAMES").parse::<u64>().unwrap() >= 119);
    assert_eq!(output_field(&stdout, "DUPLICATE_SOURCES"), "0");
    let collection = output_field(&stdout, "COLLECTION");
    let verify = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-collection", "--root"])
        .arg(&workspace)
        .args(["--collection", collection])
        .output()
        .unwrap();
    assert!(verify.status.success());
    assert_eq!(
        String::from_utf8(verify.stdout).unwrap(),
        "COLLECTION_VERIFY\tPASS\n"
    );
    let duplicate_args = [
        "inspector",
        "duplicates",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        collection,
    ];
    let duplicate_text = run_success(&duplicate_args);
    assert_eq!(duplicate_text, run_success(&duplicate_args));
    assert!(
        output_field(&duplicate_text, "GROUPS")
            .parse::<u64>()
            .unwrap()
            > 0
    );
    let mut duplicate_json_args = duplicate_args.to_vec();
    duplicate_json_args.extend(["--format", "jsonl"]);
    let duplicate_json = run_success(&duplicate_json_args);
    assert_eq!(duplicate_json, run_success(&duplicate_json_args));
    let group: serde_json::Value =
        serde_json::from_str(duplicate_json.lines().nth(1).unwrap()).unwrap();
    let occurrences = group["occurrences"].as_array().unwrap();
    assert!(occurrences.len() >= 2);
    assert_ne!(occurrences[0]["source_id"], occurrences[1]["source_id"]);
    let mut exported = Vec::new();
    for (index, occurrence) in occurrences.iter().take(2).enumerate() {
        let output = format!("out/duplicate-{index}.pgn");
        run_success(&[
            "inspector",
            "export-occurrence",
            "--root",
            workspace.to_str().unwrap(),
            "--collection",
            collection,
            "--source-id",
            &occurrence["source_id"].as_u64().unwrap().to_string(),
            "--game-id",
            &occurrence["game_id"].as_u64().unwrap().to_string(),
            "--output",
            &output,
        ]);
        exported.push(fs::read(workspace.join(output)).unwrap());
    }
    assert_eq!(exported[0], exported[1]);
    let query = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "query-collection", "--root"])
        .arg(&workspace)
        .args(["--collection", collection])
        .output()
        .unwrap();
    assert!(query.status.success());
    let query = String::from_utf8(query.stdout).unwrap();
    assert!(output_field(&query, "MATCHES").parse::<u64>().unwrap() > 0);
    let fields = query
        .lines()
        .find(|line| line.starts_with("GAME\t") && line.contains("/pilot-real.pgn\""))
        .unwrap()
        .split('\t')
        .collect::<Vec<_>>();
    let source_id = fields[1];
    let game_id = fields[2];
    let start = fields[6].parse::<usize>().unwrap();
    let end = fields[7].parse::<usize>().unwrap();
    let output = PathBuf::from("out/real-occurrence.pgn");
    let export = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-occurrence", "--root"])
        .arg(&workspace)
        .args([
            "--collection",
            collection,
            "--source-id",
            source_id,
            "--game-id",
            game_id,
            "--output",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let real_bytes = fs::read(&real).unwrap();
    assert_eq!(
        fs::read(workspace.join(output)).unwrap(),
        real_bytes[start..end]
    );
    let second = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "ingest-directory", "--root"])
        .arg(&workspace)
        .args(["--input"])
        .arg(&input)
        .args(["--previous", collection])
        .output()
        .unwrap();
    assert!(second.status.success());
    let second = String::from_utf8(second.stdout).unwrap();
    assert!(
        output_field(&second, "REUSED_PREVIOUS")
            .parse::<u64>()
            .unwrap()
            > 0
    );
    assert_eq!(blake3::hash(&fs::read(&real).unwrap()), original_hash);
    let files = workspace_files(&workspace);
    let derived = [
        "segmentations",
        "tokenizations",
        "parses",
        "interpretations",
        "catalogs",
        "collections",
    ];
    let mut logical_bytes = 0_u64;
    let mut compressed_bytes = 0_u64;
    let mut legacy_json = 0_u64;
    let mut workspace_bytes = 0_u64;
    for (relative, _) in &files {
        let path = workspace.join(relative);
        workspace_bytes += fs::metadata(&path).unwrap().len();
        if derived
            .iter()
            .any(|area| relative.starts_with(&format!("inspector/{area}/")))
        {
            if relative.ends_with(".json.xz") {
                compressed_bytes += fs::metadata(&path).unwrap().len();
                logical_bytes += logical_json(&path).len() as u64;
            } else if relative.ends_with(".json") {
                legacy_json += 1;
                logical_bytes += fs::metadata(&path).unwrap().len();
            }
        }
    }
    println!(
        "PILOT_COLLECTION\tPASS\tinput_bytes={}\tlogical_json_bytes={}\tcompressed_bytes={}\tlegacy_json={}\tphysical_logical_ratio={:.6}\tworkspace_expansion={:.6}\tsources=3\tgames={}\tduplicate_groups={}",
        original_before.len(), logical_bytes, compressed_bytes, legacy_json,
        compressed_bytes as f64 / logical_bytes as f64,
        workspace_bytes as f64 / original_before.len() as f64,
        output_field(&stdout, "GAMES"),
        output_field(&duplicate_text, "GROUPS")
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_import_export_is_byte_exact_and_idempotent() {
    let root = test_root();
    fs::create_dir_all(&root).expect("creazione workspace");
    let source = root.join("source.pgn");
    let output = root.join("exported.pgn");
    let bytes = b"[Event \"CLI\"]\r\n[Annotator \"\xff\"]\r\n\r\n1. e4 $1 {test} *\r\n";
    fs::write(&source, bytes).expect("scrittura sorgente");

    let first = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "import", "--root"])
        .arg(&root)
        .arg(&source)
        .output()
        .expect("esecuzione primo import");
    assert!(
        first.status.success(),
        "primo import fallito: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_stdout = String::from_utf8(first.stdout).expect("stdout UTF-8");
    assert!(first_stdout.contains("NEW_OBJECT\ttrue\n"));
    assert!(first_stdout.contains(&format!("BYTES_ADDED\t{}\n", bytes.len())));
    let manifest = first_stdout
        .lines()
        .find_map(|line| line.strip_prefix("MANIFEST\t"))
        .expect("percorso manifest nell'output");

    let second = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "import", "--root"])
        .arg(&root)
        .arg(&source)
        .output()
        .expect("esecuzione secondo import");
    assert!(
        second.status.success(),
        "secondo import fallito: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_stdout = String::from_utf8(second.stdout).expect("stdout UTF-8");
    assert!(second_stdout.contains("NEW_OBJECT\tfalse\n"));
    assert!(second_stdout.contains("BYTES_ADDED\t0\n"));

    let export = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-source", "--root"])
        .arg(&root)
        .args(["--manifest", manifest, "--output"])
        .arg(&output)
        .output()
        .expect("esecuzione export");
    assert!(
        export.status.success(),
        "export fallito: {}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert_eq!(fs::read(&output).expect("lettura export"), bytes);

    fs::remove_dir_all(root).expect("pulizia directory test");
}

#[test]
fn cli_manifest_path_is_reusable_with_relative_root() {
    let directory = test_root();
    let relative_root = PathBuf::from("workspace");
    let root = directory.join(&relative_root);
    fs::create_dir_all(&root).expect("creazione workspace");
    let source = root.join("source.pgn");
    let output = directory.join("exported.pgn");
    let bytes = b"[Event \"Relative root\"]\n\n1. d4 d5 *\n";
    fs::write(&source, bytes).expect("scrittura sorgente");

    let import = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "import", "--root"])
        .arg(&relative_root)
        .arg(relative_root.join("source.pgn"))
        .output()
        .expect("esecuzione import");
    assert!(
        import.status.success(),
        "import fallito: {}",
        String::from_utf8_lossy(&import.stderr)
    );
    let stdout = String::from_utf8(import.stdout).expect("stdout UTF-8");
    let manifest = stdout
        .lines()
        .find_map(|line| line.strip_prefix("MANIFEST\t"))
        .expect("percorso manifest nell'output");
    assert!(manifest.starts_with("inspector/manifests/"));
    assert!(!manifest.starts_with("workspace/"));

    let export = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "export-source", "--root"])
        .arg(&relative_root)
        .args(["--manifest", manifest, "--output"])
        .arg(&output)
        .output()
        .expect("esecuzione export");
    assert!(
        export.status.success(),
        "export fallito: {}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert_eq!(fs::read(&output).expect("lettura export"), bytes);

    fs::remove_dir_all(directory).expect("pulizia directory test");
}

#[test]
fn cli_position_id_is_counter_independent_and_reports_canonical_key() {
    let a = run_success(&[
        "inspector",
        "position-id",
        "--fen",
        "7k/8/8/8/8/8/8/K7 w - - 0 1",
    ]);
    let b = run_success(&[
        "inspector",
        "position-id",
        "--fen",
        "7k/8/8/8/8/8/8/K7 w - - 99 42",
    ]);
    assert_eq!(
        output_field(&a, "POSITION_ID"),
        output_field(&b, "POSITION_ID")
    );
    assert_eq!(output_field(&a, "POSITION_KEY"), "7k/8/8/8/8/8/8/K7 w - -");
    assert_eq!(
        output_field(&a, "POSITION_PROFILE"),
        "antiskill_position_standard_v1"
    );
}

#[test]
fn cli_position_id_rejects_illegal_position() {
    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args([
            "inspector",
            "position-id",
            "--fen",
            "8/8/8/8/8/8/8/8 w - - 0 1",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("illegal_position"));
}

#[test]
fn cli_validate_annotations_exercises_attributed_contract() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("annotations.json");
    fs::write(
        &input,
        br#"{"schema_version":1,"annotation_profile":"antiskill_annotation_group_v1","group_id":"group:1","source_id":"source:book","snapshot_id":"snapshot:abc","attributed_to":"Example Author","observed_date":"2026-10-11","rights":"private_research","visibility":"private","method":"published commentary","engine":null,"items":[{"item_id":"annotation:1","target_kind":"position","target_id":"position:abc","kind":"text","payload":"isolated pawn"}]}"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "validate-annotations", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ANNOTATION_GROUP_ID\tgroup:1"));
    assert!(stdout.contains("ANNOTATION_ITEMS\t1"));
    assert!(stdout.contains("ANNOTATIONS_VERIFY\tPASS"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_validate_annotations_rejects_unknown_fields() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("annotations.json");
    fs::write(
        &input,
        br#"{"schema_version":1,"annotation_profile":"antiskill_annotation_group_v1","group_id":"group:1","source_id":"source:book","snapshot_id":"snapshot:abc","rights":"private_research","visibility":"private","items":[{"item_id":"annotation:1","target_kind":"position","target_id":"position:abc","kind":"text","payload":"x"}],"mystery":true}"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "validate-annotations", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("JSON annotazioni non valido"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_store_annotations_is_content_addressed_repeatable_and_verifiable() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("annotations-store.json");
    fs::write(
        &input,
        br#"{
  "schema_version": 1,
  "annotation_profile": "antiskill_annotation_group_v1",
  "group_id": "group:store:1",
  "source_id": "source:book",
  "snapshot_id": "snapshot:abc",
  "attributed_to": "Example Author",
  "observed_date": "2026-10-11",
  "rights": "private_research",
  "visibility": "private",
  "method": "published commentary",
  "engine": null,
  "items": [{"item_id":"annotation:1","target_kind":"position","target_id":"position:abc","kind":"text","payload":"isolated pawn"}]
}"#,
    )
    .unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-annotations", "--root"])
        .arg(&directory)
        .arg("--input")
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first = String::from_utf8(first.stdout).unwrap();
    assert_eq!(output_field(&first, "ANNOTATION_GROUP_ID"), "group:store:1");
    assert_eq!(output_field(&first, "ANNOTATION_ITEMS"), "1");
    assert_eq!(output_field(&first, "NEW_ARTIFACT"), "true");
    let path = output_field(&first, "ANNOTATION_PATH").to_owned();
    let hash = output_field(&first, "ANNOTATION_BLAKE3").to_owned();
    assert_eq!(path, format!("inspector/annotations/groups/{hash}.json.xz"));
    assert!(directory.join(&path).is_file());

    fs::write(
        &input,
        br#"{"items":[{"payload":"isolated pawn","kind":"text","target_id":"position:abc","target_kind":"position","item_id":"annotation:1"}],"engine":null,"method":"published commentary","visibility":"private","rights":"private_research","observed_date":"2026-10-11","attributed_to":"Example Author","snapshot_id":"snapshot:abc","source_id":"source:book","group_id":"group:store:1","annotation_profile":"antiskill_annotation_group_v1","schema_version":1}"#,
    )
    .unwrap();

    let second = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-annotations", "--root"])
        .arg(&directory)
        .arg("--input")
        .arg(&input)
        .output()
        .unwrap();
    assert!(second.status.success());
    let second = String::from_utf8(second.stdout).unwrap();
    assert_eq!(output_field(&second, "ANNOTATION_PATH"), path);
    assert_eq!(output_field(&second, "ANNOTATION_BLAKE3"), hash);
    assert_eq!(output_field(&second, "NEW_ARTIFACT"), "false");

    let verified = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-annotations", "--root"])
        .arg(&directory)
        .arg("--annotations")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let verified = String::from_utf8(verified.stdout).unwrap();
    assert_eq!(output_field(&verified, "ANNOTATION_BLAKE3"), hash);
    assert_eq!(output_field(&verified, "ANNOTATION_PATH"), path);
    assert!(verified.contains("ANNOTATIONS_VERIFY\tPASS"));

    let logical = logical_json(directory.join(&path));
    let value: serde_json::Value = serde_json::from_slice(&logical).unwrap();
    assert_eq!(value["attributed_to"], "Example Author");
    assert_eq!(value["items"][0]["target_id"], "position:abc");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_verify_annotations_rejects_noncanonical_or_corrupt_storage() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("annotations-store.json");
    fs::write(
        &input,
        br#"{"schema_version":1,"annotation_profile":"antiskill_annotation_group_v1","group_id":"group:store:2","source_id":"source:book","snapshot_id":"snapshot:abc","rights":"private_research","visibility":"private","items":[{"item_id":"annotation:1","target_kind":"position","target_id":"position:abc","kind":"text","payload":"x"}]}"#,
    )
    .unwrap();
    let stored = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-annotations", "--root"])
        .arg(&directory)
        .arg("--input")
        .arg(&input)
        .output()
        .unwrap();
    assert!(stored.status.success());
    let stored = String::from_utf8(stored.stdout).unwrap();
    let path = output_field(&stored, "ANNOTATION_PATH").to_owned();
    let canonical = directory.join(&path);

    let alias_rel = "inspector/annotations/groups/alias.json.xz";
    fs::copy(&canonical, directory.join(alias_rel)).unwrap();
    let alias = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-annotations", "--root"])
        .arg(&directory)
        .arg("--annotations")
        .arg(alias_rel)
        .output()
        .unwrap();
    assert!(!alias.status.success());
    assert!(String::from_utf8_lossy(&alias.stderr).contains("non canonico"));

    fs::write(&canonical, b"not-xz").unwrap();
    let corrupt = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-annotations", "--root"])
        .arg(&directory)
        .arg("--annotations")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!corrupt.status.success());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_store_analysis_is_content_addressed_repeatable_and_verifiable() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("analysis-store.json");
    fs::write(
        &input,
        br#"{
  "schema_version": 1,
  "analysis_profile": "antiskill_analysis_artifact_v1",
  "subject_scope": "position",
  "subject_id": "8a6adc669f50706498570870a2febd962b5cc2111ddaaaea73bbdfbbd96ea2b4",
  "origin": "external_source",
  "origin_identity": "lichess-evals:snapshot:record:1",
  "source_key": "lichess-evals",
  "snapshot_id": "snapshot:2026-10",
  "locator": "line:1",
  "rights": "cc0",
  "method": "stockfish",
  "engine_build_id": null,
  "search_profile_id": null,
  "status": "completed",
  "observed_at": "2026-10-11",
  "lines": [{
    "multipv_rank": 1,
    "raw_score": "cp 34",
    "score_perspective": "side_to_move",
    "white_cp": 34,
    "side_to_move_cp": 34,
    "mate": null,
    "wdl": null,
    "pv_uci": ["e2e4", "e7e5"],
    "depth": 24,
    "seldepth": null,
    "nodes": 1000000,
    "time_ms": null
  }]
}"#,
    )
    .unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-analysis", "--root"])
        .arg(&directory)
        .arg("--input")
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first = String::from_utf8(first.stdout).unwrap();
    assert_eq!(output_field(&first, "ANALYSIS_LINES"), "1");
    assert_eq!(output_field(&first, "NEW_ARTIFACT"), "true");
    let path = output_field(&first, "ANALYSIS_PATH").to_owned();
    let hash = output_field(&first, "ANALYSIS_BLAKE3").to_owned();
    assert_eq!(path, format!("inspector/analyses/artifacts/{hash}.json.xz"));
    assert!(directory.join(&path).is_file());

    let second = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-analysis", "--root"])
        .arg(&directory)
        .arg("--input")
        .arg(&input)
        .output()
        .unwrap();
    assert!(second.status.success());
    let second = String::from_utf8(second.stdout).unwrap();
    assert_eq!(output_field(&second, "ANALYSIS_PATH"), path);
    assert_eq!(output_field(&second, "ANALYSIS_BLAKE3"), hash);
    assert_eq!(output_field(&second, "NEW_ARTIFACT"), "false");

    let verified = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-analysis", "--root"])
        .arg(&directory)
        .arg("--analysis")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let verified = String::from_utf8(verified.stdout).unwrap();
    assert_eq!(output_field(&verified, "ANALYSIS_BLAKE3"), hash);
    assert_eq!(output_field(&verified, "ANALYSIS_PATH"), path);
    assert!(verified.contains("ANALYSIS_VERIFY\tPASS"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_analysis_rejects_missing_external_provenance_and_unknown_fields() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("analysis-invalid.json");
    fs::write(
        &input,
        br#"{"schema_version":1,"analysis_profile":"antiskill_analysis_artifact_v1","subject_scope":"position","subject_id":"8a6adc669f50706498570870a2febd962b5cc2111ddaaaea73bbdfbbd96ea2b4","origin":"external_source","origin_identity":"source:1","source_key":null,"snapshot_id":null,"locator":null,"rights":"cc0","method":"stockfish","engine_build_id":null,"search_profile_id":null,"status":"completed","observed_at":null,"lines":[{"multipv_rank":1,"raw_score":"cp 1","score_perspective":"side_to_move","white_cp":1,"side_to_move_cp":1,"mate":null,"wdl":null,"pv_uci":[],"depth":10,"seldepth":null,"nodes":100,"time_ms":null}]}"#,
    )
    .unwrap();
    let missing = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "validate-analysis", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("source_key"));

    fs::write(
        &input,
        br#"{"schema_version":1,"analysis_profile":"antiskill_analysis_artifact_v1","subject_scope":"position","subject_id":"8a6adc669f50706498570870a2febd962b5cc2111ddaaaea73bbdfbbd96ea2b4","origin":"local_run","origin_identity":"run:1","source_key":null,"snapshot_id":null,"locator":null,"rights":"private_research","method":"stockfish","engine_build_id":"engine:1","search_profile_id":"profile:1","status":"completed","observed_at":null,"lines":[{"multipv_rank":1,"raw_score":"cp 1","score_perspective":"side_to_move","white_cp":1,"side_to_move_cp":1,"mate":null,"wdl":null,"pv_uci":[],"depth":10,"seldepth":null,"nodes":100,"time_ms":null}],"mystery":true}"#,
    )
    .unwrap();
    let unknown = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "validate-analysis", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown field"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_store_lichess_eval_record_preserves_all_evaluations_and_provenance() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("lichess-eval-record.json");
    fs::write(
        &input,
        br#"{
  "fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
  "evals": [
    {"knodes": 1234, "depth": 30, "pvs": [{"cp": 20, "line": "e2e4 e7e5"}]},
    {"knodes": 5678, "depth": 28, "pvs": [
      {"cp": 18, "line": "e2e4 c7c5"},
      {"cp": 12, "line": "d2d4 d7d5"}
    ]}
  ]
}"#,
    )
    .unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-lichess-eval-record", "--root"])
        .arg(&directory)
        .arg("--input")
        .arg(&input)
        .args(["--snapshot-id", "lichess-evals:2026-10-05"])
        .args(["--observed-at", "2026-10-05"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first = String::from_utf8(first.stdout).unwrap();
    assert_eq!(
        output_field(&first, "POSITION_ID"),
        "8a6adc669f50706498570870a2febd962b5cc2111ddaaaea73bbdfbbd96ea2b4"
    );
    assert_eq!(output_field(&first, "LICHESS_EVALUATIONS"), "2");
    assert_eq!(output_field(&first, "ANALYSIS_1_NEW"), "true");
    assert_eq!(output_field(&first, "ANALYSIS_2_NEW"), "true");
    assert!(first.contains("LICHESS_EVAL_STORE\tPASS"));

    let first_path = output_field(&first, "ANALYSIS_1_PATH").to_owned();
    let first_value: serde_json::Value =
        serde_json::from_slice(&logical_json(directory.join(&first_path))).unwrap();
    assert_eq!(first_value["source_key"], "lichess-evals");
    assert_eq!(first_value["snapshot_id"], "lichess-evals:2026-10-05");
    assert_eq!(first_value["rights"], "CC0-1.0");
    assert_eq!(
        first_value["method"],
        "lichess_stockfish_eval_unspecified_profile"
    );
    assert!(first_value["engine_build_id"].is_null());
    assert!(first_value["search_profile_id"].is_null());
    assert_eq!(first_value["lines"][0]["depth"], 30);
    assert_eq!(first_value["lines"][0]["nodes"], 1_234_000);
    assert_eq!(first_value["lines"][0]["score_perspective"], "white");
    assert_eq!(first_value["lines"][0]["white_cp"], 20);
    assert!(first_value["lines"][0]["side_to_move_cp"].is_null());

    let second_path = output_field(&first, "ANALYSIS_2_PATH").to_owned();
    let second_value: serde_json::Value =
        serde_json::from_slice(&logical_json(directory.join(&second_path))).unwrap();
    assert_eq!(second_value["lines"].as_array().unwrap().len(), 2);
    assert_eq!(second_value["lines"][0]["multipv_rank"], 1);
    assert_eq!(second_value["lines"][1]["multipv_rank"], 2);

    let repeated = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-lichess-eval-record", "--root"])
        .arg(&directory)
        .arg("--input")
        .arg(&input)
        .args(["--snapshot-id", "lichess-evals:2026-10-05"])
        .args(["--observed-at", "2026-10-05"])
        .output()
        .unwrap();
    assert!(repeated.status.success());
    let repeated = String::from_utf8(repeated.stdout).unwrap();
    assert_eq!(output_field(&repeated, "ANALYSIS_1_PATH"), first_path);
    assert_eq!(output_field(&repeated, "ANALYSIS_2_PATH"), second_path);
    assert_eq!(output_field(&repeated, "ANALYSIS_1_NEW"), "false");
    assert_eq!(output_field(&repeated, "ANALYSIS_2_NEW"), "false");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_store_lichess_eval_record_rejects_ambiguous_scores() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("lichess-eval-invalid.json");
    fs::write(
        &input,
        br#"{"fen":"8/8/8/8/8/8/8/K6k w - -","evals":[{"knodes":1,"depth":1,"pvs":[{"cp":0,"mate":2,"line":"a1a2"}]}]}"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-lichess-eval-record", "--root"])
        .arg(&directory)
        .arg("--input")
        .arg(&input)
        .args(["--snapshot-id", "snapshot:test"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("esattamente uno"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_select_analysis_obeys_explicit_requirement_and_fails_closed() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let subject = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let local_input = directory.join("analysis-local.json");
    fs::write(
        &local_input,
        format!(
            r#"{{"schema_version":1,"analysis_profile":"antiskill_analysis_artifact_v1","subject_scope":"position","subject_id":"{subject}","origin":"local_run","origin_identity":"local:standardized:1","source_key":null,"snapshot_id":null,"locator":null,"rights":"private_research","method":"standardized_stockfish","engine_build_id":"sf:test","search_profile_id":"profile:test","status":"completed","observed_at":"2026-10-11","lines":[{{"multipv_rank":1,"raw_score":"cp 12","score_perspective":"white","white_cp":12,"side_to_move_cp":null,"mate":null,"wdl":null,"pv_uci":["e2e4"],"depth":22,"seldepth":null,"nodes":2000,"time_ms":null}}]}}"#
        ),
    )
    .unwrap();
    let external_input = directory.join("analysis-external.json");
    fs::write(
        &external_input,
        format!(
            r#"{{"schema_version":1,"analysis_profile":"antiskill_analysis_artifact_v1","subject_scope":"position","subject_id":"{subject}","origin":"external_source","origin_identity":"lichess:test:1","source_key":"lichess-evals","snapshot_id":"snapshot:test","locator":"line:1","rights":"CC0-1.0","method":"lichess_stockfish_eval_unspecified_profile","engine_build_id":null,"search_profile_id":null,"status":"completed","observed_at":"2026-10-11","lines":[{{"multipv_rank":1,"raw_score":"cp 10","score_perspective":"white","white_cp":10,"side_to_move_cp":null,"mate":null,"wdl":null,"pv_uci":["d2d4"],"depth":40,"seldepth":null,"nodes":50000000,"time_ms":null}}]}}"#
        ),
    )
    .unwrap();

    let store = |input: &std::path::Path| {
        let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(["inspector", "store-analysis", "--root"])
            .arg(&directory)
            .arg("--input")
            .arg(input)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        output_field(&stdout, "ANALYSIS_PATH").to_owned()
    };
    let local_path = store(&local_input);
    let external_path = store(&external_input);

    let requirement = directory.join("requirement.json");
    fs::write(
        &requirement,
        format!(
            r#"{{"schema_version":1,"requirement_profile":"antiskill_analysis_requirement_v1","subject_scope":"position","subject_id":"{subject}","allowed_origins":["external_source","local_run"],"origin_preference":["local_run","external_source"],"allowed_methods":["lichess_stockfish_eval_unspecified_profile","standardized_stockfish"],"method_preference":["standardized_stockfish","lichess_stockfish_eval_unspecified_profile"],"selection_priority":["origin","method","completion","multipv","nodes","depth"],"allowed_rights":["CC0-1.0","private_research"],"required_engine_build_id":null,"required_search_profile_id":null,"accept_partial":false,"min_multipv":1,"min_depth":20,"min_nodes":1000,"require_wdl":false,"require_pv":true}}"#
        ),
    )
    .unwrap();

    let validated = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "validate-analysis-requirement", "--input"])
        .arg(&requirement)
        .output()
        .unwrap();
    assert!(validated.status.success());
    assert!(
        String::from_utf8_lossy(&validated.stdout).contains("ANALYSIS_REQUIREMENT_VERIFY\tPASS")
    );

    let selected = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "select-analysis", "--root"])
        .arg(&directory)
        .arg("--requirement")
        .arg(&requirement)
        .arg("--analysis")
        .arg(&external_path)
        .arg("--analysis")
        .arg(&local_path)
        .output()
        .unwrap();
    assert!(
        selected.status.success(),
        "{}",
        String::from_utf8_lossy(&selected.stderr)
    );
    let selected = String::from_utf8(selected.stdout).unwrap();
    assert_eq!(output_field(&selected, "ANALYSIS_CANDIDATES"), "2");
    assert_eq!(output_field(&selected, "ANALYSIS_QUALIFIED"), "2");
    assert_eq!(
        output_field(&selected, "SELECTED_ANALYSIS_PATH"),
        local_path
    );
    assert_eq!(
        output_field(&selected, "SELECTED_ANALYSIS_ORIGIN"),
        "local_run"
    );
    assert_eq!(
        output_field(&selected, "SELECTED_ANALYSIS_METHOD"),
        "standardized_stockfish"
    );
    assert!(selected.contains("ANALYSIS_SELECTION\tPASS"));

    let weak_requirement = directory.join("requirement-too-strong.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&requirement).unwrap()).unwrap();
    value["min_depth"] = serde_json::json!(50);
    fs::write(&weak_requirement, serde_json::to_vec(&value).unwrap()).unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "select-analysis", "--root"])
        .arg(&directory)
        .arg("--requirement")
        .arg(&weak_requirement)
        .arg("--analysis")
        .arg(&external_path)
        .arg("--analysis")
        .arg(&local_path)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("Nessuna analisi"));

    fs::remove_dir_all(directory).unwrap();
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

fn workspace_files(root: &std::path::Path) -> Vec<(String, String)> {
    fn visit(root: &std::path::Path, path: &std::path::Path, out: &mut Vec<(String, String)>) {
        let mut entries = fs::read_dir(path)
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                let hash = blake3::hash(&fs::read(&path).unwrap()).to_hex().to_string();
                out.push((relative, hash));
            }
        }
    }
    let mut files = Vec::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn cli_exact_duplicates_reports_verified_deterministic_and_non_destructive() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(input.join("copies")).unwrap();
    let game = b"[Event \"Exact\"]\n[White \"Alice\"]\n[Black \"Bob\"]\n[Result \"*\"]\n\n1. e4 $1 {same} e5 (1... c5) *\n";
    fs::write(input.join("a.pgn"), game).unwrap();
    fs::write(input.join("copies/a-copy.pgn"), game).unwrap();
    let mut prefixed = b"unclassified preface\n".to_vec();
    prefixed.extend_from_slice(game);
    fs::write(input.join("b.pgn"), prefixed).unwrap();
    let mut twice = game.to_vec();
    twice.push(b'\n');
    twice.extend_from_slice(game);
    fs::write(input.join("c.pgn"), twice).unwrap();
    fs::write(
        input.join("different.pgn"),
        b"[Event \"Exact\"]\n[White \"Alice\"]\n[Black \"Bob\"]\n[Result \"*\"]\n\n1. e4 $1 {different} e5 (1... c5) *\n",
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
    assert_eq!(output_field(&ingest, "SOURCES"), "4");
    assert_eq!(output_field(&ingest, "DUPLICATE_SOURCES"), "1");
    let collection = output_field(&ingest, "COLLECTION").to_owned();
    let args = [
        "inspector",
        "duplicates",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        collection.as_str(),
    ];
    let before = workspace_files(&workspace);
    let text = run_success(&args);
    assert_eq!(text, run_success(&args));
    assert_eq!(
        output_field(&text, "DUPLICATE_PROFILE"),
        "exact_source_game_bytes_v1"
    );
    assert_eq!(output_field(&text, "GROUPS"), "1");
    assert_eq!(output_field(&text, "DUPLICATE_OCCURRENCES"), "4");
    assert_eq!(text.matches("\nOCCURRENCE\t").count(), 4);
    assert!(text.contains("a.pgn"));
    assert!(text.contains("a-copy.pgn"));
    assert!(text.ends_with("DUPLICATES_REPORT\tPASS\n"));
    let mut json_args = args.to_vec();
    json_args.extend(["--format", "jsonl"]);
    let jsonl = run_success(&json_args);
    assert_eq!(jsonl, run_success(&json_args));
    assert!(jsonl.ends_with('\n'));
    let records = jsonl
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["record"], "summary");
    assert_eq!(records[1]["occurrences"].as_array().unwrap().len(), 4);
    assert_eq!(before, workspace_files(&workspace));

    fs::remove_dir_all(&input).unwrap();
    assert_eq!(
        text,
        run_success(&args),
        "observed paths non devono essere riletti"
    );
    assert_eq!(before, workspace_files(&workspace));

    let verify = run_success(&[
        "inspector",
        "verify-collection",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        &collection,
    ]);
    assert_eq!(verify, "COLLECTION_VERIFY\tPASS\n");
    let query = run_success(&[
        "inspector",
        "query-collection",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        &collection,
        "--player",
        "Alice",
    ]);
    assert_eq!(output_field(&query, "MATCHES"), "5");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_exact_duplicates_empty_report_and_argument_errors() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("only.pgn"), b"[Event \"Only\"]\n\n1. d4 d5 *\n").unwrap();
    let ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
    ]);
    let collection = output_field(&ingest, "COLLECTION");
    let text = run_success(&[
        "inspector",
        "duplicates",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        collection,
    ]);
    assert_eq!(text, "DUPLICATE_PROFILE\texact_source_game_bytes_v1\nGROUPS\t0\nDUPLICATE_OCCURRENCES\t0\nDUPLICATES_REPORT\tPASS\n");
    let jsonl = run_success(&[
        "inspector",
        "duplicates",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        collection,
        "--format",
        "jsonl",
    ]);
    assert_eq!(jsonl.lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(jsonl.trim_end()).unwrap()["groups"],
        0
    );
    for bad in [
        vec!["--format", "yaml"],
        vec!["--format"],
        vec!["--format", "text", "--format", "jsonl"],
        vec!["--root", workspace.to_str().unwrap()],
        vec!["unexpected"],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_antiskill"));
        command
            .args(["inspector", "duplicates", "--root"])
            .arg(&workspace)
            .args(["--collection", collection])
            .args(bad);
        assert!(!command.output().unwrap().status.success());
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_exact_duplicates_rejects_corruption_before_any_report() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("one.pgn"), b"[Event \"One\"]\n\n1. e4 e5 *\n").unwrap();
    let ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
    ]);
    let collection_rel = output_field(&ingest, "COLLECTION").to_owned();
    let collection_path = workspace.join(&collection_rel);
    let collection_json: serde_json::Value =
        serde_json::from_slice(&logical_json(&collection_path)).unwrap();
    let catalog_rel = collection_json["sources"][0]["catalog"].as_str().unwrap();
    let catalog_path = workspace.join(catalog_rel);
    let object_path = workspace_files(&workspace)
        .into_iter()
        .find(|(path, _)| path.starts_with("raw/objects/"))
        .map(|(path, _)| workspace.join(path))
        .unwrap();
    let command = || {
        Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(["inspector", "duplicates", "--root"])
            .arg(&workspace)
            .args(["--collection", &collection_rel])
            .output()
            .unwrap()
    };
    let assert_rejected = |output: std::process::Output| {
        assert!(!output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stdout.contains("DUPLICATES_REPORT\tPASS"));
        assert!(!stdout.contains("GROUP\t"));
    };

    let original_collection = fs::read(&collection_path).unwrap();
    let mut corrupt_collection = original_collection.clone();
    corrupt_collection.push(b' ');
    fs::write(&collection_path, corrupt_collection).unwrap();
    assert_rejected(command());
    fs::write(&collection_path, &original_collection).unwrap();

    let original_catalog = fs::read(&catalog_path).unwrap();
    fs::write(&catalog_path, b"not json\n").unwrap();
    assert_rejected(command());
    fs::write(&catalog_path, &original_catalog).unwrap();

    let mut catalog_json: serde_json::Value =
        serde_json::from_slice(&logical_json(&catalog_path)).unwrap();
    catalog_json["games"][0]["source_game_blake3"] = serde_json::Value::String("0".repeat(64));
    let mut altered_hash = serde_json::to_vec_pretty(&catalog_json).unwrap();
    altered_hash.push(b'\n');
    fs::write(&catalog_path, altered_hash).unwrap();
    assert_rejected(command());
    fs::write(&catalog_path, &original_catalog).unwrap();

    let original_object = fs::read(&object_path).unwrap();
    let mut altered_object = original_object.clone();
    altered_object[0] ^= 1;
    fs::write(&object_path, altered_object).unwrap();
    assert_rejected(command());
    fs::write(&object_path, original_object).unwrap();
    assert!(command().status.success());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_interpretation_is_content_addressed_verified_and_idempotent() {
    let root = test_root();
    fs::create_dir_all(&root).expect("workspace");
    let source = root.join("interpret.pgn");
    fs::write(&source, b"[Variant \"Standard\"]\n[SetUp \"1\"]\n[FEN \"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1\"]\n\n1. 0-0 (1. O-O-O (1. Kf2)) *\n").expect("source");
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(args)
            .output()
            .expect("CLI");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("stdout")
    };
    let import = run(&[
        "inspector",
        "import",
        "--root",
        root.to_str().unwrap(),
        source.to_str().unwrap(),
    ]);
    let segment = run(&[
        "inspector",
        "segment",
        "--root",
        root.to_str().unwrap(),
        "--manifest",
        output_field(&import, "MANIFEST"),
    ]);
    let tokenize = run(&[
        "inspector",
        "tokenize",
        "--root",
        root.to_str().unwrap(),
        "--segmentation",
        output_field(&segment, "SEGMENTATION"),
    ]);
    let parse = run(&[
        "inspector",
        "parse",
        "--root",
        root.to_str().unwrap(),
        "--tokenization",
        output_field(&tokenize, "TOKENIZATION"),
    ]);
    let first = run(&[
        "inspector",
        "interpret",
        "--root",
        root.to_str().unwrap(),
        "--parse",
        output_field(&parse, "PARSE"),
    ]);
    assert_eq!(output_field(&first, "NEW_MANIFEST"), "true");
    assert!(
        output_field(&first, "VARIATION_LINES")
            .parse::<u64>()
            .unwrap()
            >= 2
    );
    assert!(
        output_field(&first, "APPLIED_MOVES")
            .parse::<u64>()
            .unwrap()
            >= 1
    );
    let interpretation_path = PathBuf::from(output_field(&first, "INTERPRETATION"));
    let interpretation_hash = output_field(&first, "INTERPRETATION_BLAKE3");
    let expected_name = format!("{interpretation_hash}.json.xz");
    assert_eq!(
        interpretation_path
            .file_name()
            .and_then(|name| name.to_str()),
        Some(expected_name.as_str())
    );
    let persisted = logical_json(root.join(&interpretation_path));
    assert_eq!(
        persisted.len() as u64,
        output_field(&first, "INTERPRETATION_BYTES")
            .parse::<u64>()
            .expect("INTERPRETATION_BYTES numerico")
    );
    assert_eq!(
        blake3::hash(&persisted).to_hex().as_str(),
        interpretation_hash
    );
    let verify = run(&[
        "inspector",
        "verify-interpretation",
        "--root",
        root.to_str().unwrap(),
        "--interpretation",
        output_field(&first, "INTERPRETATION"),
    ]);
    assert_eq!(verify, "INTERPRETATION_VERIFY\tPASS\n");
    let second = run(&[
        "inspector",
        "interpret",
        "--root",
        root.to_str().unwrap(),
        "--parse",
        output_field(&parse, "PARSE"),
    ]);
    assert_eq!(output_field(&second, "NEW_MANIFEST"), "false");
    for field in [
        "INTERPRETATION",
        "INTERPRETATION_BLAKE3",
        "INTERPRETATION_BYTES",
    ] {
        assert_eq!(output_field(&first, field), output_field(&second, field));
    }
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn cli_segments_and_verifies_with_relative_reusable_paths() {
    let directory = test_root();
    let relative_root = PathBuf::from("workspace");
    let root = directory.join(&relative_root);
    fs::create_dir_all(&root).expect("creazione workspace");
    let source = relative_root.join("mixed.pgn");
    let bytes = b"preamble\r\n\
[Site \"First\"]\r\n\r\n1. e4 {\xff} *\r\n\
\r\nseparator\n\
[White \"Second\"]\n\n1. d4 *\n\
\ntrailer";
    fs::write(directory.join(&source), bytes).expect("scrittura sorgente");

    let import = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "import", "--root"])
        .arg(&relative_root)
        .arg(&source)
        .output()
        .expect("import");
    assert!(
        import.status.success(),
        "import fallito: {}",
        String::from_utf8_lossy(&import.stderr)
    );
    let import_stdout = String::from_utf8(import.stdout).expect("stdout import");
    let source_manifest = output_field(&import_stdout, "MANIFEST");

    let segment_once = || {
        Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .current_dir(&directory)
            .args(["inspector", "segment", "--root"])
            .arg(&relative_root)
            .args(["--manifest", source_manifest])
            .output()
            .expect("segment")
    };
    let first = segment_once();
    let second = segment_once();
    assert!(
        first.status.success(),
        "segment fallito: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(second.status.success());
    let first_stdout = String::from_utf8(first.stdout).expect("stdout segment");
    let second_stdout = String::from_utf8(second.stdout).expect("stdout segment");
    assert_eq!(
        output_field(&first_stdout, "SOURCE_BYTES"),
        bytes.len().to_string()
    );
    assert_eq!(output_field(&first_stdout, "GAME_CANDIDATES"), "2");
    assert_ne!(
        output_field(&first_stdout, "SEGMENTATION"),
        output_field(&second_stdout, "SEGMENTATION")
    );
    let segmentation = output_field(&first_stdout, "SEGMENTATION");
    assert!(segmentation.starts_with("inspector/segmentations/"));

    let verify = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "verify-segmentation", "--root"])
        .arg(&relative_root)
        .args(["--segmentation", segmentation])
        .output()
        .expect("verify");
    assert!(
        verify.status.success(),
        "verify fallito: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_eq!(
        String::from_utf8(verify.stdout).expect("stdout verify"),
        "SEGMENTATION_VERIFY\tPASS\n"
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&logical_json(root.join(segmentation))).expect("JSON segmentazione");
    assert_eq!(manifest["source_manifest"], source_manifest);
    assert_eq!(manifest["source_bytes"], bytes.len() as u64);
    let segments = manifest["segments"].as_array().expect("segments");
    let reconstructed: Vec<u8> = segments
        .iter()
        .flat_map(|segment| {
            let start = segment["start"].as_u64().expect("start") as usize;
            let end = segment["end"].as_u64().expect("end") as usize;
            bytes[start..end].iter()
        })
        .copied()
        .collect();
    assert_eq!(reconstructed, bytes);

    fs::remove_dir_all(directory).expect("pulizia directory test");
}

#[test]
fn cli_tokenizes_and_verifies_with_relative_reusable_paths() {
    let directory = test_root();
    let relative_root = PathBuf::from("workspace");
    let root = directory.join(&relative_root);
    fs::create_dir_all(&root).expect("creazione workspace");
    let source = relative_root.join("tokens.pgn");
    let bytes = b"outside\n[Event \"Tokens\"]\r\n\r\n1. e4$1!? (e5) 1-0\r\n\r\ntail";
    fs::write(directory.join(&source), bytes).expect("scrittura sorgente");

    let import = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "import", "--root"])
        .arg(&relative_root)
        .arg(&source)
        .output()
        .expect("import");
    assert!(
        import.status.success(),
        "import fallito: {}",
        String::from_utf8_lossy(&import.stderr)
    );
    let import_stdout = String::from_utf8(import.stdout).expect("stdout import");
    let source_manifest = output_field(&import_stdout, "MANIFEST");

    let segment = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "segment", "--root"])
        .arg(&relative_root)
        .args(["--manifest", source_manifest])
        .output()
        .expect("segment");
    assert!(
        segment.status.success(),
        "segment fallito: {}",
        String::from_utf8_lossy(&segment.stderr)
    );
    let segment_stdout = String::from_utf8(segment.stdout).expect("stdout segment");
    let segmentation = output_field(&segment_stdout, "SEGMENTATION");

    let tokenize = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "tokenize", "--root"])
        .arg(&relative_root)
        .args(["--segmentation", segmentation])
        .output()
        .expect("tokenize");
    assert!(
        tokenize.status.success(),
        "tokenize fallito: {}",
        String::from_utf8_lossy(&tokenize.stderr)
    );
    let tokenize_stdout = String::from_utf8(tokenize.stdout).expect("stdout tokenize");
    assert_eq!(
        output_field(&tokenize_stdout, "SOURCE_BYTES"),
        bytes.len().to_string()
    );
    assert_eq!(output_field(&tokenize_stdout, "GAME_CANDIDATES"), "1");
    assert_eq!(output_field(&tokenize_stdout, "TOKEN_STREAMS"), "1");
    let tokenization = output_field(&tokenize_stdout, "TOKENIZATION");
    assert!(tokenization.starts_with("inspector/tokenizations/"));

    let verify = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "verify-tokenization", "--root"])
        .arg(&relative_root)
        .args(["--tokenization", tokenization])
        .output()
        .expect("verify tokenization");
    assert!(
        verify.status.success(),
        "verify-tokenization fallito: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_eq!(
        String::from_utf8(verify.stdout).expect("stdout verify"),
        "TOKENIZATION_VERIFY\tPASS\n"
    );

    fs::remove_dir_all(directory).expect("pulizia directory test");
}

#[test]
fn cli_parses_and_verifies_with_relative_reusable_paths() {
    let directory = test_root();
    let relative_root = PathBuf::from("workspace");
    let root = directory.join(&relative_root);
    fs::create_dir_all(&root).expect("creazione workspace");
    let source = relative_root.join("parse.pgn");
    let bytes = b"outside\n[Event \"Parse\"]\r\n\r\n1. e4$1 (1... e5) {note} @\xff\r\n\r\ntail";
    fs::write(directory.join(&source), bytes).expect("scrittura sorgente");

    let import = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "import", "--root"])
        .arg(&relative_root)
        .arg(&source)
        .output()
        .expect("import");
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let import_stdout = String::from_utf8(import.stdout).expect("stdout import");
    let source_manifest = output_field(&import_stdout, "MANIFEST");

    let segment = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "segment", "--root"])
        .arg(&relative_root)
        .args(["--manifest", source_manifest])
        .output()
        .expect("segment");
    assert!(
        segment.status.success(),
        "{}",
        String::from_utf8_lossy(&segment.stderr)
    );
    let segment_stdout = String::from_utf8(segment.stdout).expect("stdout segment");
    let segmentation = output_field(&segment_stdout, "SEGMENTATION");

    let tokenize = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "tokenize", "--root"])
        .arg(&relative_root)
        .args(["--segmentation", segmentation])
        .output()
        .expect("tokenize");
    assert!(
        tokenize.status.success(),
        "{}",
        String::from_utf8_lossy(&tokenize.stderr)
    );
    let tokenize_stdout = String::from_utf8(tokenize.stdout).expect("stdout tokenize");
    let tokenization = output_field(&tokenize_stdout, "TOKENIZATION");

    let parse = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "parse", "--root"])
        .arg(&relative_root)
        .args(["--tokenization", tokenization])
        .output()
        .expect("parse");
    assert!(
        parse.status.success(),
        "parse fallita: {}",
        String::from_utf8_lossy(&parse.stderr)
    );
    let parse_stdout = String::from_utf8(parse.stdout).expect("stdout parse");
    assert_eq!(
        output_field(&parse_stdout, "SOURCE_BYTES"),
        bytes.len().to_string()
    );
    assert_eq!(output_field(&parse_stdout, "GAME_CANDIDATES"), "1");
    assert_eq!(output_field(&parse_stdout, "PARSE_TREES"), "1");
    assert_ne!(output_field(&parse_stdout, "RECOVERED_NODES"), "0");
    assert_ne!(output_field(&parse_stdout, "UNPARSED_NODES"), "0");
    let parse_path = output_field(&parse_stdout, "PARSE");
    assert!(parse_path.starts_with("inspector/parses/"));

    let verify = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .current_dir(&directory)
        .args(["inspector", "verify-parse", "--root"])
        .arg(&relative_root)
        .args(["--parse", parse_path])
        .output()
        .expect("verify parse");
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_eq!(
        String::from_utf8(verify.stdout).expect("stdout verify"),
        "PARSE_VERIFY\tPASS\n"
    );

    fs::remove_dir_all(directory).expect("pulizia directory test");
}

#[test]
#[ignore = "collaudo manuale: richiede il file reale del pilot"]
fn manual_real_pilot_segmentation() {
    let source = real_pilot_pgn();
    let root = test_root();
    fs::create_dir_all(&root).expect("creazione workspace");
    let import = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "import", "--root"])
        .arg(&root)
        .arg(&source)
        .output()
        .expect("import pilot");
    assert!(import.status.success());
    let stdout = String::from_utf8(import.stdout).expect("stdout import");
    assert_eq!(
        output_field(&stdout, "BLAKE3"),
        "913d6ee7cf8a6dece754dfd70b5471b531466b90880593a8c010a659d2f3e7fb"
    );
    assert_eq!(output_field(&stdout, "BYTES"), "40990");
    let manifest = output_field(&stdout, "MANIFEST");
    let segment = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "segment", "--root"])
        .arg(&root)
        .args(["--manifest", manifest])
        .output()
        .expect("segment pilot");
    assert!(
        segment.status.success(),
        "{}",
        String::from_utf8_lossy(&segment.stderr)
    );
    let stdout = String::from_utf8(segment.stdout).expect("stdout segment");
    let segmentation = output_field(&stdout, "SEGMENTATION");
    let verify = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-segmentation", "--root"])
        .arg(&root)
        .args(["--segmentation", segmentation])
        .status()
        .expect("verify pilot");
    assert!(verify.success());
    fs::remove_dir_all(root).expect("pulizia workspace");
}

#[test]
#[ignore = "collaudo manuale: richiede il file reale del pilot"]
fn manual_real_pilot_tokenization() {
    let source = real_pilot_pgn();
    let root = test_root();
    fs::create_dir_all(&root).expect("creazione workspace");
    let import = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "import", "--root"])
        .arg(&root)
        .arg(&source)
        .output()
        .expect("import pilot");
    assert!(import.status.success());
    let stdout = String::from_utf8(import.stdout).expect("stdout import");
    let manifest = output_field(&stdout, "MANIFEST");
    let segment = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "segment", "--root"])
        .arg(&root)
        .args(["--manifest", manifest])
        .output()
        .expect("segment pilot");
    assert!(
        segment.status.success(),
        "{}",
        String::from_utf8_lossy(&segment.stderr)
    );
    let stdout = String::from_utf8(segment.stdout).expect("stdout segment");
    let segmentation = output_field(&stdout, "SEGMENTATION");
    let tokenize = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "tokenize", "--root"])
        .arg(&root)
        .args(["--segmentation", segmentation])
        .output()
        .expect("tokenize pilot");
    assert!(
        tokenize.status.success(),
        "{}",
        String::from_utf8_lossy(&tokenize.stderr)
    );
    let stdout = String::from_utf8(tokenize.stdout).expect("stdout tokenize");
    let tokenization = output_field(&stdout, "TOKENIZATION");
    let verify = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-tokenization", "--root"])
        .arg(&root)
        .args(["--tokenization", tokenization])
        .status()
        .expect("verify tokenization pilot");
    assert!(verify.success());
    fs::remove_dir_all(root).expect("pulizia workspace");
}

#[test]
#[ignore = "collaudo manuale: richiede il file reale del pilot"]
fn manual_real_pilot_parsing() {
    let source = real_pilot_pgn();
    let root = test_root();
    fs::create_dir_all(&root).expect("creazione workspace");

    let import = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "import", "--root"])
        .arg(&root)
        .arg(&source)
        .output()
        .expect("import pilot");
    assert!(import.status.success());
    let stdout = String::from_utf8(import.stdout).expect("stdout import");
    let manifest = output_field(&stdout, "MANIFEST");

    let segment = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "segment", "--root"])
        .arg(&root)
        .args(["--manifest", manifest])
        .output()
        .expect("segment pilot");
    assert!(
        segment.status.success(),
        "{}",
        String::from_utf8_lossy(&segment.stderr)
    );
    let stdout = String::from_utf8(segment.stdout).expect("stdout segment");
    let segmentation = output_field(&stdout, "SEGMENTATION");

    let tokenize = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "tokenize", "--root"])
        .arg(&root)
        .args(["--segmentation", segmentation])
        .output()
        .expect("tokenize pilot");
    assert!(
        tokenize.status.success(),
        "{}",
        String::from_utf8_lossy(&tokenize.stderr)
    );
    let stdout = String::from_utf8(tokenize.stdout).expect("stdout tokenize");
    let tokenization = output_field(&stdout, "TOKENIZATION");

    let parse = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "parse", "--root"])
        .arg(&root)
        .args(["--tokenization", tokenization])
        .output()
        .expect("parse pilot");
    assert!(
        parse.status.success(),
        "{}",
        String::from_utf8_lossy(&parse.stderr)
    );
    let stdout = String::from_utf8(parse.stdout).expect("stdout parse");
    let parse_path = output_field(&stdout, "PARSE");

    let verify = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-parse", "--root"])
        .arg(&root)
        .args(["--parse", parse_path])
        .output()
        .expect("verify parse pilot");
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_eq!(
        String::from_utf8(verify.stdout).expect("stdout verify"),
        "PARSE_VERIFY\tPASS\n"
    );

    let interpret = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "interpret", "--root"])
        .arg(&root)
        .args(["--parse", parse_path])
        .output()
        .expect("interpret pilot");
    assert!(
        interpret.status.success(),
        "{}",
        String::from_utf8_lossy(&interpret.stderr)
    );
    let first = String::from_utf8(interpret.stdout).expect("stdout interpret");
    assert_eq!(output_field(&first, "GAMES"), "59");
    assert!(output_field(&first, "LINES").parse::<u64>().unwrap() >= 59);
    assert!(
        output_field(&first, "APPLIED_MOVES")
            .parse::<u64>()
            .unwrap()
            > 0
    );
    assert_eq!(output_field(&first, "NEW_MANIFEST"), "true");
    let interpretation = output_field(&first, "INTERPRETATION");
    let interpretation_hash = output_field(&first, "INTERPRETATION_BLAKE3");
    let interpretation_bytes = output_field(&first, "INTERPRETATION_BYTES");
    let verify = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-interpretation", "--root"])
        .arg(&root)
        .args(["--interpretation", interpretation])
        .output()
        .expect("verify interpretation pilot");
    assert!(verify.status.success());
    assert_eq!(
        String::from_utf8(verify.stdout).unwrap(),
        "INTERPRETATION_VERIFY\tPASS\n"
    );
    let second = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "interpret", "--root"])
        .arg(&root)
        .args(["--parse", parse_path])
        .output()
        .expect("second interpret pilot");
    assert!(second.status.success());
    let second = String::from_utf8(second.stdout).unwrap();
    assert_eq!(output_field(&second, "NEW_MANIFEST"), "false");
    assert_eq!(output_field(&second, "INTERPRETATION"), interpretation);
    assert_eq!(
        output_field(&second, "INTERPRETATION_BLAKE3"),
        interpretation_hash
    );
    assert_eq!(
        output_field(&second, "INTERPRETATION_BYTES"),
        interpretation_bytes
    );
    println!(
        "PILOT\tgames=59\tlines={}\tapplied={}\thash={}\tbytes={}",
        output_field(&first, "LINES"),
        output_field(&first, "APPLIED_MOVES"),
        interpretation_hash,
        interpretation_bytes
    );
    fs::remove_dir_all(root).expect("pulizia workspace");
}

#[test]
fn cli_catalog_pipeline_is_content_addressed_verified_and_queryable() {
    let root = test_root();
    fs::create_dir_all(&root).expect("workspace");
    let source = root.join("catalog.pgn");
    fs::write(&source, "[Event \"Open Città\"]\n[Date \"2026.08.01\"]\n[White \"Èva\"]\n[Black \"Bob\"]\n[Result \"1-0\"]\n[White \"Duplicate\"]\n[Technical \"x\"]\n\n1. e4 (1. d4) e5 1-0\n\n[Event \"Open Due\"]\n[UTCDate \"2026.08.02\"]\n[White \"Carol\"]\n[Black \"José\"]\n[Result \"0-1\"]\n[Broken \"unterminated\n\n1. d4 0-1\n".as_bytes()).expect("source");
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(args)
            .output()
            .expect("CLI");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).expect("stdout")
    };
    let root_s = root.to_str().unwrap();
    let source_s = source.to_str().unwrap();
    let import = run(&["inspector", "import", "--root", root_s, source_s]);
    let segment = run(&[
        "inspector",
        "segment",
        "--root",
        root_s,
        "--manifest",
        output_field(&import, "MANIFEST"),
    ]);
    let tokenize = run(&[
        "inspector",
        "tokenize",
        "--root",
        root_s,
        "--segmentation",
        output_field(&segment, "SEGMENTATION"),
    ]);
    let parse = run(&[
        "inspector",
        "parse",
        "--root",
        root_s,
        "--tokenization",
        output_field(&tokenize, "TOKENIZATION"),
    ]);
    let interpret = run(&[
        "inspector",
        "interpret",
        "--root",
        root_s,
        "--parse",
        output_field(&parse, "PARSE"),
    ]);
    let first = run(&[
        "inspector",
        "catalog",
        "--root",
        root_s,
        "--interpretation",
        output_field(&interpret, "INTERPRETATION"),
    ]);
    assert_eq!(output_field(&first, "NEW_MANIFEST"), "true");
    assert!(first.contains("UNDECODABLE_TAGS\t"));
    assert!(!first.contains("MALFORMED_TAGS\t"));
    assert!(!first.contains("PARSE_BLAKE3\t"));
    assert!(output_field(&first, "GAMES").parse::<u64>().unwrap() >= 2);
    let catalog = output_field(&first, "CATALOG");
    let hash = output_field(&first, "CATALOG_BLAKE3");
    let persisted = logical_json(root.join(catalog));
    assert_eq!(
        PathBuf::from(catalog)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        format!("{hash}.json.xz")
    );
    assert_eq!(
        persisted.len().to_string(),
        output_field(&first, "CATALOG_BYTES")
    );
    assert_eq!(blake3::hash(&persisted).to_hex().as_str(), hash);
    let verify = run(&[
        "inspector",
        "verify-catalog",
        "--root",
        root_s,
        "--catalog",
        catalog,
    ]);
    assert_eq!(verify, "CATALOG_VERIFY\tPASS\n");
    let manifest_json: serde_json::Value = serde_json::from_slice(&persisted).unwrap();
    assert!(manifest_json.get("parse_manifest").is_none());
    assert_eq!(
        manifest_json["catalog_profile"],
        "antiskill_game_catalog_v1"
    );
    let game0 = &manifest_json["games"][0];
    let export_path = root.join("exported-game.pgn");
    let export = run(&[
        "inspector",
        "export-game",
        "--root",
        root_s,
        "--catalog",
        catalog,
        "--game-id",
        "0",
        "--output",
        export_path.to_str().unwrap(),
    ]);
    assert_eq!(output_field(&export, "GAME_EXPORT"), "PASS");
    let source_bytes = fs::read(&source).unwrap();
    let start = game0["source_start"].as_u64().unwrap() as usize;
    let end = game0["source_end"].as_u64().unwrap() as usize;
    assert_eq!(fs::read(&export_path).unwrap(), source_bytes[start..end]);
    assert_eq!(
        output_field(&export, "GAME_BLAKE3"),
        game0["source_game_blake3"].as_str().unwrap()
    );
    assert_eq!(
        output_field(&export, "GAME_BYTES"),
        (end - start).to_string()
    );
    for filters in [
        &["--player", "èVA"][..],
        &["--player", "Nobody", "--player", "Bob"][..],
        &["--event", "città"][..],
        &["--date", "2026-08-01"][..],
        &["--player", "Bob", "--date", "2026-08-01"][..],
    ] {
        let mut args = vec![
            "inspector",
            "query-catalog",
            "--root",
            root_s,
            "--catalog",
            catalog,
        ];
        args.extend(filters);
        let q = run(&args);
        assert_eq!(output_field(&q, "MATCHES"), "1");
    }
    let second = run(&[
        "inspector",
        "catalog",
        "--root",
        root_s,
        "--interpretation",
        output_field(&interpret, "INTERPRETATION"),
    ]);
    assert_eq!(output_field(&second, "NEW_MANIFEST"), "false");
    for field in ["CATALOG", "CATALOG_BLAKE3", "CATALOG_BYTES"] {
        assert_eq!(output_field(&first, field), output_field(&second, field));
    }
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
#[ignore = "collaudo manuale: richiede il file reale del pilot"]
fn manual_real_pilot_catalog_and_occurrences_pipeline() {
    struct Guard(PathBuf);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let source = real_pilot_pgn();
    let root = test_root();
    fs::create_dir_all(&root).unwrap();
    let _guard = Guard(root.clone());
    let run = |args: &[&str]| {
        let o = Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8(o.stdout).unwrap()
    };
    let r = root.to_str().unwrap();
    let s = source.to_str().unwrap();
    let i = run(&["inspector", "import", "--root", r, s]);
    assert_eq!(
        output_field(&i, "BLAKE3"),
        "913d6ee7cf8a6dece754dfd70b5471b531466b90880593a8c010a659d2f3e7fb"
    );
    assert_eq!(output_field(&i, "BYTES"), "40990");
    let sg = run(&[
        "inspector",
        "segment",
        "--root",
        r,
        "--manifest",
        output_field(&i, "MANIFEST"),
    ]);
    let t = run(&[
        "inspector",
        "tokenize",
        "--root",
        r,
        "--segmentation",
        output_field(&sg, "SEGMENTATION"),
    ]);
    let p = run(&[
        "inspector",
        "parse",
        "--root",
        r,
        "--tokenization",
        output_field(&t, "TOKENIZATION"),
    ]);
    let x = run(&[
        "inspector",
        "interpret",
        "--root",
        r,
        "--parse",
        output_field(&p, "PARSE"),
    ]);
    assert_eq!(output_field(&x, "GAMES"), "59");
    let vi = run(&[
        "inspector",
        "verify-interpretation",
        "--root",
        r,
        "--interpretation",
        output_field(&x, "INTERPRETATION"),
    ]);
    assert_eq!(vi, "INTERPRETATION_VERIFY\tPASS\n");
    let c = run(&[
        "inspector",
        "catalog",
        "--root",
        r,
        "--interpretation",
        output_field(&x, "INTERPRETATION"),
    ]);
    assert_eq!(output_field(&c, "GAMES"), "59");
    assert!(output_field(&c, "DECODED_TAGS").parse::<u64>().unwrap() > 0);
    let v = run(&[
        "inspector",
        "verify-catalog",
        "--root",
        r,
        "--catalog",
        output_field(&c, "CATALOG"),
    ]);
    assert_eq!(v, "CATALOG_VERIFY\tPASS\n");
    let occurrences = run(&[
        "inspector",
        "create-occurrences",
        "--root",
        r,
        "--catalog",
        output_field(&c, "CATALOG"),
    ]);
    assert_eq!(output_field(&occurrences, "GAME_OCCURRENCES"), "59");
    let occurrence_path = output_field(&occurrences, "OCCURRENCES");
    let occurrence_json: serde_json::Value =
        serde_json::from_slice(&logical_json(root.join(occurrence_path))).unwrap();
    assert_eq!(occurrence_json["occurrences"].as_array().unwrap().len(), 59);
    let first_occurrence = &occurrence_json["occurrences"][0];
    let mut identity = blake3::Hasher::new();
    identity.update(b"antiskill_game_occurrence_v1\0");
    for value in [
        occurrence_json["source_blake3"]
            .as_str()
            .unwrap()
            .to_owned(),
        occurrence_json["source_bytes"]
            .as_u64()
            .unwrap()
            .to_string(),
        first_occurrence["source_start"]
            .as_u64()
            .unwrap()
            .to_string(),
        first_occurrence["source_end"].as_u64().unwrap().to_string(),
    ] {
        identity.update(value.as_bytes());
        identity.update(b"\0");
    }
    identity.update(
        first_occurrence["source_game_blake3"]
            .as_str()
            .unwrap()
            .as_bytes(),
    );
    identity.update(b"\n");
    assert_eq!(
        first_occurrence["occurrence_id"].as_str().unwrap(),
        identity.finalize().to_hex().as_str()
    );
    let jsonl = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-occurrences", "--root", r])
        .args(["--occurrences", occurrence_path])
        .output()
        .unwrap();
    assert!(jsonl.status.success());
    assert_eq!(jsonl.stdout.last(), Some(&b'\n'));
    assert_eq!(
        jsonl.stdout.iter().filter(|byte| **byte == b'\n').count(),
        59
    );
    let occurrence_export = root.join("pilot-occurrence.pgn");
    let single = run(&[
        "inspector",
        "export-occurrence",
        "--root",
        r,
        "--occurrences",
        occurrence_path,
        "--occurrence-id",
        first_occurrence["occurrence_id"].as_str().unwrap(),
        "--output",
        occurrence_export.to_str().unwrap(),
    ]);
    assert_eq!(output_field(&single, "OCCURRENCE_EXPORT"), "PASS");
    let source_bytes = fs::read(&source).unwrap();
    assert_eq!(
        fs::read(&occurrence_export).unwrap(),
        source_bytes[first_occurrence["source_start"].as_u64().unwrap() as usize
            ..first_occurrence["source_end"].as_u64().unwrap() as usize]
    );
    let q = run(&[
        "inspector",
        "query-catalog",
        "--root",
        r,
        "--catalog",
        output_field(&c, "CATALOG"),
    ]);
    assert_eq!(output_field(&q, "MATCHES"), "59");
    let catalog_json: serde_json::Value =
        serde_json::from_slice(&logical_json(root.join(output_field(&c, "CATALOG")))).unwrap();
    let first = &catalog_json["games"][0];
    let start = first["source_start"].as_u64().unwrap() as usize;
    let end = first["source_end"].as_u64().unwrap() as usize;
    let export_path = root.join("pilot-game.pgn");
    let export = run(&[
        "inspector",
        "export-game",
        "--root",
        r,
        "--catalog",
        output_field(&c, "CATALOG"),
        "--game-id",
        "0",
        "--output",
        export_path.to_str().unwrap(),
    ]);
    assert_eq!(output_field(&export, "GAME_EXPORT"), "PASS");
    let source_bytes = fs::read(&source).unwrap();
    assert_eq!(
        fs::read(&export_path).unwrap(),
        source_bytes[start..end],
        "pilot export byte-esatto"
    );
    let c2 = run(&[
        "inspector",
        "catalog",
        "--root",
        r,
        "--interpretation",
        output_field(&x, "INTERPRETATION"),
    ]);
    assert_eq!(output_field(&c2, "NEW_MANIFEST"), "false");
    for f in ["CATALOG", "CATALOG_BLAKE3", "CATALOG_BYTES"] {
        assert_eq!(output_field(&c, f), output_field(&c2, f));
    }
    println!(
        "PILOT_CATALOG\tgames={}\ttags={}\tdecoded={}\thash={}\tbytes={}",
        output_field(&c, "GAMES"),
        output_field(&c, "TAGS"),
        output_field(&c, "DECODED_TAGS"),
        output_field(&c, "CATALOG_BLAKE3"),
        output_field(&c, "CATALOG_BYTES")
    );
}

#[test]
fn cli_game_occurrence_v1_create_verify_jsonl_and_byte_exact_export() {
    let root = test_root();
    fs::create_dir_all(&root).unwrap();
    let source = root.join("occurrences.pgn");
    let source_bytes = b"[Event \"One\"]\r\n[White \"A\"]\r\n\r\n1. e4 $1 {kept} (1. d4) *\r\n\r\n[Event \"Two\"]\n\n1. c4 ( ) *\n";
    fs::write(&source, source_bytes).unwrap();
    let root_value = root.to_str().unwrap();
    let run = |arguments: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let imported = run(&[
        "inspector",
        "import",
        "--root",
        root_value,
        source.to_str().unwrap(),
    ]);
    let segmented = run(&[
        "inspector",
        "segment",
        "--root",
        root_value,
        "--manifest",
        output_field(&imported, "MANIFEST"),
    ]);
    let tokenized = run(&[
        "inspector",
        "tokenize",
        "--root",
        root_value,
        "--segmentation",
        output_field(&segmented, "SEGMENTATION"),
    ]);
    let parsed = run(&[
        "inspector",
        "parse",
        "--root",
        root_value,
        "--tokenization",
        output_field(&tokenized, "TOKENIZATION"),
    ]);
    let interpreted = run(&[
        "inspector",
        "interpret",
        "--root",
        root_value,
        "--parse",
        output_field(&parsed, "PARSE"),
    ]);
    let catalog = run(&[
        "inspector",
        "catalog",
        "--root",
        root_value,
        "--interpretation",
        output_field(&interpreted, "INTERPRETATION"),
    ]);
    let first = run(&[
        "inspector",
        "create-occurrences",
        "--root",
        root_value,
        "--catalog",
        output_field(&catalog, "CATALOG"),
    ]);
    assert_eq!(output_field(&first, "GAME_OCCURRENCES"), "2");
    assert_eq!(output_field(&first, "NEW_MANIFEST"), "true");
    let occurrence_path = output_field(&first, "OCCURRENCES");
    assert!(occurrence_path.ends_with(".json.xz"));
    let second = run(&[
        "inspector",
        "create-occurrences",
        "--root",
        root_value,
        "--catalog",
        output_field(&catalog, "CATALOG"),
    ]);
    assert_eq!(output_field(&second, "NEW_MANIFEST"), "false");
    for field in ["OCCURRENCES", "OCCURRENCES_BLAKE3", "OCCURRENCES_BYTES"] {
        assert_eq!(output_field(&first, field), output_field(&second, field));
    }
    let verified = run(&[
        "inspector",
        "verify-occurrences",
        "--root",
        root_value,
        "--occurrences",
        occurrence_path,
    ]);
    assert_eq!(verified, "OCCURRENCES_VERIFY\tPASS\n");

    let stdout_one = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-occurrences", "--root", root_value])
        .args(["--occurrences", occurrence_path])
        .output()
        .unwrap();
    let stdout_two = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-occurrences", "--root", root_value])
        .args(["--occurrences", occurrence_path])
        .output()
        .unwrap();
    assert!(stdout_one.status.success() && stdout_two.status.success());
    assert_eq!(stdout_one.stdout, stdout_two.stdout);
    assert_eq!(stdout_one.stdout.last(), Some(&b'\n'));
    assert_eq!(
        stdout_one
            .stdout
            .iter()
            .filter(|byte| **byte == b'\n')
            .count(),
        2
    );
    let records = stdout_one
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(records
        .iter()
        .all(|record| record.get("catalog_manifest").is_none()));

    let jsonl_path = root.join("occurrences.jsonl");
    let file_export = run(&[
        "inspector",
        "export-occurrences",
        "--root",
        root_value,
        "--occurrences",
        occurrence_path,
        "--output",
        jsonl_path.to_str().unwrap(),
    ]);
    assert_eq!(output_field(&file_export, "OCCURRENCES_EXPORT"), "PASS");
    assert_eq!(fs::read(&jsonl_path).unwrap(), stdout_one.stdout);

    let occurrence_id = records[0]["occurrence_id"].as_str().unwrap();
    let output = root.join("single.pgn");
    let single = run(&[
        "inspector",
        "export-occurrence",
        "--root",
        root_value,
        "--occurrences",
        occurrence_path,
        "--occurrence-id",
        occurrence_id,
        "--output",
        output.to_str().unwrap(),
    ]);
    assert_eq!(output_field(&single, "OCCURRENCE_EXPORT"), "PASS");
    let start = records[0]["source_start"].as_u64().unwrap() as usize;
    let end = records[0]["source_end"].as_u64().unwrap() as usize;
    assert_eq!(fs::read(output).unwrap(), source_bytes[start..end]);

    let bad = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-occurrences", "--root", root_value])
        .args(["--occurrences", "missing.json.xz"])
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(bad.stdout.is_empty(), "nessun JSONL parziale dopo errore");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_canonical_mainline_v1_create_verify_and_export() {
    let root = test_root();
    fs::create_dir_all(&root).unwrap();
    let source = root.join("mainlines.pgn");
    fs::write(
        &source,
        b"[Event \"One\"]\n\n1. e4 {kept} e5 (1... c5) 1-0\n\n[Event \"Two\"]\n\n1. d4 BadSan *\n",
    )
    .unwrap();
    let root_value = root.to_str().unwrap();
    let imported = run_success(&[
        "inspector",
        "import",
        "--root",
        root_value,
        source.to_str().unwrap(),
    ]);
    let segmented = run_success(&[
        "inspector",
        "segment",
        "--root",
        root_value,
        "--manifest",
        output_field(&imported, "MANIFEST"),
    ]);
    let tokenized = run_success(&[
        "inspector",
        "tokenize",
        "--root",
        root_value,
        "--segmentation",
        output_field(&segmented, "SEGMENTATION"),
    ]);
    let parsed = run_success(&[
        "inspector",
        "parse",
        "--root",
        root_value,
        "--tokenization",
        output_field(&tokenized, "TOKENIZATION"),
    ]);
    let interpreted = run_success(&[
        "inspector",
        "interpret",
        "--root",
        root_value,
        "--parse",
        output_field(&parsed, "PARSE"),
    ]);
    let catalog = run_success(&[
        "inspector",
        "catalog",
        "--root",
        root_value,
        "--interpretation",
        output_field(&interpreted, "INTERPRETATION"),
    ]);
    let occurrences = run_success(&[
        "inspector",
        "create-occurrences",
        "--root",
        root_value,
        "--catalog",
        output_field(&catalog, "CATALOG"),
    ]);
    let first = run_success(&[
        "inspector",
        "create-mainlines",
        "--root",
        root_value,
        "--occurrences",
        output_field(&occurrences, "OCCURRENCES"),
    ]);
    assert_eq!(output_field(&first, "MAINLINES"), "2");
    assert_eq!(output_field(&first, "COMPARABLE_MAINLINES"), "1");
    assert_eq!(output_field(&first, "NEW_MANIFEST"), "true");
    let mainlines = output_field(&first, "MAINLINES_ARTIFACT");
    assert!(mainlines.ends_with(".json.xz"));
    let second = run_success(&[
        "inspector",
        "create-mainlines",
        "--root",
        root_value,
        "--occurrences",
        output_field(&occurrences, "OCCURRENCES"),
    ]);
    assert_eq!(output_field(&second, "NEW_MANIFEST"), "false");
    for field in ["MAINLINES_ARTIFACT", "MAINLINES_BLAKE3", "MAINLINES_BYTES"] {
        assert_eq!(output_field(&first, field), output_field(&second, field));
    }
    assert_eq!(
        run_success(&[
            "inspector",
            "verify-mainlines",
            "--root",
            root_value,
            "--mainlines",
            mainlines,
        ]),
        "MAINLINES_VERIFY\tPASS\n"
    );
    let command = || {
        Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(["inspector", "export-mainlines", "--root", root_value])
            .args(["--mainlines", mainlines])
            .output()
            .unwrap()
    };
    let one = command();
    let two = command();
    assert!(one.status.success() && two.status.success());
    assert_eq!(one.stdout, two.stdout);
    assert_eq!(one.stdout.last(), Some(&b'\n'));
    assert_eq!(one.stdout.iter().filter(|byte| **byte == b'\n').count(), 2);
    let records = one
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(records[0]["comparable"].as_bool().unwrap());
    assert!(!records[1]["comparable"].as_bool().unwrap());
    assert!(records.iter().all(|record| {
        record.get("occurrences_manifest").is_none()
            && record.get("interpretation_manifest").is_none()
    }));
    let output = root.join("mainlines.jsonl");
    let exported = run_success(&[
        "inspector",
        "export-mainlines",
        "--root",
        root_value,
        "--mainlines",
        mainlines,
        "--output",
        output.to_str().unwrap(),
    ]);
    assert_eq!(output_field(&exported, "MAINLINES_EXPORT"), "PASS");
    assert_eq!(fs::read(&output).unwrap(), one.stdout);
    let existing = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-mainlines", "--root", root_value])
        .args(["--mainlines", mainlines, "--output"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!existing.status.success());
    let missing = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "export-mainlines", "--root", root_value])
        .args(["--mainlines", "missing.json.xz"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[derive(serde::Serialize)]
struct TestCollectionV1 {
    schema_version: u32,
    collection_profile: String,
    sources: Vec<TestCollectionSourceV1>,
}

#[derive(serde::Serialize)]
struct TestCollectionSourceV1 {
    id: u64,
    source_blake3: String,
    source_bytes: u64,
    catalog: String,
    catalog_blake3: String,
    catalog_bytes: u64,
    games: u64,
    observations: Vec<serde_json::Value>,
}

#[test]
fn cli_archive_occurrence_inventory_v1_is_complete_deterministic_and_fail_closed() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(input.join("copies")).unwrap();
    let multiple = b"[Event \"Comparable\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n[Event \"Non comparable\"]\n[Result \"*\"]\n\n1. d4 BadSan *\n";
    let other = b"[Event \"Other\"]\n[Result \"1/2-1/2\"]\n\n1. c4 c5 1/2-1/2\n";
    fs::write(input.join("multiple.pgn"), multiple).unwrap();
    fs::write(input.join("copies/multiple-copy.PGN"), multiple).unwrap();
    fs::write(input.join("other.pgn"), other).unwrap();

    let ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
    ]);
    assert_eq!(output_field(&ingest, "SOURCES"), "2");
    assert_eq!(output_field(&ingest, "GAMES"), "3");
    let collection = output_field(&ingest, "COLLECTION").to_owned();
    let args = [
        "inspector",
        "inventory-collection",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        &collection,
    ];
    let workspace_before = workspace_files(&workspace);
    let first = run_success(&args);
    let second = run_success(&args);
    assert_eq!(first, second);
    assert_eq!(first.bytes().filter(|byte| *byte == b'\n').count(), 3);
    assert!(first.ends_with('\n'));
    assert_eq!(workspace_before, workspace_files(&workspace));
    assert_eq!(fs::read(input.join("multiple.pgn")).unwrap(), multiple);
    assert_eq!(fs::read(input.join("other.pgn")).unwrap(), other);

    let records = first
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let keys = records
        .iter()
        .map(|record| {
            (
                record["source_id"].as_u64().unwrap(),
                record["game_id"].as_u64().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(keys.iter().any(|(source, game)| {
        keys.iter()
            .any(|(other_source, other_game)| source == other_source && game != other_game)
    }));
    assert!(records.iter().all(|record| {
        record["schema_version"] == 1
            && record["inventory_profile"] == "antiskill_archive_occurrence_inventory_v1"
            && record.get("catalog_game_id").is_none()
            && record["artifacts"]["collection"]["path"] == collection
            && record["artifacts"]["collection"]["blake3"]
                .as_str()
                .is_some_and(|hash| hash.len() == 64)
            && record["artifacts"]["collection"]["bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes > 0)
            && ["catalog", "occurrences", "mainlines"].iter().all(|name| {
                record["artifacts"][name]["path"].is_string()
                    && record["artifacts"][name]["blake3"].is_string()
                    && record["artifacts"][name]["bytes"]
                        .as_u64()
                        .is_some_and(|bytes| bytes > 0)
            })
    }));
    let comparable = records
        .iter()
        .find(|record| record["comparable"] == true)
        .unwrap();
    assert!(comparable["canonical_mainline_id"].is_string());
    let non_comparable = records
        .iter()
        .find(|record| record["comparable"] == false)
        .unwrap();
    assert!(non_comparable["canonical_mainline_id"].is_null());
    assert!(!non_comparable["noncomparability_reasons"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(records
        .iter()
        .any(|record| record["results"] == serde_json::json!(["1-0"])));
    let observations = records
        .iter()
        .map(|record| record["observations"].clone())
        .find(|observations| observations.as_array().unwrap().len() == 2)
        .unwrap();
    assert_eq!(
        observations
            .as_array()
            .unwrap()
            .iter()
            .map(|observation| observation["input_relative_path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["copies/multiple-copy.PGN", "multiple.pgn"]
    );
    assert!(observations.as_array().unwrap().iter().all(|observation| {
        observation["canonical_path"]
            .as_str()
            .is_some_and(|path| std::path::Path::new(path).is_absolute())
    }));

    let collection_value: serde_json::Value =
        serde_json::from_slice(&logical_json(workspace.join(&collection))).unwrap();
    let source_values = collection_value["sources"].as_array().unwrap();
    let v1_sources = source_values
        .iter()
        .map(|source| TestCollectionSourceV1 {
            id: source["id"].as_u64().unwrap(),
            source_blake3: source["source_blake3"].as_str().unwrap().into(),
            source_bytes: source["source_bytes"].as_u64().unwrap(),
            catalog: source["catalog"].as_str().unwrap().into(),
            catalog_blake3: source["catalog_blake3"].as_str().unwrap().into(),
            catalog_bytes: source["catalog_bytes"].as_u64().unwrap(),
            games: source["games"].as_u64().unwrap(),
            observations: source["observations"].as_array().unwrap().clone(),
        })
        .collect();
    let mut v1_bytes = serde_json::to_vec_pretty(&TestCollectionV1 {
        schema_version: 1,
        collection_profile: "antiskill_workspace_collection_v1".into(),
        sources: v1_sources,
    })
    .unwrap();
    v1_bytes.push(b'\n');
    let v1_path = publish_test_collection(&workspace, &v1_bytes);
    let v1 = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "inventory-collection", "--root"])
        .arg(&workspace)
        .args(["--collection", &v1_path])
        .output()
        .unwrap();
    assert!(!v1.status.success());
    assert!(v1.stdout.is_empty());
    assert!(String::from_utf8_lossy(&v1.stderr).contains("Collection v1"));

    let mut mismatched = collection_value;
    let sources = mismatched["sources"].as_array_mut().unwrap();
    let other_catalog = [
        sources[1]["catalog"].clone(),
        sources[1]["catalog_blake3"].clone(),
        sources[1]["catalog_bytes"].clone(),
    ];
    sources[0]["catalog"] = other_catalog[0].clone();
    sources[0]["catalog_blake3"] = other_catalog[1].clone();
    sources[0]["catalog_bytes"] = other_catalog[2].clone();
    let mut mismatch_bytes = serde_json::to_vec_pretty(&mismatched).unwrap();
    mismatch_bytes.push(b'\n');
    let mismatch_path = publish_test_collection(&workspace, &mismatch_bytes);
    let mismatch = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "inventory-collection", "--root"])
        .arg(&workspace)
        .args(["--collection", &mismatch_path])
        .output()
        .unwrap();
    assert!(!mismatch.status.success());
    assert!(mismatch.stdout.is_empty());

    let mainlines = records[0]["artifacts"]["mainlines"]["path"]
        .as_str()
        .unwrap();
    let mainline_path = workspace.join(mainlines);
    let original_mainline = fs::read(&mainline_path).unwrap();
    fs::write(&mainline_path, b"altered").unwrap();
    let corrupt = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(args)
        .output()
        .unwrap();
    assert!(!corrupt.status.success());
    assert!(corrupt.stdout.is_empty());
    fs::write(mainline_path, original_mainline).unwrap();

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_archive_occurrence_inventory_accepts_an_empty_collection_v2() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    let ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
    ]);
    assert_eq!(output_field(&ingest, "SOURCES"), "0");
    assert_eq!(output_field(&ingest, "GAMES"), "0");
    let inventory = run_success(&[
        "inspector",
        "inventory-collection",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        output_field(&ingest, "COLLECTION"),
    ]);
    assert!(inventory.is_empty());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_archive_occurrence_inventory_keeps_identical_games_as_distinct_occurrences() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&input).unwrap();
    let game = b"[Event \"Repeated\"]\n[Result \"*\"]\n\n1. e4 e5 *\n";
    let mut source = game.to_vec();
    source.push(b'\n');
    source.extend_from_slice(game);
    fs::write(input.join("repeated.pgn"), source).unwrap();

    let ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
    ]);
    assert_eq!(output_field(&ingest, "SOURCES"), "1");
    assert_eq!(output_field(&ingest, "GAMES"), "2");
    let inventory = run_success(&[
        "inspector",
        "inventory-collection",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        output_field(&ingest, "COLLECTION"),
    ]);
    let records = inventory
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["source_id"], records[1]["source_id"]);
    assert_eq!(records[0]["game_id"], 0);
    assert_eq!(records[1]["game_id"], 1);
    assert_ne!(records[0]["occurrence_id"], records[1]["occurrence_id"]);
    assert_eq!(
        records[0]["source_game_blake3"],
        records[1]["source_game_blake3"]
    );
    assert_eq!(records[0]["game_bytes"], records[1]["game_bytes"]);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_archive_occurrence_inventory_missing_mainlines_is_fail_closed() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&input).unwrap();
    fs::write(
        input.join("one.pgn"),
        b"[Event \"One\"]\n[Result \"*\"]\n\n1. d4 d5 *\n",
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
    let args = [
        "inspector",
        "inventory-collection",
        "--root",
        workspace.to_str().unwrap(),
        "--collection",
        collection,
    ];
    let valid_inventory = run_success(&args);
    let record: serde_json::Value = serde_json::from_str(valid_inventory.trim_end()).unwrap();
    let mainlines = workspace.join(record["artifacts"]["mainlines"]["path"].as_str().unwrap());
    let saved_mainlines = fs::read(&mainlines).unwrap();
    fs::remove_file(&mainlines).unwrap();
    let missing = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(args)
        .output()
        .unwrap();
    fs::write(&mainlines, saved_mainlines).unwrap();

    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_archive_occurrence_inventory_filters_share_catalog_semantics_and_verify_all() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&input).unwrap();
    let carlsen = b"[Event \"World Final\"]\n[Date \"2024.01.01\"]\n[White \"Magnus Carlsen\"]\n[Black \"Ian\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n";
    let bob = b"[Event \"Open Cup\"]\n[Date \"2024.06.15\"]\n[White \"Alice\"]\n[Black \"BOB\"]\n[Result \"*\"]\n\n1. d4 d5 *\n";
    let other = b"[Event \"World Final\"]\n[Date \"2025.01.01\"]\n[White \"Carol\"]\n[Black \"Dave\"]\n[Result \"*\"]\n\n1. c4 c5 *\n";
    fs::write(input.join("carlsen.pgn"), carlsen).unwrap();
    fs::write(input.join("bob.pgn"), bob).unwrap();
    fs::write(input.join("other.pgn"), other).unwrap();
    let ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
    ]);
    let collection = output_field(&ingest, "COLLECTION").to_owned();
    let workspace_before = workspace_files(&workspace);

    let run_inventory = |filters: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_antiskill"));
        command
            .args(["inspector", "inventory-collection", "--root"])
            .arg(&workspace)
            .args(["--collection", &collection])
            .args(filters)
            .output()
            .unwrap()
    };
    let baseline = run_inventory(&[]);
    assert!(baseline.status.success());
    assert_eq!(baseline.stdout, run_inventory(&[]).stdout);

    let matched_paths = |filters: &[&str]| {
        let output = run_inventory(filters);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut paths = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
            .map(|record| {
                record["observations"][0]["input_relative_path"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths
    };
    assert_eq!(matched_paths(&["--player", "cArLsEn"]), ["carlsen.pgn"]);
    assert_eq!(matched_paths(&["--player", "bob"]), ["bob.pgn"]);
    assert_eq!(matched_paths(&["--event", "open"]), ["bob.pgn"]);
    assert_eq!(matched_paths(&["--date", "2024-06-15"]), ["bob.pgn"]);
    assert_eq!(
        matched_paths(&["--date-from", "2024-01-01", "--date-to", "2024-06-15",]),
        ["bob.pgn", "carlsen.pgn"]
    );
    assert_eq!(
        matched_paths(&[
            "--player",
            "CARLSEN",
            "--event",
            "world",
            "--date-from",
            "2024-01-01",
            "--date-to",
            "2024-01-01",
        ]),
        ["carlsen.pgn"]
    );
    let zero = run_inventory(&["--player", "Nobody"]);
    assert!(zero.status.success());
    assert!(zero.stdout.is_empty());

    for invalid in [
        vec!["--date", "2024-02-30"],
        vec!["--date", "2024-01-01", "--date-from", "2024-01-01"],
        vec!["--date-from", "2024-06-16", "--date-to", "2024-06-15"],
        vec!["--player", ""],
        vec!["--event", " \t"],
        vec!["--unknown"],
    ] {
        let output = run_inventory(&invalid);
        assert!(
            !output.status.success(),
            "argomenti invalidi accettati: {invalid:?}"
        );
        assert!(output.stdout.is_empty());
    }

    let unrelated: serde_json::Value = baseline
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .find(|record: &serde_json::Value| {
            record["observations"][0]["input_relative_path"] == "other.pgn"
        })
        .unwrap();
    let mainlines = workspace.join(
        unrelated["artifacts"]["mainlines"]["path"]
            .as_str()
            .unwrap(),
    );
    let saved_mainlines = fs::read(&mainlines).unwrap();
    fs::write(&mainlines, b"altered").unwrap();
    let corrupted_unmatched = run_inventory(&["--player", "Carlsen"]);
    fs::write(&mainlines, saved_mainlines).unwrap();
    assert!(!corrupted_unmatched.status.success());
    assert!(corrupted_unmatched.stdout.is_empty());

    assert_eq!(workspace_before, workspace_files(&workspace));
    assert_eq!(fs::read(input.join("carlsen.pgn")).unwrap(), carlsen);
    assert_eq!(fs::read(input.join("bob.pgn")).unwrap(), bob);
    assert_eq!(fs::read(input.join("other.pgn")).unwrap(), other);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_archive_view_v1_create_verify_filters_and_fail_closed() {
    let directory = test_root();
    let workspace = directory.join("workspace");
    let input = directory.join("input");
    fs::create_dir_all(&input).unwrap();
    let carlsen = b"[Event \"World Final\"]\n[Date \"2024.01.01\"]\n[White \"Magnus Carlsen\"]\n[Black \"Ian\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n";
    let bob = b"[Event \"Open Cup\"]\n[Date \"2024.06.15\"]\n[White \"Alice\"]\n[Black \"BOB\"]\n[Result \"*\"]\n\n1. d4 d5 *\n";
    let other = b"[Event \"World Final\"]\n[Date \"2025.01.01\"]\n[White \"Carol\"]\n[Black \"Dave\"]\n[Result \"*\"]\n\n1. c4 c5 *\n";
    fs::write(input.join("carlsen.pgn"), carlsen).unwrap();
    fs::write(input.join("bob.pgn"), bob).unwrap();
    fs::write(input.join("other.pgn"), other).unwrap();
    let ingest = run_success(&[
        "inspector",
        "ingest-directory",
        "--root",
        workspace.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
    ]);
    let collection = output_field(&ingest, "COLLECTION").to_owned();

    let inventory_output = |filters: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_antiskill"));
        command
            .args(["inspector", "inventory-collection", "--root"])
            .arg(&workspace)
            .args(["--collection", &collection])
            .args(filters)
            .output()
            .unwrap()
    };
    let create_output = |collection_path: &str, filters: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_antiskill"));
        command
            .args(["inspector", "create-archive-view", "--root"])
            .arg(&workspace)
            .args(["--collection", collection_path])
            .args(filters)
            .output()
            .unwrap()
    };
    let verify_output = |view: &str| {
        Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(["inspector", "verify-archive-view", "--root"])
            .arg(&workspace)
            .args(["--view", view])
            .output()
            .unwrap()
    };
    let inventory_view_output = |view: &str, extra: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_antiskill"));
        command
            .args(["inspector", "inventory-view", "--root"])
            .arg(&workspace)
            .args(["--view", view])
            .args(extra)
            .output()
            .unwrap()
    };
    let successful_create = |filters: &[&str]| {
        let output = create_output(&collection, filters);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let view_json = |stdout: &str| {
        serde_json::from_slice::<serde_json::Value>(&logical_json(
            workspace.join(output_field(stdout, "ARCHIVE_VIEW")),
        ))
        .unwrap()
    };
    let projected_inventory = |filters: &[&str]| {
        let output = inventory_output(filters);
        assert!(output.status.success());
        output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
            .map(|record| {
                serde_json::json!({
                    "source_id": record["source_id"],
                    "game_id": record["game_id"],
                    "occurrence_id": record["occurrence_id"],
                    "canonical_mainline_id": record["canonical_mainline_id"],
                })
            })
            .collect::<Vec<_>>()
    };

    let baseline_inventory = inventory_output(&[]);
    assert!(baseline_inventory.status.success());
    let baseline_records = baseline_inventory
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let collection_value: serde_json::Value =
        serde_json::from_slice(&logical_json(workspace.join(&collection))).unwrap();
    let v1_sources = collection_value["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| TestCollectionSourceV1 {
            id: source["id"].as_u64().unwrap(),
            source_blake3: source["source_blake3"].as_str().unwrap().into(),
            source_bytes: source["source_bytes"].as_u64().unwrap(),
            catalog: source["catalog"].as_str().unwrap().into(),
            catalog_blake3: source["catalog_blake3"].as_str().unwrap().into(),
            catalog_bytes: source["catalog_bytes"].as_u64().unwrap(),
            games: source["games"].as_u64().unwrap(),
            observations: source["observations"].as_array().unwrap().clone(),
        })
        .collect();
    let mut v1_bytes = serde_json::to_vec_pretty(&TestCollectionV1 {
        schema_version: 1,
        collection_profile: "antiskill_workspace_collection_v1".into(),
        sources: v1_sources,
    })
    .unwrap();
    v1_bytes.push(b'\n');
    let v1_path = publish_test_collection(&workspace, &v1_bytes);
    let rejected_v1 = create_output(&v1_path, &[]);
    assert!(!rejected_v1.status.success());
    assert!(rejected_v1.stdout.is_empty());
    assert!(String::from_utf8_lossy(&rejected_v1.stderr).contains("Collection v1"));
    assert!(!workspace.join("inspector/archive_views").exists());
    for filters in [
        vec!["--date", "2024-02-30"],
        vec!["--player", ""],
        vec!["--date", "2024-01-01", "--date-from", "2024-01-01"],
        vec!["--date-from", "2024-06-16", "--date-to", "2024-06-15"],
    ] {
        let invalid = create_output(&collection, &filters);
        assert!(!invalid.status.success());
        assert!(invalid.stdout.is_empty());
        assert!(!workspace.join("inspector/archive_views").exists());
    }

    let unrelated = baseline_records
        .iter()
        .find(|record| record["observations"][0]["input_relative_path"] == "other.pgn")
        .unwrap();
    let unrelated_mainlines = workspace.join(
        unrelated["artifacts"]["mainlines"]["path"]
            .as_str()
            .unwrap(),
    );
    let saved_unrelated_mainlines = fs::read(&unrelated_mainlines).unwrap();
    fs::write(&unrelated_mainlines, b"altered").unwrap();
    let corrupt_nonmatching = create_output(&collection, &["--player", "Carlsen"]);
    fs::write(&unrelated_mainlines, &saved_unrelated_mainlines).unwrap();
    assert!(!corrupt_nonmatching.status.success());
    assert!(corrupt_nonmatching.stdout.is_empty());
    assert!(!workspace.join("inspector/archive_views").exists());

    let upstream_before_create = workspace_files(&workspace);
    let first = successful_create(&[]);
    assert_eq!(output_field(&first, "ARCHIVE_VIEW_OCCURRENCES"), "3");
    assert_eq!(output_field(&first, "NEW_MANIFEST"), "true");
    let first_path = output_field(&first, "ARCHIVE_VIEW").to_owned();
    assert_eq!(
        first_path,
        format!(
            "inspector/archive_views/{}.json.xz",
            output_field(&first, "ARCHIVE_VIEW_BLAKE3")
        )
    );
    assert_eq!(
        logical_json(workspace.join(&first_path)).len().to_string(),
        output_field(&first, "ARCHIVE_VIEW_BYTES")
    );
    let first_json = view_json(&first);
    assert_eq!(first_json["schema_version"], 1);
    assert_eq!(
        first_json["archive_view_profile"],
        "antiskill_archive_view_v1"
    );
    assert_eq!(first_json["collection"]["path"], collection);
    assert_eq!(
        first_json["filters"],
        serde_json::json!({
            "player": null,
            "event": null,
            "date": null,
            "date_from": null,
            "date_to": null,
        })
    );
    assert_eq!(
        first_json["occurrences"],
        serde_json::Value::Array(projected_inventory(&[]))
    );
    let keys = first_json["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| {
            (
                record["source_id"].as_u64().unwrap(),
                record["game_id"].as_u64().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(first_json["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .all(|record| {
            record.as_object().unwrap().keys().collect::<Vec<_>>()
                == [
                    "canonical_mainline_id",
                    "game_id",
                    "occurrence_id",
                    "source_id",
                ]
        }));
    let verified = verify_output(&first_path);
    assert!(verified.status.success());
    assert_eq!(verified.stdout, b"ARCHIVE_VIEW_VERIFY\tPASS\n");
    let before_inventory_view = workspace_files(&workspace);
    let first_inventory_view = inventory_view_output(&first_path, &[]);
    assert!(first_inventory_view.status.success());
    assert_eq!(first_inventory_view.stdout, baseline_inventory.stdout);
    assert_eq!(
        first_inventory_view.stdout,
        inventory_view_output(&first_path, &[]).stdout
    );
    assert_eq!(before_inventory_view, workspace_files(&workspace));
    let repeated = successful_create(&[]);
    assert_eq!(output_field(&repeated, "NEW_MANIFEST"), "false");
    for field in ["ARCHIVE_VIEW", "ARCHIVE_VIEW_BLAKE3", "ARCHIVE_VIEW_BYTES"] {
        assert_eq!(output_field(&first, field), output_field(&repeated, field));
    }
    for original in &upstream_before_create {
        assert!(workspace_files(&workspace).contains(original));
    }

    let upper = successful_create(&["--player", "CARLSEN"]);
    let lower = successful_create(&["--player", "carlsen"]);
    for field in ["ARCHIVE_VIEW", "ARCHIVE_VIEW_BLAKE3", "ARCHIVE_VIEW_BYTES"] {
        assert_eq!(output_field(&upper, field), output_field(&lower, field));
    }
    assert_eq!(output_field(&lower, "NEW_MANIFEST"), "false");
    assert_eq!(view_json(&upper)["filters"]["player"], "carlsen");
    assert_eq!(
        inventory_view_output(output_field(&upper, "ARCHIVE_VIEW"), &[]).stdout,
        inventory_output(&["--player", "CARLSEN"]).stdout
    );
    for filters in [
        vec!["--event", "open"],
        vec!["--date", "2024-06-15"],
        vec!["--date-from", "2024-01-01", "--date-to", "2024-06-15"],
        vec![
            "--player",
            "CARLSEN",
            "--event",
            "world",
            "--date-from",
            "2024-01-01",
            "--date-to",
            "2024-01-01",
        ],
    ] {
        let created = successful_create(&filters);
        assert_eq!(
            view_json(&created)["occurrences"],
            serde_json::Value::Array(projected_inventory(&filters))
        );
        let view_inventory = inventory_view_output(output_field(&created, "ARCHIVE_VIEW"), &[]);
        assert!(view_inventory.status.success());
        assert_eq!(view_inventory.stdout, inventory_output(&filters).stdout);
    }
    let zero = successful_create(&["--player", "Nobody"]);
    assert_eq!(output_field(&zero, "ARCHIVE_VIEW_OCCURRENCES"), "0");
    assert_eq!(view_json(&zero)["occurrences"], serde_json::json!([]));
    assert!(verify_output(output_field(&zero, "ARCHIVE_VIEW"))
        .status
        .success());
    let zero_inventory = inventory_view_output(output_field(&zero, "ARCHIVE_VIEW"), &[]);
    assert!(zero_inventory.status.success());
    assert!(zero_inventory.stdout.is_empty());

    for extra in [
        vec!["--player", "Carlsen"],
        vec!["--event", "World"],
        vec!["--date", "2024-01-01"],
        vec!["--unexpected", "value"],
    ] {
        let unexpected = inventory_view_output(&first_path, &extra);
        assert!(!unexpected.status.success());
        assert!(unexpected.stdout.is_empty());
    }

    let before_verify = workspace_files(&workspace);
    assert!(verify_output(&first_path).status.success());
    assert_eq!(before_verify, workspace_files(&workspace));

    let original_view = fs::read(workspace.join(&first_path)).unwrap();
    fs::write(workspace.join(&first_path), b"altered").unwrap();
    let altered = verify_output(&first_path);
    fs::write(workspace.join(&first_path), &original_view).unwrap();
    assert!(!altered.status.success());
    assert!(altered.stdout.is_empty());
    fs::write(workspace.join(&first_path), b"altered").unwrap();
    let altered_inventory = inventory_view_output(&first_path, &[]);
    fs::write(workspace.join(&first_path), &original_view).unwrap();
    assert!(!altered_inventory.status.success());
    assert!(altered_inventory.stdout.is_empty());

    let alias = "inspector/archive_views/not-canonical.json.xz";
    fs::write(workspace.join(alias), &original_view).unwrap();
    let noncanonical = verify_output(alias);
    assert!(!noncanonical.status.success());
    assert!(noncanonical.stdout.is_empty());
    let noncanonical_inventory = inventory_view_output(alias, &[]);
    assert!(!noncanonical_inventory.status.success());
    assert!(noncanonical_inventory.stdout.is_empty());

    let not_file = "inspector/archive_views/not-file.json.xz";
    fs::create_dir(workspace.join(not_file)).unwrap();
    let directory_inventory = inventory_view_output(not_file, &[]);
    assert!(!directory_inventory.status.success());
    assert!(directory_inventory.stdout.is_empty());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let symlink_path = "inspector/archive_views/symlink.json.xz";
        symlink(workspace.join(&first_path), workspace.join(symlink_path)).unwrap();
        let symlink_inventory = inventory_view_output(symlink_path, &[]);
        assert!(!symlink_inventory.status.success());
        assert!(symlink_inventory.stdout.is_empty());
    }

    let mut wrong_identity = first_json.clone();
    wrong_identity["collection"]["bytes"] =
        serde_json::json!(wrong_identity["collection"]["bytes"].as_u64().unwrap() + 1);
    let mut wrong_identity_bytes = serde_json::to_vec_pretty(&wrong_identity).unwrap();
    wrong_identity_bytes.push(b'\n');
    let wrong_identity_path = publish_test_json(&workspace, "archive_views", &wrong_identity_bytes);
    let identity_mismatch = verify_output(&wrong_identity_path);
    assert!(!identity_mismatch.status.success());
    assert!(identity_mismatch.stdout.is_empty());
    let identity_inventory = inventory_view_output(&wrong_identity_path, &[]);
    assert!(!identity_inventory.status.success());
    assert!(identity_inventory.stdout.is_empty());

    let mut noncanonical_filters = view_json(&upper);
    noncanonical_filters["filters"]["player"] = serde_json::json!("CARLSEN");
    let mut noncanonical_filter_bytes = serde_json::to_vec_pretty(&noncanonical_filters).unwrap();
    noncanonical_filter_bytes.push(b'\n');
    let noncanonical_filter_path =
        publish_test_json(&workspace, "archive_views", &noncanonical_filter_bytes);
    let noncanonical_filter_inventory = inventory_view_output(&noncanonical_filter_path, &[]);
    assert!(!noncanonical_filter_inventory.status.success());
    assert!(noncanonical_filter_inventory.stdout.is_empty());

    let saved_upstream = fs::read(&unrelated_mainlines).unwrap();
    fs::write(&unrelated_mainlines, b"altered").unwrap();
    let upstream_corrupt = verify_output(&first_path);
    let upstream_inventory_corrupt = inventory_view_output(&first_path, &[]);
    fs::write(&unrelated_mainlines, saved_upstream).unwrap();
    assert!(!upstream_corrupt.status.success());
    assert!(upstream_corrupt.stdout.is_empty());
    assert!(!upstream_inventory_corrupt.status.success());
    assert!(upstream_inventory_corrupt.stdout.is_empty());

    let inventory_after = inventory_output(&[]);
    assert!(inventory_after.status.success());
    assert_eq!(baseline_inventory.stdout, inventory_after.stdout);
    assert_eq!(fs::read(input.join("carlsen.pgn")).unwrap(), carlsen);
    assert_eq!(fs::read(input.join("bob.pgn")).unwrap(), bob);
    assert_eq!(fs::read(input.join("other.pgn")).unwrap(), other);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_store_source_assertions_is_content_addressed_repeatable_and_verifiable() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("source-assertions.json");
    fs::write(
        &input,
        br#"{
  "schema_version": 1,
  "assertion_profile": "antiskill_source_assertion_group_v1",
  "group_id": "fide-ratings:2026-10:123456",
  "source_key": "fide-rating-lists",
  "snapshot_id": "snapshot:2026-10",
  "locator": "player:123456",
  "observed_at": "2026-10-11",
  "rights": "open",
  "assertions": [{
    "assertion_id": "rating:standard",
    "subject_kind": "person",
    "subject_id": "fide:123456",
    "predicate": "fide.rating.standard",
    "value": 2412,
    "valid_from": "2026-10-01",
    "valid_to": null
  }, {
    "assertion_id": "title",
    "subject_kind": "person",
    "subject_id": "fide:123456",
    "predicate": "fide.title",
    "value": "IM",
    "valid_from": null,
    "valid_to": null
  }]
}"#,
    )
    .unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-source-assertions", "--root"])
        .arg(&directory)
        .arg("--input")
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first = String::from_utf8(first.stdout).unwrap();
    assert_eq!(output_field(&first, "ASSERTIONS"), "2");
    assert_eq!(output_field(&first, "NEW_ARTIFACT"), "true");
    let path = output_field(&first, "ASSERTIONS_PATH").to_owned();
    let hash = output_field(&first, "ASSERTIONS_BLAKE3").to_owned();
    assert_eq!(path, format!("inspector/assertions/groups/{hash}.json.xz"));
    assert!(directory.join(&path).is_file());

    let second = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "store-source-assertions", "--root"])
        .arg(&directory)
        .arg("--input")
        .arg(&input)
        .output()
        .unwrap();
    assert!(second.status.success());
    let second = String::from_utf8(second.stdout).unwrap();
    assert_eq!(output_field(&second, "ASSERTIONS_PATH"), path);
    assert_eq!(output_field(&second, "ASSERTIONS_BLAKE3"), hash);
    assert_eq!(output_field(&second, "NEW_ARTIFACT"), "false");

    let verified = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "verify-source-assertions", "--root"])
        .arg(&directory)
        .arg("--assertions")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let verified = String::from_utf8(verified.stdout).unwrap();
    assert_eq!(output_field(&verified, "ASSERTIONS_BLAKE3"), hash);
    assert!(verified.contains("SOURCE_ASSERTIONS_VERIFY\tPASS"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_index_source_assertions_preserves_conflicting_values_and_provenance() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();

    let store = |name: &str, source: &str, snapshot: &str, group: &str, value: i64| {
        let input = directory.join(name);
        fs::write(
            &input,
            format!(
                r#"{{"schema_version":1,"assertion_profile":"antiskill_source_assertion_group_v1","group_id":"{group}","source_key":"{source}","snapshot_id":"{snapshot}","locator":"player:123456","observed_at":"2026-10-11","rights":"open","assertions":[{{"assertion_id":"rating:standard","subject_kind":"person","subject_id":"person:123456","predicate":"rating.standard","value":{value},"valid_from":"2026-10-01","valid_to":null}}]}}"#
            ),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(["inspector", "store-source-assertions", "--root"])
            .arg(&directory)
            .arg("--input")
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        output_field(&stdout, "ASSERTIONS_PATH").to_owned()
    };

    let fide = store(
        "fide.json",
        "fide-rating-lists",
        "snapshot:fide:2026-10",
        "fide:123456:2026-10",
        2412,
    );
    let other = store(
        "other.json",
        "event-official",
        "snapshot:event:2026-10",
        "event:123456:2026-10",
        2408,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "index-source-assertions", "--root"])
        .arg(&directory)
        .arg("--assertions")
        .arg(&other)
        .arg("--assertions")
        .arg(&fide)
        .arg("--assertions")
        .arg(&fide)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["subject_kind"], "person");
    assert_eq!(row["subject_id"], "person:123456");
    assert_eq!(row["predicate"], "rating.standard");
    let assertions = row["assertions"].as_array().unwrap();
    assert_eq!(
        assertions.len(),
        2,
        "duplicate artifact must be deduplicated"
    );
    assert_eq!(assertions[0]["source_key"], "event-official");
    assert_eq!(assertions[0]["value"], 2408);
    assert_eq!(assertions[1]["source_key"], "fide-rating-lists");
    assert_eq!(assertions[1]["value"], 2412);
    assert_eq!(assertions[1]["valid_from"], "2026-10-01");
    assert_eq!(assertions[1]["snapshot_id"], "snapshot:fide:2026-10");
    assert!(assertions[1]["path"]
        .as_str()
        .unwrap()
        .ends_with(".json.xz"));

    let missing = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "index-source-assertions", "--root"])
        .arg(&directory)
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("almeno un"));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_reconcile_source_assertion_is_contextual_and_fails_closed_on_bad_dates() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();

    let store = |name: &str, source: &str, snapshot: &str, value: i64, valid_from: &str| {
        let input = directory.join(name);
        fs::write(
            &input,
            format!(
                r#"{{"schema_version":1,"assertion_profile":"antiskill_source_assertion_group_v1","group_id":"{source}:{snapshot}","source_key":"{source}","snapshot_id":"{snapshot}","locator":"player:123456","observed_at":"2026-10-11","rights":"open","assertions":[{{"assertion_id":"rating:standard","subject_kind":"person","subject_id":"person:123456","predicate":"rating.standard","value":{value},"valid_from":"{valid_from}","valid_to":null}}]}}"#
            ),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
            .args(["inspector", "store-source-assertions", "--root"])
            .arg(&directory)
            .arg("--input")
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        output_field(&stdout, "ASSERTIONS_PATH").to_owned()
    };

    let fide = store(
        "fide.json",
        "fide-rating-lists",
        "snapshot:fide:2026-10",
        2412,
        "2026-10-01",
    );
    let event = store(
        "event.json",
        "event-official",
        "snapshot:event:2026-10",
        2408,
        "2026-10-01",
    );
    let policy = directory.join("reconciliation.json");
    fs::write(
        &policy,
        br#"{"schema_version":1,"reconciliation_profile":"antiskill_contextual_reconciliation_v1","subject_kind":"person","subject_id":"person:123456","predicate":"rating.standard","as_of":"2026-10-11","source_preference":["fide-rating-lists","event-official"],"rationale":"FIDE is authoritative for published FIDE ratings at this date"}"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "reconcile-source-assertion", "--root"])
        .arg(&directory)
        .arg("--policy")
        .arg(&policy)
        .arg("--assertions")
        .arg(&event)
        .arg("--assertions")
        .arg(&fide)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result["reconciliation_profile"],
        "antiskill_contextual_reconciliation_v1"
    );
    assert_eq!(result["selected_source_rank"], 0);
    assert_eq!(result["eligible_candidates"], 2);
    assert_eq!(result["selected"]["source_key"], "fide-rating-lists");
    assert_eq!(result["selected"]["value"], 2412);
    assert_eq!(
        result["rationale"],
        "FIDE is authoritative for published FIDE ratings at this date"
    );

    let malformed = store(
        "malformed.json",
        "fide-rating-lists",
        "snapshot:fide:bad-date",
        2412,
        "October",
    );
    let rejected = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "reconcile-source-assertion", "--root"])
        .arg(&directory)
        .arg("--policy")
        .arg(&policy)
        .arg("--assertions")
        .arg(&malformed)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("valid_from"));
    assert!(stderr.contains("Data non canonica YYYY-MM-DD: October"));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_source_assertions_rejects_null_values_unknown_fields_and_missing_provenance() {
    let directory = test_root();
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("source-assertions-invalid.json");

    fs::write(
        &input,
        br#"{"schema_version":1,"assertion_profile":"antiskill_source_assertion_group_v1","group_id":"g","source_key":"","snapshot_id":"s","locator":null,"observed_at":null,"rights":"open","assertions":[{"assertion_id":"a","subject_kind":"person","subject_id":"fide:1","predicate":"fide.rating.standard","value":1,"valid_from":null,"valid_to":null}]}"#,
    ).unwrap();
    let missing = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "validate-source-assertions", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("source_key"));

    fs::write(
        &input,
        br#"{"schema_version":1,"assertion_profile":"antiskill_source_assertion_group_v1","group_id":"g","source_key":"fide","snapshot_id":"s","locator":null,"observed_at":null,"rights":"open","assertions":[{"assertion_id":"a","subject_kind":"person","subject_id":"fide:1","predicate":"fide.rating.standard","value":null,"valid_from":null,"valid_to":null}]}"#,
    ).unwrap();
    let null = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "validate-source-assertions", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!null.status.success());
    assert!(String::from_utf8_lossy(&null.stderr).contains("nullo"));

    fs::write(
        &input,
        br#"{"schema_version":1,"assertion_profile":"antiskill_source_assertion_group_v1","group_id":"g","source_key":"fide","snapshot_id":"s","locator":null,"observed_at":null,"rights":"open","assertions":[{"assertion_id":"a","subject_kind":"person","subject_id":"fide:1","predicate":"fide.rating.standard","value":1,"valid_from":null,"valid_to":null}],"canonical":true}"#,
    ).unwrap();
    let unknown = Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["inspector", "validate-source-assertions", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown field"));
    fs::remove_dir_all(directory).unwrap();
}
