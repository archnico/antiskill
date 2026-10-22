use super::*;
use crate::inspector::{catalog, interpretation, parsing, segmentation, source, tokenization};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    source: Vec<u8>,
    catalog: PathBuf,
    occurrences: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "antiskill-occurrences-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn catalog_pipeline(root: &Path, source_path: &Path) -> PathBuf {
    let imported = source::import_source(root, source_path).unwrap();
    let segmented = segmentation::segment_source(root, &imported.manifest_path).unwrap();
    let tokenized = tokenization::tokenize(root, &segmented.path).unwrap();
    let parsed = parsing::parse(root, &tokenized.path).unwrap();
    let interpreted = interpretation::create_interpretation(root, &parsed.path).unwrap();
    catalog::create_catalog(root, &interpreted.path)
        .unwrap()
        .path
}

fn fixture(label: &str, bytes: &[u8]) -> Fixture {
    let root = root(label);
    fs::create_dir_all(&root).unwrap();
    let source_path = root.join("source.pgn");
    fs::write(&source_path, bytes).unwrap();
    let catalog = catalog_pipeline(&root, &source_path);
    let occurrences = create(&root, &catalog).unwrap().path;
    Fixture {
        root,
        source: bytes.to_vec(),
        catalog,
        occurrences,
    }
}

fn manifest(fixture: &Fixture) -> OccurrenceManifestV1 {
    load_verified(&fixture.root, &fixture.occurrences)
        .unwrap()
        .manifest
}

fn serialized(manifest: &OccurrenceManifestV1) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("Serializzazione GameOccurrence: {error}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[test]
fn frozen_occurrence_identity_uses_the_exact_preimage() {
    assert_eq!(
        extraction::occurrence_id(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            42,
            3,
            17,
            "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
        ),
        "83ee8e9dad61b07b365a2fd9f7967081c8caf0f5afd8e7b5a0e603f49190018e"
    );
}

#[test]
fn streaming_occurrence_loader_does_not_semantically_replay_catalog() {
    let source = include_str!("mod.rs");
    let loader = source
        .split("pub(in crate::inspector) fn load_streaming_occurrences")
        .nth(1)
        .unwrap()
        .split("#[derive(Serialize)]")
        .next()
        .unwrap();
    assert!(!loader.contains("for_each_verified_catalog_game"));
}

#[test]
fn occurrence_creation_verifies_catalog_during_the_output_pass() {
    let f = fixture("fused-catalog-verification", b"[Event \"X\"]\n\n1. e4 *\n");
    let logical =
        crate::inspector::common::logical_json_bytes(&f.root.join(&f.catalog), "catalog test")
            .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&logical).unwrap();
    let original = value["games"][0]["source_game_blake3"].as_str().unwrap();
    let text = String::from_utf8(logical).unwrap();
    let needle = format!("\"source_game_blake3\": \"{original}\"");
    let replacement = format!("\"source_game_blake3\": \"{}\"", "0".repeat(64));
    let mutated = text.replacen(&needle, &replacement, 1);
    assert_ne!(
        mutated, text,
        "fixture must expose the semantic field under test"
    );

    let hash = blake3::hash(mutated.as_bytes()).to_hex().to_string();
    let mutated_catalog = PathBuf::from("inspector/catalogs").join(format!("{hash}.json"));
    fs::write(f.root.join(&mutated_catalog), mutated.as_bytes()).unwrap();

    assert!(
        create(&f.root, &mutated_catalog).is_err(),
        "content-addressed but semantically corrupt Catalog accepted by fused occurrence creation"
    );
}

