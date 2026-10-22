use super::manifest::*;
use super::StreamingOccurrencesRefs;
use crate::inspector::common::validate_relative_path;
use std::collections::HashSet;

pub(super) fn validate_header(refs: &StreamingOccurrencesRefs<'_>) -> Result<(), String> {
    if refs.schema_version != OCCURRENCE_SCHEMA_VERSION {
        return Err("Versione GameOccurrence non supportata".into());
    }
    if refs.occurrence_profile != OCCURRENCE_PROFILE {
        return Err("Profilo GameOccurrence non supportato".into());
    }
    for (path, label) in [
        (refs.catalog_manifest, "catalog_manifest"),
        (refs.interpretation_manifest, "interpretation_manifest"),
        (refs.parse_manifest, "parse_manifest"),
        (refs.tokenization_manifest, "tokenization_manifest"),
        (refs.segmentation_manifest, "segmentation_manifest"),
        (refs.source_manifest, "source_manifest"),
    ] {
        validate_relative_path(path, label)?;
    }
    for hash in [
        refs.catalog_manifest_blake3,
        refs.interpretation_manifest_blake3,
        refs.parse_manifest_blake3,
        refs.tokenization_manifest_blake3,
        refs.segmentation_manifest_blake3,
        refs.source_manifest_blake3,
        refs.source_blake3,
    ] {
        valid_hash(hash)?;
    }
    if [
        refs.catalog_manifest_bytes,
        refs.interpretation_manifest_bytes,
        refs.parse_manifest_bytes,
        refs.tokenization_manifest_bytes,
        refs.segmentation_manifest_bytes,
        refs.source_manifest_bytes,
        refs.source_bytes,
    ]
    .into_iter()
    .any(|size| size == 0)
    {
        return Err("Dimensione logica occurrence nulla".into());
    }
    Ok(())
}

pub(super) fn validate(manifest: &OccurrenceManifestV1) -> Result<(), String> {
    validate_header(&refs_from_manifest(manifest))?;

    let expected_provenance = provenance(manifest);
    let mut ids = HashSet::new();
    let mut previous_end = None;
    for (index, occurrence) in manifest.occurrences.iter().enumerate() {
        let ordinal = u64::try_from(index).map_err(|_| "Ordinal occurrence oltre u64")?;
        if occurrence.ordinal != ordinal
            || occurrence.parse_tree_index != ordinal
            || occurrence.interpretation_game_id != ordinal
            || occurrence.catalog_game_id != ordinal
        {
            return Err("Ordinal e identificatori occurrence non consecutivi".into());
        }
        if !ids.insert(&occurrence.occurrence_id) {
            return Err("Collisione occurrence_id".into());
        }
        if previous_end.is_some_and(|end| end > occurrence.source_start) {
            return Err("Intervallo occurrence non valido, ordinato o disgiunto".into());
        }
        previous_end = Some(occurrence.source_end);
        validate_occurrence(
            occurrence,
            (&manifest.source_blake3, manifest.source_bytes),
            &expected_provenance,
        )?;
    }
    Ok(())
}

fn refs_from_manifest(manifest: &OccurrenceManifestV1) -> StreamingOccurrencesRefs<'_> {
    StreamingOccurrencesRefs {
        schema_version: manifest.schema_version,
        occurrence_profile: &manifest.occurrence_profile,
        catalog_manifest: &manifest.catalog_manifest,
        catalog_manifest_blake3: &manifest.catalog_manifest_blake3,
        catalog_manifest_bytes: manifest.catalog_manifest_bytes,
        interpretation_manifest: &manifest.interpretation_manifest,
        interpretation_manifest_blake3: &manifest.interpretation_manifest_blake3,
        interpretation_manifest_bytes: manifest.interpretation_manifest_bytes,
        parse_manifest: &manifest.parse_manifest,
        parse_manifest_blake3: &manifest.parse_manifest_blake3,
        parse_manifest_bytes: manifest.parse_manifest_bytes,
        tokenization_manifest: &manifest.tokenization_manifest,
        tokenization_manifest_blake3: &manifest.tokenization_manifest_blake3,
        tokenization_manifest_bytes: manifest.tokenization_manifest_bytes,
        segmentation_manifest: &manifest.segmentation_manifest,
        segmentation_manifest_blake3: &manifest.segmentation_manifest_blake3,
        segmentation_manifest_bytes: manifest.segmentation_manifest_bytes,
        source_manifest: &manifest.source_manifest,
        source_manifest_blake3: &manifest.source_manifest_blake3,
        source_manifest_bytes: manifest.source_manifest_bytes,
        source_blake3: &manifest.source_blake3,
        source_bytes: manifest.source_bytes,
    }
}

