use super::manifest::*;
use super::{load_verified_collection, publish, CreatedCollection};
use crate::inspector::{
    catalog, interpretation, mainlines, occurrences, parsing, segmentation, source, tokenization,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub(in crate::inspector) enum FileStatus {
    Ingested,
    ReusedPrevious,
    ReusedWorkspace,
    DuplicateSource,
    Failed,
}
impl FileStatus {
    pub(in crate::inspector) fn text(self) -> &'static str {
        match self {
            Self::Ingested => "ingested",
            Self::ReusedPrevious => "reused_previous",
            Self::ReusedWorkspace => "reused_workspace",
            Self::DuplicateSource => "duplicate_source",
            Self::Failed => "failed",
        }
    }
}
pub(in crate::inspector) struct FileResult {
    pub(in crate::inspector) relative: String,
    pub(in crate::inspector) status: FileStatus,
    pub(in crate::inspector) source: Option<String>,
    pub(in crate::inspector) catalog: Option<String>,
    pub(in crate::inspector) error_code: Option<String>,
    pub(in crate::inspector) error_message: Option<String>,
}
pub(in crate::inspector) struct IngestResult {
    pub(in crate::inspector) created: Option<CreatedCollection>,
    pub(in crate::inspector) files: Vec<FileResult>,
    pub(in crate::inspector) scanned_files: u64,
    pub(in crate::inspector) pgn_files: u64,
    pub(in crate::inspector) skipped_symlinks: u64,
    pub(in crate::inspector) failed: u64,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
enum EmptySnapshotPolicy {
    Allow,
    Reject,
}

struct Reusable {
    path: String,
    hash: String,
    bytes: u64,
    games: u64,
}

struct ArtifactChain {
    catalog: Reusable,
    occurrences_path: String,
    occurrences_hash: String,
    occurrences_bytes: u64,
    mainlines_path: String,
    mainlines_hash: String,
    mainlines_bytes: u64,
}

pub(super) fn preferred_cache_path(candidate: &str, current: &str) -> bool {
    let candidate_compressed = candidate.ends_with(".json.xz");
    let current_compressed = current.ends_with(".json.xz");
    (candidate_compressed && !current_compressed)
        || (candidate_compressed == current_compressed && candidate < current)
}

fn catalog_index(root: &Path) -> Result<BTreeMap<(String, u64), Reusable>, String> {
    let directory = root.join("inspector/catalogs");
    let mut result: BTreeMap<(String, u64), Reusable> = BTreeMap::new();
    let entries = match fs::read_dir(&directory) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(result),
        Err(e) => return Err(format!("Scansione Catalog: {e}")),
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let is_json = path
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(|v| v.ends_with(".json") || v.ends_with(".json.xz"));
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_file() && !kind.is_symlink() && is_json)
                .map(|_| path)
        })
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let Some(relative_value) = relative.to_str() else {
            continue;
        };
        let Ok(loaded) = catalog::load_streaming_catalog(root, relative) else {
            continue;
        };
        let Ok(games) = loaded.for_each_verified_catalog_game(|_| Ok(())) else {
            continue;
        };
        let reusable = Reusable {
            path: relative_value.replace(std::path::MAIN_SEPARATOR, "/"),
            hash: loaded.manifest_blake3.clone(),
            bytes: loaded.manifest_bytes,
            games: u64::try_from(games).map_err(|_| "Numero partite oltre u64")?,
        };
        result
            .entry((
                loaded.header.source_blake3.clone(),
                loaded.header.source_bytes,
            ))
            .and_modify(|current| {
                if preferred_cache_path(&reusable.path, &current.path) {
                    *current = Reusable {
                        path: reusable.path.clone(),
                        hash: reusable.hash.clone(),
                        bytes: reusable.bytes,
                        games: reusable.games,
                    };
                }
            })
            .or_insert(reusable);
    }
    Ok(result)
}

fn catalog_pipeline(root: &Path, source_manifest: &Path) -> Result<Reusable, String> {
    let segmented = segmentation::segment_source(root, source_manifest)?;
    let tokenized = tokenization::tokenize(root, &segmented.path)?;
    let parsed = parsing::parse(root, &tokenized.path)?;
    let interpreted = interpretation::create_interpretation(root, &parsed.path)?;
    let created = catalog::create_catalog(root, &interpreted.path)?;
    Ok(Reusable {
        path: crate::inspector::common::path_to_manifest_string(&created.path),
        hash: created.manifest_blake3,
        bytes: created.manifest_bytes,
        games: u64::try_from(created.counts.games).map_err(|_| "Numero partite oltre u64")?,
    })
}

