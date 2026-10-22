use super::manifest::*;
use crate::inspector::common::validate_relative_path;
use std::collections::HashSet;

pub(super) fn validate(manifest: &ArchiveViewManifestV1) -> Result<(), String> {
    if manifest.schema_version != ARCHIVE_VIEW_SCHEMA_VERSION {
        return Err("Versione Archive View non supportata".into());
    }
    if manifest.archive_view_profile != ARCHIVE_VIEW_PROFILE {
        return Err("Profilo Archive View non supportato".into());
    }
    validate_collection(&manifest.collection)?;
    let filters = manifest.filters.to_query()?;
    if manifest.filters != ArchiveViewFiltersV1::from_query(&filters) {
        return Err("Filtri Archive View non canonici".into());
    }

    let mut previous = None;
    let mut occurrence_ids = HashSet::new();
    for occurrence in &manifest.occurrences {
        let key = (occurrence.source_id, occurrence.game_id);
        if previous.is_some_and(|prior| prior >= key) {
            return Err("Occurrence Archive View non ordinate o duplicate".into());
        }
        previous = Some(key);
        if !valid_hash(&occurrence.occurrence_id)
            || occurrence
                .canonical_mainline_id
                .as_deref()
                .is_some_and(|hash| !valid_hash(hash))
        {
            return Err("Identità occurrence Archive View non valida".into());
        }
        if !occurrence_ids.insert(&occurrence.occurrence_id) {
            return Err("occurrence_id duplicato nella Archive View".into());
        }
    }
    Ok(())
}

fn validate_collection(collection: &ArchiveViewCollectionV1) -> Result<(), String> {
    validate_relative_path(&collection.path, "collection Archive View")?;
    if !valid_hash(&collection.blake3) || collection.bytes == 0 {
        return Err("Identità Collection Archive View non valida".into());
    }
    let compressed = format!("inspector/collections/{}.json.xz", collection.blake3);
    let legacy = format!("inspector/collections/{}.json", collection.blake3);
    if collection.path != compressed && collection.path != legacy {
        return Err("Percorso Collection Archive View non canonico".into());
    }
    Ok(())
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
