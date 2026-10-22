mod analyses;
mod analysis_requirements;
mod annotation_index;
mod annotations;
mod archive_view;
mod assertion_index;
mod assertion_reconciliation;
mod catalog;
mod chess_rules;
mod collection;
mod common;
mod interpretation;
mod lichess_evals;
mod mainlines;
mod occurrences;
mod parsing;
mod player_identity;
mod segmentation;
mod semantic_probe;
mod source;
mod source_assertions;
mod source_index;
mod tokenization;
mod unique_games;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedArtifactIdentityV1 {
    pub(crate) blake3: String,
    pub(crate) bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedMainlineProvenanceInputV1 {
    pub(crate) occurrences_manifest: VerifiedArtifactIdentityV1,
    pub(crate) interpretation_manifest: VerifiedArtifactIdentityV1,
    pub(crate) source_manifest: VerifiedArtifactIdentityV1,
    pub(crate) segmentation_manifest: VerifiedArtifactIdentityV1,
    pub(crate) tokenization_manifest: VerifiedArtifactIdentityV1,
    pub(crate) parse_manifest: VerifiedArtifactIdentityV1,
    pub(crate) catalog_manifest: VerifiedArtifactIdentityV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedMainlineInputV1 {
    pub(crate) occurrence_id: String,
    pub(crate) canonical_mainline_id: Option<String>,
    pub(crate) comparable: bool,
    pub(crate) noncomparability_reasons: Vec<String>,
    pub(crate) source_blake3: String,
    pub(crate) source_bytes: u64,
    pub(crate) source_game_blake3: String,
    pub(crate) game_bytes: u64,
    pub(crate) results: Vec<String>,
    pub(crate) provenance: VerifiedMainlineProvenanceInputV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedMainlineArtifactInputV1 {
    pub(crate) artifact: VerifiedArtifactIdentityV1,
    pub(crate) records: Vec<VerifiedMainlineInputV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductLogicalGameRowV1 {
    pub(crate) occurrence_id: String,
    pub(crate) canonical_mainline_id: Option<String>,
    pub(crate) comparable: bool,
    pub(crate) representative_quality: u8,
}

fn metadata_value_is_informative(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.chars().any(char::is_alphanumeric)
}

pub(crate) fn product_representative_quality(
    white: &[String],
    black: &[String],
    date: &[String],
    event: &[String],
    result: &[String],
) -> u8 {
    let present = |values: &[String]| {
        values
            .iter()
            .any(|value| metadata_value_is_informative(value))
    };
    let mut score = 0_u8;
    if present(white) {
        score += 8;
    }
    if present(black) {
        score += 8;
    }
    if present(date) {
        score += 4;
    }
    if present(event) {
        score += 2;
    }
    if present(result) {
        score += 1;
    }
    score
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PlayerSideV1 {
    White,
    Black,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductPlayerSideEvidenceV1 {
    pub(crate) occurrence_id: String,
    pub(crate) side: PlayerSideV1,
    pub(crate) names: Vec<String>,
    pub(crate) fide_id_values: Vec<String>,
    pub(crate) source_blake3: String,
    pub(crate) source_bytes: u64,
    pub(crate) source_start: u64,
    pub(crate) source_end: u64,
    pub(crate) ordinal: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductContextOccurrenceV1 {
    pub(crate) occurrence_id: String,
    pub(crate) white_names: Vec<String>,
    pub(crate) black_names: Vec<String>,
    pub(crate) white_fide_id_values: Vec<String>,
    pub(crate) black_fide_id_values: Vec<String>,
    pub(crate) event: Vec<String>,
    pub(crate) date: Vec<String>,
    pub(crate) result: Vec<String>,
    pub(crate) round: Vec<String>,
    pub(crate) source_blake3: String,
    pub(crate) source_bytes: u64,
    pub(crate) source_start: u64,
    pub(crate) source_end: u64,
    pub(crate) ordinal: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedPlayerSideScanV1 {
    pub(crate) collection_blake3: String,
    pub(crate) collection_bytes: u64,
    pub(crate) source_occurrences: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedLogicalGameScanV1 {
    pub(crate) collection_blake3: String,
    pub(crate) collection_bytes: u64,
    pub(crate) rows: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ProductDuplicateRowV1 {
    pub(crate) occurrence_id: String,
    pub(crate) source_game_blake3: String,
    pub(crate) game_bytes: u64,
    pub(crate) canonical_mainline_id: Option<String>,
    pub(crate) comparable: bool,
    pub(crate) mainline_results: Vec<String>,
    pub(crate) date: Vec<String>,
    pub(crate) white: Vec<String>,
    pub(crate) black: Vec<String>,
    pub(crate) result: Vec<String>,
    pub(crate) event: Vec<String>,
}

pub(crate) fn for_each_verified_duplicate_row<F>(
    root: &Path,
    collection: &Path,
    on_row: F,
) -> Result<u64, String>
where
    F: FnMut(ProductDuplicateRowV1) -> Result<(), String>,
{
    collection::for_each_verified_duplicate_row(root, collection, on_row)
}

pub(crate) fn for_each_verified_logical_game_row<F>(
    root: &Path,
    collection: &Path,
    on_row: F,
) -> Result<VerifiedLogicalGameScanV1, String>
where
    F: FnMut(ProductLogicalGameRowV1) -> Result<(), String>,
{
    collection::for_each_verified_logical_game_row(root, collection, on_row)
}

pub(crate) use player_identity::{
    FederationBridgeInputV2, FederationBridgeObservationV2, OfficialContextInputV3,
    PlayerIdentityViewV1, PlayerIdentityViewV2, PlayerIdentityViewV3,
};

pub(crate) fn query_player_identity(
    root: &Path,
    collection: &Path,
    fide_id: &str,
    include_candidates: bool,
) -> Result<PlayerIdentityViewV1, String> {
    player_identity::query(root, collection, fide_id, include_candidates)
}

pub(crate) fn query_player_identity_v3(
    root: &Path,
    collection: &Path,
    fide_id: &str,
    bridge: Option<FederationBridgeInputV2>,
    contexts: Vec<OfficialContextInputV3>,
) -> Result<PlayerIdentityViewV3, String> {
    player_identity::query_with_official_context(root, collection, fide_id, bridge, contexts)
}

pub(crate) fn query_player_identity_v2(
    root: &Path,
    collection: &Path,
    fide_id: &str,
    bridge: FederationBridgeInputV2,
) -> Result<PlayerIdentityViewV2, String> {
    player_identity::query_with_federation_bridge(root, collection, fide_id, bridge)
}

pub(crate) use unique_games::UniqueGameViewSummaryV1;

pub(crate) fn create_unique_game_view(
    root: &Path,
    collection: &Path,
) -> Result<UniqueGameViewSummaryV1, String> {
    unique_games::create(root, collection)
}

pub(crate) fn write_unique_game_records(
    root: &Path,
    collection: &Path,
    writer: &mut dyn Write,
) -> Result<(), String> {
    unique_games::write_records(root, collection, writer)
}

pub(crate) fn load_verified_mainlines_for_dedup(
    root: &Path,
    path: &Path,
) -> Result<VerifiedMainlineArtifactInputV1, String> {
    mainlines::load_verified_for_dedup(root, path)
}

pub(crate) fn load_verified_mainlines_for_dedup_from_collection(
    root: &Path,
    path: &Path,
) -> Result<Vec<VerifiedMainlineArtifactInputV1>, String> {
    collection::load_verified_mainlines_for_dedup_from_collection(root, path)
}

#[cfg(test)]
pub(crate) use collection::CollectionQueryMatch;
pub(crate) use collection::CollectionQueryResult;

pub(crate) fn query_active_collection<I>(
    root: &Path,
    args: I,
) -> Result<CollectionQueryResult, String>
where
    I: IntoIterator<Item = String>,
{
    let filters = catalog::parse_query_filters(args)?;
    let collection = collection::resolve_active_collection_for_verified_scan(root)?;
    collection::query_collection_result(root, &collection, &filters)
}

pub(crate) fn resolve_active_collection_path(root: &Path) -> Result<PathBuf, String> {
    collection::resolve_active_collection(root)
}

#[cfg(test)]
pub(crate) use collection::{ProductExactDuplicateGroup, ProductExactDuplicateReport};

pub(crate) struct ResolvedPublicGame {
    pub(crate) occurrence_id: String,
    pub(crate) bytes: Vec<u8>,
}

pub(crate) fn resolve_active_public_games(
    root: &Path,
    occurrence_ids: &std::collections::BTreeSet<String>,
) -> Result<Vec<ResolvedPublicGame>, String> {
    let collection = collection::resolve_active_collection(root)?;
    collection::resolve_occurrences_bulk(root, &collection, occurrence_ids).map(|games| {
        games
            .into_iter()
            .map(|game| ResolvedPublicGame {
                occurrence_id: game.occurrence_id,
                bytes: game.bytes,
            })
            .collect()
    })
}

pub(crate) fn resolve_active_public_game(
    root: &Path,
    occurrence_id: &str,
) -> Result<ResolvedPublicGame, String> {
    let collection = collection::resolve_active_collection_for_verified_scan(root)?;
    let resolved = collection::resolve_occurrence(root, &collection, occurrence_id)?;
    Ok(ResolvedPublicGame {
        occurrence_id: resolved.occurrence_id,
        bytes: resolved.bytes,
    })
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct IngestSnapshotFailure {
    pub(crate) file: String,
    pub(crate) cause: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct IngestSnapshotResult {
    pub(crate) pgn_files: u64,
    pub(crate) sources: u64,
    pub(crate) games: u64,
    pub(crate) reused_sources: u64,
    pub(crate) failures: Vec<IngestSnapshotFailure>,
}

pub(crate) fn ingest_snapshot(root: &Path, input: &Path) -> Result<IngestSnapshotResult, String> {
    let ingested = collection::ingest_nonempty_snapshot(root, input)?;
    let sources = u64::try_from(
        ingested
            .created
            .as_ref()
            .map_or(0, |created| created.manifest.source_count()),
    )
    .map_err(|_| "Numero sorgenti oltre u64")?;
    let games = ingested
        .created
        .as_ref()
        .map_or(0, |created| created.manifest.games());
    let reused_sources = u64::try_from(
        ingested
            .files
            .iter()
            .filter(|file| matches!(file.status, collection::FileStatus::ReusedWorkspace))
            .count(),
    )
    .map_err(|_| "Numero sorgenti riusate oltre u64")?;
    let mut failures = Vec::new();
    for file in ingested.files {
        if matches!(file.status, collection::FileStatus::Failed) {
            failures.push(IngestSnapshotFailure {
                file: file.relative,
                cause: file
                    .error_message
                    .ok_or("Failure ingest senza messaggio diagnostico")?,
            });
        }
    }
    if failures.len() as u64 != ingested.failed {
        return Err("Conteggio failure ingest incoerente".into());
    }
    Ok(IngestSnapshotResult {
        pgn_files: ingested.pgn_files,
        sources,
        games,
        reused_sources,
        failures,
    })
}

pub(crate) use collection::SnapshotComparison;
pub(crate) use collection::WorkspaceDiagnosis;

pub(crate) fn diagnose_workspace(root: &Path) -> Result<WorkspaceDiagnosis, String> {
    collection::diagnose_workspace(root)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AnnotationCoverageV1 {
    pub(crate) groups: u64,
    pub(crate) items: u64,
}

pub(crate) fn annotation_coverage(root: &Path) -> Result<AnnotationCoverageV1, String> {
    let coverage = annotations::coverage(root)?;
    Ok(AnnotationCoverageV1 {
        groups: coverage.groups,
        items: coverage.items,
    })
}

pub(crate) fn compare_active_snapshot(
    root: &Path,
    input: &Path,
) -> Result<SnapshotComparison, String> {
    collection::compare_active_snapshot(root, input)
}

pub(crate) fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let result = match args.next().as_deref() {
        None | Some("help" | "--help" | "-h") => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Some("import") => parse_import_args(args).and_then(|(root, source_path)| {
            let imported = source::import_source(&root, &source_path)?;
            println!("BLAKE3\t{}", imported.manifest.blake3);
            println!("BYTES\t{}", imported.manifest.bytes);
            println!("NEW_OBJECT\t{}", imported.manifest.new_object);
            println!("BYTES_ADDED\t{}", imported.manifest.bytes_added);
            println!(
                "MANIFEST\t{}",
                common::path_to_manifest_string(&imported.manifest_path)
            );
            Ok(())
        }),
        Some("export-source") => parse_export_args(args).and_then(|(root, manifest, output)| {
            source::export_source(&root, &manifest, &output)?;
            println!("OUTPUT\t{}", output.display());
            Ok(())
        }),
        Some("segment") => parse_path_args(args, "--manifest").and_then(|(root, manifest)| {
            let segmented = segmentation::segment_source(&root, &manifest)?;
            let games = segmented.game_candidates;
            let unclassified = segmented.segments - games;
            println!("SOURCE_BLAKE3\t{}", segmented.manifest.source_blake3);
            println!("SOURCE_BYTES\t{}", segmented.manifest.source_bytes);
            println!("SEGMENTS\t{}", segmented.segments);
            println!("GAME_CANDIDATES\t{games}");
            println!("UNCLASSIFIED_SEGMENTS\t{unclassified}");
            println!(
                "SEGMENTATION\t{}",
                common::path_to_manifest_string(&segmented.path)
            );
            Ok(())
        }),
        Some("verify-segmentation") => {
            parse_path_args(args, "--segmentation").and_then(|(root, segmentation_path)| {
                segmentation::verify_segmentation(&root, &segmentation_path)?;
                println!("SEGMENTATION_VERIFY\tPASS");
                Ok(())
            })
        }
        Some("semantic-probe") => {
            parse_path_args(args, "--segmentation").and_then(|(root, segmentation_path)| {
                chess_rules::probe_apply_profile_start();
                let run_result = semantic_probe::run(&root, &segmentation_path);
                let apply_profile = chess_rules::probe_apply_profile_finish();
                let result = run_result?;
                let (direct_fast, direct_fallback) = semantic_probe::direct_identity_counts();
                println!("DIRECT_FAST_GAMES\t{direct_fast}");
                println!("DIRECT_FALLBACK_GAMES\t{direct_fallback}");
                println!("GAMES\t{}", result.games);
                println!("TOKENS\t{}", result.tokens);
                println!("NODES\t{}", result.nodes);
                println!("MOVES\t{}", result.moves);
                println!("COMPARABLE\t{}", result.comparable);
                println!("MAINLINE_RESULTS\t{}", result.mainline_results);
                println!("DIGEST\t{}", result.digest);
                println!("MAINLINE_DIGEST\t{}", result.mainline_digest);
                println!("TOTAL_MS\t{:.3}", result.total.as_secs_f64() * 1000.0);
                println!("TOKENIZE_MS\t{:.3}", result.tokenize.as_secs_f64() * 1000.0);
                println!("PARSE_MS\t{:.3}", result.parse.as_secs_f64() * 1000.0);
                println!(
                    "INTERPRET_MS\t{:.3}",
                    result.interpret.as_secs_f64() * 1000.0
                );
                println!(
                    "INTERPRET_CORE_MS\t{:.3}",
                    result.interpret_core.as_secs_f64() * 1000.0
                );
                println!(
                    "INTERPRET_CORE_VIEW_MS\t{:.3}",
                    result.interpret_core_view.as_secs_f64() * 1000.0
                );
                println!(
                    "INTERPRET_CORE_SETUP_MS\t{:.3}",
                    result.interpret_core_setup.as_secs_f64() * 1000.0
                );
                println!(
                    "INTERPRET_CORE_MOVETEXT_MS\t{:.3}",
                    result.interpret_core_movetext.as_secs_f64() * 1000.0
                );
                println!(
                    "INTERPRET_CORE_FINALIZE_MS\t{:.3}",
                    result.interpret_core_finalize.as_secs_f64() * 1000.0
                );
                println!("MOVE_COUNT_PROFILED\t{}", result.move_count_profiled);
                println!("MOVE_APPLY_ATTEMPTS\t{}", result.move_apply_attempts);
                println!(
                    "MOVE_TOTAL_MS\t{:.3}",
                    result.move_total.as_secs_f64() * 1000.0
                );
                println!(
                    "MOVE_RANGE_RAW_MS\t{:.3}",
                    result.move_range_raw.as_secs_f64() * 1000.0
                );
                println!(
                    "MOVE_POSITION_CLONE_MS\t{:.3}",
                    result.move_position_clone.as_secs_f64() * 1000.0
                );
                println!(
                    "MOVE_APPLY_SAN_MS\t{:.3}",
                    result.move_apply_san.as_secs_f64() * 1000.0
                );
                println!("APPLY_PROFILE_ATTEMPTS\t{}", apply_profile.attempts);
                println!(
                    "APPLY_PARSE_MS\t{:.3}",
                    apply_profile.parse.as_secs_f64() * 1000.0
                );
                println!(
                    "APPLY_RESOLVE_MS\t{:.3}",
                    apply_profile.resolve.as_secs_f64() * 1000.0
                );
                println!(
                    "APPLY_CANONICAL_MS\t{:.3}",
                    apply_profile.canonical.as_secs_f64() * 1000.0
                );
                println!(
                    "APPLY_NORMALIZE_MS\t{:.3}",
                    apply_profile.normalize.as_secs_f64() * 1000.0
                );
                println!(
                    "APPLY_PLAY_MS\t{:.3}",
                    apply_profile.play.as_secs_f64() * 1000.0
                );
                println!(
                    "APPLY_COMPONENTS_MS\t{:.3}",
                    apply_profile.components.as_secs_f64() * 1000.0
                );
                println!(
                    "APPLY_FINALIZE_MS\t{:.3}",
                    apply_profile.finalize.as_secs_f64() * 1000.0
                );
                println!(
                    "INTERPRET_CONVERT_MS\t{:.3}",
                    result.interpret_convert.as_secs_f64() * 1000.0
                );
                println!(
                    "INTERPRET_VALIDATE_MS\t{:.3}",
                    result.interpret_validate.as_secs_f64() * 1000.0
                );
                println!("CATALOG_MS\t{:.3}", result.catalog.as_secs_f64() * 1000.0);
                println!(
                    "OCCURRENCE_MS\t{:.3}",
                    result.occurrence.as_secs_f64() * 1000.0
                );
                println!("MAINLINE_MS\t{:.3}", result.mainline.as_secs_f64() * 1000.0);
                Ok(())
            })
        }
        Some("tokenize") => {
            parse_path_args(args, "--segmentation").and_then(|(root, segmentation_path)| {
                let tokenized = tokenization::tokenize(&root, &segmentation_path)?;
                println!("SOURCE_BLAKE3\t{}", tokenized.source_blake3);
                println!("SOURCE_BYTES\t{}", tokenized.source_bytes);
                println!("GAME_CANDIDATES\t{}", tokenized.game_candidates);
                println!("TOKEN_STREAMS\t{}", tokenized.token_streams);
                println!("TOKENS\t{}", tokenized.tokens);
                println!("UNKNOWN_TOKENS\t{}", tokenized.unknown_tokens);
                println!("UNTERMINATED_TOKENS\t{}", tokenized.unterminated_tokens);
                println!(
                    "TOKENIZATION\t{}",
                    common::path_to_manifest_string(&tokenized.path)
                );
                Ok(())
            })
        }
        Some("verify-tokenization") => {
            parse_path_args(args, "--tokenization").and_then(|(root, tokenization_path)| {
                tokenization::verify_tokenization(&root, &tokenization_path)?;
                println!("TOKENIZATION_VERIFY\tPASS");
                Ok(())
            })
        }
        Some("parse") => {
            parse_path_args(args, "--tokenization").and_then(|(root, tokenization_path)| {
                let parsed = parsing::parse(&root, &tokenization_path)?;
                println!("SOURCE_BLAKE3\t{}", parsed.source_blake3);
                println!("SOURCE_BYTES\t{}", parsed.source_bytes);
                println!("GAME_CANDIDATES\t{}", parsed.game_candidates);
                println!("PARSE_TREES\t{}", parsed.parse_trees);
                println!("NODES\t{}", parsed.nodes);
                println!("VALID_NODES\t{}", parsed.valid_nodes);
                println!("RECOVERED_NODES\t{}", parsed.recovered_nodes);
                println!("INCOMPLETE_NODES\t{}", parsed.incomplete_nodes);
                println!("UNPARSED_NODES\t{}", parsed.unparsed_nodes);
                println!("DIAGNOSTICS\t{}", parsed.diagnostics);
                println!("PARSE\t{}", common::path_to_manifest_string(&parsed.path));
                Ok(())
            })
        }
        Some("verify-parse") => parse_path_args(args, "--parse").and_then(|(root, parse_path)| {
            parsing::verify_parse(&root, &parse_path)?;
            println!("PARSE_VERIFY\tPASS");
            Ok(())
        }),
        Some("interpret") => parse_path_args(args, "--parse").and_then(|(root, parse_path)| {
            let created = interpretation::create_interpretation(&root, &parse_path)?;
            for (name, value) in created.cli_counts() {
                println!("{name}\t{value}");
            }
            println!("INTERPRETATION_BLAKE3\t{}", created.manifest_blake3);
            println!("INTERPRETATION_BYTES\t{}", created.manifest_bytes);
            println!("NEW_MANIFEST\t{}", created.new_manifest);
            println!(
                "INTERPRETATION\t{}",
                common::path_to_manifest_string(&created.path)
            );
            Ok(())
        }),
        Some("verify-interpretation") => {
            parse_path_args(args, "--interpretation").and_then(|(root, path)| {
                interpretation::verify_interpretation(&root, &path)?;
                println!("INTERPRETATION_VERIFY\tPASS");
                Ok(())
            })
        }
        Some("catalog") => parse_path_args(args, "--interpretation").and_then(|(root, path)| {
            let created = catalog::create_catalog(&root, &path)?;
            for (name, value) in created.cli_fields() {
                println!("{name}\t{value}");
            }
            Ok(())
        }),
        Some("verify-catalog") => parse_path_args(args, "--catalog").and_then(|(root, path)| {
            catalog::verify_catalog(&root, &path)?;
            println!("CATALOG_VERIFY\tPASS");
            Ok(())
        }),
        Some("create-occurrences") => {
            parse_path_args(args, "--catalog").and_then(|(root, catalog)| {
                let created = occurrences::create(&root, &catalog)?;
                println!("GAME_OCCURRENCES\t{}", created.occurrences);
                println!("OCCURRENCES_BLAKE3\t{}", created.manifest_blake3);
                println!("OCCURRENCES_BYTES\t{}", created.manifest_bytes);
                println!("NEW_MANIFEST\t{}", created.new_manifest);
                println!(
                    "OCCURRENCES\t{}",
                    common::path_to_manifest_string(&created.path)
                );
                Ok(())
            })
        }
        Some("verify-occurrences") => {
            parse_path_args(args, "--occurrences").and_then(|(root, path)| {
                occurrences::verify(&root, &path)?;
                println!("OCCURRENCES_VERIFY\tPASS");
                Ok(())
            })
        }
        Some("create-mainlines") => {
            parse_path_args(args, "--occurrences").and_then(|(root, occurrences)| {
                let created = mainlines::create(&root, &occurrences)?;
                println!("MAINLINES\t{}", created.mainlines);
                println!("COMPARABLE_MAINLINES\t{}", created.comparable);
                println!("MAINLINES_BLAKE3\t{}", created.manifest_blake3);
                println!("MAINLINES_BYTES\t{}", created.manifest_bytes);
                println!("NEW_MANIFEST\t{}", created.new_manifest);
                println!(
                    "MAINLINES_ARTIFACT\t{}",
                    common::path_to_manifest_string(&created.path)
                );
                Ok(())
            })
        }
        Some("verify-mainlines") => {
            parse_path_args(args, "--mainlines").and_then(|(root, path)| {
                mainlines::verify(&root, &path)?;
                println!("MAINLINES_VERIFY\tPASS");
                Ok(())
            })
        }
        Some("export-mainlines") => {
            parse_mainlines_jsonl_args(args).and_then(|(root, mainline_path, output)| {
                let bytes = mainlines::export_jsonl(&root, &mainline_path, output.as_deref())?;
                if let Some(path) = output {
                    println!("OUTPUT\t{}", path.display());
                    println!("MAINLINES_EXPORT\tPASS");
                } else {
                    std::io::stdout()
                        .write_all(&bytes)
                        .map_err(|error| format!("Scrittura JSONL mainlines su stdout: {error}"))?;
                }
                Ok(())
            })
        }
        Some("export-occurrences") => {
            parse_occurrences_jsonl_args(args).and_then(|(root, occurrence_path, output)| {
                let bytes = occurrences::export_jsonl(&root, &occurrence_path, output.as_deref())?;
                if let Some(path) = output {
                    println!("OUTPUT\t{}", path.display());
                    println!("OCCURRENCES_EXPORT\tPASS");
                } else {
                    std::io::stdout()
                        .write_all(&bytes)
                        .map_err(|error| format!("Scrittura JSONL su stdout: {error}"))?;
                }
                Ok(())
            })
        }
        Some("query-catalog") => catalog::parse_query_args(args).and_then(|query| {
            let output = catalog::query_catalog(&query)?;
            print!("{output}");
            Ok(())
        }),
        Some("export-game") => {
            parse_export_game_args(args).and_then(|(root, catalog_path, game_id, output)| {
                let exported = catalog::export_game(&root, &catalog_path, game_id, &output)?;
                println!("GAME_ID\t{}", exported.game_id);
                println!("GAME_BLAKE3\t{}", exported.blake3);
                println!("GAME_BYTES\t{}", exported.bytes);
                println!("OUTPUT\t{}", exported.output.display());
                println!("GAME_EXPORT\tPASS");
                Ok(())
            })
        }
        Some("ingest-directory") => {
            let (root, input, previous) = match parse_ingest_directory_args(args) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("ERRORE inspector: {e}");
                    return ExitCode::from(1);
                }
            };
            let ingested = match collection::ingest_directory(&root, &input, previous.as_deref()) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("ERRORE inspector: {e}");
                    return ExitCode::from(1);
                }
            };
            for file in &ingested.files {
                let json = |v: Option<&str>| serde_json::to_string(&v).expect("JSON stringa");
                println!(
                    "FILE\t{}\t{}\t{}\t{}\t{}",
                    serde_json::to_string(&file.relative).expect("JSON path"),
                    file.status.text(),
                    json(file.source.as_deref()),
                    json(file.catalog.as_deref()),
                    json(file.error_code.as_deref())
                );
            }
            let count = |status: collection::FileStatus| {
                ingested
                    .files
                    .iter()
                    .filter(|f| {
                        std::mem::discriminant(&f.status) == std::mem::discriminant(&status)
                    })
                    .count()
            };
            println!("SCANNED_FILES\t{}", ingested.scanned_files);
            println!("PGN_FILES\t{}", ingested.pgn_files);
            println!("INGESTED\t{}", count(collection::FileStatus::Ingested));
            println!(
                "REUSED_PREVIOUS\t{}",
                count(collection::FileStatus::ReusedPrevious)
            );
            println!(
                "REUSED_WORKSPACE\t{}",
                count(collection::FileStatus::ReusedWorkspace)
            );
            println!(
                "DUPLICATE_SOURCES\t{}",
                count(collection::FileStatus::DuplicateSource)
            );
            println!("FAILED\t{}", ingested.failed);
            println!("SKIPPED_SYMLINKS\t{}", ingested.skipped_symlinks);
            let sources = ingested
                .created
                .as_ref()
                .map_or(0, |created| created.manifest.source_count());
            let games = ingested
                .created
                .as_ref()
                .map_or(0, |created| created.manifest.games());
            println!("SOURCES\t{sources}");
            println!("GAMES\t{games}");
            if let Some(created) = &ingested.created {
                println!("COLLECTION_BLAKE3\t{}", created.manifest_blake3);
                println!("COLLECTION_BYTES\t{}", created.manifest_bytes);
                println!("NEW_MANIFEST\t{}", created.new_manifest);
                println!(
                    "COLLECTION\t{}",
                    common::path_to_manifest_string(&created.path)
                );
            }
            println!(
                "INGEST_RESULT\t{}",
                if ingested.failed == 0 {
                    "PASS"
                } else {
                    "PARTIAL"
                }
            );
            if ingested.failed == 0 {
                Ok(())
            } else {
                return ExitCode::from(1);
            }
        }
        Some("verify-collection") => {
            parse_path_args(args, "--collection").and_then(|(root, path)| {
                collection::verify(&root, &path)?;
                println!("COLLECTION_VERIFY\tPASS");
                Ok(())
            })
        }
        Some("duplicates") => {
            parse_duplicates_args(args).and_then(|(root, collection_path, format)| {
                print!(
                    "{}",
                    collection::duplicates_report(&root, &collection_path, format)?
                );
                Ok(())
            })
        }
        Some("query-collection") => {
            parse_optional_collection_filter_args(args).and_then(|(root, collection, filters)| {
                let collection_path = match collection {
                    Some(path) => path,
                    None => collection::resolve_active_collection(&root)?,
                };
                print!(
                    "{}",
                    collection::query_collection(&root, &collection_path, &filters)?
                );
                Ok(())
            })
        }
        Some("inventory-collection") => {
            parse_collection_filter_args(args).and_then(|(root, collection_path, filters)| {
                let bytes = collection::inventory_collection(&root, &collection_path, &filters)?;
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|error| format!("Scrittura inventory JSONL su stdout: {error}"))
            })
        }
        Some("create-archive-view") => {
            parse_collection_filter_args(args).and_then(|(root, collection_path, filters)| {
                let created = archive_view::create(&root, &collection_path, &filters)?;
                println!("ARCHIVE_VIEW_OCCURRENCES\t{}", created.occurrences);
                println!("ARCHIVE_VIEW_BLAKE3\t{}", created.manifest_blake3);
                println!("ARCHIVE_VIEW_BYTES\t{}", created.manifest_bytes);
                println!("NEW_MANIFEST\t{}", created.new_manifest);
                println!(
                    "ARCHIVE_VIEW\t{}",
                    common::path_to_manifest_string(&created.path)
                );
                Ok(())
            })
        }
        Some("verify-archive-view") => parse_path_args(args, "--view").and_then(|(root, view)| {
            archive_view::verify(&root, &view)?;
            println!("ARCHIVE_VIEW_VERIFY\tPASS");
            Ok(())
        }),
        Some("inventory-view") => parse_path_args(args, "--view").and_then(|(root, view)| {
            let bytes = archive_view::inventory(&root, &view)?;
            std::io::stdout()
                .write_all(&bytes)
                .map_err(|error| format!("Scrittura inventory JSONL su stdout: {error}"))
        }),
        Some("position-id") => parse_position_id_args(args).and_then(|fen| {
            let identity =
                chess_rules::position_identity_from_fen(fen.as_bytes()).map_err(|failure| {
                    format!("FEN posizione non valido: {}", fen_failure_name(failure))
                })?;
            let fen_status = match identity.fen_status {
                chess_rules::FenStatus::Canonical => "canonical",
                chess_rules::FenStatus::Recovered => "recovered",
            };
            println!("POSITION_PROFILE\t{}", identity.profile);
            println!("POSITION_ID\t{}", identity.position_id);
            println!("POSITION_KEY\t{}", identity.position_key);
            println!("FEN_STATUS\t{fen_status}");
            Ok(())
        }),
        Some("validate-analysis") => parse_analysis_validation_args(args).and_then(|input| {
            let artifact = analyses::load_and_validate(&input)?;
            println!("ANALYSIS_SUBJECT_ID\t{}", artifact.subject_id);
            println!("ANALYSIS_LINES\t{}", artifact.lines.len());
            println!("ANALYSIS_VERIFY\tPASS");
            Ok(())
        }),
        Some("store-analysis") => parse_analysis_store_args(args).and_then(|(root, input)| {
            let stored = analyses::store(&root, &input)?;
            println!("ANALYSIS_SUBJECT_ID\t{}", stored.subject_id);
            println!("ANALYSIS_LINES\t{}", stored.lines);
            println!("ANALYSIS_BLAKE3\t{}", stored.blake3);
            println!("ANALYSIS_BYTES\t{}", stored.bytes);
            println!(
                "ANALYSIS_PATH\t{}",
                common::path_to_manifest_string(&stored.path)
            );
            println!("NEW_ARTIFACT\t{}", stored.new_artifact);
            println!("ANALYSIS_STORE\tPASS");
            Ok(())
        }),
        Some("verify-analysis") => parse_analysis_verify_args(args).and_then(|(root, path)| {
            let verified = analyses::load_verified(&root, &path)?;
            println!("ANALYSIS_SUBJECT_ID\t{}", verified.artifact.subject_id);
            println!("ANALYSIS_LINES\t{}", verified.artifact.lines.len());
            println!("ANALYSIS_BLAKE3\t{}", verified.blake3);
            println!("ANALYSIS_BYTES\t{}", verified.bytes);
            println!(
                "ANALYSIS_PATH\t{}",
                common::path_to_manifest_string(&verified.path)
            );
            println!("ANALYSIS_VERIFY\tPASS");
            Ok(())
        }),
        Some("validate-analysis-requirement") => {
            parse_analysis_validation_args(args).and_then(|input| {
                let requirement = analysis_requirements::load_and_validate(&input)?;
                println!(
                    "ANALYSIS_REQUIREMENT_PROFILE\t{}",
                    requirement.requirement_profile
                );
                println!(
                    "ANALYSIS_REQUIREMENT_SUBJECT_ID\t{}",
                    requirement.subject_id
                );
                println!(
                    "ANALYSIS_REQUIREMENT_MIN_MULTIPV\t{}",
                    requirement.min_multipv
                );
                println!("ANALYSIS_REQUIREMENT_VERIFY\tPASS");
                Ok(())
            })
        }
        Some("select-analysis") => {
            parse_analysis_selection_args(args).and_then(|(root, requirement_path, analyses)| {
                let requirement = analysis_requirements::load_and_validate(&requirement_path)?;
                let selection = analysis_requirements::select(&root, &requirement, &analyses)?;
                println!("ANALYSIS_CANDIDATES\t{}", selection.candidates);
                println!("ANALYSIS_QUALIFIED\t{}", selection.qualified);
                println!(
                    "SELECTED_ANALYSIS_PATH\t{}",
                    common::path_to_manifest_string(&selection.selected.path)
                );
                println!("SELECTED_ANALYSIS_BLAKE3\t{}", selection.selected.blake3);
                println!(
                    "SELECTED_ANALYSIS_ORIGIN\t{}",
                    analysis_requirements::origin_name(selection.selected.artifact.origin)
                );
                println!(
                    "SELECTED_ANALYSIS_METHOD\t{}",
                    selection.selected.artifact.method
                );
                println!("ANALYSIS_SELECTION\tPASS");
                Ok(())
            })
        }
        Some("store-lichess-eval-record") => parse_lichess_eval_store_args(args).and_then(
            |(root, input, snapshot_id, observed_at)| {
                let stored = lichess_evals::store_record(
                    &root,
                    &input,
                    &snapshot_id,
                    observed_at.as_deref(),
                )?;
                println!("POSITION_ID\t{}", stored.position_id);
                println!("LICHESS_EVALUATIONS\t{}", stored.evaluations);
                for (index, artifact) in stored.artifacts.iter().enumerate() {
                    let number = index + 1;
                    println!("ANALYSIS_{number}_BLAKE3\t{}", artifact.blake3);
                    println!(
                        "ANALYSIS_{number}_PATH\t{}",
                        common::path_to_manifest_string(&artifact.path)
                    );
                    println!("ANALYSIS_{number}_NEW\t{}", artifact.new_artifact);
                }
                println!("LICHESS_EVAL_STORE\tPASS");
                Ok(())
            },
        ),
        Some("validate-source-assertions") => {
            parse_assertion_validation_args(args).and_then(|input| {
                let group = source_assertions::load_and_validate(&input)?;
                println!("ASSERTION_GROUP_ID\t{}", group.group_id);
                println!("ASSERTIONS\t{}", group.assertions.len());
                println!("SOURCE_ASSERTIONS_VERIFY\tPASS");
                Ok(())
            })
        }
        Some("store-source-assertions") => {
            parse_assertion_store_args(args).and_then(|(root, input)| {
                let stored = source_assertions::store(&root, &input)?;
                println!("ASSERTION_GROUP_ID\t{}", stored.group_id);
                println!("ASSERTIONS\t{}", stored.assertions);
                println!("ASSERTIONS_BLAKE3\t{}", stored.blake3);
                println!("ASSERTIONS_BYTES\t{}", stored.bytes);
                println!(
                    "ASSERTIONS_PATH\t{}",
                    common::path_to_manifest_string(&stored.path)
                );
                println!("NEW_ARTIFACT\t{}", stored.new_artifact);
                println!("SOURCE_ASSERTIONS_STORE\tPASS");
                Ok(())
            })
        }
        Some("verify-source-assertions") => {
            parse_assertion_verify_args(args).and_then(|(root, path)| {
                let verified = source_assertions::load_verified(&root, &path)?;
                println!("ASSERTION_GROUP_ID\t{}", verified.group.group_id);
                println!("ASSERTIONS\t{}", verified.group.assertions.len());
                println!("ASSERTIONS_BLAKE3\t{}", verified.blake3);
                println!("ASSERTIONS_BYTES\t{}", verified.bytes);
                println!(
                    "ASSERTIONS_PATH\t{}",
                    common::path_to_manifest_string(&verified.path)
                );
                println!("SOURCE_ASSERTIONS_VERIFY\tPASS");
                Ok(())
            })
        }
        Some("index-source-assertions") => {
            parse_assertion_index_args(args).and_then(|(root, paths)| {
                let rows = assertion_index::build(&root, &paths)?;
                let bytes = assertion_index::jsonl(&rows)?;
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|error| format!("Scrittura indice asserzioni su stdout: {error}"))
            })
        }
        Some("validate-reconciliation-policy") => parse_annotation_validation_args(args)
            .and_then(|policy| assertion_reconciliation::validate_policy(&policy)),
        Some("reconcile-source-assertion") => {
            parse_assertion_reconciliation_args(args).and_then(|(root, paths, policy)| {
                let result = assertion_reconciliation::reconcile(&root, &paths, &policy)?;
                serde_json::to_writer(std::io::stdout(), &result)
                    .map_err(|e| format!("Serializzazione riconciliazione: {e}"))?;
                println!();
                Ok(())
            })
        }
        Some("validate-annotations") => parse_annotation_validation_args(args).and_then(|input| {
            let group = annotations::load_and_validate(&input)?;
            println!("ANNOTATION_GROUP_ID\t{}", group.group_id);
            println!("ANNOTATION_ITEMS\t{}", group.items.len());
            println!("ANNOTATIONS_VERIFY\tPASS");
            Ok(())
        }),
        Some("store-annotations") => parse_annotation_store_args(args).and_then(|(root, input)| {
            let stored = annotations::store(&root, &input)?;
            println!("ANNOTATION_GROUP_ID\t{}", stored.group_id);
            println!("ANNOTATION_ITEMS\t{}", stored.items);
            println!("ANNOTATION_BLAKE3\t{}", stored.blake3);
            println!("ANNOTATION_BYTES\t{}", stored.bytes);
            println!(
                "ANNOTATION_PATH\t{}",
                common::path_to_manifest_string(&stored.path)
            );
            println!("NEW_ARTIFACT\t{}", stored.new_artifact);
            println!("ANNOTATIONS_STORE\tPASS");
            Ok(())
        }),
        Some("verify-annotations") => {
            parse_annotation_verify_args(args).and_then(|(root, path)| {
                let verified = annotations::load_verified(&root, &path)?;
                println!("ANNOTATION_GROUP_ID\t{}", verified.group.group_id);
                println!("ANNOTATION_ITEMS\t{}", verified.group.items.len());
                println!("ANNOTATION_BLAKE3\t{}", verified.blake3);
                println!("ANNOTATION_BYTES\t{}", verified.bytes);
                println!(
                    "ANNOTATION_PATH\t{}",
                    common::path_to_manifest_string(&verified.path)
                );
                println!("ANNOTATIONS_VERIFY\tPASS");
                Ok(())
            })
        }
        Some("index-annotations") => parse_annotation_index_args(args).and_then(|(root, paths)| {
            let rows = annotation_index::build(&root, &paths)?;
            let bytes = annotation_index::jsonl(&rows)?;
            std::io::stdout()
                .write_all(&bytes)
                .map_err(|error| format!("Scrittura indice annotazioni su stdout: {error}"))
        }),
        Some("export-occurrence") => {
            let values = args.collect::<Vec<_>>();
            if values.iter().any(|value| value == "--occurrences") {
                parse_single_occurrence_args(values).and_then(
                    |(root, occurrence_path, occurrence_id, output)| {
                        let exported = occurrences::export_one(
                            &root,
                            &occurrence_path,
                            &occurrence_id,
                            &output,
                        )?;
                        println!("OCCURRENCE_ID\t{}", exported.occurrence_id);
                        println!("GAME_BLAKE3\t{}", exported.blake3);
                        println!("GAME_BYTES\t{}", exported.bytes);
                        println!("OUTPUT\t{}", exported.output.display());
                        println!("OCCURRENCE_EXPORT\tPASS");
                        Ok(())
                    },
                )
            } else {
                parse_export_occurrence_args(values).and_then(
                    |(root, collection_path, source_id, game_id, output)| {
                        let e = collection::export_occurrence(
                            &root,
                            &collection_path,
                            source_id,
                            game_id,
                            &output,
                        )?;
                        println!("SOURCE_ID\t{}", e.source_id);
                        println!("GAME_ID\t{}", e.game_id);
                        println!("SOURCE_BLAKE3\t{}", e.source_blake3);
                        println!("GAME_BLAKE3\t{}", e.game_blake3);
                        println!("GAME_BYTES\t{}", e.bytes);
                        println!("OUTPUT\t{}", e.output.display());
                        println!("OCCURRENCE_EXPORT\tPASS");
                        Ok(())
                    },
                )
            }
        }
        Some(command) => Err(format!("Comando inspector sconosciuto: {command}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ERRORE inspector: {error}");
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    println!(
        "\
antiskill inspector

Importazione ed esportazione lossless di sorgenti PGN.

USO:
    antiskill inspector import --root WORKSPACE FILE.pgn
    antiskill inspector export-source --root WORKSPACE --manifest MANIFEST --output FILE.pgn
    antiskill inspector segment --root WORKSPACE --manifest SOURCE_MANIFEST
    antiskill inspector verify-segmentation --root WORKSPACE --segmentation SEGMENTATION
    antiskill inspector tokenize --root WORKSPACE --segmentation SEGMENTATION
    antiskill inspector verify-tokenization --root WORKSPACE --tokenization TOKENIZATION
    antiskill inspector parse --root WORKSPACE --tokenization TOKENIZATION
    antiskill inspector verify-parse --root WORKSPACE --parse PARSE
    antiskill inspector interpret --root WORKSPACE --parse PARSE
    antiskill inspector verify-interpretation --root WORKSPACE --interpretation INTERPRETATION
    antiskill inspector catalog --root WORKSPACE --interpretation INTERPRETATION
    antiskill inspector verify-catalog --root WORKSPACE --catalog CATALOG
    antiskill inspector create-occurrences --root WORKSPACE --catalog CATALOG
    antiskill inspector verify-occurrences --root WORKSPACE --occurrences OCCURRENCES
    antiskill inspector export-occurrences --root WORKSPACE --occurrences OCCURRENCES [--output FILE]
    antiskill inspector export-occurrence --root WORKSPACE --occurrences OCCURRENCES --occurrence-id BLAKE3 --output FILE
    antiskill inspector create-mainlines --root WORKSPACE --occurrences OCCURRENCES
    antiskill inspector verify-mainlines --root WORKSPACE --mainlines MAINLINES
    antiskill inspector export-mainlines --root WORKSPACE --mainlines MAINLINES [--output FILE]
    antiskill inspector query-catalog --root WORKSPACE --catalog CATALOG [--player TEXT] [--event TEXT] [--date YYYY-MM-DD] [--date-from YYYY-MM-DD] [--date-to YYYY-MM-DD]
    antiskill inspector export-game --root WORKSPACE --catalog CATALOG --game-id ID --output FILE
    antiskill inspector ingest-directory --root WORKSPACE --input DIRECTORY [--previous COLLECTION]
    antiskill inspector verify-collection --root WORKSPACE --collection COLLECTION
    antiskill inspector duplicates --root WORKSPACE --collection COLLECTION [--format text|jsonl]
    antiskill inspector query-collection --root WORKSPACE [--collection COLLECTION] [--player TEXT] [--event TEXT] [--date YYYY-MM-DD] [--date-from YYYY-MM-DD] [--date-to YYYY-MM-DD]
    antiskill inspector inventory-collection --root WORKSPACE --collection COLLECTION [--player TEXT] [--event TEXT] [--date YYYY-MM-DD] [--date-from YYYY-MM-DD] [--date-to YYYY-MM-DD]
    antiskill inspector create-archive-view --root WORKSPACE --collection COLLECTION [--player TEXT] [--event TEXT] [--date YYYY-MM-DD] [--date-from YYYY-MM-DD] [--date-to YYYY-MM-DD]
    antiskill inspector verify-archive-view --root WORKSPACE --view VIEW
    antiskill inspector inventory-view --root WORKSPACE --view VIEW
    antiskill inspector position-id --fen \"FEN\"
    antiskill inspector validate-analysis --input FILE.json
    antiskill inspector store-analysis --root WORKSPACE --input FILE.json
    antiskill inspector verify-analysis --root WORKSPACE --analysis PATH
    antiskill inspector validate-analysis-requirement --input REQUIREMENT.json
    antiskill inspector select-analysis --root WORKSPACE --requirement REQUIREMENT.json --analysis PATH [--analysis PATH ...]
    antiskill inspector store-lichess-eval-record --root WORKSPACE --input RECORD.json --snapshot-id ID [--observed-at DATE]
    antiskill inspector validate-source-assertions --input FILE.json
    antiskill inspector store-source-assertions --root WORKSPACE --input FILE.json
    antiskill inspector verify-source-assertions --root WORKSPACE --assertions PATH
    antiskill inspector index-source-assertions --root WORKSPACE --assertions PATH [--assertions PATH ...]
    antiskill inspector validate-reconciliation-policy --input POLICY.json
    antiskill inspector reconcile-source-assertion --root WORKSPACE --policy POLICY.json --assertions PATH [--assertions PATH ...]
    antiskill inspector validate-annotations --input FILE.json
    antiskill inspector store-annotations --root WORKSPACE --input FILE.json
    antiskill inspector verify-annotations --root WORKSPACE --annotations PATH
    antiskill inspector index-annotations --root WORKSPACE --annotations PATH [--annotations PATH ...]
    antiskill inspector export-occurrence --root WORKSPACE --collection COLLECTION --source-id ID --game-id ID --output FILE

duplicates rileva duplicati byte-esatti senza modificare la Collection; text è il formato predefinito, jsonl è deterministico.
"
    );
}

fn fen_failure_name(failure: chess_rules::FenFailure) -> &'static str {
    match failure {
        chess_rules::FenFailure::InvalidEncoding => "encoding",
        chess_rules::FenFailure::InvalidFen => "syntax",
        chess_rules::FenFailure::IllegalPosition => "illegal_position",
    }
}

fn parse_position_id_args<I: IntoIterator<Item = String>>(args: I) -> Result<String, String> {
    let mut fen = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--fen" if fen.is_none() => fen = Some(next_value(&mut args, "--fen")?),
            "--fen" => return Err("Opzione duplicata: --fen".into()),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    fen.ok_or("Manca --fen FEN".into())
}

fn parse_analysis_validation_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<PathBuf, String> {
    let mut input = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--input" if input.is_none() => {
                input = Some(PathBuf::from(next_value(&mut args, "--input")?));
            }
            "--input" => return Err("Opzione duplicata: --input".into()),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    input.ok_or("Manca --input FILE.json".into())
}

fn parse_analysis_store_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf), String> {
    let (mut root, mut input) = (None, None);
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?));
            }
            "--input" if input.is_none() => {
                input = Some(PathBuf::from(next_value(&mut args, "--input")?));
            }
            "--root" => return Err("Opzione duplicata: --root".into()),
            "--input" => return Err("Opzione duplicata: --input".into()),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        input.ok_or("Manca --input FILE.json")?,
    ))
}

