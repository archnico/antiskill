use super::{
    ingest, load_verified_collection, manifest::*, materialized_v2_loads, publish,
    reset_materialized_v2_loads, scan, validation, FileStatus, WorkspaceDiagnosis,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "antiskill-collection-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn scan_is_recursive_case_insensitive_and_deterministic() {
    let input = root("scan");
    fs::create_dir(input.join("z")).unwrap();
    fs::write(input.join("z/b.PGN"), b"b").unwrap();
    fs::write(input.join("a.pgn"), b"a").unwrap();
    fs::write(input.join("ignored.txt"), b"x").unwrap();
    let first = scan::directory(&input).unwrap();
    let second = scan::directory(&input).unwrap();
    assert_eq!(
        first
            .candidates
            .iter()
            .map(|c| c.relative.as_str())
            .collect::<Vec<_>>(),
        vec!["a.pgn", "z/b.PGN"]
    );
    assert_eq!(
        first
            .candidates
            .iter()
            .map(|c| &c.relative)
            .collect::<Vec<_>>(),
        second
            .candidates
            .iter()
            .map(|c| &c.relative)
            .collect::<Vec<_>>()
    );
    assert_eq!(first.scanned_files, 3);
    fs::remove_dir_all(input).unwrap();
}

#[test]
fn cache_reuse_prefers_the_compressed_canonical_copy() {
    assert!(ingest::preferred_cache_path(
        "inspector/catalogs/a.json.xz",
        "inspector/catalogs/a.json"
    ));
    assert!(!ingest::preferred_cache_path(
        "inspector/catalogs/a.json",
        "inspector/catalogs/a.json.xz"
    ));
}

#[test]
fn empty_directory_has_no_candidates() {
    let input = root("empty");
    let result = scan::directory(&input).unwrap();
    assert!(result.candidates.is_empty());
    fs::remove_dir_all(input).unwrap();
}

