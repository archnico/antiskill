use super::{load_verified_summary, CreatedCollection, VerifiedCollectionSummary};
use crate::inspector::common::{
    create_unique_temporary, ensure_within, path_to_manifest_string, validate_relative_path,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;
const PROFILE: &str = "antiskill_workspace_state_v1";
const STATE_FILE: &str = "workspace-state.json";

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct WorkspaceStateV1 {
    schema_version: u32,
    workspace_state_profile: String,
    active_collection: ActiveCollectionV1,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ActiveCollectionV1 {
    path: String,
    blake3: String,
    bytes: u64,
}

fn state_bytes(state: &WorkspaceStateV1) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("Serializzazione stato workspace: {error}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate(state: &WorkspaceStateV1) -> Result<(), String> {
    if state.schema_version != SCHEMA_VERSION {
        return Err("Versione stato workspace non supportata".into());
    }
    if state.workspace_state_profile != PROFILE {
        return Err("Profilo stato workspace non supportato".into());
    }
    let active = &state.active_collection;
    validate_relative_path(&active.path, "active Collection")?;
    if !valid_hash(&active.blake3) || active.bytes == 0 {
        return Err("Identità active Collection non valida".into());
    }
    let legacy = format!("inspector/collections/{}.json", active.blake3);
    let compressed = format!("inspector/collections/{}.json.xz", active.blake3);
    if active.path != legacy && active.path != compressed {
        return Err("Percorso active Collection non canonico".into());
    }
    Ok(())
}

fn inspector_directory(root: &Path) -> Result<(PathBuf, PathBuf), String> {
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let inspector = root.join("inspector");
    let metadata = fs::symlink_metadata(&inspector)
        .map_err(|error| format!("Directory stato workspace non valida: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Directory stato workspace non è una directory reale".into());
    }
    let canonical_inspector = fs::canonicalize(&inspector)
        .map_err(|error| format!("Directory stato workspace non valida: {error}"))?;
    ensure_within(
        &canonical_root,
        &canonical_inspector,
        "directory stato workspace",
    )?;
    Ok((canonical_root, canonical_inspector))
}

fn read(root: &Path) -> Result<WorkspaceStateV1, String> {
    let (canonical_root, canonical_inspector) = inspector_directory(root).map_err(|error| {
        if !root.join("inspector").exists() {
            "Stato workspace mancante: eseguire ingest-directory o usare --collection".to_owned()
        } else {
            error
        }
    })?;
    let path = canonical_inspector.join(STATE_FILE);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "Stato workspace mancante: eseguire ingest-directory o usare --collection".to_owned()
        } else {
            format!("Stato workspace non leggibile: {error}")
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Stato workspace corrotto: non è un file regolare reale".into());
    }
    let canonical =
        fs::canonicalize(&path).map_err(|error| format!("Stato workspace corrotto: {error}"))?;
    ensure_within(&canonical_root, &canonical, "stato workspace")?;
    if canonical.parent() != Some(canonical_inspector.as_path()) {
        return Err("Stato workspace fuori dalla directory inspector".into());
    }
    let bytes =
        fs::read(&canonical).map_err(|error| format!("Stato workspace non leggibile: {error}"))?;
    let state: WorkspaceStateV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Stato workspace corrotto: {error}"))?;
    validate(&state).map_err(|error| format!("Stato workspace corrotto: {error}"))?;
    if state_bytes(&state)? != bytes {
        return Err("Stato workspace corrotto: rappresentazione non canonica".into());
    }
    Ok(state)
}

pub(super) fn resolve_active_collection(root: &Path) -> Result<PathBuf, String> {
    let state = read(root)?;
    load_active_collection(root, &state)?;
    Ok(PathBuf::from(state.active_collection.path))
}

/// Resolves the active Collection binding without replaying its derived artifacts.
///
/// This is only for callers that immediately perform their own complete, fail-closed
/// Collection scan. The workspace state and the content-addressed Collection envelope
/// are still verified here; the caller remains responsible for verifying every artifact
/// referenced by that Collection before returning product data.
pub(super) fn resolve_active_collection_for_verified_scan(root: &Path) -> Result<PathBuf, String> {
    let state = read(root)?;
    let active = &state.active_collection;
    let path = PathBuf::from(&active.path);
    let loaded = super::load_common(root, &path)
        .map_err(|error| format!("Active Collection non verificabile: {error}"))?;
    validate_active_binding(
        &loaded.canonical_root,
        active,
        &loaded.canonical_path,
        &loaded.hash,
        loaded.size,
    )?;
    Ok(path)
}

pub(super) fn active_reference_if_present(
    root: &Path,
) -> Result<Option<(PathBuf, String, u64)>, String> {
    if !root.join("inspector").exists() {
        return Ok(None);
    }
    let (_canonical_root, canonical_inspector) = inspector_directory(root)?;
    let state_path = canonical_inspector.join(STATE_FILE);
    match fs::symlink_metadata(&state_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Stato workspace non verificabile: {error}")),
        Ok(_) => {
            let state = read(root)?;
            Ok(Some((
                PathBuf::from(&state.active_collection.path),
                state.active_collection.blake3,
                state.active_collection.bytes,
            )))
        }
    }
}

fn load_active_collection(
    root: &Path,
    state: &WorkspaceStateV1,
) -> Result<VerifiedCollectionSummary, String> {
    let active = &state.active_collection;
    let path = PathBuf::from(&active.path);
    let loaded = load_verified_summary(root, &path)
        .map_err(|error| format!("Active Collection non verificabile: {error}"))?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    validate_active_binding(
        &canonical_root,
        active,
        &loaded.canonical_path,
        &loaded.manifest_blake3,
        loaded.manifest_bytes,
    )?;
    Ok(loaded)
}

fn validate_active_binding(
    canonical_root: &Path,
    active: &ActiveCollectionV1,
    canonical_path: &Path,
    blake3: &str,
    bytes: u64,
) -> Result<(), String> {
    if blake3 != active.blake3 || bytes != active.bytes {
        return Err("Active Collection non verificabile: identità diversa dallo stato".into());
    }
    let relative = canonical_path
        .strip_prefix(canonical_root)
        .map_err(|_| "Active Collection verificata fuori workspace")?;
    if path_to_manifest_string(relative) != active.path {
        return Err("Active Collection non verificabile: percorso diverso dallo stato".into());
    }
    Ok(())
}

pub(super) fn load_active_collection_if_present(
    root: &Path,
) -> Result<Option<VerifiedCollectionSummary>, String> {
    let (_canonical_root, canonical_inspector) = inspector_directory(root)?;
    let state_path = canonical_inspector.join(STATE_FILE);
    match fs::symlink_metadata(&state_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Stato workspace non verificabile: {error}")),
        Ok(_) => {
            let state = read(root)?;
            load_active_collection(root, &state).map(Some)
        }
    }
}

pub(super) fn update(root: &Path, created: &CreatedCollection) -> Result<(), String> {
    let fast_activate = std::env::var_os("ANTISKILL_FAST_ACTIVATE").is_some();
    let loaded = if fast_activate {
        let common = super::load_common(root, &created.path)
            .map_err(|error| format!("Collection non valida prima dello stato: {error}"))?;
        let source_count = u64::try_from(common.manifest.source_count())
            .map_err(|_| "Numero sorgenti oltre u64")?;
        let games = common.manifest.games();
        VerifiedCollectionSummary {
            canonical_path: common.canonical_path,
            manifest_blake3: common.hash,
            manifest_bytes: common.size,
            source_count,
            games,
        }
    } else {
        load_verified_summary(root, &created.path)
            .map_err(|error| format!("Collection non valida prima dello stato: {error}"))?
    };
    if loaded.manifest_blake3 != created.manifest_blake3
        || loaded.manifest_bytes != created.manifest_bytes
    {
        return Err("Identità Collection pubblicata incoerente".into());
    }
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let relative = loaded
        .canonical_path
        .strip_prefix(&canonical_root)
        .map_err(|_| "Collection pubblicata verificata fuori workspace")?;
    if path_to_manifest_string(relative) != path_to_manifest_string(&created.path) {
        return Err("Percorso Collection pubblicata incoerente".into());
    }
    let state = WorkspaceStateV1 {
        schema_version: SCHEMA_VERSION,
        workspace_state_profile: PROFILE.into(),
        active_collection: ActiveCollectionV1 {
            path: path_to_manifest_string(&created.path),
            blake3: created.manifest_blake3.clone(),
            bytes: created.manifest_bytes,
        },
    };
    validate(&state)?;
    let bytes = state_bytes(&state)?;
    let (_canonical_root, directory) = inspector_directory(root)?;
    let target = directory.join(STATE_FILE);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err("Stato workspace esistente non è un file regolare reale".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Stato workspace esistente non verificabile: {error}"
            ))
        }
    }
    let (mut file, temporary) = create_unique_temporary(&directory, ".workspace-state")?;
    let written = file
        .write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("Scrittura stato workspace: {error}"));
    drop(file);
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, &target) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Aggiornamento atomico stato workspace: {error}"));
    }
    Ok(())
}