fn parse_analysis_verify_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf), String> {
    let (mut root, mut analysis) = (None, None);
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?));
            }
            "--analysis" if analysis.is_none() => {
                analysis = Some(PathBuf::from(next_value(&mut args, "--analysis")?));
            }
            "--root" => return Err("Opzione duplicata: --root".into()),
            "--analysis" => return Err("Opzione duplicata: --analysis".into()),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        analysis.ok_or("Manca --analysis PATH")?,
    ))
}

fn parse_analysis_selection_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf, Vec<PathBuf>), String> {
    let (mut root, mut requirement) = (None, None);
    let mut analyses = Vec::new();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?));
            }
            "--requirement" if requirement.is_none() => {
                requirement = Some(PathBuf::from(next_value(&mut args, "--requirement")?));
            }
            "--analysis" => {
                analyses.push(PathBuf::from(next_value(&mut args, "--analysis")?));
            }
            "--root" | "--requirement" => {
                return Err(format!("Opzione duplicata: {argument}"));
            }
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    if analyses.is_empty() {
        return Err("Manca almeno un --analysis PATH".into());
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        requirement.ok_or("Manca --requirement REQUIREMENT.json")?,
        analyses,
    ))
}

fn parse_lichess_eval_store_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf, String, Option<String>), String> {
    let (mut root, mut input, mut snapshot_id, mut observed_at) = (None, None, None, None);
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?));
            }
            "--input" if input.is_none() => {
                input = Some(PathBuf::from(next_value(&mut args, "--input")?));
            }
            "--snapshot-id" if snapshot_id.is_none() => {
                snapshot_id = Some(next_value(&mut args, "--snapshot-id")?);
            }
            "--observed-at" if observed_at.is_none() => {
                observed_at = Some(next_value(&mut args, "--observed-at")?);
            }
            "--root" | "--input" | "--snapshot-id" | "--observed-at" => {
                return Err(format!("Opzione duplicata: {argument}"));
            }
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        input.ok_or("Manca --input RECORD.json")?,
        snapshot_id.ok_or("Manca --snapshot-id ID")?,
        observed_at,
    ))
}