#[cfg(unix)]
#[test]
fn scan_does_not_follow_symlinks() {
    use std::os::unix::fs::symlink;
    let input = root("links");
    let outside = root("outside");
    fs::write(outside.join("x.pgn"), b"x").unwrap();
    symlink(outside.join("x.pgn"), input.join("file.pgn")).unwrap();
    symlink(&outside, input.join("dir")).unwrap();
    let result = scan::directory(&input).unwrap();
    assert!(result.candidates.is_empty());
    assert_eq!(result.skipped_symlinks, 2);
    fs::remove_dir_all(input).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

fn observation(path: &str) -> SourceObservationRecordV1 {
    SourceObservationRecordV1 {
        canonical_path: format!("/tmp/{path}"),
        input_relative_path: path.into(),
    }
}
fn source(observations: Vec<SourceObservationRecordV1>) -> CollectionSourceRecordV1 {
    let hash = "a".repeat(64);
    CollectionSourceRecordV1 {
        id: 0,
        source_blake3: hash.clone(),
        source_bytes: 1,
        catalog: format!("inspector/catalogs/{hash}.json"),
        catalog_blake3: hash,
        catalog_bytes: 1,
        games: 0,
        observations,
    }
}

#[test]
fn validation_rejects_duplicate_and_unsorted_observations() {
    for observations in [
        vec![observation("a.pgn"), observation("a.pgn")],
        vec![observation("b.pgn"), observation("a.pgn")],
    ] {
        let manifest = CollectionManifestV1 {
            schema_version: 1,
            collection_profile: COLLECTION_PROFILE.into(),
            sources: vec![source(observations)],
        };
        assert!(validation::validate(&manifest).is_err());
    }
}

#[test]
fn validation_accepts_missing_observed_paths() {
    let manifest = CollectionManifestV1 {
        schema_version: 1,
        collection_profile: COLLECTION_PROFILE.into(),
        sources: vec![source(vec![observation("missing.pgn")])],
    };
    validation::validate(&manifest).unwrap();
}

fn pgn(event: &str) -> Vec<u8> {
    format!("[Event \"{event}\"]\n[White \"Alice\"]\n[Black \"Bob\"]\n[Date \"2026.08.01\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n").into_bytes()
}

fn collection_v2(manifest: &CollectionManifest) -> &CollectionManifestV2 {
    let CollectionManifest::V2(manifest) = manifest else {
        panic!("ingest deve produrre Collection v2");
    };
    manifest
}

fn owned_v2(manifest: &CollectionManifest) -> CollectionManifestV2 {
    collection_v2(manifest).clone()
}

fn source_manifest_count(workspace: &Path) -> usize {
    fs::read_dir(workspace.join("inspector/manifests"))
        .unwrap()
        .count()
}

#[test]
fn previous_first_occurrence_then_current_duplicate_and_unobserved_source() {
    let base = root("previous");
    let workspace = base.join("workspace");
    let first = base.join("first");
    let second = base.join("second");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(second.join("nested")).unwrap();
    fs::write(first.join("kept.pgn"), pgn("Kept")).unwrap();
    fs::write(first.join("unobserved.pgn"), pgn("Unobserved")).unwrap();
    let initial = ingest::run(&workspace, &first, None).unwrap();
    let previous = initial.created.unwrap().path;
    let manifests_before_reuse = source_manifest_count(&workspace);
    fs::write(second.join("current.pgn"), pgn("Kept")).unwrap();
    fs::write(second.join("nested/copy.PGN"), pgn("Kept")).unwrap();
    let merged = ingest::run(&workspace, &second, Some(&previous)).unwrap();
    assert!(matches!(merged.files[0].status, FileStatus::ReusedPrevious));
    assert!(matches!(
        merged.files[1].status,
        FileStatus::DuplicateSource
    ));
    let manifest = collection_v2(&merged.created.as_ref().unwrap().manifest);
    assert_eq!(manifest.sources.len(), 2);
    let kept = manifest
        .sources
        .iter()
        .find(|source| {
            source
                .observations
                .iter()
                .any(|o| o.input_relative_path == "current.pgn")
        })
        .unwrap();
    assert_eq!(kept.observations.len(), 3);
    assert!(manifest.sources.iter().any(|source| source
        .observations
        .iter()
        .any(|o| o.input_relative_path == "unobserved.pgn")));
    assert_eq!(source_manifest_count(&workspace), manifests_before_reuse);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn empty_input_publishes_empty_collection_but_all_failed_does_not() {
    let base = root("zero");
    let workspace = base.join("workspace");
    let empty = base.join("empty");
    let failed = base.join("failed");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&empty).unwrap();
    fs::create_dir_all(&failed).unwrap();
    let empty_result = ingest::run(&workspace, &empty, None).unwrap();
    assert!(empty_result.created.is_some());
    assert_eq!(empty_result.failed, 0);
    fs::write(failed.join("empty.pgn"), b"").unwrap();
    let failed_result = ingest::run(&workspace, &failed, None).unwrap();
    assert!(failed_result.created.is_none());
    assert_eq!(failed_result.failed, 1);
    assert!(!failed_result.files[0]
        .error_message
        .as_deref()
        .unwrap()
        .is_empty());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn valid_and_failed_publish_partial_collection() {
    let base = root("partial");
    let workspace = base.join("workspace");
    let input = base.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("good.pgn"), pgn("Good")).unwrap();
    fs::write(input.join("bad.pgn"), b"").unwrap();
    let result = ingest::run(&workspace, &input, None).unwrap();
    assert!(result.created.is_some());
    assert_eq!(result.failed, 1);
    assert_eq!(result.created.unwrap().manifest.source_count(), 1);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn orphan_catalog_is_reused_and_invalid_json_is_ignored() {
    let base = root("orphan");
    let workspace = base.join("workspace");
    let input = base.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("game.pgn"), pgn("Reuse")).unwrap();
    let first = ingest::run(&workspace, &input, None).unwrap();
    assert!(matches!(first.files[0].status, FileStatus::Ingested));
    let first_created = first.created.unwrap();
    let first_manifest = owned_v2(&first_created.manifest);
    let manifests_before_reuse = source_manifest_count(&workspace);
    fs::write(
        workspace.join("inspector/catalogs/not-a-catalog.json"),
        b"broken",
    )
    .unwrap();
    let second = ingest::run(&workspace, &input, None).unwrap();
    assert!(matches!(
        second.files[0].status,
        FileStatus::ReusedWorkspace
    ));
    let second_created = second.created.unwrap();
    assert!(!second_created.new_manifest);
    assert_eq!(first_created.path, second_created.path);
    assert_eq!(first_manifest, owned_v2(&second_created.manifest));
    assert_eq!(source_manifest_count(&workspace), manifests_before_reuse);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn ingest_persists_and_verifies_the_complete_v2_chain_without_touching_sources() {
    let base = root("v2-chain");
    let workspace = base.join("workspace");
    let input = base.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    let first = pgn("First");
    let second = b"[Event \"Non comparable\"]\n[Result \"*\"]\n\n1. c4 BadSan *\n".to_vec();
    fs::write(input.join("one.pgn"), &first).unwrap();
    fs::write(input.join("two.pgn"), &second).unwrap();

    let result = ingest::run(&workspace, &input, None).unwrap();
    let created = result.created.unwrap();
    let manifest = collection_v2(&created.manifest);
    assert_eq!(manifest.schema_version, COLLECTION_SCHEMA_VERSION_V2);
    assert_eq!(manifest.collection_profile, COLLECTION_PROFILE_V2);
    assert_eq!(manifest.sources.len(), 2);
    for source in &manifest.sources {
        assert!(source.catalog.starts_with("inspector/catalogs/"));
        assert!(source.occurrences.starts_with("inspector/occurrences/"));
        assert!(source.mainlines.starts_with("inspector/mainlines/"));
    }
    load_verified_collection(&workspace, &created.path).unwrap();
    let projected =
        super::load_verified_mainlines_for_dedup_from_collection(&workspace, &created.path)
            .unwrap();
    assert_eq!(projected.len(), 2);
    assert!(projected
        .iter()
        .flat_map(|artifact| &artifact.records)
        .any(|record| !record.comparable));
    assert_eq!(fs::read(input.join("one.pgn")).unwrap(), first);
    assert_eq!(fs::read(input.join("two.pgn")).unwrap(), second);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn product_ingest_update_doctor_and_active_resolution_do_not_materialize_v2_games() {
    let base = root("v2-bounded-finalization");
    let workspace = base.join("workspace");
    let input = base.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(
        input.join("games.pgn"),
        b"[Event \"A\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n[Event \"B\"]\n[Result \"0-1\"]\n\n1. d4 d5 0-1\n",
    )
    .unwrap();

    reset_materialized_v2_loads();
    let created = ingest::run_nonempty_snapshot(&workspace, &input)
        .unwrap()
        .created
        .unwrap();
    assert_eq!(materialized_v2_loads(), 0);
    assert_eq!(
        super::diagnose_workspace(&workspace).unwrap(),
        WorkspaceDiagnosis::Active {
            sources: 1,
            games: 2,
        }
    );
    assert_eq!(materialized_v2_loads(), 0);
    assert_eq!(
        super::resolve_active_collection(&workspace).unwrap(),
        created.path
    );
    assert_eq!(materialized_v2_loads(), 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn active_v2_bounded_validation_rejects_each_corrupt_game_stream() {
    let base = root("v2-bounded-corruption");
    let workspace = base.join("workspace");
    let input = base.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("one.pgn"), pgn("One")).unwrap();
    let created = ingest::run_nonempty_snapshot(&workspace, &input)
        .unwrap()
        .created
        .unwrap();
    let record = collection_v2(&created.manifest).sources[0].clone();

    for relative in [record.catalog, record.occurrences, record.mainlines] {
        let path = workspace.join(relative);
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"corrupt active dependency").unwrap();
        assert!(super::diagnose_workspace(&workspace).is_err());
        assert!(super::resolve_active_collection(&workspace).is_err());
        fs::write(path, original).unwrap();
    }
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn v2_verification_fails_closed_for_missing_and_altered_artifacts() {
    let base = root("v2-missing");
    let workspace = base.join("workspace");
    let input = base.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("one.pgn"), pgn("One")).unwrap();
    let created = ingest::run(&workspace, &input, None)
        .unwrap()
        .created
        .unwrap();
    let record = collection_v2(&created.manifest).sources[0].clone();

    for relative in [&record.occurrences, &record.mainlines] {
        let path = workspace.join(relative);
        let original = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(load_verified_collection(&workspace, &created.path).is_err());
        fs::write(&path, original).unwrap();
    }

    let mainlines = workspace.join(&record.mainlines);
    let original = fs::read(&mainlines).unwrap();
    fs::write(&mainlines, b"altered").unwrap();
    assert!(load_verified_collection(&workspace, &created.path).is_err());
    fs::write(&mainlines, original).unwrap();
    load_verified_collection(&workspace, &created.path).unwrap();
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn v2_rejects_incoherent_identities_and_catalog_from_another_source() {
    let base = root("v2-cross-source");
    let workspace = base.join("workspace");
    let input = base.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("one.pgn"), pgn("One")).unwrap();
    fs::write(input.join("two.pgn"), pgn("Two")).unwrap();
    let created = ingest::run(&workspace, &input, None)
        .unwrap()
        .created
        .unwrap();
    let valid = owned_v2(&created.manifest);

    let mut wrong_bytes = valid.clone();
    wrong_bytes.sources[0].occurrences_bytes += 1;
    let wrong_bytes = publish(&workspace, CollectionManifest::V2(wrong_bytes)).unwrap();
    assert!(load_verified_collection(&workspace, &wrong_bytes.path).is_err());

    for field in ["catalog", "occurrences", "mainlines"] {
        let mut mismatch = valid.clone();
        match field {
            "catalog" => mismatch.sources[0].catalog_blake3 = "0".repeat(64),
            "occurrences" => mismatch.sources[0].occurrences_blake3 = "0".repeat(64),
            "mainlines" => mismatch.sources[0].mainlines_blake3 = "0".repeat(64),
            _ => unreachable!(),
        }
        assert!(validation::validate_v2(&mismatch).is_err(), "field {field}");
    }

    let mut cross_source = valid;
    let other = cross_source.sources[1].clone();
    cross_source.sources[0].catalog = other.catalog;
    cross_source.sources[0].catalog_blake3 = other.catalog_blake3;
    cross_source.sources[0].catalog_bytes = other.catalog_bytes;
    let cross_source = publish(&workspace, CollectionManifest::V2(cross_source)).unwrap();
    assert!(load_verified_collection(&workspace, &cross_source.path).is_err());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn ingest_order_changes_only_observations_not_logical_artifact_identity() {
    let base = root("v2-order");
    let workspace = base.join("workspace");
    let first_input = base.join("first");
    let second_input = base.join("second");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&first_input).unwrap();
    fs::create_dir_all(&second_input).unwrap();
    fs::write(first_input.join("a.pgn"), pgn("One")).unwrap();
    fs::write(first_input.join("z.pgn"), pgn("Two")).unwrap();
    fs::write(second_input.join("a.pgn"), pgn("Two")).unwrap();
    fs::write(second_input.join("z.pgn"), pgn("One")).unwrap();
    let first = ingest::run(&workspace, &first_input, None).unwrap();
    let second = ingest::run(&workspace, &second_input, None).unwrap();
    let logical = |manifest: &CollectionManifest| {
        collection_v2(manifest)
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_blake3.clone(),
                    source.source_bytes,
                    source.catalog_blake3.clone(),
                    source.occurrences_blake3.clone(),
                    source.mainlines_blake3.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        logical(&first.created.unwrap().manifest),
        logical(&second.created.unwrap().manifest)
    );
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn collection_v1_still_loads_and_verifies() {
    let base = root("v1-compatible");
    let workspace = base.join("workspace");
    let input = base.join("input");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("one.pgn"), pgn("One")).unwrap();
    let v2 = ingest::run(&workspace, &input, None).unwrap();
    let source = collection_v2(&v2.created.as_ref().unwrap().manifest).sources[0].clone();
    let v1 = CollectionManifestV1 {
        schema_version: COLLECTION_SCHEMA_VERSION,
        collection_profile: COLLECTION_PROFILE.into(),
        sources: vec![CollectionSourceRecordV1 {
            id: source.id,
            source_blake3: source.source_blake3,
            source_bytes: source.source_bytes,
            catalog: source.catalog,
            catalog_blake3: source.catalog_blake3,
            catalog_bytes: source.catalog_bytes,
            games: source.games,
            observations: source.observations,
        }],
    };
    let created = publish(&workspace, CollectionManifest::V1(v1)).unwrap();
    let loaded = load_verified_collection(&workspace, &created.path).unwrap();
    assert!(matches!(loaded.manifest, CollectionManifest::V1(_)));
    let query = super::query_collection_result(
        &workspace,
        &created.path,
        &crate::inspector::catalog::QueryFilters::default(),
    )
    .unwrap();
    assert!(!query.occurrence_ids_available);
    assert_eq!(query.matches.len(), 1);
    assert!(query.matches[0].occurrence_id.is_none());
    assert!(
        super::resolve_occurrence(&workspace, &created.path, &"a".repeat(64))
            .unwrap_err()
            .contains("legacy v1")
    );
    assert!(
        super::load_verified_mainlines_for_dedup_from_collection(&workspace, &created.path)
            .unwrap_err()
            .contains("Collection v1")
    );
    fs::remove_dir_all(base).unwrap();
}

#[cfg(unix)]
#[test]
fn non_utf8_foreign_file_is_ignored_and_non_utf8_pgn_is_isolated() {
    use std::os::unix::ffi::OsStringExt;
    let input = root("nonutf8");
    fs::write(
        input.join(std::ffi::OsString::from_vec(b"foreign-\xff.txt".to_vec())),
        b"x",
    )
    .unwrap();
    fs::write(
        input.join(std::ffi::OsString::from_vec(b"game-\xff.pGn".to_vec())),
        b"x",
    )
    .unwrap();
    fs::write(input.join("valid.pgn"), pgn("Valid")).unwrap();
    let result = scan::directory(&input).unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].code, "non_utf8_path");
    assert!(result.failures[0].relative.starts_with("hex:"));
    assert!(!result.failures[0].relative.contains('\u{fffd}'));
    fs::remove_dir_all(input).unwrap();
}

#[test]
fn validation_matrix_names_each_persistent_invariant() {
    let valid = CollectionManifestV1 {
        schema_version: 1,
        collection_profile: COLLECTION_PROFILE.into(),
        sources: vec![source(vec![observation("a.pgn")])],
    };
    let mut cases: Vec<(&str, CollectionManifestV1)> = Vec::new();
    let mut v = valid.clone();
    v.collection_profile = "wrong".into();
    cases.push(("profile", v));
    let mut v = valid.clone();
    v.sources[0].id = 1;
    cases.push(("source_id", v));
    let mut v = valid.clone();
    v.sources[0].observations.clear();
    cases.push(("observations_empty", v));
    let mut v = valid.clone();
    v.sources[0].catalog = "../catalog.json".into();
    cases.push(("catalog_path", v));
    let mut v = valid.clone();
    v.sources[0].catalog_blake3 = "b".repeat(64);
    cases.push(("catalog_identity_path", v));
    let mut v = valid.clone();
    v.sources[0].source_blake3 = "invalid".into();
    cases.push(("source_hash", v));
    let mut v = valid.clone();
    v.sources[0].source_bytes = 0;
    cases.push(("source_bytes", v));
    let mut v = valid.clone();
    v.sources[0].catalog_bytes = 0;
    cases.push(("catalog_bytes", v));
    let mut v = valid.clone();
    v.sources[0].observations.push(observation("a.pgn"));
    cases.push(("observation_duplicate", v));
    let mut v = valid.clone();
    v.sources[0].observations = vec![observation("z.pgn"), observation("a.pgn")];
    cases.push(("observation_order", v));
    let mut v = valid.clone();
    v.sources.push(v.sources[0].clone());
    v.sources[1].id = 1;
    cases.push(("source_duplicate", v));
    let mut v = valid.clone();
    let mut second = source(vec![observation("b.pgn")]);
    second.source_blake3 = "0".repeat(64);
    second.catalog_blake3 = "0".repeat(64);
    second.catalog = format!("inspector/catalogs/{}.json", second.catalog_blake3);
    second.id = 1;
    v.sources.push(second);
    cases.push(("source_order", v));
    for (name, manifest) in cases {
        assert!(validation::validate(&manifest).is_err(), "case {name}");
    }
}

#[test]
fn v2_collection_verification_uses_one_fused_semantic_replay() {
    let source = include_str!("mod.rs");
    let body = source
        .split("fn verify_v2_source(")
        .nth(1)
        .unwrap()
        .split("pub(crate) use compare::SnapshotComparison")
        .next()
        .unwrap();
    assert!(!body.contains("catalog.for_each_verified_catalog_game"));
    assert!(!body.contains("occurrences.for_each_verified_occurrence"));
    assert!(body.contains("loaded_mainlines.for_each_verified_mainline"));
}