pub(super) fn validate_occurrence(
    occurrence: &GameOccurrenceV1,
    source: (&str, u64),
    expected_provenance: &OccurrenceProvenanceV1,
) -> Result<(), String> {
    valid_hash(&occurrence.occurrence_id)?;
    valid_hash(&occurrence.source_blake3)?;
    valid_hash(&occurrence.source_game_blake3)?;
    if occurrence.source_blake3 != source.0 || occurrence.source_bytes != source.1 {
        return Err("Identità SourceBlob occurrence incoerente".into());
    }
    if occurrence.source_start >= occurrence.source_end || occurrence.source_end > source.1 {
        return Err("Intervallo occurrence non valido o non contenuto".into());
    }
    if occurrence.provenance != *expected_provenance {
        return Err("Provenienza occurrence incoerente".into());
    }
    validate_syntax_diagnostics(occurrence)?;
    validate_interpretation_diagnostics(occurrence)?;
    Ok(())
}

pub(super) fn provenance(manifest: &OccurrenceManifestV1) -> OccurrenceProvenanceV1 {
    OccurrenceProvenanceV1 {
        source_manifest_blake3: manifest.source_manifest_blake3.clone(),
        source_manifest_bytes: manifest.source_manifest_bytes,
        segmentation_manifest_blake3: manifest.segmentation_manifest_blake3.clone(),
        segmentation_manifest_bytes: manifest.segmentation_manifest_bytes,
        tokenization_manifest_blake3: manifest.tokenization_manifest_blake3.clone(),
        tokenization_manifest_bytes: manifest.tokenization_manifest_bytes,
        parse_manifest_blake3: manifest.parse_manifest_blake3.clone(),
        parse_manifest_bytes: manifest.parse_manifest_bytes,
        interpretation_manifest_blake3: manifest.interpretation_manifest_blake3.clone(),
        interpretation_manifest_bytes: manifest.interpretation_manifest_bytes,
        catalog_manifest_blake3: manifest.catalog_manifest_blake3.clone(),
        catalog_manifest_bytes: manifest.catalog_manifest_bytes,
    }
}

fn validate_syntax_diagnostics(occurrence: &GameOccurrenceV1) -> Result<(), String> {
    for (index, diagnostic) in occurrence.syntax_diagnostics.iter().enumerate() {
        let ordinal = u64::try_from(index).map_err(|_| "Ordinal diagnostica oltre u64")?;
        if diagnostic.ordinal != ordinal
            || diagnostic.start_token >= diagnostic.end_token
            || diagnostic.start_byte >= diagnostic.end_byte
            || diagnostic.start_byte < occurrence.source_start
            || diagnostic.end_byte > occurrence.source_end
        {
            return Err("Diagnostica sintattica occurrence non valida".into());
        }
    }
    Ok(())
}

fn validate_interpretation_diagnostics(occurrence: &GameOccurrenceV1) -> Result<(), String> {
    for diagnostic in &occurrence.interpretation_diagnostics {
        paired(
            diagnostic.start_token,
            diagnostic.end_token,
            "token diagnostica interpretativa",
        )?;
        paired(
            diagnostic.start_byte,
            diagnostic.end_byte,
            "byte diagnostica interpretativa",
        )?;
        if diagnostic
            .start_token
            .zip(diagnostic.end_token)
            .is_some_and(|(start, end)| start >= end)
        {
            return Err("Intervallo token diagnostica interpretativa non valido".into());
        }
        if diagnostic
            .start_byte
            .zip(diagnostic.end_byte)
            .is_some_and(|(start, end)| {
                start >= end || start < occurrence.source_start || end > occurrence.source_end
            })
        {
            return Err("Intervallo byte diagnostica interpretativa non valido".into());
        }
    }
    Ok(())
}

fn paired<T>(start: Option<T>, end: Option<T>, label: &str) -> Result<(), String> {
    if start.is_some() == end.is_some() {
        Ok(())
    } else {
        Err(format!("Intervallo {label} incompleto"))
    }
}

fn valid_hash(hash: &str) -> Result<(), String> {
    if hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err("Hash BLAKE3 occurrence non valido".into())
    }
}
