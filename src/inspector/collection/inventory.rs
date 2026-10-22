use super::{relative_manifest_path, CollectionManifest, SourceObservationRecordV1};
use crate::inspector::catalog::{self, QueryFilters};
use crate::inspector::{mainlines, VerifiedArtifactIdentityV1, VerifiedMainlineInputV1};
use serde::Serialize;
use std::fs;
use std::path::Path;

const INVENTORY_SCHEMA_VERSION: u32 = 1;
const INVENTORY_PROFILE: &str = "antiskill_archive_occurrence_inventory_v1";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(in crate::inspector) struct InventoryArtifactV1 {
    pub(in crate::inspector) path: String,
    pub(in crate::inspector) blake3: String,
    pub(in crate::inspector) bytes: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct InventoryArtifactsV1 {
    collection: InventoryArtifactV1,
    catalog: InventoryArtifactV1,
    occurrences: InventoryArtifactV1,
    mainlines: InventoryArtifactV1,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(in crate::inspector) struct ArchiveOccurrenceInventoryRecordV1 {
    schema_version: u32,
    inventory_profile: String,
    pub(in crate::inspector) source_id: u64,
    pub(in crate::inspector) game_id: u64,
    source_blake3: String,
    source_bytes: u64,
    pub(in crate::inspector) occurrence_id: String,
    source_game_blake3: String,
    game_bytes: u64,
    pub(in crate::inspector) canonical_mainline_id: Option<String>,
    comparable: bool,
    noncomparability_reasons: Vec<String>,
    results: Vec<String>,
    artifacts: InventoryArtifactsV1,
    observations: Vec<SourceObservationRecordV1>,
}

pub(in crate::inspector) struct VerifiedInventoryV1 {
    pub(in crate::inspector) collection: InventoryArtifactV1,
    pub(in crate::inspector) records: Vec<ArchiveOccurrenceInventoryRecordV1>,
}

pub(in crate::inspector) fn inventory_collection(
    root: &Path,
    path: &Path,
    filters: &QueryFilters,
) -> Result<Vec<u8>, String> {
    let verified = load_verified(root, path, filters)?;
    serialize_inventory_records(&verified)
}

pub(in crate::inspector) fn serialize_inventory_records(
    verified: &VerifiedInventoryV1,
) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    for record in &verified.records {
        serde_json::to_writer(&mut output, record)
            .map_err(|error| format!("Serializzazione archive occurrence inventory: {error}"))?;
        output.push(b'\n');
    }
    Ok(output)
}

pub(in crate::inspector) fn load_verified(
    root: &Path,
    path: &Path,
    filters: &QueryFilters,
) -> Result<VerifiedInventoryV1, String> {
    catalog::query::validate_filters(filters)?;
    let loaded = super::load_verified_collection(root, path)?;
    let CollectionManifest::V2(manifest) = &loaded.manifest else {
        return Err("Collection v1 non supporta archive occurrence inventory v1".into());
    };
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let collection_path =
        relative_manifest_path(&canonical_root, &loaded.canonical_path, "collection")?;
    let collection_artifact = InventoryArtifactV1 {
        path: collection_path,
        blake3: loaded.manifest_blake3.clone(),
        bytes: loaded.manifest_bytes,
    };

    if manifest.sources.len() != loaded.catalogs.len() {
        return Err("Collection e Catalog verificati hanno cardinalità diversa".into());
    }

    let mut records = Vec::new();
    for (source_index, (source, catalog)) in
        manifest.sources.iter().zip(&loaded.catalogs).enumerate()
    {
        let source_id = u64::try_from(source_index).map_err(|_| "source_id oltre u64")?;
        if source.id != source_id {
            return Err("source_id Collection non allineato".into());
        }

        let projected = mainlines::load_verified_for_dedup(root, Path::new(&source.mainlines))?;
        if projected.artifact.blake3 != source.mainlines_blake3
            || projected.artifact.bytes != source.mainlines_bytes
        {
            return Err("Mainlines diverse dal record Collection".into());
        }
        if projected.records.len() != catalog.manifest.games.len()
            || u64::try_from(projected.records.len()).map_err(|_| "Numero record oltre u64")?
                != source.games
        {
            return Err("Inventory, Catalog e Collection hanno cardinalità diversa".into());
        }

        for (game_index, (game, projected_mainline)) in catalog
            .manifest
            .games
            .iter()
            .zip(&projected.records)
            .enumerate()
        {
            let game_id = u64::try_from(game_index).map_err(|_| "game_id oltre u64")?;
            validate_alignment(source, game, projected_mainline, game_id)?;
            if !catalog::query::matches_game(game, filters) {
                continue;
            }
            records.push(ArchiveOccurrenceInventoryRecordV1 {
                schema_version: INVENTORY_SCHEMA_VERSION,
                inventory_profile: INVENTORY_PROFILE.into(),
                source_id,
                game_id,
                source_blake3: projected_mainline.source_blake3.clone(),
                source_bytes: projected_mainline.source_bytes,
                occurrence_id: projected_mainline.occurrence_id.clone(),
                source_game_blake3: projected_mainline.source_game_blake3.clone(),
                game_bytes: projected_mainline.game_bytes,
                canonical_mainline_id: projected_mainline.canonical_mainline_id.clone(),
                comparable: projected_mainline.comparable,
                noncomparability_reasons: projected_mainline.noncomparability_reasons.clone(),
                results: projected_mainline.results.clone(),
                artifacts: InventoryArtifactsV1 {
                    collection: collection_artifact.clone(),
                    catalog: artifact(
                        &source.catalog,
                        &source.catalog_blake3,
                        source.catalog_bytes,
                    ),
                    occurrences: artifact(
                        &source.occurrences,
                        &source.occurrences_blake3,
                        source.occurrences_bytes,
                    ),
                    mainlines: artifact(
                        &source.mainlines,
                        &source.mainlines_blake3,
                        source.mainlines_bytes,
                    ),
                },
                observations: source.observations.clone(),
            });
        }
    }

    records.sort_by_key(|record| (record.source_id, record.game_id));
    Ok(VerifiedInventoryV1 {
        collection: collection_artifact,
        records,
    })
}

fn validate_alignment(
    source: &super::CollectionSourceRecordV2,
    game: &crate::inspector::catalog::manifest::CatalogGameRecordV1,
    projected: &VerifiedMainlineInputV1,
    expected_game_id: u64,
) -> Result<(), String> {
    if game.id != expected_game_id {
        return Err("Catalog game id non allineato all'ordinal atteso".into());
    }
    if projected.source_blake3 != source.source_blake3
        || projected.source_bytes != source.source_bytes
    {
        return Err("Identità SourceBlob inventory diversa dalla Collection".into());
    }
    if projected.source_game_blake3 != game.source_game_blake3 {
        return Err("source_game_blake3 inventory diverso dal Catalog".into());
    }
    let game_bytes = game
        .source_end
        .checked_sub(game.source_start)
        .ok_or("Intervallo Catalog invertito")?;
    if projected.game_bytes != game_bytes {
        return Err("game_bytes inventory diverso dall'intervallo Catalog".into());
    }
    if projected.provenance.catalog_manifest
        != (VerifiedArtifactIdentityV1 {
            blake3: source.catalog_blake3.clone(),
            bytes: source.catalog_bytes,
        })
        || projected.provenance.occurrences_manifest
            != (VerifiedArtifactIdentityV1 {
                blake3: source.occurrences_blake3.clone(),
                bytes: source.occurrences_bytes,
            })
    {
        return Err("Provenance inventory diversa dalla Collection".into());
    }
    Ok(())
}

fn artifact(path: &str, blake3: &str, bytes: u64) -> InventoryArtifactV1 {
    InventoryArtifactV1 {
        path: path.into(),
        blake3: blake3.into(),
        bytes,
    }
}