#[test]
fn verified_chain_mapping_diagnostics_jsonl_and_exact_export() {
    let f = fixture(
        "mapping",
        b"[Event \"Valid\"]\n[White \"A\"]\n\n1. e4 ( ) *\n\n[Broken \"unterminated\n",
    );
    let created_again = create(&f.root, &f.catalog).unwrap();
    assert!(!created_again.new_manifest);
    assert_eq!(created_again.path, f.occurrences);
    let loaded = load_verified(&f.root, &f.occurrences).unwrap();
    assert_eq!(loaded.manifest.occurrence_profile, OCCURRENCE_PROFILE);
    assert!(!loaded.manifest.occurrences.is_empty());
    for occurrence in &loaded.manifest.occurrences {
        assert_eq!(occurrence.ordinal, occurrence.catalog_game_id);
        assert_eq!(occurrence.ordinal, occurrence.interpretation_game_id);
        assert_eq!(occurrence.ordinal, occurrence.parse_tree_index);
        assert_eq!(
            occurrence.provenance,
            validation::provenance(&loaded.manifest)
        );
        for diagnostic in &occurrence.syntax_diagnostics {
            assert!(diagnostic.start_byte >= occurrence.source_start);
            assert!(diagnostic.end_byte <= occurrence.source_end);
        }
    }
    let jsonl_one = export_jsonl(&f.root, &f.occurrences, None).unwrap();
    let jsonl_two = export_jsonl(&f.root, &f.occurrences, None).unwrap();
    assert_eq!(jsonl_one, jsonl_two);
    assert_eq!(jsonl_one.last(), Some(&b'\n'));
    assert!(!String::from_utf8(jsonl_one.clone())
        .unwrap()
        .contains("_manifest\":"));
    assert_eq!(
        jsonl_one.iter().filter(|byte| **byte == b'\n').count(),
        loaded.manifest.occurrences.len()
    );
    for line in jsonl_one
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let _: GameOccurrenceV1 = serde_json::from_slice(line).unwrap();
        assert!(!line.contains(&b'\n'));
    }

    let occurrence = &loaded.manifest.occurrences[0];
    let output = f.root.join("round-trip.pgn");
    let exported = export_one(&f.root, &f.occurrences, &occurrence.occurrence_id, &output).unwrap();
    assert_eq!(exported.blake3, occurrence.source_game_blake3);
    assert_eq!(
        fs::read(output).unwrap(),
        f.source[occurrence.source_start as usize..occurrence.source_end as usize]
    );
}

