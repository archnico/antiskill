use super::manifest::*;
use super::StreamingMainlinesRefs;
use crate::inspector::common::validate_relative_path;

pub(super) fn validate_header(refs: &StreamingMainlinesRefs<'_>) -> Result<(), String> {
    if refs.schema_version != MAINLINE_SCHEMA_VERSION {
        return Err("Versione canonical mainline non supportata".into());
    }
    if refs.mainline_profile != MAINLINE_PROFILE {
        return Err("Profilo canonical mainline non supportato".into());
    }
    validate_relative_path(refs.occurrences_manifest, "occurrences_manifest")?;
    validate_relative_path(refs.interpretation_manifest, "interpretation_manifest")?;
    for hash in [
        refs.occurrences_manifest_blake3,
        refs.interpretation_manifest_blake3,
        refs.source_blake3,
    ] {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("Hash BLAKE3 canonical mainline non valido".into());
        }
    }
    if refs.occurrences_manifest_bytes == 0
        || refs.interpretation_manifest_bytes == 0
        || refs.source_bytes == 0
    {
        return Err("Identità upstream canonical mainline vuota".into());
    }
    Ok(())
}

fn refs_from_manifest(manifest: &MainlineManifestV1) -> StreamingMainlinesRefs<'_> {
    StreamingMainlinesRefs {
        schema_version: manifest.schema_version,
        mainline_profile: &manifest.mainline_profile,
        occurrences_manifest: &manifest.occurrences_manifest,
        occurrences_manifest_blake3: &manifest.occurrences_manifest_blake3,
        occurrences_manifest_bytes: manifest.occurrences_manifest_bytes,
        interpretation_manifest: &manifest.interpretation_manifest,
        interpretation_manifest_blake3: &manifest.interpretation_manifest_blake3,
        interpretation_manifest_bytes: manifest.interpretation_manifest_bytes,
        source_blake3: &manifest.source_blake3,
        source_bytes: manifest.source_bytes,
    }
}

pub(super) fn validate(manifest: &MainlineManifestV1) -> Result<(), String> {
    validate_header(&refs_from_manifest(manifest))?;
    let mut occurrences = std::collections::HashSet::new();
    for (ordinal, record) in manifest.mainlines.iter().enumerate() {
        if !occurrences.insert(&record.occurrence_id) {
            return Err("Ordinal o occurrence_id mainline non valido".into());
        }
        validate_mainline(
            ordinal,
            record,
            &manifest.occurrences_manifest_blake3,
            manifest.occurrences_manifest_bytes,
            &manifest.interpretation_manifest_blake3,
            manifest.interpretation_manifest_bytes,
        )?;
    }
    Ok(())
}

pub(super) fn validate_mainline(
    ordinal: usize,
    record: &CanonicalMainlineV1,
    occurrences_manifest_blake3: &str,
    occurrences_manifest_bytes: u64,
    interpretation_manifest_blake3: &str,
    interpretation_manifest_bytes: u64,
) -> Result<(), String> {
    if record.ordinal != ordinal as u64 {
        return Err("Ordinal o occurrence_id mainline non valido".into());
    }
    if record.comparable != record.noncomparability_reasons.is_empty()
        || record.comparable != record.canonical_mainline_id.is_some()
    {
        return Err("Confrontabilità canonical mainline incoerente".into());
    }
    if record.initial_position.is_some() != record.initial_fen.is_some() {
        return Err("Posizione iniziale e FEN non allineate".into());
    }
    if record.provenance.occurrences_manifest_blake3 != occurrences_manifest_blake3
        || record.provenance.occurrences_manifest_bytes != occurrences_manifest_bytes
        || record.provenance.interpretation_manifest_blake3 != interpretation_manifest_blake3
        || record.provenance.interpretation_manifest_bytes != interpretation_manifest_bytes
    {
        return Err("Provenance canonical mainline incoerente".into());
    }
    for (index, mv) in record.moves.iter().enumerate() {
        if mv.ordinal_in_mainline != index as u64 {
            return Err("Allineamento mosse canonical mainline non valido".into());
        }
        if mv.start_token >= mv.end_token || mv.start_byte >= mv.end_byte {
            return Err("Intervallo mossa canonical mainline non valido".into());
        }
    }
    for (index, result) in record.results.iter().enumerate() {
        if result.ordinal != index as u64 {
            return Err("Ordinal result canonical mainline non valido".into());
        }
        if result.start_token >= result.end_token || result.start_byte >= result.end_byte {
            return Err("Intervallo result canonical mainline non valido".into());
        }
        if !matches!(result.value.as_str(), "1-0" | "0-1" | "1/2-1/2" | "*") {
            return Err("Valore result canonical mainline non valido".into());
        }
    }
    Ok(())
}