fn parse_assertion_validation_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<PathBuf, String> {
    parse_analysis_validation_args(args)
}

fn parse_assertion_store_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf), String> {
    parse_analysis_store_args(args)
}

fn parse_assertion_verify_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf), String> {
    let (mut root, mut assertions) = (None, None);
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?))
            }
            "--assertions" if assertions.is_none() => {
                assertions = Some(PathBuf::from(next_value(&mut args, "--assertions")?))
            }
            "--root" => return Err("Opzione duplicata: --root".into()),
            "--assertions" => return Err("Opzione duplicata: --assertions".into()),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        assertions.ok_or("Manca --assertions PATH")?,
    ))
}

fn parse_assertion_index_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, Vec<PathBuf>), String> {
    let mut root = None;
    let mut assertions = Vec::new();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?));
            }
            "--assertions" => {
                assertions.push(PathBuf::from(next_value(&mut args, "--assertions")?));
            }
            "--root" => return Err("Opzione duplicata: --root".into()),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    if assertions.is_empty() {
        return Err("Serve almeno un --assertions PATH".into());
    }
    Ok((root.ok_or("Manca --root WORKSPACE")?, assertions))
}

fn parse_assertion_reconciliation_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, Vec<PathBuf>, PathBuf), String> {
    let mut root = None;
    let mut policy = None;
    let mut assertions = Vec::new();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?))
            }
            "--policy" if policy.is_none() => {
                policy = Some(PathBuf::from(next_value(&mut args, "--policy")?))
            }
            "--assertions" => {
                assertions.push(PathBuf::from(next_value(&mut args, "--assertions")?))
            }
            "--root" | "--policy" => return Err(format!("Opzione duplicata: {argument}")),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    if assertions.is_empty() {
        return Err("Serve almeno un --assertions PATH".into());
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        assertions,
        policy.ok_or("Manca --policy POLICY.json")?,
    ))
}

