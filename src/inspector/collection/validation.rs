use super::manifest::*;
use crate::inspector::common::validate_relative_path;
use std::collections::BTreeSet;
use std::path::Path;

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn validate_observations(observations: &[SourceObservationRecordV1]) -> Result<(), String> {
    if observations.is_empty() {
        return Err("Source senza observations".into());
    }
    let mut prior = None;
    for observation in observations {
        if !Path::new(&observation.canonical_path).is_absolute() {
            return Err("canonical_path non assoluto".into());
        }
        validate_relative_path(&observation.input_relative_path, "input_relative_path")?;
        if prior.as_ref().is_some_and(|p| *p >= observation) {
            return Err("Observations non ordinate o duplicate".into());
        }
        prior = Some(observation);
    }
    Ok(())
}

fn validate_source_identities(sources: &[(u64, &str, u64)]) -> Result<(), String> {
    let mut hashes = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut previous: Option<(&str, u64)> = None;
    for (index, (id, source_blake3, source_bytes)) in sources.iter().enumerate() {
        if *id != index as u64 {
            return Err("Source id non consecutivi".into());
        }
        let key = (*source_blake3, *source_bytes);
        if previous.is_some_and(|p| p >= key) {
            return Err("Sources non ordinate".into());
        }
        previous = Some(key);
        if !hash(source_blake3) || *source_bytes == 0 {
            return Err("Identità SourceBlob non valida".into());
        }
        if !hashes.insert(source_blake3.to_string())
            || !identities.insert((source_blake3.to_string(), *source_bytes))
        {
            return Err("Source duplicata".into());
        }
    }
    Ok(())
}

fn validate_catalog_path(catalog: &str, catalog_blake3: &str) -> Result<(), String> {
    validate_relative_path(catalog, "catalog")?;
    let legacy = format!("inspector/catalogs/{catalog_blake3}.json");
    let compressed = format!("inspector/catalogs/{catalog_blake3}.json.xz");
    if catalog != legacy && catalog != compressed {
        return Err("Percorso Catalog non canonico".into());
    }
    Ok(())
}

fn validate_occurrences_path(occurrences: &str, occurrences_blake3: &str) -> Result<(), String> {
    validate_relative_path(occurrences, "occurrences")?;
    let legacy = format!("inspector/occurrences/{occurrences_blake3}.json");
    let compressed = format!("inspector/occurrences/{occurrences_blake3}.json.xz");
    if occurrences != legacy && occurrences != compressed {
        return Err("Percorso Occurrences non canonico".into());
    }
    Ok(())
}

fn validate_mainlines_path(mainlines: &str, mainlines_blake3: &str) -> Result<(), String> {
    validate_relative_path(mainlines, "mainlines")?;
    let legacy = format!("inspector/mainlines/{mainlines_blake3}.json");
    let compressed = format!("inspector/mainlines/{mainlines_blake3}.json.xz");
    if mainlines != legacy && mainlines != compressed {
        return Err("Percorso Mainlines non canonico".into());
    }
    Ok(())
}

pub(super) fn validate(m: &CollectionManifestV1) -> Result<(), String> {
    if m.schema_version != COLLECTION_SCHEMA_VERSION || m.collection_profile != COLLECTION_PROFILE {
        return Err("Schema o profilo Collection non supportato".into());
    }
    let sources: Vec<(u64, &str, u64)> = m
        .sources
        .iter()
        .map(|s| (s.id, s.source_blake3.as_str(), s.source_bytes))
        .collect();
    validate_source_identities(&sources)?;
    for source in &m.sources {
        validate_catalog_path(&source.catalog, &source.catalog_blake3)?;
        if !hash(&source.catalog_blake3) || source.catalog_bytes == 0 {
            return Err("Identità Catalog non valida".into());
        }
        validate_observations(&source.observations)?;
    }
    Ok(())
}

