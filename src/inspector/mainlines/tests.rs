use super::*;
use crate::inspector::{catalog, interpretation, parsing, segmentation, source, tokenization};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    mainlines: PathBuf,
    occurrences: PathBuf,
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

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fixture(label: &str, pgn: &[u8]) -> Fixture {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "antiskill-mainlines-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let input = root.join("source.pgn");
    fs::write(&input, pgn).unwrap();
    let imported = source::import_source(&root, &input).unwrap();
    let segmented = segmentation::segment_source(&root, &imported.manifest_path).unwrap();
    let tokenized = tokenization::tokenize(&root, &segmented.path).unwrap();
    let parsed = parsing::parse(&root, &tokenized.path).unwrap();
    let interpreted = interpretation::create_interpretation(&root, &parsed.path).unwrap();
    let catalog = catalog::create_catalog(&root, &interpreted.path).unwrap();
    let occurrences = occurrences::create(&root, &catalog.path).unwrap().path;
    let mainlines = create(&root, &occurrences).unwrap().path;
    Fixture {
        root,
        mainlines,
        occurrences,
    }
}

fn records(fixture: &Fixture) -> Vec<CanonicalMainlineV1> {
    load_verified(&fixture.root, &fixture.mainlines)
        .unwrap()
        .manifest
        .mainlines
}

#[test]
fn frozen_id_uses_exact_preimage() {
    let f = fixture("frozen", b"[Result \"*\"]\n\n1. e4 e5 *\n");
    let record = &records(&f)[0];
    assert_eq!(
        record.canonical_mainline_id.as_deref(),
        Some("0a9f7850e6ee18a851f0c827008f29afd7b0037a3a29e833a0e65a69f0cac7d5")
    );
}

#[test]
fn whitespace_comments_nags_ravs_castling_and_results_do_not_change_identity() {
    let inputs: [&[u8]; 4] = [
        b"[Event \"A\"]\n\n1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O *\n",
        b"[Event \"A\"]\n\n1.e4 {x} e5 $1 2.Nf3 Nc6 (2...Nf6) 3.Bb5 a6 4.Ba4 Nf6 5.O-O 1-0\n",
        b"[Event \"A\"]\n\n1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. 0-0 0-1\n",
        b"[Event \"A\"]\n\n1. e4 (1. d4 ( )) e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O *\n",
    ];
    let fixtures = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| fixture(&format!("invariance-{index}"), input))
        .collect::<Vec<_>>();
    let ids = fixtures
        .iter()
        .map(|fixture| records(fixture)[0].canonical_mainline_id.clone())
        .collect::<Vec<_>>();
    assert!(ids.iter().all(|id| id.is_some()));
    assert!(ids.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(records(&fixtures[0])[0].results[0].value, "*");
    assert_eq!(records(&fixtures[1])[0].results[0].value, "1-0");
}

#[test]
fn result_nodes_are_preserved_but_excluded_from_identity() {
    let a = fixture("result-a", b"[Event \"A\"]\n\n1. e4 1-0 0-1 *\n");
    let b = fixture("result-b", b"[Event \"A\"]\n\n1. e4 1/2-1/2\n");
    let a_record = &records(&a)[0];
    let b_record = &records(&b)[0];
    assert_eq!(
        a_record.canonical_mainline_id,
        b_record.canonical_mainline_id
    );
    assert_eq!(
        a_record
            .results
            .iter()
            .map(|r| r.value.as_str())
            .collect::<Vec<_>>(),
        ["1-0", "0-1", "*"]
    );
}