fn parse_annotation_validation_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<PathBuf, String> {
    let mut input = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--input" if input.is_none() => {
                input = Some(PathBuf::from(next_value(&mut args, "--input")?));
            }
            "--input" => return Err("Opzione duplicata: --input".into()),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    input.ok_or("Manca --input FILE.json".into())
}

fn parse_annotation_store_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf), String> {
    let (mut root, mut input) = (None, None);
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?));
            }
            "--input" if input.is_none() => {
                input = Some(PathBuf::from(next_value(&mut args, "--input")?));
            }
            "--root" => return Err("Opzione duplicata: --root".into()),
            "--input" => return Err("Opzione duplicata: --input".into()),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        input.ok_or("Manca --input FILE.json")?,
    ))
}

fn parse_annotation_verify_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf), String> {
    let (mut root, mut annotations) = (None, None);
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?));
            }
            "--annotations" if annotations.is_none() => {
                annotations = Some(PathBuf::from(next_value(&mut args, "--annotations")?));
            }
            "--root" => return Err("Opzione duplicata: --root".into()),
            "--annotations" => return Err("Opzione duplicata: --annotations".into()),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        annotations.ok_or("Manca --annotations PATH")?,
    ))
}

fn parse_annotation_index_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, Vec<PathBuf>), String> {
    let mut root = None;
    let mut annotations = Vec::new();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?))
            }
            "--annotations" => {
                annotations.push(PathBuf::from(next_value(&mut args, "--annotations")?))
            }
            "--root" => return Err("Opzione duplicata: --root".into()),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    if annotations.is_empty() {
        return Err("Serve almeno un --annotations PATH".into());
    }
    Ok((root.ok_or("Manca --root WORKSPACE")?, annotations))
}

