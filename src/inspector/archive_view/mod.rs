mod manifest;
mod validation;

use crate::inspector::catalog::QueryFilters;
use crate::inspector::collection;
use crate::inspector::common::{
    canonical_artifact_path, ensure_within, logical_json_bytes, path_to_manifest_string,
    prepare_directory_chain, publish_compressed_json, resolve_workspace_path,
    validate_relative_path,
};
use manifest::*;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) struct CreatedArchiveView {
    pub(super) path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) new_manifest: bool,
    pub(super) occurrences: usize,
}

pub(super) struct VerifiedArchiveViewV1 {
    inventory: collection::VerifiedInventoryV1,
}

pub(super) fn create(
    root: &Path,
    collection_path: &Path,
    filters: &QueryFilters,
) -> Result<CreatedArchiveView, String> {
    let canonical_filters = filters.canonicalized();
    let inventory = collection::load_verified_inventory(root, collection_path, &canonical_filters)?;
    let manifest = build(&inventory, &canonical_filters);
    validation::validate(&manifest)?;
    let bytes = serialized(&manifest)?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let size = u64::try_from(bytes.len()).map_err(|_| "Archive View oltre u64")?;

    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let directory = prepare_directory_chain(
        root,
        &canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("archive_views", "directory Archive View"),
        ],
    )?;
    let (target, new_manifest) =
        publish_compressed_json(&canonical_root, &directory, &hash, &bytes, "Archive View")?;
    let name = target
        .file_name()
        .ok_or("Artefatto Archive View senza nome")?;
    let path = PathBuf::from("inspector/archive_views").join(name);
    if !path.to_string_lossy().ends_with(".json.xz") {
        return Err("Archive View legacy non supportata".into());
    }
    Ok(CreatedArchiveView {
        path,
        manifest_blake3: hash,
        manifest_bytes: size,
        new_manifest,
        occurrences: manifest.occurrences.len(),
    })
}

pub(super) fn load_verified(root: &Path, path: &Path) -> Result<VerifiedArchiveViewV1, String> {
    let path_value = path.to_str().ok_or("Percorso Archive View non UTF-8")?;
    validate_relative_path(path_value, "view")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let metadata = fs::symlink_metadata(&resolved)
        .map_err(|error| format!("Archive View non valida: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Archive View non è un file regolare reale".into());
    }
    let canonical =
        fs::canonicalize(&resolved).map_err(|error| format!("Archive View non valida: {error}"))?;
    ensure_within(&canonical_root, &canonical, "Archive View")?;
    let bytes = logical_json_bytes(&canonical, "Archive View")?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let size = u64::try_from(bytes.len()).map_err(|_| "Archive View oltre u64")?;
    let actual = relative_path(&canonical_root, &canonical)?;
    let expected = canonical_artifact_path("archive_views", &hash, &canonical)?;
    if actual != expected || !actual.ends_with(".json.xz") {
        return Err("Percorso Archive View non canonico".into());
    }

    let manifest: ArchiveViewManifestV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Manifest Archive View non valido: {error}"))?;
    validation::validate(&manifest)?;
    let filters = manifest.filters.to_query()?;
    let inventory =
        collection::load_verified_inventory(root, Path::new(&manifest.collection.path), &filters)?;
    if manifest.collection.path != inventory.collection.path
        || manifest.collection.blake3 != inventory.collection.blake3
        || manifest.collection.bytes != inventory.collection.bytes
    {
        return Err("Identità Collection diversa dalla Archive View".into());
    }
    let rebuilt = build(&inventory, &filters);
    validation::validate(&rebuilt)?;
    let rebuilt_bytes = serialized(&rebuilt)?;
    if manifest != rebuilt
        || bytes != rebuilt_bytes
        || hash != blake3::hash(&rebuilt_bytes).to_hex().to_string()
        || size
            != u64::try_from(rebuilt_bytes.len()).map_err(|_| "Archive View attesa oltre u64")?
    {
        return Err("Archive View diversa dal ricalcolo byte-esatto".into());
    }
    Ok(VerifiedArchiveViewV1 { inventory })
}

pub(super) fn inventory(root: &Path, path: &Path) -> Result<Vec<u8>, String> {
    let verified = load_verified(root, path)?;
    collection::serialize_inventory_records(&verified.inventory)
}

pub(super) fn verify(root: &Path, path: &Path) -> Result<(), String> {
    load_verified(root, path).map(|_| ())
}

fn build(
    inventory: &crate::inspector::collection::VerifiedInventoryV1,
    filters: &QueryFilters,
) -> ArchiveViewManifestV1 {
    ArchiveViewManifestV1 {
        schema_version: ARCHIVE_VIEW_SCHEMA_VERSION,
        archive_view_profile: ARCHIVE_VIEW_PROFILE.into(),
        collection: ArchiveViewCollectionV1 {
            path: inventory.collection.path.clone(),
            blake3: inventory.collection.blake3.clone(),
            bytes: inventory.collection.bytes,
        },
        filters: ArchiveViewFiltersV1::from_query(filters),
        occurrences: inventory
            .records
            .iter()
            .map(|record| ArchiveViewOccurrenceV1 {
                source_id: record.source_id,
                game_id: record.game_id,
                occurrence_id: record.occurrence_id.clone(),
                canonical_mainline_id: record.canonical_mainline_id.clone(),
            })
            .collect(),
    }
}

fn serialized(manifest: &ArchiveViewManifestV1) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("Serializzazione Archive View: {error}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "Archive View fuori workspace")?;
    let value = relative.to_str().ok_or("Percorso Archive View non UTF-8")?;
    validate_relative_path(value, "view")?;
    Ok(path_to_manifest_string(relative))
}