#[test]
fn deterministic_content_addressing_jsonl_and_verified_recalculation() {
    let f = fixture(
        "determinism",
        b"[Event \"A\"]\n\n1. e4 e5 *\n\n[Event \"B\"]\n\n1. d4 d5 1/2-1/2\n",
    );
    let second = create(&f.root, &f.occurrences).unwrap();
    assert!(!second.new_manifest);
    assert_eq!(second.path, f.mainlines);
    verify(&f.root, &f.mainlines).unwrap();
    let first = export_jsonl(&f.root, &f.mainlines, None).unwrap();
    let again = export_jsonl(&f.root, &f.mainlines, None).unwrap();
    assert_eq!(first, again);
    assert_eq!(first.last(), Some(&b'\n'));
    assert_eq!(first.iter().filter(|byte| **byte == b'\n').count(), 2);
    assert!(!String::from_utf8(first).unwrap().contains("_manifest\""));
}

#[test]
fn non_applied_move_and_nonstandard_variant_are_not_comparable() {
    let f = fixture(
        "not-comparable",
        b"[Event \"A\"]\n\n1. e4 BadSan e5 *\n\n[Event \"B\"]\n[Variant \"Chess960\"]\n\n1. e4 *\n",
    );
    let records = records(&f);
    assert!(records.iter().all(|record| !record.comparable));
    assert!(records
        .iter()
        .all(|record| record.canonical_mainline_id.is_none()));
    assert!(records[0]
        .noncomparability_reasons
        .contains(&NoncomparabilityReasonV1::NonAppliedMove));
    assert!(records[1]
        .noncomparability_reasons
        .contains(&NoncomparabilityReasonV1::NonStandardVariant));
}

#[test]
fn output_is_no_overwrite_and_traversal_safe() {
    let f = fixture("filesystem", b"[Event \"A\"]\n\n1. e4 *\n");
    let output = f.root.join("mainlines.jsonl");
    export_jsonl(&f.root, &f.mainlines, Some(Path::new("mainlines.jsonl"))).unwrap();
    let original = fs::read(&output).unwrap();
    assert!(export_jsonl(&f.root, &f.mainlines, Some(Path::new("mainlines.jsonl"))).is_err());
    assert_eq!(fs::read(output).unwrap(), original);
    assert!(export_jsonl(&f.root, &f.mainlines, Some(Path::new("../outside"))).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("mainlines.jsonl", f.root.join("link.jsonl")).unwrap();
        assert!(export_jsonl(&f.root, &f.mainlines, Some(Path::new("link.jsonl"))).is_err());
    }
}

#[test]
fn all_six_fen_fields_participate_in_identity() {
    let f = fixture("fen-fields", b"[Event \"A\"]\n\n1. e4 *\n");
    let moves = records(&f)[0].moves.clone();
    let base = "8/8/8/8/8/8/8/K6k w KQ e3 0 1";
    let variants = [
        "7k/8/8/8/8/8/8/K7 w KQ e3 0 1",
        "8/8/8/8/8/8/8/K6k b KQ e3 0 1",
        "8/8/8/8/8/8/8/K6k w - e3 0 1",
        "8/8/8/8/8/8/8/K6k w KQ - 0 1",
        "8/8/8/8/8/8/8/K6k w KQ e3 1 1",
        "8/8/8/8/8/8/8/K6k w KQ e3 0 2",
    ];
    let base_id = extraction::canonical_mainline_id(base, &moves).unwrap();
    let ids = variants
        .iter()
        .map(|fen| extraction::canonical_mainline_id(fen, &moves).unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 6);
    assert!(!ids.contains(&base_id));
}

#[test]
fn recovered_and_canonical_fen_produce_the_same_identity() {
    let canonical = fixture(
        "fen-canonical",
        b"[SetUp \"1\"]\n[FEN \"7k/P7/8/8/8/8/8/7K w - - 0 1\"]\n\n1. a8=Q+ *\n",
    );
    let recovered = fixture(
        "fen-recovered",
        b"[SetUp \"1\"]\n[FEN \"7k/P7/8/8/8/8/8/7K w\"]\n\n1. a8=Q+ *\n",
    );
    let a = &records(&canonical)[0];
    let b = &records(&recovered)[0];
    assert_eq!(a.initial_fen, b.initial_fen);
    assert_eq!(a.canonical_mainline_id, b.canonical_mainline_id);
}