#[test]
fn source_manifest_paths_do_not_affect_occurrence_ids() {
    let root = root("source-paths");
    fs::create_dir_all(&root).unwrap();
    let bytes = b"[Event \"Same\"]\n\n1. e4 *\n";
    let first_source = root.join("first.pgn");
    let second_source = root.join("second.pgn");
    fs::write(&first_source, bytes).unwrap();
    fs::write(&second_source, bytes).unwrap();
    let first_catalog = catalog_pipeline(&root, &first_source);
    let second_catalog = catalog_pipeline(&root, &second_source);
    let first = create(&root, &first_catalog).unwrap();
    let second = create(&root, &second_catalog).unwrap();
    let first_ids = load_verified(&root, &first.path)
        .unwrap()
        .manifest
        .occurrences
        .into_iter()
        .map(|occurrence| occurrence.occurrence_id)
        .collect::<Vec<_>>();
    let second_ids = load_verified(&root, &second.path)
        .unwrap()
        .manifest
        .occurrences
        .into_iter()
        .map(|occurrence| occurrence.occurrence_id)
        .collect::<Vec<_>>();
    assert_eq!(first_ids, second_ids);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn duplicate_intervals_and_different_source_blobs_have_distinct_ids() {
    let game = b"[Event \"Same\"]\n\n1. e4 *\n";
    let mut bytes = game.to_vec();
    bytes.push(b'\n');
    bytes.extend_from_slice(game);
    let f = fixture("duplicate-intervals", &bytes);
    let occurrences = manifest(&f).occurrences;
    assert_eq!(occurrences.len(), 2);
    assert_ne!(occurrences[0].source_start, occurrences[1].source_start);
    assert_ne!(occurrences[0].occurrence_id, occurrences[1].occurrence_id);
    for occurrence in &occurrences {
        assert_eq!(
            occurrence.occurrence_id,
            extraction::occurrence_id(
                &occurrence.source_blake3,
                occurrence.source_bytes,
                occurrence.source_start,
                occurrence.source_end,
                &occurrence.source_game_blake3,
            )
        );
    }

    let game_hash = "f".repeat(64);
    let one = extraction::occurrence_id(&"1".repeat(64), 100, 10, 20, &game_hash);
    let two = extraction::occurrence_id(&"2".repeat(64), 100, 10, 20, &game_hash);
    assert_ne!(one, two);
}

#[test]
fn identical_game_bytes_in_distinct_source_blobs_have_distinct_ids() {
    let root = root("different-blobs");
    fs::create_dir_all(&root).unwrap();
    let game = b"[Event \"Same\"]\n\n1. e4 *\n";
    let first_source = root.join("first.pgn");
    let second_source = root.join("second.pgn");
    fs::write(&first_source, game).unwrap();
    let mut second_blob = game.to_vec();
    second_blob.push(b'\n');
    fs::write(&second_source, second_blob).unwrap();
    let first_catalog = catalog_pipeline(&root, &first_source);
    let second_catalog = catalog_pipeline(&root, &second_source);
    let first_path = create(&root, &first_catalog).unwrap().path;
    let second_path = create(&root, &second_catalog).unwrap().path;
    let first = load_verified(&root, &first_path).unwrap().manifest;
    let second = load_verified(&root, &second_path).unwrap().manifest;
    assert_ne!(first.source_blake3, second.source_blake3);
    assert_eq!(
        first.occurrences[0].source_game_blake3,
        second.occurrences[0].source_game_blake3
    );
    assert_ne!(
        first.occurrences[0].occurrence_id,
        second.occurrences[0].occurrence_id
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn verified_candidates_preserve_valid_recovered_and_incomplete_root_statuses() {
    let f = fixture(
        "syntax-statuses",
        b"[Event \"Valid\"]\n\n1. e4 *\n\n[Event \"Recovered\"]\n\n1. d4 ( ) *\n\n[Event \"Incomplete\"]\n\n1. c4 {open\n\n[Event \"Internal unparsed\"]\n\n1. h4 [Bad \"X\"] *\n",
    );
    let loaded = load_verified(&f.root, &f.occurrences).unwrap();
    let statuses = loaded
        .manifest
        .occurrences
        .iter()
        .map(|occurrence| occurrence.syntax_status)
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        vec![
            SyntaxStatusV1::Valid,
            SyntaxStatusV1::Recovered,
            SyntaxStatusV1::Incomplete,
            SyntaxStatusV1::Recovered,
        ]
    );
    let tree = &loaded.catalog.interpretation.parse.manifest.trees[3];
    assert!(tree
        .nodes
        .iter()
        .any(|node| node.kind == crate::inspector::parsing::model::NodeKind::Unparsed));
    let diagnostic = tree
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code
                == crate::inspector::parsing::model::DiagnosticCode::TagPairOutsideTagSection
        })
        .unwrap();
    let occurrence_diagnostic = loaded.manifest.occurrences[3]
        .syntax_diagnostics
        .iter()
        .find(|candidate| candidate.ordinal == diagnostic.ordinal)
        .unwrap();
    assert_eq!(occurrence_diagnostic.code, diagnostic.code);
    assert_eq!(occurrence_diagnostic.node_id, diagnostic.node_id);
    assert_eq!(occurrence_diagnostic.start_token, diagnostic.start_token);
    assert_eq!(occurrence_diagnostic.end_token, diagnostic.end_token);
    assert!(occurrence_diagnostic.start_byte >= loaded.manifest.occurrences[3].source_start);
    assert!(occurrence_diagnostic.end_byte <= loaded.manifest.occurrences[3].source_end);
    let mut invalid = serde_json::to_value(&loaded.manifest.occurrences[3]).unwrap();
    invalid["syntax_status"] = "unparsed".into();
    assert!(serde_json::from_value::<GameOccurrenceV1>(invalid).is_err());
}

#[test]
fn canonical_json_reuse_and_corrupt_compressed_reference_fails_closed() {
    let f = fixture("storage", b"[Event \"X\"]\n\n1. e4 *\n");
    let compressed = f.root.join(&f.occurrences);
    let logical = crate::inspector::common::logical_json_bytes(&compressed, "test").unwrap();
    assert_eq!(logical.last(), Some(&b'\n'));
    assert_eq!(
        serde_json::to_vec_pretty(&manifest(&f)).unwrap(),
        logical[..logical.len() - 1]
    );
    let legacy = compressed.with_extension("").with_extension("json");
    fs::write(&legacy, &logical).unwrap();
    fs::remove_file(&compressed).unwrap();
    let relative_legacy = legacy.strip_prefix(&f.root).unwrap();
    verify(&f.root, relative_legacy).unwrap();

    fs::write(&compressed, b"not an xz stream").unwrap();
    assert!(verify(&f.root, &f.occurrences).is_err());
    assert!(verify(&f.root, relative_legacy).is_ok());
    assert_eq!(fs::read(&legacy).unwrap(), logical);
}