fn complete_chain(root: &Path, catalog: Reusable) -> Result<ArtifactChain, String> {
    let created_occurrences = occurrences::create(root, Path::new(&catalog.path))?;
    let created_mainlines = mainlines::create(root, &created_occurrences.path)?;
    Ok(ArtifactChain {
        catalog,
        occurrences_path: crate::inspector::common::path_to_manifest_string(
            &created_occurrences.path,
        ),
        occurrences_hash: created_occurrences.manifest_blake3,
        occurrences_bytes: created_occurrences.manifest_bytes,
        mainlines_path: crate::inspector::common::path_to_manifest_string(&created_mainlines.path),
        mainlines_hash: created_mainlines.manifest_blake3,
        mainlines_bytes: created_mainlines.manifest_bytes,
    })
}

fn v2_record(
    source_blake3: String,
    source_bytes: u64,
    observations: Vec<SourceObservationRecordV1>,
    chain: ArtifactChain,
) -> CollectionSourceRecordV2 {
    CollectionSourceRecordV2 {
        id: 0,
        source_blake3,
        source_bytes,
        catalog: chain.catalog.path,
        catalog_blake3: chain.catalog.hash,
        catalog_bytes: chain.catalog.bytes,
        occurrences: chain.occurrences_path,
        occurrences_blake3: chain.occurrences_hash,
        occurrences_bytes: chain.occurrences_bytes,
        mainlines: chain.mainlines_path,
        mainlines_blake3: chain.mainlines_hash,
        mainlines_bytes: chain.mainlines_bytes,
        games: chain.catalog.games,
        observations,
    }
}

pub(in crate::inspector) fn run(
    root: &Path,
    input: &Path,
    previous: Option<&Path>,
) -> Result<IngestResult, String> {
    run_with_policy(root, input, previous, EmptySnapshotPolicy::Allow)
}

#[allow(dead_code)]
pub(in crate::inspector) fn run_nonempty_snapshot(
    root: &Path,
    input: &Path,
) -> Result<IngestResult, String> {
    run_with_policy(root, input, None, EmptySnapshotPolicy::Reject)
}

