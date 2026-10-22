mod compare;
mod duplicates;
mod ingest;
mod ingest_v3;
mod inventory;
mod manifest;
mod query;
mod scan;
#[cfg(test)]
mod tests;
mod validation;
mod workspace_state;

use crate::inspector::catalog;
use crate::inspector::common::{
    canonical_artifact_path, ensure_within, logical_json_bytes, path_to_manifest_string,
    prepare_directory_chain, publish_compressed_json, resolve_workspace_path,
    validate_relative_path,
};
use crate::inspector::{mainlines, occurrences, source_index, VerifiedMainlineArtifactInputV1};
use manifest::*;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) struct CreatedCollection {
    pub(super) path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) new_manifest: bool,
    pub(super) manifest: CollectionManifest,
}

pub(super) struct LoadedCollection {
    pub(super) manifest: CollectionManifest,
    pub(super) catalogs: Vec<catalog::VerifiedCatalog>,
    pub(super) occurrence_identities: Option<Vec<Vec<occurrences::VerifiedOccurrenceIdentityV1>>>,
    pub(super) canonical_path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
}

pub(super) struct VerifiedCollectionSummary {
    pub(super) canonical_path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) source_count: u64,
    pub(super) games: u64,
}

struct LoadedCollectionCommon {
    canonical_root: PathBuf,
    canonical_path: PathBuf,
    bytes: Vec<u8>,
    hash: String,
    size: u64,
    manifest: CollectionManifest,
}

#[derive(Clone, Copy)]
struct CollectionSourceView<'a> {
    id: u64,
    source_blake3: &'a str,
    catalog: &'a str,
    observations: &'a [SourceObservationRecordV1],
}

impl LoadedCollection {
    fn sources(&self) -> Vec<CollectionSourceView<'_>> {
        match &self.manifest {
            CollectionManifest::V1(m) => m
                .sources
                .iter()
                .map(|source| CollectionSourceView {
                    id: source.id,
                    source_blake3: &source.source_blake3,
                    catalog: &source.catalog,
                    observations: &source.observations,
                })
                .collect(),
            CollectionManifest::V2(m) => m
                .sources
                .iter()
                .map(|source| CollectionSourceView {
                    id: source.id,
                    source_blake3: &source.source_blake3,
                    catalog: &source.catalog,
                    observations: &source.observations,
                })
                .collect(),
            CollectionManifest::V3(_) => Vec::new(),
        }
    }
}