#[test]
fn every_occurrence_identity_status_diagnostic_tag_and_provenance_mutation_is_rejected() {
    type Mutation = (&'static str, fn(&mut OccurrenceManifestV1));
    let f = fixture(
        "mutations",
        b"[Event \"X\"]\n\n1. e4 ( ) *\n\n[Broken \"unterminated\n",
    );
    let path = f.root.join(&f.occurrences);
    let base = manifest(&f);
    assert!(!base.occurrences[0].syntax_diagnostics.is_empty());
    assert!(!base.occurrences[0].interpretation_diagnostics.is_empty());
    let mutations: Vec<Mutation> = vec![
        ("source identity", |m| m.source_blake3 = "0".repeat(64)),
        ("source bytes", |m| m.source_bytes += 1),
        ("source manifest hash", |m| {
            m.source_manifest_blake3 = "0".repeat(64)
        }),
        ("source manifest bytes", |m| m.source_manifest_bytes += 1),
        ("segmentation hash", |m| {
            m.segmentation_manifest_blake3 = "0".repeat(64)
        }),
        ("segmentation bytes", |m| m.segmentation_manifest_bytes += 1),
        ("tokenization hash", |m| {
            m.tokenization_manifest_blake3 = "0".repeat(64)
        }),
        ("tokenization bytes", |m| m.tokenization_manifest_bytes += 1),
        ("parse hash", |m| m.parse_manifest_blake3 = "0".repeat(64)),
        ("parse bytes", |m| m.parse_manifest_bytes += 1),
        ("interpretation hash", |m| {
            m.interpretation_manifest_blake3 = "0".repeat(64)
        }),
        ("interpretation bytes", |m| {
            m.interpretation_manifest_bytes += 1
        }),
        ("catalog hash", |m| {
            m.catalog_manifest_blake3 = "0".repeat(64)
        }),
        ("catalog bytes", |m| m.catalog_manifest_bytes += 1),
        ("ordinal", |m| m.occurrences[0].ordinal += 1),
        ("source start", |m| m.occurrences[0].source_start += 1),
        ("source end", |m| m.occurrences[0].source_end -= 1),
        ("game hash", |m| {
            m.occurrences[0].source_game_blake3 = "0".repeat(64)
        }),
        ("segment ordinal", |m| m.occurrences[0].segment_ordinal += 1),
        ("parse tree", |m| m.occurrences[0].parse_tree_index += 1),
        ("interpretation game", |m| {
            m.occurrences[0].interpretation_game_id += 1
        }),
        ("catalog game", |m| m.occurrences[0].catalog_game_id += 1),
        ("occurrence id", |m| {
            m.occurrences[0].occurrence_id = "0".repeat(64)
        }),
        ("syntax status", |m| {
            m.occurrences[0].syntax_status = SyntaxStatusV1::Valid
        }),
        ("interpretation status", |m| {
            m.occurrences[0].interpretation_status =
                crate::inspector::catalog::manifest::CatalogInterpretationStatusV1::UnexpectedStructure
        }),
        ("variant", |m| {
            m.occurrences[0].variant = crate::inspector::catalog::manifest::CatalogVariantV1::Other
        }),
        ("syntax diagnostic", |m| {
            m.occurrences[0].syntax_diagnostics[0].start_byte += 1
        }),
        ("interpretation diagnostic", |m| {
            m.occurrences[0].interpretation_diagnostics[0].node_id = Some(999)
        }),
        ("tag", |m| m.occurrences[0].tags.clear()),
        ("provenance", |m| {
            m.occurrences[0].provenance.catalog_manifest_bytes += 1
        }),
    ];
    for (name, mutate) in mutations {
        let mut candidate = base.clone();
        mutate(&mut candidate);
        crate::inspector::common::replace_logical_json(&path, &serialized(&candidate).unwrap());
        assert!(
            verify(&f.root, &f.occurrences).is_err(),
            "mutation accepted: {name}"
        );
    }
}

#[test]
fn existing_traversal_and_external_symlink_outputs_fail_without_external_changes() {
    let f = fixture("filesystem", b"[Event \"X\"]\n\n1. e4 *\n");
    let occurrence = manifest(&f).occurrences.remove(0);
    let existing = f.root.join("existing.pgn");
    fs::write(&existing, b"keep").unwrap();
    assert!(export_one(
        &f.root,
        &f.occurrences,
        &occurrence.occurrence_id,
        &existing
    )
    .is_err());
    assert_eq!(fs::read(&existing).unwrap(), b"keep");
    assert!(export_one(
        &f.root,
        &f.occurrences,
        &occurrence.occurrence_id,
        Path::new("../escape.pgn")
    )
    .is_err());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = f.root.with_extension("outside");
        fs::write(&outside, b"outside").unwrap();
        let link = f.root.join("linked-output.pgn");
        symlink(&outside, &link).unwrap();
        assert!(export_one(&f.root, &f.occurrences, &occurrence.occurrence_id, &link).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
        fs::remove_file(outside).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn occurrence_artifact_and_publication_directory_symlinks_are_fail_closed() {
    use std::os::unix::fs::symlink;
    let f = fixture("artifact-symlink", b"[Event \"X\"]\n\n1. e4 *\n");
    let artifact = f.root.join(&f.occurrences);
    let outside_artifact = f.root.with_extension("outside-occurrence");
    let original = fs::read(&artifact).unwrap();
    fs::write(&outside_artifact, &original).unwrap();
    fs::remove_file(&artifact).unwrap();
    symlink(&outside_artifact, &artifact).unwrap();
    assert!(verify(&f.root, &f.occurrences).is_err());
    assert_eq!(fs::read(&outside_artifact).unwrap(), original);
    fs::remove_file(outside_artifact).unwrap();

    let root = root("publication-directory-symlink");
    fs::create_dir_all(&root).unwrap();
    let source_path = root.join("source.pgn");
    fs::write(&source_path, b"[Event \"X\"]\n\n1. e4 *\n").unwrap();
    let catalog = catalog_pipeline(&root, &source_path);
    let outside = root.with_extension("outside-directory");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, root.join("inspector/occurrences")).unwrap();
    assert!(create(&root, &catalog).is_err());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir(outside).unwrap();
}

#[test]
fn unknown_fields_noncanonical_json_and_upstream_corruption_are_rejected() {
    let f = fixture("canonical-rejection", b"[Event \"X\"]\n\n1. e4 *\n");
    let path = f.root.join(&f.occurrences);
    let canonical = crate::inspector::common::logical_json_bytes(&path, "test").unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unknown".into(), true.into());
    crate::inspector::common::replace_logical_json(
        &path,
        &serde_json::to_vec_pretty(&value).unwrap(),
    );
    assert!(verify(&f.root, &f.occurrences).is_err());
    crate::inspector::common::replace_logical_json(&path, &canonical[..canonical.len() - 1]);
    assert!(verify(&f.root, &f.occurrences).is_err());

    let fresh = fixture("upstream-corruption", b"[Event \"X\"]\n\n1. e4 *\n");
    let occurrence_manifest = manifest(&fresh);
    for upstream in [
        occurrence_manifest.catalog_manifest,
        occurrence_manifest.interpretation_manifest,
        occurrence_manifest.parse_manifest,
        occurrence_manifest.tokenization_manifest,
        occurrence_manifest.segmentation_manifest,
        occurrence_manifest.source_manifest,
    ] {
        let target = fresh.root.join(upstream);
        let original = fs::read(&target).unwrap();
        fs::write(&target, b"corrupt").unwrap();
        assert!(verify(&fresh.root, &fresh.occurrences).is_err());
        fs::write(&target, original).unwrap();
    }
    let loaded = load_verified(&fresh.root, &fresh.occurrences).unwrap();
    let object = loaded
        .catalog
        .interpretation
        .parse
        .tokenization
        .segmentation
        .object_path
        .clone();
    let original = fs::read(&object).unwrap();
    fs::write(&object, b"corrupt SourceBlob").unwrap();
    assert!(verify(&fresh.root, &fresh.occurrences).is_err());
    fs::write(object, original).unwrap();
}