fn parse_duplicates_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf, collection::DuplicateFormat), String> {
    let (mut root, mut collection, mut format) = (None, None, None);
    let mut it = args.into_iter();
    while let Some(argument) = it.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut it, "--root")?));
            }
            "--collection" if collection.is_none() => {
                collection = Some(PathBuf::from(next_value(&mut it, "--collection")?));
            }
            "--format" if format.is_none() => {
                format = Some(match next_value(&mut it, "--format")?.as_str() {
                    "text" => collection::DuplicateFormat::Text,
                    "jsonl" => collection::DuplicateFormat::Jsonl,
                    value => return Err(format!("Formato duplicates sconosciuto: {value}")),
                });
            }
            "--root" | "--collection" | "--format" => {
                return Err(format!("Opzione duplicata: {argument}"));
            }
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        collection.ok_or("Manca --collection COLLECTION")?,
        format.unwrap_or(collection::DuplicateFormat::Text),
    ))
}

fn parse_ingest_directory_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf, Option<PathBuf>), String> {
    let (mut root, mut input, mut previous) = (None, None, None);
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = Some(PathBuf::from(next_value(&mut it, "--root")?)),
            "--input" => input = Some(PathBuf::from(next_value(&mut it, "--input")?)),
            "--previous" => previous = Some(PathBuf::from(next_value(&mut it, "--previous")?)),
            _ => return Err(format!("Argomento inatteso: {a}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        input.ok_or("Manca --input DIRECTORY")?,
        previous,
    ))
}

