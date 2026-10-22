use super::manifest::{
    CollectionManifest, CollectionManifestV3, CollectionSourceRecordV3, COLLECTION_PROFILE_V3,
    COLLECTION_SCHEMA_VERSION_V3,
};
use super::publish;
use crate::inspector::{source, source_index};
use std::collections::BTreeMap;
use std::path::Path;

use super::ingest::{FileResult, FileStatus, IngestResult};

fn reusable_active_sources(
    root: &Path,
) -> Result<BTreeMap<(String, u64), CollectionSourceRecordV3>, String> {
    let Some((path, expected_hash, expected_bytes)) =
        super::workspace_state::active_reference_if_present(root)?
    else {
        return Ok(BTreeMap::new());
    };
    let common = super::load_common(root, &path)?;
    if common.hash != expected_hash || common.size != expected_bytes {
        return Err("Active Collection v3 diversa dallo stato workspace".into());
    }
    let CollectionManifest::V3(manifest) = common.manifest else {
        return Ok(BTreeMap::new());
    };
    super::validation::validate_v3(&manifest)?;
    let mut reusable = BTreeMap::new();
    for source in manifest.sources {
        reusable.insert((source.source_blake3.clone(), source.source_bytes), source);
    }
    Ok(reusable)
}

pub(in crate::inspector) fn run_product(root: &Path, input: &Path) -> Result<IngestResult, String> {
    let scan = super::scan::directory(input)?;
    if scan.pgn_files == 0 && scan.failures.is_empty() {
        return Err("La directory sorgente non contiene file PGN".into());
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
    let reusable = reusable_active_sources(root)?;
    let mut sources: BTreeMap<(String, u64), CollectionSourceRecordV3> = BTreeMap::new();

    for candidate in scan.candidates {
        let stored = match source::store_source(root, &candidate.path) {
            Ok(value) => value,
            Err(error) => {
                files.push(FileResult {
                    relative: candidate.relative,
                    status: FileStatus::Failed,
                    source: None,
                    catalog: None,
                    error_code: Some(
                        if error.contains("vuoto") {
                            "empty_source"
                        } else {
                            "source_read"
                        }
                        .into(),
                    ),
                    error_message: Some(error),
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
            files.push(FileResult {
                relative: candidate.relative,
                status: FileStatus::DuplicateSource,
                source: Some(hash),
                catalog: None,
                error_code: None,
                error_message: None,
            });
            continue;
        }
        let (record, status) = if let Some(existing) = reusable.get(&key) {
            let games = source_index::for_each_verified_record(
                root,
                Path::new(&existing.source_index),
                &existing.source_index_blake3,
                existing.source_index_bytes,
                &hash,
                size,
                |_record, _blob| Ok(()),
            )?;
            if games != existing.games {
                return Err("SourceIndex riusato con cardinalità incoerente".into());
            }
            let mut record = existing.clone();
            record.id = 0;
            record.observations = vec![candidate.observation];
            (record, FileStatus::ReusedWorkspace)
        } else {
            let created = match source_index::create(root, stored.object_path(), &hash, size) {
                Ok(value) => value,
                Err(error) => {
                    files.push(FileResult {
                        relative: candidate.relative,
                        status: FileStatus::Failed,
                        source: Some(hash),
                        catalog: None,
                        error_code: Some("source_index_failed".into()),
                        error_message: Some(format!("SourceIndex fused fallito: {error}")),
                    });
                    failed += 1;
                    continue;
                }
            };
            (
                CollectionSourceRecordV3 {
                    id: 0,
                    source_blake3: hash.clone(),
                    source_bytes: size,
                    source_index: crate::inspector::common::path_to_manifest_string(&created.path),
                    source_index_blake3: created.blake3,
                    source_index_bytes: created.bytes,
                    games: created.games,
                    observations: vec![candidate.observation],
                },
                FileStatus::Ingested,
            )
        };
        sources.insert(key, record);
        files.push(FileResult {
            relative: candidate.relative,
            status,
            source: Some(hash),
            catalog: None,
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
    let manifest = CollectionManifestV3 {
        schema_version: COLLECTION_SCHEMA_VERSION_V3,
        collection_profile: COLLECTION_PROFILE_V3.into(),
        sources: records,
    };
    let created = if failed > 0 && manifest.sources.is_empty() {
        None
    } else {
        Some(publish(root, CollectionManifest::V3(manifest))?)
    };
    if failed == 0 {
        let collection = created
            .as_ref()
            .ok_or("Ingest v3 riuscito senza Collection")?;
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