#[test]
fn missing_invalid_ambiguous_and_empty_mainlines_are_not_comparable() {
    let cases: [(&str, &[u8], NoncomparabilityReasonV1); 4] = [
        (
            "missing",
            b"[SetUp \"1\"]\n\n1. e4 *\n",
            NoncomparabilityReasonV1::MissingInitialPosition,
        ),
        (
            "invalid",
            b"[SetUp \"1\"]\n[FEN \"invalid\"]\n\n1. e4 *\n",
            NoncomparabilityReasonV1::InvalidInitialPosition,
        ),
        (
            "ambiguous",
            b"[SetUp \"1\"]\n[FEN \"7k/P7/8/8/8/8/8/7K w - - 0 1\"]\n[FEN \"7k/8/P7/8/8/8/8/7K w - - 0 1\"]\n\n1. a8=Q+ *\n",
            NoncomparabilityReasonV1::AmbiguousInitialPosition,
        ),
        (
            "empty",
            b"[Event \"A\"]\n\n*\n",
            NoncomparabilityReasonV1::EmptyMainline,
        ),
    ];
    for (label, input, reason) in cases {
        let fixture = fixture(label, input);
        let record = &records(&fixture)[0];
        assert!(!record.comparable, "{label}");
        assert!(record.canonical_mainline_id.is_none(), "{label}");
        assert!(record.noncomparability_reasons.contains(&reason), "{label}");
    }
}

#[test]
fn every_non_applied_status_causes_the_frozen_reason() {
    let f = fixture("statuses", b"[Event \"A\"]\n\n1. e4 *\n");
    let verified_occurrences = occurrences::load_verified(&f.root, &f.occurrences).unwrap();
    let game = &verified_occurrences.catalog.interpretation.manifest.games[0];
    let mut moves = records(&f)[0].moves.clone();
    use crate::inspector::interpretation::manifest::MoveStatusV1;
    for status in [
        MoveStatusV1::InvalidEncoding,
        MoveStatusV1::InvalidSan,
        MoveStatusV1::AmbiguousSan,
        MoveStatusV1::IllegalMove,
        MoveStatusV1::BlockedByPriorError,
        MoveStatusV1::UnsupportedVariant,
        MoveStatusV1::UnknownInitialPosition,
        MoveStatusV1::AfterResult,
    ] {
        moves[0].status = status;
        assert!(extraction::reasons(game, game.lines[0].status, &moves)
            .contains(&NoncomparabilityReasonV1::NonAppliedMove));
    }
}

#[test]
fn create_mainlines_rejects_content_addressed_occurrence_body_mutation() {
    let f = fixture("occurrence-body-mutation", b"[Event \"A\"]\n\n1. e4 e5 *\n");
    let original = crate::inspector::common::logical_json_bytes(
        &f.root.join(&f.occurrences),
        "occurrence mutation test",
    )
    .unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    value["occurrences"][0]["occurrence_id"] = serde_json::Value::String("0".repeat(64));
    let mut mutated = serde_json::to_vec_pretty(&value).unwrap();
    mutated.push(b'\n');
    let hash = blake3::hash(&mutated).to_hex().to_string();
    let relative = PathBuf::from(format!("inspector/occurrences/{hash}.json.xz"));
    crate::inspector::common::replace_logical_json(&f.root.join(&relative), &mutated);

    // The streaming loader intentionally validates identity/provenance only.
    occurrences::load_streaming_occurrences(&f.root, &relative, None).unwrap();
    // Mainline creation must consume and byte-verify the persisted Occurrences body.
    assert!(create(&f.root, &relative).is_err());
}