fn parse_collection_filter_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf, catalog::QueryFilters), String> {
    let mut values = Vec::new();
    let mut collection = None;
    let mut root = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let v = next_value(&mut it, &a)?;
        match a.as_str() {
            "--collection" => collection = Some(PathBuf::from(v)),
            "--root" => {
                root = Some(PathBuf::from(&v));
            }
            _ => values.extend([a, v]),
        }
    }
    let filters = catalog::parse_query_filters(values)?;
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        collection.ok_or("Manca --collection COLLECTION")?,
        filters,
    ))
}

fn parse_optional_collection_filter_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, Option<PathBuf>, catalog::QueryFilters), String> {
    let mut values = Vec::new();
    let mut collection = None;
    let mut root = None;
    let mut it = args.into_iter();
    while let Some(argument) = it.next() {
        let value = next_value(&mut it, &argument)?;
        match argument.as_str() {
            "--collection" if collection.is_none() => collection = Some(PathBuf::from(value)),
            "--root" if root.is_none() => root = Some(PathBuf::from(value)),
            "--collection" | "--root" => {
                return Err(format!("Opzione duplicata: {argument}"));
            }
            _ => values.extend([argument, value]),
        }
    }
    let filters = catalog::parse_query_filters(values)?;
    Ok((root.ok_or("Manca --root WORKSPACE")?, collection, filters))
}