fn run_with_policy(
    root: &Path,
    input: &Path,
    previous: Option<&Path>,
    empty_snapshot: EmptySnapshotPolicy,
) -> Result<IngestResult, String> {
    let scan = super::scan::directory(input)?;
    if matches!(empty_snapshot, EmptySnapshotPolicy::Reject)
        && scan.pgn_files == 0
        && scan.failures.is_empty()
    {
        return Err("La directory sorgente non contiene file PGN".into());
    }
    let workspace = catalog_index(root)?;
    let prior = previous
        .map(|p| load_verified_collection(root, p))
        .transpose()?;
    let mut sources: BTreeMap<(String, u64), CollectionSourceRecordV2> = BTreeMap::new();
    if let Some(previous) = prior {
        match previous.manifest {
            CollectionManifest::V1(manifest) => {
                for source in manifest.sources {
                    let chain = complete_chain(
                        root,
                        Reusable {
                            path: source.catalog,
                            hash: source.catalog_blake3,
                            bytes: source.catalog_bytes,
                            games: source.games,
                        },
                    )?;
                    let key = (source.source_blake3.clone(), source.source_bytes);
                    sources.insert(
                        key,
                        v2_record(
                            source.source_blake3,
                            source.source_bytes,
                            source.observations,
                            chain,
                        ),
                    );
                }
            }
            CollectionManifest::V2(manifest) => {
                for source in manifest.sources {
                    sources.insert((source.source_blake3.clone(), source.source_bytes), source);
                }
            }
            CollectionManifest::V3(_) => {
                return Err(
                    "Collection v3 non può essere usata come --previous dal percorso ingest legacy"
                        .into(),
                )
            }
        }
    }
    let mut files = scan
        .failures
        .iter()
        .map(|failure| FileResult {
            relative: failure.relative.clone(),
            status: FileStatus::Failed,
            source: None,
            catalog: None,
            error_code: Some(failure.code.clone()),
            error_message: Some(failure.message.clone()),
        })
        .collect::<Vec<_>>();
    let mut failed = files.len() as u64;
    let mut seen_current = BTreeSet::new();
    for candidate in scan.candidates {
        // Object custody is the identity read; only a new pipeline publishes its manifest.
        // Later stages consume the stored bytes rather than reopening the mutable input path.
        let stored = match source::store_source(root, &candidate.path) {
            Ok(value) => value,
            Err(error) if error.contains("vuoto") => {
                files.push(FileResult {
                    relative: candidate.relative,
                    status: FileStatus::Failed,
                    source: None,
                    catalog: None,
                    error_code: Some("empty_source".into()),
                    error_message: Some("Il SourceBlob candidato è vuoto".into()),
                });
                failed += 1;
                continue;
            }
            Err(error) => {
                files.push(FileResult {
                    relative: candidate.relative,
                    status: FileStatus::Failed,
                    source: None,
                    catalog: None,
                    error_code: Some("source_read".into()),
                    error_message: Some(format!("Impossibile leggere il PGN candidato: {error}")),
                });
                failed += 1;
                continue;
            }
        };
        let hash = stored.blake3.clone();
        let size = stored.bytes;
        let key = (hash.clone(), size);
        if let Some(existing) = sources.get_mut(&key) {
            existing.observations.push(candidate.observation);
            existing.observations.sort();
            existing.observations.dedup();
            let status = if seen_current.contains(&key) {
                FileStatus::DuplicateSource
            } else {
                FileStatus::ReusedPrevious
            };
            seen_current.insert(key);
            files.push(FileResult {
                relative: candidate.relative,
                status,
                source: Some(hash),
                catalog: Some(existing.catalog.clone()),
                error_code: None,
                error_message: None,
            });
            continue;
        }
        let reuse = workspace.get(&key);
        let (chain_result, status) = if let Some(value) = reuse {
            (
                complete_chain(
                    root,
                    Reusable {
                        path: value.path.clone(),
                        hash: value.hash.clone(),
                        bytes: value.bytes,
                        games: value.games,
                    },
                ),
                FileStatus::ReusedWorkspace,
            )
        } else {
            (
                source::publish_stored_source(root, stored)
                    .and_then(|imported| catalog_pipeline(root, &imported.manifest_path))
                    .and_then(|catalog| complete_chain(root, catalog)),
                FileStatus::Ingested,
            )
        };
        let chain = match chain_result {
            Ok(value) => value,
            Err(error) => {
                files.push(FileResult {
                    relative: candidate.relative,
                    status: FileStatus::Failed,
                    source: Some(hash),
                    catalog: None,
                    error_code: Some("pipeline_failed".into()),
                    error_message: Some(format!("Pipeline Inspector fallita: {error}")),
                });
                failed += 1;
                continue;
            }
        };
        let catalog_path = chain.catalog.path.clone();
        let record = v2_record(hash.clone(), size, vec![candidate.observation], chain);
        sources.insert(key, record);
        seen_current.insert((hash.clone(), size));
        files.push(FileResult {
            relative: candidate.relative,
            status,
            source: Some(hash),
            catalog: Some(catalog_path),
            error_code: None,
            error_message: None,
        });
    }
    files.sort_by(|a, b| a.relative.cmp(&b.relative));
    let mut records = sources.into_values().collect::<Vec<_>>();
    for (id, source) in records.iter_mut().enumerate() {
        source.id = id as u64;
        source.observations.sort();
        source.observations.dedup();
    }
    let manifest = CollectionManifestV2 {
        schema_version: COLLECTION_SCHEMA_VERSION_V2,
        collection_profile: COLLECTION_PROFILE_V2.into(),
        sources: records,
    };
    let created = if failed > 0 && manifest.sources.is_empty() {
        None
    } else {
        Some(publish(root, CollectionManifest::V2(manifest))?)
    };
    if files.iter().any(|file| {
        matches!(file.status, FileStatus::Failed)
            && (file.error_code.as_deref().is_none_or(str::is_empty)
                || file.error_message.as_deref().is_none_or(str::is_empty))
    }) {
        return Err("Failure priva di codice o messaggio diagnostico".into());
    }
    if failed == 0 {
        let collection = created
            .as_ref()
            .ok_or("Ingest riuscito senza Collection pubblicata")?;
        super::workspace_state::update(root, collection)?;
    }
    Ok(IngestResult {
        created,
        files,
        scanned_files: scan.scanned_files,
        pgn_files: scan.pgn_files,
        skipped_symlinks: scan.skipped_symlinks,
        failed,
    })
}