pub(super) fn validate_v2(m: &CollectionManifestV2) -> Result<(), String> {
    if m.schema_version != COLLECTION_SCHEMA_VERSION_V2
        || m.collection_profile != COLLECTION_PROFILE_V2
    {
        return Err("Schema o profilo Collection non supportato".into());
    }
    let sources: Vec<(u64, &str, u64)> = m
        .sources
        .iter()
        .map(|s| (s.id, s.source_blake3.as_str(), s.source_bytes))
        .collect();
    validate_source_identities(&sources)?;
    for source in &m.sources {
        validate_catalog_path(&source.catalog, &source.catalog_blake3)?;
        if !hash(&source.catalog_blake3) || source.catalog_bytes == 0 {
            return Err("Identità Catalog non valida".into());
        }
        validate_occurrences_path(&source.occurrences, &source.occurrences_blake3)?;
        if !hash(&source.occurrences_blake3) || source.occurrences_bytes == 0 {
            return Err("Identità Occurrences non valida".into());
        }
        validate_mainlines_path(&source.mainlines, &source.mainlines_blake3)?;
        if !hash(&source.mainlines_blake3) || source.mainlines_bytes == 0 {
            return Err("Identità Mainlines non valida".into());
        }
        validate_observations(&source.observations)?;
    }
    Ok(())
}

fn validate_source_index_path(source_index: &str, source_index_blake3: &str) -> Result<(), String> {
    validate_relative_path(source_index, "source_index")?;
    let expected = format!("inspector/source-indexes/{source_index_blake3}.bin");
    if source_index != expected {
        return Err("Percorso SourceIndex non canonico".into());
    }
    Ok(())
}

pub(super) fn validate_v3(m: &CollectionManifestV3) -> Result<(), String> {
    if m.schema_version != COLLECTION_SCHEMA_VERSION_V3
        || m.collection_profile != COLLECTION_PROFILE_V3
    {
        return Err("Schema o profilo Collection non supportato".into());
    }
    let sources: Vec<(u64, &str, u64)> = m
        .sources
        .iter()
        .map(|s| (s.id, s.source_blake3.as_str(), s.source_bytes))
        .collect();
    validate_source_identities(&sources)?;
    for source in &m.sources {
        validate_source_index_path(&source.source_index, &source.source_index_blake3)?;
        if !hash(&source.source_index_blake3) || source.source_index_bytes == 0 {
            return Err("Identità SourceIndex non valida".into());
        }
        let expected = crate::inspector::source_index::expected_index_bytes(source.games)?;
        if source.source_index_bytes != expected {
            return Err("Dimensione SourceIndex incoerente con games".into());
        }
        validate_observations(&source.observations)?;
    }
    Ok(())
}

pub(super) fn validate_manifest(m: &CollectionManifest) -> Result<(), String> {
    match m {
        CollectionManifest::V1(m) => validate(m),
        CollectionManifest::V2(m) => validate_v2(m),
        CollectionManifest::V3(m) => validate_v3(m),
    }
}

pub(super) fn detect_profile(bytes: &[u8]) -> Result<CollectionManifest, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| format!("Manifest Collection non valido: {e}"))?;
    let profile = value
        .get("collection_profile")
        .and_then(|v| v.as_str())
        .ok_or("Manifest Collection senza collection_profile")?;
    match profile {
        COLLECTION_PROFILE => {
            let manifest: CollectionManifestV1 = serde_json::from_value(value)
                .map_err(|e| format!("Manifest Collection v1 non valido: {e}"))?;
            Ok(CollectionManifest::V1(manifest))
        }
        COLLECTION_PROFILE_V2 => {
            let manifest: CollectionManifestV2 = serde_json::from_value(value)
                .map_err(|e| format!("Manifest Collection v2 non valido: {e}"))?;
            Ok(CollectionManifest::V2(manifest))
        }
        COLLECTION_PROFILE_V3 => {
            let manifest: CollectionManifestV3 = serde_json::from_value(value)
                .map_err(|e| format!("Manifest Collection v3 non valido: {e}"))?;
            Ok(CollectionManifest::V3(manifest))
        }
        _ => Err("Schema o profilo Collection non supportato".into()),
    }
}