fn parse_export_occurrence_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf, u64, u64, PathBuf), String> {
    let (mut root, mut collection, mut source_id, mut game_id, mut output) =
        (None, None, None, None, None);
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = Some(PathBuf::from(next_value(&mut it, &a)?)),
            "--collection" => collection = Some(PathBuf::from(next_value(&mut it, &a)?)),
            "--source-id" => {
                source_id = Some(
                    next_value(&mut it, &a)?
                        .parse()
                        .map_err(|_| "source-id non valido")?,
                )
            }
            "--game-id" => {
                game_id = Some(
                    next_value(&mut it, &a)?
                        .parse()
                        .map_err(|_| "game-id non valido")?,
                )
            }
            "--output" => output = Some(PathBuf::from(next_value(&mut it, &a)?)),
            _ => return Err(format!("Argomento inatteso: {a}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        collection.ok_or("Manca --collection COLLECTION")?,
        source_id.ok_or("Manca --source-id ID")?,
        game_id.ok_or("Manca --game-id ID")?,
        output.ok_or("Manca --output FILE")?,
    ))
}

fn parse_occurrences_jsonl_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf, Option<PathBuf>), String> {
    let (mut root, mut occurrences, mut output) = (None, None, None);
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?))
            }
            "--occurrences" if occurrences.is_none() => {
                occurrences = Some(PathBuf::from(next_value(&mut args, "--occurrences")?))
            }
            "--output" if output.is_none() => {
                output = Some(PathBuf::from(next_value(&mut args, "--output")?))
            }
            "--root" | "--occurrences" | "--output" => {
                return Err(format!("Opzione duplicata: {argument}"))
            }
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        occurrences.ok_or("Manca --occurrences OCCURRENCES")?,
        output,
    ))
}