fn serialized_v1(m: &CollectionManifestV1) -> Result<Vec<u8>, String> {
    let mut bytes =
        serde_json::to_vec_pretty(m).map_err(|e| format!("Serializzazione Collection: {e}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn serialized_v2(m: &CollectionManifestV2) -> Result<Vec<u8>, String> {
    let mut bytes =
        serde_json::to_vec_pretty(m).map_err(|e| format!("Serializzazione Collection: {e}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn serialized_v3(m: &CollectionManifestV3) -> Result<Vec<u8>, String> {
    let mut bytes =
        serde_json::to_vec_pretty(m).map_err(|e| format!("Serializzazione Collection: {e}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn serialized(m: &CollectionManifest) -> Result<Vec<u8>, String> {
    match m {
        CollectionManifest::V1(m) => serialized_v1(m),
        CollectionManifest::V2(m) => serialized_v2(m),
        CollectionManifest::V3(m) => serialized_v3(m),
    }
}

pub(in crate::inspector) fn publish(
    root: &Path,
    manifest: CollectionManifest,
) -> Result<CreatedCollection, String> {
    validation::validate_manifest(&manifest)?;
    let bytes = serialized(&manifest)?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let size = bytes.len() as u64;
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let directory = prepare_directory_chain(
        root,
        &canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("collections", "directory collections"),
        ],
    )?;
    let (target, new_manifest) =
        publish_compressed_json(&canonical_root, &directory, &hash, &bytes, "Collection")?;
    Ok(CreatedCollection {
        path: PathBuf::from("inspector/collections").join(target.file_name().unwrap()),
        manifest_blake3: hash,
        manifest_bytes: size,
        new_manifest,
        manifest,
    })
}

fn load_common(root: &Path, path: &Path) -> Result<LoadedCollectionCommon, String> {
    let value = path.to_str().ok_or("Percorso Collection non UTF-8")?;
    validate_relative_path(value, "collection")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let metadata =
        fs::symlink_metadata(&resolved).map_err(|e| format!("Collection non valida: {e}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Collection non è un file regolare reale".into());
    }
    let canonical =
        fs::canonicalize(&resolved).map_err(|e| format!("Collection non valida: {e}"))?;
    ensure_within(&canonical_root, &canonical, "collection")?;
    let bytes = logical_json_bytes(&canonical, "Collection")?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let size = bytes.len() as u64;
    let expected_path = canonical_artifact_path("collections", &hash, &canonical)?;
    let actual = path_to_manifest_string(
        canonical
            .strip_prefix(&canonical_root)
            .map_err(|_| "Collection fuori workspace")?,
    );
    if actual != expected_path {
        return Err("Percorso Collection non canonico".into());
    }
    let manifest = validation::detect_profile(&bytes)?;
    Ok(LoadedCollectionCommon {
        canonical_root,
        canonical_path: canonical,
        bytes,
        hash,
        size,
        manifest,
    })
}

fn validate_v3_common(
    common: &LoadedCollectionCommon,
    manifest: &CollectionManifestV3,
) -> Result<(), String> {
    validation::validate_v3(manifest)?;
    let canonical = serialized_v3(manifest)?;
    if canonical != common.bytes
        || blake3::hash(&canonical).to_hex().as_str() != common.hash
        || canonical.len() as u64 != common.size
    {
        return Err("Collection diversa dal ricalcolo byte-esatto".into());
    }
    Ok(())
}

fn load_verified_v1(
    root: &Path,
    canonical_root: &Path,
    canonical_path: PathBuf,
    bytes: &[u8],
    hash: &str,
    size: u64,
    manifest: CollectionManifestV1,
) -> Result<LoadedCollection, String> {
    validation::validate(&manifest)?;
    let mut catalogs = Vec::new();
    let mut rebuilt = manifest.clone();
    for (record, expected) in manifest.sources.iter().zip(rebuilt.sources.iter_mut()) {
        let verified = catalog::load_verified_catalog(root, Path::new(&record.catalog))?;
        let verified_relative = verified
            .canonical_path
            .strip_prefix(canonical_root)
            .map_err(|_| "Catalog verificato fuori workspace")?;
        let verified_path = verified_relative
            .to_str()
            .ok_or("Percorso Catalog verificato non UTF-8")?
            .replace(std::path::MAIN_SEPARATOR, "/");
        if verified_path != record.catalog {
            return Err("Percorso Catalog diverso dalla Collection".into());
        }
        if u64::try_from(verified.bytes.len()).map_err(|_| "Catalog oltre u64")?
            != verified.manifest_bytes
            || blake3::hash(&verified.bytes).to_hex().as_str() != verified.manifest_blake3
        {
            return Err("Identità runtime VerifiedCatalog incoerente".into());
        }
        if verified.manifest_blake3 != record.catalog_blake3
            || verified.manifest_bytes != record.catalog_bytes
        {
            return Err("Identità Catalog diversa dalla Collection".into());
        }
        if verified.manifest.source_blake3 != record.source_blake3
            || verified.manifest.source_bytes != record.source_bytes
            || verified.manifest.games.len() as u64 != record.games
        {
            return Err("Catalog incoerente con source record".into());
        }
        expected.catalog_blake3 = verified.manifest_blake3.clone();
        expected.catalog_bytes = verified.manifest_bytes;
        expected.games = verified.manifest.games.len() as u64;
        catalogs.push(verified);
    }
    let canonical_bytes = serialized_v1(&rebuilt)?;
    if rebuilt != manifest
        || canonical_bytes != bytes
        || blake3::hash(&canonical_bytes).to_hex().as_str() != hash
        || canonical_bytes.len() as u64 != size
    {
        return Err("Collection diversa dal ricalcolo byte-esatto".into());
    }
    Ok(LoadedCollection {
        manifest: CollectionManifest::V1(manifest),
        catalogs,
        occurrence_identities: None,
        canonical_path,
        manifest_blake3: hash.to_owned(),
        manifest_bytes: size,
    })
}

fn load_verified_v2(
    root: &Path,
    canonical_root: &Path,
    canonical_path: PathBuf,
    bytes: &[u8],
    hash: &str,
    size: u64,
    manifest: CollectionManifestV2,
) -> Result<LoadedCollection, String> {
    #[cfg(test)]
    MATERIALIZED_V2_LOADS.with(|loads| loads.set(loads.get() + 1));
    validation::validate_v2(&manifest)?;
    let mut catalogs = Vec::new();
    let mut occurrence_identities = Vec::new();
    let mut rebuilt = manifest.clone();
    for (record, expected) in manifest.sources.iter().zip(rebuilt.sources.iter_mut()) {
        let verified_mainlines =
            mainlines::load_verified_collection_chain(root, Path::new(&record.mainlines))?;
        let mainlines_path = relative_manifest_path(
            canonical_root,
            &verified_mainlines.canonical_path,
            "mainlines",
        )?;
        if verified_mainlines.manifest_blake3 != record.mainlines_blake3
            || verified_mainlines.manifest_bytes != record.mainlines_bytes
        {
            return Err("Identità Mainlines diversa dalla Collection".into());
        }
        if mainlines_path != record.mainlines {
            return Err("Percorso Mainlines diverso dalla Collection".into());
        }
        let verified_occurrences = &verified_mainlines.occurrences;
        let occurrences_path = relative_manifest_path(
            canonical_root,
            &verified_occurrences.canonical_path,
            "occurrences",
        )?;
        if verified_occurrences.manifest_blake3 != record.occurrences_blake3
            || verified_occurrences.manifest_bytes != record.occurrences_bytes
        {
            return Err("Identità Occurrences diversa dalla Collection".into());
        }
        if occurrences_path != record.occurrences {
            return Err("Percorso Occurrences diverso dalla Collection".into());
        }
        let verified_catalog = &verified_occurrences.catalog;
        let catalog_path =
            relative_manifest_path(canonical_root, &verified_catalog.canonical_path, "catalog")?;
        if catalog_path != record.catalog {
            return Err("Percorso Catalog diverso dalla Collection".into());
        }
        if verified_catalog.manifest_blake3 != record.catalog_blake3
            || verified_catalog.manifest_bytes != record.catalog_bytes
        {
            return Err("Identità Catalog diversa dalla Collection".into());
        }
        if verified_catalog.manifest.source_blake3 != record.source_blake3
            || verified_catalog.manifest.source_bytes != record.source_bytes
            || verified_catalog.manifest.games.len() as u64 != record.games
        {
            return Err("Catena artefatti incoerente con source record".into());
        }
        if verified_occurrences.manifest.source_blake3 != record.source_blake3
            || verified_occurrences.manifest.source_bytes != record.source_bytes
        {
            return Err("Occurrences appartenenti ad altra source".into());
        }
        let identities = occurrences::verified_identity_projection(verified_occurrences)?;
        validate_occurrence_alignment(record, verified_catalog, &identities)?;
        expected.catalog_blake3 = verified_catalog.manifest_blake3.clone();
        expected.catalog_bytes = verified_catalog.manifest_bytes;
        expected.occurrences_blake3 = verified_occurrences.manifest_blake3.clone();
        expected.occurrences_bytes = verified_occurrences.manifest_bytes;
        expected.mainlines_blake3 = verified_mainlines.manifest_blake3.clone();
        expected.mainlines_bytes = verified_mainlines.manifest_bytes;
        expected.games = verified_catalog.manifest.games.len() as u64;
        occurrence_identities.push(identities);
        catalogs.push(verified_mainlines.occurrences.catalog);
    }
    let canonical_bytes = serialized_v2(&rebuilt)?;
    if rebuilt != manifest
        || canonical_bytes != bytes
        || blake3::hash(&canonical_bytes).to_hex().as_str() != hash
        || canonical_bytes.len() as u64 != size
    {
        return Err("Collection diversa dal ricalcolo byte-esatto".into());
    }
    Ok(LoadedCollection {
        manifest: CollectionManifest::V2(manifest),
        catalogs,
        occurrence_identities: Some(occurrence_identities),
        canonical_path,
        manifest_blake3: hash.to_owned(),
        manifest_bytes: size,
    })
}

fn validate_occurrence_alignment(
    source: &CollectionSourceRecordV2,
    catalog: &catalog::VerifiedCatalog,
    identities: &[occurrences::VerifiedOccurrenceIdentityV1],
) -> Result<(), String> {
    if identities.len() != catalog.manifest.games.len()
        || u64::try_from(identities.len()).map_err(|_| "Numero occurrence oltre u64")?
            != source.games
    {
        return Err("Occurrences, Catalog e Collection hanno cardinalità diversa".into());
    }
    for (game_index, (identity, game)) in identities.iter().zip(&catalog.manifest.games).enumerate()
    {
        let game_id = u64::try_from(game_index).map_err(|_| "game_id oltre u64")?;
        let game_bytes = game
            .source_end
            .checked_sub(game.source_start)
            .ok_or("Intervallo Catalog invertito")?;
        if identity.ordinal != game_id
            || game.id != game_id
            || identity.source_blake3 != source.source_blake3
            || identity.source_bytes != source.source_bytes
            || identity.source_game_blake3 != game.source_game_blake3
            || identity.game_bytes != game_bytes
        {
            return Err("Occurrence, Catalog e Collection non allineati".into());
        }
    }
    Ok(())
}

pub(super) fn load_verified_collection(
    root: &Path,
    path: &Path,
) -> Result<LoadedCollection, String> {
    let LoadedCollectionCommon {
        canonical_root,
        canonical_path,
        bytes,
        hash,
        size,
        manifest,
    } = load_common(root, path)?;
    match manifest {
        CollectionManifest::V1(m) => load_verified_v1(
            root,
            &canonical_root,
            canonical_path,
            &bytes,
            &hash,
            size,
            m,
        ),
        CollectionManifest::V2(m) => load_verified_v2(
            root,
            &canonical_root,
            canonical_path,
            &bytes,
            &hash,
            size,
            m,
        ),
        CollectionManifest::V3(_) => {
            Err("Collection v3 fused non materializzabile tramite API legacy".into())
        }
    }
}

pub(in crate::inspector) fn load_verified_mainlines_for_dedup_from_collection(
    root: &Path,
    path: &Path,
) -> Result<Vec<VerifiedMainlineArtifactInputV1>, String> {
    let loaded = load_verified_collection(root, path)?;
    let CollectionManifest::V2(manifest) = loaded.manifest else {
        return Err("Collection v1 non supporta proiezione mainline per Dedup".into());
    };
    let mut inputs = Vec::with_capacity(manifest.sources.len());
    for record in &manifest.sources {
        let input = mainlines::load_verified_for_dedup(root, Path::new(&record.mainlines))?;
        if input.artifact.blake3 != record.mainlines_blake3
            || input.artifact.bytes != record.mainlines_bytes
        {
            return Err("Proiezione mainline incoerente con Collection".into());
        }
        inputs.push(input);
    }
    Ok(inputs)
}

pub(super) fn verify(root: &Path, path: &Path) -> Result<(), String> {
    load_verified_summary(root, path).map(|_| ())
}

pub(super) fn load_verified_summary(
    root: &Path,
    path: &Path,
) -> Result<VerifiedCollectionSummary, String> {
    let common = load_common(root, path)?;
    match &common.manifest {
        CollectionManifest::V1(_) => {
            drop(common);
            let loaded = load_verified_collection(root, path)?;
            let source_count = u64::try_from(loaded.manifest.source_count())
                .map_err(|_| "Numero sorgenti oltre u64")?;
            Ok(VerifiedCollectionSummary {
                canonical_path: loaded.canonical_path,
                manifest_blake3: loaded.manifest_blake3,
                manifest_bytes: loaded.manifest_bytes,
                source_count,
                games: loaded.manifest.games(),
            })
        }
        CollectionManifest::V2(manifest) => {
            validation::validate_v2(manifest)?;
            let canonical_bytes = serialized_v2(manifest)?;
            if canonical_bytes != common.bytes
                || blake3::hash(&canonical_bytes).to_hex().as_str() != common.hash
                || canonical_bytes.len() as u64 != common.size
            {
                return Err("Collection diversa dal ricalcolo byte-esatto".into());
            }
            for source in &manifest.sources {
                verify_v2_source(root, &common.canonical_root, source)?;
            }
            let source_count =
                u64::try_from(manifest.sources.len()).map_err(|_| "Numero sorgenti oltre u64")?;
            let games = manifest.sources.iter().try_fold(0_u64, |total, source| {
                total
                    .checked_add(source.games)
                    .ok_or("Numero partite oltre u64")
            })?;
            Ok(VerifiedCollectionSummary {
                canonical_path: common.canonical_path,
                manifest_blake3: common.hash,
                manifest_bytes: common.size,
                source_count,
                games,
            })
        }
        CollectionManifest::V3(manifest) => {
            validation::validate_v3(manifest)?;
            let canonical_bytes = serialized_v3(manifest)?;
            if canonical_bytes != common.bytes
                || blake3::hash(&canonical_bytes).to_hex().as_str() != common.hash
                || canonical_bytes.len() as u64 != common.size
            {
                return Err("Collection diversa dal ricalcolo byte-esatto".into());
            }
            let mut games = 0_u64;
            for source in &manifest.sources {
                let count = source_index::for_each_verified_record(
                    root,
                    Path::new(&source.source_index),
                    &source.source_index_blake3,
                    source.source_index_bytes,
                    &source.source_blake3,
                    source.source_bytes,
                    |_record, _blob| Ok(()),
                )?;
                if count != source.games {
                    return Err("SourceIndex e Collection hanno cardinalità diversa".into());
                }
                games = games.checked_add(count).ok_or("Numero partite oltre u64")?;
            }
            Ok(VerifiedCollectionSummary {
                canonical_path: common.canonical_path,
                manifest_blake3: common.hash,
                manifest_bytes: common.size,
                source_count: manifest.sources.len() as u64,
                games,
            })
        }
    }
}

#[cfg(test)]
thread_local! {
    static MATERIALIZED_V2_LOADS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn reset_materialized_v2_loads() {
    MATERIALIZED_V2_LOADS.with(|loads| loads.set(0));
}

#[cfg(test)]
fn materialized_v2_loads() -> usize {
    MATERIALIZED_V2_LOADS.with(std::cell::Cell::get)
}

fn load_v2_source_chain(
    root: &Path,
    canonical_root: &Path,
    record: &CollectionSourceRecordV2,
) -> Result<mainlines::LoadedStreamingMainlines, String> {
    let loaded_mainlines =
        mainlines::load_streaming_mainlines(root, Path::new(&record.mainlines), None)?;
    let occurrences = &loaded_mainlines.occurrences;
    let catalog = &occurrences.catalog;

    let mainlines_path = relative_manifest_path(
        canonical_root,
        &loaded_mainlines.canonical_path,
        "mainlines",
    )?;
    if mainlines_path != record.mainlines {
        return Err("Percorso Mainlines diverso dalla Collection".into());
    }
    if loaded_mainlines.manifest_blake3 != record.mainlines_blake3
        || loaded_mainlines.manifest_bytes != record.mainlines_bytes
    {
        return Err("Identità Mainlines diversa dalla Collection".into());
    }

    let occurrences_path =
        relative_manifest_path(canonical_root, &occurrences.canonical_path, "occurrences")?;
    if occurrences_path != record.occurrences {
        return Err("Percorso Occurrences diverso dalla Collection".into());
    }
    if occurrences.manifest_blake3 != record.occurrences_blake3
        || occurrences.manifest_bytes != record.occurrences_bytes
    {
        return Err("Identità Occurrences diversa dalla Collection".into());
    }

    let catalog_path = relative_manifest_path(canonical_root, &catalog.canonical_path, "catalog")?;
    if catalog_path != record.catalog {
        return Err("Percorso Catalog diverso dalla Collection".into());
    }
    if catalog.manifest_blake3 != record.catalog_blake3
        || catalog.manifest_bytes != record.catalog_bytes
    {
        return Err("Identità Catalog diversa dalla Collection".into());
    }
    if catalog.header.source_blake3 != record.source_blake3
        || catalog.header.source_bytes != record.source_bytes
    {
        return Err("Catena artefatti incoerente con source record".into());
    }
    Ok(loaded_mainlines)
}

fn verify_v2_source(
    root: &Path,
    canonical_root: &Path,
    record: &CollectionSourceRecordV2,
) -> Result<(), String> {
    let loaded_mainlines = load_v2_source_chain(root, canonical_root, record)?;
    // Mainline verification consumes and byte-verifies Occurrences, which in turn consumes
    // and byte-verifies Catalog. One fused replay validates the whole game chain.
    let mainlines_count = u64::try_from(loaded_mainlines.for_each_verified_mainline(|_| Ok(()))?)
        .map_err(|_| "Numero mainline oltre u64")?;
    if mainlines_count != record.games {
        return Err("Occurrences, Catalog e Collection hanno cardinalità diversa".into());
    }
    Ok(())
}

pub(in crate::inspector) fn for_each_verified_logical_game_row<F>(
    root: &Path,
    path: &Path,
    mut on_row: F,
) -> Result<crate::inspector::VerifiedLogicalGameScanV1, String>
where
    F: FnMut(crate::inspector::ProductLogicalGameRowV1) -> Result<(), String>,
{
    let common = load_common(root, path)?;
    let mut total = 0_u64;
    match &common.manifest {
        CollectionManifest::V1(_) => {
            return Err(
                "La Collection attiva legacy v1 non supporta la vista partite logiche".into(),
            );
        }
        CollectionManifest::V2(manifest) => {
            validation::validate_v2(manifest)?;
            let canonical_bytes = serialized_v2(manifest)?;
            if canonical_bytes != common.bytes
                || blake3::hash(&canonical_bytes).to_hex().as_str() != common.hash
                || canonical_bytes.len() as u64 != common.size
            {
                return Err("Collection diversa dal ricalcolo byte-esatto".into());
            }
            for record in &manifest.sources {
                let loaded_mainlines = load_v2_source_chain(root, &common.canonical_root, record)?;
                let count = u64::try_from(
                    loaded_mainlines.for_each_verified_product_logical_game_row(&mut on_row)?,
                )
                .map_err(|_| "Numero mainline oltre u64")?;
                if count != record.games {
                    return Err(
                        "Occurrences, CanonicalMainline e Collection hanno cardinalità diversa"
                            .into(),
                    );
                }
                total = total.checked_add(count).ok_or("Numero partite oltre u64")?;
            }
        }
        CollectionManifest::V3(manifest) => {
            validate_v3_common(&common, manifest)?;
            for source in &manifest.sources {
                let count = source_index::for_each_verified_record(
                    root,
                    Path::new(&source.source_index),
                    &source.source_index_blake3,
                    source.source_index_bytes,
                    &source.source_blake3,
                    source.source_bytes,
                    |record, blob| {
                        let (game_hash, occurrence_id) = occurrences::probe_occurrence_identity(
                            &source.source_blake3,
                            source.source_bytes,
                            record.source_start,
                            record.source_end,
                            blob,
                        );
                        if game_hash != record.source_game_blake3 {
                            return Err("SourceIndex e occurrence non allineati".into());
                        }
                        let representative_quality =
                            source_index::representative_quality_from_blob(blob);
                        on_row(crate::inspector::ProductLogicalGameRowV1 {
                            occurrence_id,
                            canonical_mainline_id: record.canonical_mainline_id.clone(),
                            comparable: record.comparable,
                            representative_quality,
                        })
                    },
                )?;
                if count != source.games {
                    return Err("SourceIndex e Collection hanno cardinalità diversa".into());
                }
                total = total.checked_add(count).ok_or("Numero partite oltre u64")?;
            }
        }
    }
    Ok(crate::inspector::VerifiedLogicalGameScanV1 {
        collection_blake3: common.hash,
        collection_bytes: common.size,
        rows: total,
    })
}

pub(in crate::inspector) fn for_each_verified_player_side_evidence<F>(
    root: &Path,
    path: &Path,
    mut on_side: F,
) -> Result<crate::inspector::VerifiedPlayerSideScanV1, String>
where
    F: FnMut(crate::inspector::ProductPlayerSideEvidenceV1) -> Result<(), String>,
{
    let common = load_common(root, path)?;
    let CollectionManifest::V3(manifest) = &common.manifest else {
        return Err("La vista identità giocatori v1 richiede una Collection v3".into());
    };
    validate_v3_common(&common, manifest)?;
    let mut total = 0_u64;
    for source in &manifest.sources {
        let count = source_index::for_each_verified_record(
            root,
            Path::new(&source.source_index),
            &source.source_index_blake3,
            source.source_index_bytes,
            &source.source_blake3,
            source.source_bytes,
            |record, blob| {
                let (game_hash, occurrence_id) = occurrences::probe_occurrence_identity(
                    &source.source_blake3,
                    source.source_bytes,
                    record.source_start,
                    record.source_end,
                    blob,
                );
                if game_hash != record.source_game_blake3 {
                    return Err("SourceIndex e occurrence non allineati".into());
                }
                for evidence in source_index::player_header_evidence_from_blob(blob) {
                    if evidence.names.is_empty() && evidence.fide_id_values.is_empty() {
                        continue;
                    }
                    on_side(crate::inspector::ProductPlayerSideEvidenceV1 {
                        occurrence_id: occurrence_id.clone(),
                        side: evidence.side,
                        names: evidence.names,
                        fide_id_values: evidence.fide_id_values,
                        source_blake3: source.source_blake3.clone(),
                        source_bytes: source.source_bytes,
                        source_start: record.source_start,
                        source_end: record.source_end,
                        ordinal: record.ordinal,
                    })?;
                }
                Ok(())
            },
        )?;
        if count != source.games {
            return Err("SourceIndex e Collection hanno cardinalità diversa".into());
        }
        total = total.checked_add(count).ok_or("Numero partite oltre u64")?;
    }
    Ok(crate::inspector::VerifiedPlayerSideScanV1 {
        collection_blake3: common.hash,
        collection_bytes: common.size,
        source_occurrences: total,
    })
}

pub(in crate::inspector) fn for_each_verified_context_occurrence<F>(
    root: &Path,
    path: &Path,
    mut on_game: F,
) -> Result<crate::inspector::VerifiedPlayerSideScanV1, String>
where
    F: FnMut(crate::inspector::ProductContextOccurrenceV1) -> Result<(), String>,
{
    let common = load_common(root, path)?;
    let CollectionManifest::V3(manifest) = &common.manifest else {
        return Err("La vista contesto giocatori richiede una Collection v3".into());
    };
    validate_v3_common(&common, manifest)?;
    let mut total = 0_u64;
    for source in &manifest.sources {
        let count = source_index::for_each_verified_record(
            root,
            Path::new(&source.source_index),
            &source.source_index_blake3,
            source.source_index_bytes,
            &source.source_blake3,
            source.source_bytes,
            |record, blob| {
                let projection = source_index::product_projection(
                    record,
                    blob,
                    &source.source_blake3,
                    source.source_bytes,
                )?;
                let (game_hash, occurrence_id) = occurrences::probe_occurrence_identity(
                    &source.source_blake3,
                    source.source_bytes,
                    record.source_start,
                    record.source_end,
                    blob,
                );
                if game_hash != record.source_game_blake3 {
                    return Err("SourceIndex e occurrence non allineati".into());
                }
                let mut white_names = Vec::new();
                let mut black_names = Vec::new();
                let mut white_fide_id_values = Vec::new();
                let mut black_fide_id_values = Vec::new();
                for evidence in source_index::player_header_evidence_from_blob(blob) {
                    match evidence.side {
                        crate::inspector::PlayerSideV1::White => {
                            white_names = evidence.names;
                            white_fide_id_values = evidence.fide_id_values;
                        }
                        crate::inspector::PlayerSideV1::Black => {
                            black_names = evidence.names;
                            black_fide_id_values = evidence.fide_id_values;
                        }
                    }
                }
                on_game(crate::inspector::ProductContextOccurrenceV1 {
                    occurrence_id,
                    white_names,
                    black_names,
                    white_fide_id_values,
                    black_fide_id_values,
                    event: projection.event,
                    date: projection.date,
                    result: projection.result,
                    round: projection.round,
                    source_blake3: source.source_blake3.clone(),
                    source_bytes: source.source_bytes,
                    source_start: record.source_start,
                    source_end: record.source_end,
                    ordinal: record.ordinal,
                })
            },
        )?;
        if count != source.games {
            return Err("SourceIndex e Collection hanno cardinalità diversa".into());
        }
        total = total.checked_add(count).ok_or("Numero partite oltre u64")?;
    }
    Ok(crate::inspector::VerifiedPlayerSideScanV1 {
        collection_blake3: common.hash,
        collection_bytes: common.size,
        source_occurrences: total,
    })
}

pub(in crate::inspector) fn for_each_verified_duplicate_row<F>(
    root: &Path,
    path: &Path,
    mut on_row: F,
) -> Result<u64, String>
where
    F: FnMut(crate::inspector::ProductDuplicateRowV1) -> Result<(), String>,
{
    let common = load_common(root, path)?;
    match &common.manifest {
        CollectionManifest::V1(_) => {
            Err("La Collection attiva legacy v1 non supporta l'analisi duplicati prodotto".into())
        }
        CollectionManifest::V2(manifest) => {
            validation::validate_v2(manifest)?;
            let canonical_bytes = serialized_v2(manifest)?;
            if canonical_bytes != common.bytes
                || blake3::hash(&canonical_bytes).to_hex().as_str() != common.hash
                || canonical_bytes.len() as u64 != common.size
            {
                return Err("Collection diversa dal ricalcolo byte-esatto".into());
            }
            let mut total = 0_u64;
            for record in &manifest.sources {
                let loaded_mainlines = load_v2_source_chain(root, &common.canonical_root, record)?;
                let count = u64::try_from(
                    loaded_mainlines.for_each_verified_product_duplicate_row(&mut on_row)?,
                )
                .map_err(|_| "Numero mainline oltre u64")?;
                if count != record.games {
                    return Err(
                        "Occurrences, Catalog e Collection hanno cardinalità diversa".into(),
                    );
                }
                total = total.checked_add(count).ok_or("Numero partite oltre u64")?;
            }
            Ok(total)
        }
        CollectionManifest::V3(manifest) => {
            validate_v3_common(&common, manifest)?;
            let mut total = 0_u64;
            for source in &manifest.sources {
                let count = source_index::for_each_verified_record(
                    root,
                    Path::new(&source.source_index),
                    &source.source_index_blake3,
                    source.source_index_bytes,
                    &source.source_blake3,
                    source.source_bytes,
                    |record, blob| {
                        let projection = source_index::product_projection(
                            record,
                            blob,
                            &source.source_blake3,
                            source.source_bytes,
                        )?;
                        let (game_hash, occurrence_id) = occurrences::probe_occurrence_identity(
                            &source.source_blake3,
                            source.source_bytes,
                            record.source_start,
                            record.source_end,
                            blob,
                        );
                        if game_hash != record.source_game_blake3 {
                            return Err("SourceIndex e occurrence non allineati".into());
                        }
                        on_row(crate::inspector::ProductDuplicateRowV1 {
                            occurrence_id,
                            source_game_blake3: record.source_game_blake3.clone(),
                            game_bytes: record
                                .source_end
                                .checked_sub(record.source_start)
                                .ok_or("Intervallo SourceIndex invertito")?,
                            canonical_mainline_id: record.canonical_mainline_id.clone(),
                            comparable: record.comparable,
                            mainline_results: projection.mainline_results,
                            date: projection.date,
                            white: projection.white,
                            black: projection.black,
                            result: projection.result,
                            event: projection.event,
                        })
                    },
                )?;
                if count != source.games {
                    return Err("SourceIndex e Collection hanno cardinalità diversa".into());
                }
                total = total.checked_add(count).ok_or("Numero partite oltre u64")?;
            }
            Ok(total)
        }
    }
}

pub(crate) use compare::SnapshotComparison;
pub(super) use duplicates::{duplicates_report, DuplicateFormat};
#[cfg(test)]
pub(crate) use duplicates::{ProductExactDuplicateGroup, ProductExactDuplicateReport};
pub(super) use ingest::{run as ingest_directory, FileStatus};
pub(in crate::inspector) use ingest_v3::run_product as ingest_nonempty_snapshot;
pub(in crate::inspector) use inventory::{
    inventory_collection, load_verified as load_verified_inventory, serialize_inventory_records,
    VerifiedInventoryV1,
};
#[cfg(test)]
pub(crate) use query::CollectionQueryMatch;
pub(crate) use query::CollectionQueryResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceDiagnosis {
    InitializedNoSnapshot,
    Active { sources: u64, games: u64 },
}

pub(crate) fn diagnose_workspace(root: &Path) -> Result<WorkspaceDiagnosis, String> {
    let Some(summary) = workspace_state::load_active_collection_if_present(root)? else {
        return Ok(WorkspaceDiagnosis::InitializedNoSnapshot);
    };
    Ok(WorkspaceDiagnosis::Active {
        sources: summary.source_count,
        games: summary.games,
    })
}

pub(crate) fn compare_active_snapshot(
    root: &Path,
    input: &Path,
) -> Result<SnapshotComparison, String> {
    compare::active_snapshot(root, input)
}

pub(super) fn query_collection(
    root: &Path,
    path: &Path,
    filters: &catalog::QueryFilters,
) -> Result<String, String> {
    let result = query_collection_result(root, path, filters)?;
    query::format_inspector(&result)
}

pub(crate) fn query_collection_result(
    root: &Path,
    path: &Path,
    filters: &catalog::QueryFilters,
) -> Result<CollectionQueryResult, String> {
    catalog::query::validate_filters(filters)?;
    let common = load_common(root, path)?;
    if let CollectionManifest::V3(manifest) = &common.manifest {
        validate_v3_common(&common, manifest)?;
        let mut matches = Vec::new();
        for source in &manifest.sources {
            let observation_paths = source
                .observations
                .iter()
                .map(|o| o.canonical_path.clone())
                .collect::<Vec<_>>();
            let count = source_index::for_each_verified_record(
                root,
                Path::new(&source.source_index),
                &source.source_index_blake3,
                source.source_index_bytes,
                &source.source_blake3,
                source.source_bytes,
                |record, blob| {
                    let projection = source_index::product_projection(
                        record,
                        blob,
                        &source.source_blake3,
                        source.source_bytes,
                    )?;
                    if !catalog::query::matches_projection(
                        &projection.white,
                        &projection.black,
                        &projection.event,
                        &projection.date,
                        filters,
                    ) {
                        return Ok(());
                    }
                    let (game_hash, occurrence_id) = occurrences::probe_occurrence_identity(
                        &source.source_blake3,
                        source.source_bytes,
                        record.source_start,
                        record.source_end,
                        blob,
                    );
                    if game_hash != record.source_game_blake3 {
                        return Err("SourceIndex e occurrence non allineati".into());
                    }
                    matches.push(query::CollectionQueryMatch {
                        occurrence_id: Some(occurrence_id),
                        source_id: source.id,
                        game_id: record.ordinal,
                        source_blake3: source.source_blake3.clone(),
                        observation_paths: observation_paths.clone(),
                        segment_ordinal: record.segment_ordinal,
                        source_start: record.source_start,
                        source_end: record.source_end,
                        white: projection.white,
                        black: projection.black,
                        event: projection.event,
                        date: projection.date,
                        result: projection.result,
                    });
                    Ok(())
                },
            )?;
            if count != source.games {
                return Err("SourceIndex e Collection hanno cardinalità diversa".into());
            }
        }
        return Ok(CollectionQueryResult {
            occurrence_ids_available: true,
            matches,
        });
    }
    drop(common);
    let loaded = load_verified_collection(root, path)?;
    query::run(&loaded, filters)
}

pub(super) fn resolve_active_collection(root: &Path) -> Result<PathBuf, String> {
    workspace_state::resolve_active_collection(root)
}

pub(super) fn resolve_active_collection_for_verified_scan(root: &Path) -> Result<PathBuf, String> {
    workspace_state::resolve_active_collection_for_verified_scan(root)
}

#[derive(Debug)]
pub(crate) struct ResolvedGameOccurrence {
    pub(crate) occurrence_id: String,
    pub(crate) bytes: Vec<u8>,
}

pub(crate) fn resolve_occurrences_bulk(
    root: &Path,
    collection: &Path,
    occurrence_ids: &std::collections::BTreeSet<String>,
) -> Result<Vec<ResolvedGameOccurrence>, String> {
    if occurrence_ids.is_empty() {
        return Ok(Vec::new());
    }
    let common = load_common(root, collection)?;
    let CollectionManifest::V3(manifest) = &common.manifest else {
        return Err("L'export identità giocatore v1 richiede una Collection v3".into());
    };
    validate_v3_common(&common, manifest)?;
    let mut found = std::collections::BTreeMap::<String, Vec<u8>>::new();
    for source in &manifest.sources {
        let count = source_index::for_each_verified_record(
            root,
            Path::new(&source.source_index),
            &source.source_index_blake3,
            source.source_index_bytes,
            &source.source_blake3,
            source.source_bytes,
            |record, blob| {
                let (game_hash, candidate) = occurrences::probe_occurrence_identity(
                    &source.source_blake3,
                    source.source_bytes,
                    record.source_start,
                    record.source_end,
                    blob,
                );
                if game_hash != record.source_game_blake3 {
                    return Err("SourceIndex e occurrence non allineati".into());
                }
                if occurrence_ids.contains(&candidate)
                    && found.insert(candidate.clone(), blob.to_vec()).is_some()
                {
                    return Err("Game ID duplicato nella Collection attiva".into());
                }
                Ok(())
            },
        )?;
        if count != source.games {
            return Err("SourceIndex e Collection hanno cardinalità diversa".into());
        }
    }
    if found.len() != occurrence_ids.len() {
        let missing = occurrence_ids
            .iter()
            .find(|id| !found.contains_key(*id))
            .cloned()
            .unwrap_or_else(|| "<sconosciuto>".into());
        return Err(format!("Partita non presente nel corpus attivo: {missing}"));
    }
    Ok(found
        .into_iter()
        .map(|(occurrence_id, bytes)| ResolvedGameOccurrence {
            occurrence_id,
            bytes,
        })
        .collect())
}

pub(crate) fn resolve_occurrence(
    root: &Path,
    collection: &Path,
    occurrence_id: &str,
) -> Result<ResolvedGameOccurrence, String> {
    let common = load_common(root, collection)?;
    if let CollectionManifest::V3(manifest) = &common.manifest {
        validate_v3_common(&common, manifest)?;
        let mut found: Option<Vec<u8>> = None;
        for source in &manifest.sources {
            let count = source_index::for_each_verified_record(
                root,
                Path::new(&source.source_index),
                &source.source_index_blake3,
                source.source_index_bytes,
                &source.source_blake3,
                source.source_bytes,
                |record, blob| {
                    let (game_hash, candidate) = occurrences::probe_occurrence_identity(
                        &source.source_blake3,
                        source.source_bytes,
                        record.source_start,
                        record.source_end,
                        blob,
                    );
                    if game_hash != record.source_game_blake3 {
                        return Err("SourceIndex e occurrence non allineati".into());
                    }
                    if candidate == occurrence_id {
                        if found.is_some() {
                            return Err("Game ID duplicato nella Collection attiva".into());
                        }
                        found = Some(blob.to_vec());
                    }
                    Ok(())
                },
            )?;
            if count != source.games {
                return Err("SourceIndex e Collection hanno cardinalità diversa".into());
            }
        }
        return found
            .map(|bytes| ResolvedGameOccurrence {
                occurrence_id: occurrence_id.to_owned(),
                bytes,
            })
            .ok_or("Partita non presente nel corpus attivo".into());
    }
    drop(common);
    let loaded = load_verified_collection(root, collection)?;
    let identities = loaded
        .occurrence_identities
        .as_ref()
        .ok_or("La Collection attiva legacy v1 non supporta Game ID pubblici")?;
    let mut location = None;
    for (source_index, source_identities) in identities.iter().enumerate() {
        for identity in source_identities {
            if identity.occurrence_id == occurrence_id
                && location.replace((source_index, identity)).is_some()
            {
                return Err("Game ID duplicato nella Collection attiva".into());
            }
        }
    }
    let (source_index, identity) = location.ok_or("Partita non presente nel corpus attivo")?;
    let source = loaded
        .sources()
        .get(source_index)
        .copied()
        .ok_or("Occurrence associata a source inesistente")?;
    let expected_source_id = u64::try_from(source_index).map_err(|_| "source index oltre u64")?;
    if source.id != expected_source_id || source.source_blake3 != identity.source_blake3 {
        return Err("Occurrence e source Collection non allineate".into());
    }
    let catalog = loaded
        .catalogs
        .get(source_index)
        .ok_or("Occurrence associata a Catalog inesistente")?;
    let game_index =
        usize::try_from(identity.ordinal).map_err(|_| "Ordinal occurrence oltre usize")?;
    let game = catalog
        .manifest
        .games
        .get(game_index)
        .filter(|game| game.id == identity.ordinal)
        .ok_or("Occurrence associata a game Catalog inesistente")?;
    let source = &catalog.interpretation.parse.tokenization;
    source.verify_source()?;
    let blob_bytes = source.manifest.source_bytes;
    if blob_bytes != identity.source_bytes
        || blob_bytes != catalog.manifest.source_bytes
        || catalog.manifest.source_blake3 != identity.source_blake3
    {
        return Err("SourceBlob occurrence incoerente".into());
    }
    let expected_bytes = game
        .source_end
        .checked_sub(game.source_start)
        .ok_or("Intervallo occurrence invertito")?;
    let bytes = source.read_source_range(game.source_start, game.source_end)?;
    let actual_bytes = u64::try_from(bytes.len()).map_err(|_| "Occurrence oltre u64")?;
    let actual_hash = blake3::hash(&bytes).to_hex().to_string();
    if expected_bytes != identity.game_bytes
        || actual_bytes != expected_bytes
        || game.source_game_blake3 != identity.source_game_blake3
        || actual_hash != identity.source_game_blake3
    {
        return Err("Slice occurrence diversa da identità, hash o dimensione dichiarati".into());
    }
    source.verify_source()?;
    Ok(ResolvedGameOccurrence {
        occurrence_id: identity.occurrence_id.clone(),
        bytes,
    })
}

pub(super) struct ExportedOccurrence {
    pub source_id: u64,
    pub game_id: u64,
    pub source_blake3: String,
    pub game_blake3: String,
    pub bytes: u64,
    pub output: PathBuf,
}

pub(super) fn export_occurrence(
    root: &Path,
    collection: &Path,
    source_id: u64,
    game_id: u64,
    output: &Path,
) -> Result<ExportedOccurrence, String> {
    let loaded = load_verified_collection(root, collection)?;
    let index = usize::try_from(source_id).map_err(|_| "source-id oltre usize")?;
    let source_view = loaded
        .sources()
        .into_iter()
        .find(|source| source.id == source_id)
        .ok_or("source-id inesistente")?;
    if index >= loaded.catalogs.len() {
        return Err("source-id inesistente".into());
    }
    let source_blake3 = source_view.source_blake3.to_owned();
    let catalog = &loaded.catalogs[index];
    let game = catalog
        .manifest
        .games
        .get(usize::try_from(game_id).map_err(|_| "game-id oltre usize")?)
        .filter(|g| g.id == game_id)
        .ok_or("game-id inesistente")?;
    let source = &catalog.interpretation.parse.tokenization;
    source.verify_source()?;
    let bytes = source.read_source_range(game.source_start, game.source_end)?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    if hash != game.source_game_blake3 {
        return Err("Hash GameOccurrence non valido".into());
    }
    source.verify_source()?;
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let resolved = if output.is_absolute() {
        output.to_path_buf()
    } else {
        root.join(validate_relative_path(
            output.to_str().ok_or("Output non UTF-8")?,
            "output",
        )?)
    };
    let parent = resolved.parent().ok_or("Output senza parent")?;
    let canonical_parent =
        fs::canonicalize(parent).map_err(|e| format!("Parent output non valido: {e}"))?;
    ensure_within(&canonical_root, &canonical_parent, "parent output")?;
    let target = canonical_parent.join(resolved.file_name().ok_or("Output senza nome")?);
    if fs::symlink_metadata(&target).is_ok() {
        return Err("L'output esiste già".into());
    }
    let (mut file, temp) =
        crate::inspector::common::create_unique_temporary(&canonical_parent, ".occurrence-export")?;
    let result = file
        .write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("Scrittura export: {e}"));
    drop(file);
    if let Err(e) = result {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    if let Err(e) = fs::hard_link(&temp, &target) {
        let _ = fs::remove_file(&temp);
        return Err(if e.kind() == std::io::ErrorKind::AlreadyExists {
            "L'output esiste già".into()
        } else {
            format!("Pubblicazione export: {e}")
        });
    }
    fs::remove_file(&temp).map_err(|e| format!("Pulizia temporaneo export: {e}"))?;
    Ok(ExportedOccurrence {
        source_id,
        game_id,
        source_blake3,
        game_blake3: hash,
        bytes: bytes.len() as u64,
        output: output.to_path_buf(),
    })
}

fn relative_manifest_path(root: &Path, path: &Path, label: &str) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("{label} verificato fuori workspace"))?;
    let value = relative
        .to_str()
        .ok_or_else(|| format!("Percorso {label} non UTF-8"))?;
    validate_relative_path(value, label)?;
    Ok(crate::inspector::common::path_to_manifest_string(relative))
}