#[test]
fn manifest_mutation_unknown_fields_and_upstream_corruption_are_rejected() {
    let f = fixture("corruption", b"[Event \"A\"]\n\n1. e4 *\n");
    let path = f.root.join(&f.mainlines);
    let original = crate::inspector::common::logical_json_bytes(&path, "test").unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    value["mainlines"][0]["comparable"] = serde_json::Value::Bool(false);
    let mut mutated = serde_json::to_vec_pretty(&value).unwrap();
    mutated.push(b'\n');
    crate::inspector::common::replace_logical_json(&path, &mutated);
    assert!(verify(&f.root, &f.mainlines).is_err());
    crate::inspector::common::replace_logical_json(&path, &original);

    let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    value["unknown"] = serde_json::Value::Bool(true);
    let mut unknown = serde_json::to_vec_pretty(&value).unwrap();
    unknown.push(b'\n');
    crate::inspector::common::replace_logical_json(&path, &unknown);
    assert!(verify(&f.root, &f.mainlines).is_err());
    crate::inspector::common::replace_logical_json(&path, &original);

    let occurrence_path = f.root.join(&f.occurrences);
    let occurrence_original = fs::read(&occurrence_path).unwrap();
    fs::write(&occurrence_path, b"corrupt").unwrap();
    assert!(verify(&f.root, &f.mainlines).is_err());
    fs::write(occurrence_path, occurrence_original).unwrap();
    verify(&f.root, &f.mainlines).unwrap();
}