fn parse_mainlines_jsonl_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf, Option<PathBuf>), String> {
    let (mut root, mut mainlines, mut output) = (None, None, None);
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?))
            }
            "--mainlines" if mainlines.is_none() => {
                mainlines = Some(PathBuf::from(next_value(&mut args, "--mainlines")?))
            }
            "--output" if output.is_none() => {
                output = Some(PathBuf::from(next_value(&mut args, "--output")?))
            }
            "--root" | "--mainlines" | "--output" => {
                return Err(format!("Opzione duplicata: {argument}"))
            }
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        mainlines.ok_or("Manca --mainlines MAINLINES")?,
        output,
    ))
}

fn parse_single_occurrence_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf, String, PathBuf), String> {
    let (mut root, mut occurrences, mut occurrence_id, mut output) = (None, None, None, None);
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?))
            }
            "--occurrences" if occurrences.is_none() => {
                occurrences = Some(PathBuf::from(next_value(&mut args, "--occurrences")?))
            }
            "--occurrence-id" if occurrence_id.is_none() => {
                occurrence_id = Some(next_value(&mut args, "--occurrence-id")?)
            }
            "--output" if output.is_none() => {
                output = Some(PathBuf::from(next_value(&mut args, "--output")?))
            }
            "--root" | "--occurrences" | "--occurrence-id" | "--output" => {
                return Err(format!("Opzione duplicata: {argument}"))
            }
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        occurrences.ok_or("Manca --occurrences OCCURRENCES")?,
        occurrence_id.ok_or("Manca --occurrence-id BLAKE3")?,
        output.ok_or("Manca --output FILE")?,
    ))
}

fn parse_path_args<I>(args: I, path_option: &str) -> Result<(PathBuf, PathBuf), String>
where
    I: IntoIterator<Item = String>,
{
    let mut root = None;
    let mut path = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" => root = Some(PathBuf::from(next_value(&mut args, "--root")?)),
            value if value == path_option => {
                path = Some(PathBuf::from(next_value(&mut args, path_option)?))
            }
            value => return Err(format!("Argomento inatteso: {value}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        path.ok_or_else(|| format!("Manca {path_option} PERCORSO"))?,
    ))
}

fn parse_import_args<I>(args: I) -> Result<(PathBuf, PathBuf), String>
where
    I: IntoIterator<Item = String>,
{
    let mut root = None;
    let mut source = None;
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" => root = Some(PathBuf::from(next_value(&mut args, "--root")?)),
            value if value.starts_with('-') => return Err(format!("Opzione sconosciuta: {value}")),
            value if source.is_none() => source = Some(PathBuf::from(value)),
            value => return Err(format!("Argomento inatteso: {value}")),
        }
    }

    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        source.ok_or("Manca FILE.pgn")?,
    ))
}

fn parse_export_args<I>(args: I) -> Result<(PathBuf, PathBuf, PathBuf), String>
where
    I: IntoIterator<Item = String>,
{
    let mut root = None;
    let mut manifest = None;
    let mut output = None;
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" => root = Some(PathBuf::from(next_value(&mut args, "--root")?)),
            "--manifest" => manifest = Some(PathBuf::from(next_value(&mut args, "--manifest")?)),
            "--output" => output = Some(PathBuf::from(next_value(&mut args, "--output")?)),
            value => return Err(format!("Argomento inatteso: {value}")),
        }
    }

    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        manifest.ok_or("Manca --manifest MANIFEST")?,
        output.ok_or("Manca --output FILE.pgn")?,
    ))
}

fn next_value<I>(args: &mut I, option: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .ok_or_else(|| format!("Manca il valore per {option}"))
}

fn parse_export_game_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(PathBuf, PathBuf, u64, PathBuf), String> {
    let mut root = None;
    let mut catalog = None;
    let mut game_id = None;
    let mut output = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" => root = Some(PathBuf::from(next_value(&mut args, "--root")?)),
            "--catalog" => catalog = Some(PathBuf::from(next_value(&mut args, "--catalog")?)),
            "--game-id" => {
                game_id = Some(
                    next_value(&mut args, "--game-id")?
                        .parse()
                        .map_err(|_| "game-id non valido".to_owned())?,
                )
            }
            "--output" => output = Some(PathBuf::from(next_value(&mut args, "--output")?)),
            value => return Err(format!("Argomento inatteso: {value}")),
        }
    }
    Ok((
        root.ok_or("Manca --root WORKSPACE")?,
        catalog.ok_or("Manca --catalog CATALOG")?,
        game_id.ok_or("Manca --game-id ID")?,
        output.ok_or("Manca --output FILE")?,
    ))
}