#[test]
fn every_persistent_record_category_mutation_is_rejected_after_canonical_loading() {
    let f = fixture("mutations", b"[Event \"A\"]\n\n1. e4 1-0\n");
    let original = crate::inspector::common::logical_json_bytes(
        &f.root.join(&f.mainlines),
        "mainline mutation test",
    )
    .unwrap();
    let original: serde_json::Value = serde_json::from_slice(&original).unwrap();
    type Mutation = (&'static str, fn(&mut serde_json::Value));
    let mutations: [Mutation; 8] = [
        ("identity", |value| {
            value["mainlines"][0]["occurrence_id"] = serde_json::Value::String("0".repeat(64))
        }),
        ("canonical id", |value| {
            value["mainlines"][0]["canonical_mainline_id"] =
                serde_json::Value::String("1".repeat(64))
        }),
        ("position", |value| {
            value["mainlines"][0]["initial_fen"] =
                serde_json::Value::String("8/8/8/8/8/8/8/8 w - - 0 1".into())
        }),
        ("status", |value| {
            value["mainlines"][0]["mainline_status"] = serde_json::Value::String("partial".into())
        }),
        ("move", |value| {
            value["mainlines"][0]["moves"][0]["raw_san"] = serde_json::Value::String("d4".into())
        }),
        ("result", |value| {
            value["mainlines"][0]["results"][0]["value"] = serde_json::Value::String("0-1".into())
        }),
        ("reason", |value| {
            value["mainlines"][0]["noncomparability_reasons"] =
                serde_json::json!(["non_applied_move"])
        }),
        ("provenance", |value| {
            value["mainlines"][0]["provenance"]["occurrences_manifest_bytes"] =
                serde_json::Value::from(1_u64)
        }),
    ];
    for (label, mutate) in mutations {
        let mut value = original.clone();
        mutate(&mut value);
        let mut bytes = serde_json::to_vec_pretty(&value).unwrap();
        bytes.push(b'\n');
        let hash = blake3::hash(&bytes).to_hex().to_string();
        let relative = PathBuf::from(format!("inspector/mainlines/{hash}.json.xz"));
        crate::inspector::common::replace_logical_json(&f.root.join(&relative), &bytes);
        assert!(verify(&f.root, &relative).is_err(), "{label}");
    }
}

#[test]
fn canonical_json_reuse_and_corrupt_compressed_reference_fails_closed() {
    let f = fixture("storage", b"[Event \"A\"]\n\n1. e4 *\n");
    let compressed = f.root.join(&f.mainlines);
    let logical = crate::inspector::common::logical_json_bytes(&compressed, "test").unwrap();
    let legacy = compressed.with_extension("").with_extension("json");
    fs::write(&legacy, &logical).unwrap();
    fs::remove_file(&compressed).unwrap();
    let relative_legacy = legacy.strip_prefix(&f.root).unwrap();
    verify(&f.root, relative_legacy).unwrap();

    fs::write(&compressed, b"not an xz stream").unwrap();
    assert!(verify(&f.root, &f.mainlines).is_err());
    assert!(verify(&f.root, relative_legacy).is_ok());
    assert_eq!(fs::read(&legacy).unwrap(), logical);
}

#[test]
fn incomplete_mainline_has_no_identity() {
    let f = fixture(
        "incomplete-line",
        b"[Event \"A\"]\n\n1. e4 [Broken \"inside\"] *\n",
    );
    let record = &records(&f)[0];
    assert!(!record.comparable);
    assert!(record.canonical_mainline_id.is_none());
    assert!(record
        .noncomparability_reasons
        .contains(&NoncomparabilityReasonV1::MainlineNotComplete));
}

#[test]
fn every_noncomplete_mainline_status_has_no_identity() {
    use crate::inspector::interpretation::manifest::LineStatusV1;
    let f = fixture("line-statuses", b"[Event \"A\"]\n\n1. e4 *\n");
    let verified_occurrences = occurrences::load_verified(&f.root, &f.occurrences).unwrap();
    let game = &verified_occurrences.catalog.interpretation.manifest.games[0];
    let moves = records(&f)[0].moves.clone();
    for status in [
        LineStatusV1::Partial,
        LineStatusV1::UnsupportedVariant,
        LineStatusV1::UnknownStartPosition,
        LineStatusV1::UnexpectedStructure,
    ] {
        assert!(extraction::reasons(game, status, &moves)
            .contains(&NoncomparabilityReasonV1::MainlineNotComplete));
    }
}

#[test]
#[ignore = "pilot manuale canonical mainline con il PGN reale"]
fn pilot_reale_has_59_records() {
    let real = real_pilot_pgn();
    let bytes = fs::read(real).unwrap();
    let f = fixture("pilot-real", &bytes);
    let records = records(&f);
    assert_eq!(records.len(), 59);
    let comparable = records.iter().filter(|record| record.comparable).count();
    let non_comparable = records.len() - comparable;
    println!(
        "PILOT_MAINLINES\trecords=59\tcomparable={comparable}\tnon_comparable={non_comparable}"
    );
    assert_eq!(comparable + non_comparable, 59);
    assert_eq!(
        export_jsonl(&f.root, &f.mainlines, None).unwrap(),
        export_jsonl(&f.root, &f.mainlines, None).unwrap()
    );
}

#[test]
fn verified_dedup_projection_preserves_only_required_identity() {
    let f = fixture(
        "dedup-projection",
        b"[Event \"A\"]\n\n1. e4 e5 1-0\n\n[Event \"B\"]\n\n1. d4 BadSan *\n",
    );

    let projected = load_verified_for_dedup(&f.root, &f.mainlines).unwrap();

    assert_eq!(projected.records.len(), 2);
    assert_eq!(
        projected.artifact.blake3,
        load_verified(&f.root, &f.mainlines)
            .unwrap()
            .manifest_blake3
    );
    assert!(projected.records[0].comparable);
    assert!(projected.records[0].canonical_mainline_id.is_some());
    assert_eq!(projected.records[0].results, ["1-0"]);
    assert!(!projected.records[1].comparable);
    assert!(projected.records[1].canonical_mainline_id.is_none());
    assert!(projected.records[1]
        .noncomparability_reasons
        .contains(&"non_applied_move".to_owned()));
    assert!(projected.records.iter().all(|record| record.game_bytes > 0));
}
