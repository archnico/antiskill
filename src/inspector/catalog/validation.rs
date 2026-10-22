use super::manifest::*;
use super::StreamingCatalogRefs;
use crate::inspector::common::validate_relative_path;
use std::collections::HashSet;

pub(super) fn validate_header(h: &StreamingCatalogRefs<'_>) -> Result<(), String> {
    if h.schema_version != CATALOG_SCHEMA_VERSION {
        return Err("Versione Catalog non supportata".into());
    }
    if h.catalog_profile != CATALOG_PROFILE {
        return Err("Profilo Catalog non supportato".into());
    }
    validate_relative_path(h.interpretation_manifest, "interpretation_manifest")?;
    for hash in [
        h.interpretation_manifest_blake3,
        h.parse_manifest_blake3,
        h.source_blake3,
    ] {
        valid_hash(hash)?;
    }
    if h.interpretation_manifest_bytes == 0 || h.parse_manifest_bytes == 0 || h.source_bytes == 0 {
        return Err("Identità Catalog vuota".into());
    }
    Ok(())
}

pub(super) fn validate(m: &CatalogManifestV1) -> Result<(), String> {
    let header = StreamingCatalogRefs {
        schema_version: m.schema_version,
        catalog_profile: &m.catalog_profile,
        interpretation_manifest: &m.interpretation_manifest,
        interpretation_manifest_blake3: &m.interpretation_manifest_blake3,
        interpretation_manifest_bytes: m.interpretation_manifest_bytes,
        parse_manifest_blake3: &m.parse_manifest_blake3,
        parse_manifest_bytes: m.parse_manifest_bytes,
        source_blake3: &m.source_blake3,
        source_bytes: m.source_bytes,
    };
    validate_header(&header)?;
    let mut segments = HashSet::new();
    let mut previous_end = None;
    for (i, g) in m.games.iter().enumerate() {
        if !segments.insert(g.segment_ordinal) {
            return Err("segment_ordinal duplicato".into());
        }
        if previous_end.is_some_and(|end| end > g.source_start) {
            return Err("Intervalli gioco non ordinati o sovrapposti".into());
        }
        previous_end = Some(g.source_end);
        validate_catalog_game(i, g, m.source_bytes)?;
    }
    Ok(())
}

pub(super) fn validate_catalog_game(
    index: usize,
    g: &CatalogGameRecordV1,
    source_bytes: u64,
) -> Result<(), String> {
    let expected = u64::try_from(index).map_err(|_| "Indice gioco oltre u64")?;
    if g.id != expected || g.interpretation_game_id != expected || g.parse_tree_index != expected {
        return Err("ID gioco non consecutivo".into());
    }
    if g.source_start >= g.source_end || g.source_end > source_bytes {
        return Err("Intervallo gioco non valido".into());
    }
    valid_hash(&g.source_game_blake3)?;
    let mut last_section = 0;
    let mut tag_pair_node_ids = HashSet::new();
    for (j, t) in g.tags.iter().enumerate() {
        let expected = u64::try_from(j).map_err(|_| "Indice tag oltre u64")?;
        if t.ordinal != expected {
            return Err("Ordinal tag non consecutivo".into());
        }
        if j > 0 && t.section_ordinal < last_section {
            return Err("section_ordinal non ordinato".into());
        }
        last_section = t.section_ordinal;
        if !tag_pair_node_ids.insert(t.tag_pair_node_id) {
            return Err("tag_pair_node_id duplicato nella partita".into());
        }
        unique(&t.name_node_ids)?;
        unique(&t.value_node_ids)?;
        if t.start_token >= t.end_token
            || t.start_byte >= t.end_byte
            || t.start_byte < g.source_start
            || t.end_byte > g.source_end
        {
            return Err("Intervallo tag non valido".into());
        }
        pair(t.name_start_token, t.name_end_token, "token nome")?;
        pair(t.name_start_byte, t.name_end_byte, "byte nome")?;
        pair(t.value_start_token, t.value_end_token, "token valore")?;
        pair(t.value_start_byte, t.value_end_byte, "byte valore")?;
        singular_ranges(
            t.name_node_ids.len(),
            t.name_start_token,
            t.name_end_token,
            t.name_start_byte,
            t.name_end_byte,
            "nome",
        )?;
        singular_ranges(
            t.value_node_ids.len(),
            t.value_start_token,
            t.value_end_token,
            t.value_start_byte,
            t.value_end_byte,
            "valore",
        )?;
        for range in [
            t.name_start_token.zip(t.name_end_token),
            t.value_start_token.zip(t.value_end_token),
        ] {
            if range.is_some_and(|(a, b)| a >= b || a < t.start_token || b > t.end_token) {
                return Err("Intervallo candidato fuori TagPair".into());
            }
        }
        for range in [
            t.name_start_byte.zip(t.name_end_byte),
            t.value_start_byte.zip(t.value_end_byte),
        ] {
            if range.is_some_and(|(a, b)| a >= b || a < t.start_byte || b > t.end_byte) {
                return Err("Intervallo byte candidato fuori TagPair".into());
            }
        }
        coherent(t)?;
    }
    Ok(())
}
fn valid_hash(h: &str) -> Result<(), String> {
    if h.len() == 64
        && h.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err("Hash BLAKE3 non valido".into())
    }
}
fn unique(v: &[u64]) -> Result<(), String> {
    let mut s = HashSet::new();
    if v.iter().all(|x| s.insert(*x)) {
        Ok(())
    } else {
        Err("Node id duplicato".into())
    }
}
fn pair(a: Option<u64>, b: Option<u64>, label: &str) -> Result<(), String> {
    if a.is_some() == b.is_some() {
        Ok(())
    } else {
        Err(format!("Intervallo {label} incompleto"))
    }
}
fn singular_ranges(
    candidates: usize,
    start_token: Option<u64>,
    end_token: Option<u64>,
    start_byte: Option<u64>,
    end_byte: Option<u64>,
    label: &str,
) -> Result<(), String> {
    let token_complete = start_token.is_some() && end_token.is_some();
    let byte_complete = start_byte.is_some() && end_byte.is_some();
    if (candidates == 1 && token_complete && byte_complete)
        || (candidates != 1 && !token_complete && !byte_complete)
    {
        Ok(())
    } else {
        Err(format!("Intervalli singolari {label} incoerenti"))
    }
}
fn coherent(t: &CatalogTagRecordV1) -> Result<(), String> {
    let ok = match t.decode_status {
        TagDecodeStatusV1::Decoded => {
            t.name_node_ids.len() == 1
                && t.value_node_ids.len() == 1
                && t.name.is_some()
                && t.value.is_some()
        }
        TagDecodeStatusV1::MissingName => {
            t.name_node_ids.is_empty() && t.name.is_none() && t.value.is_none()
        }
        TagDecodeStatusV1::AmbiguousName => {
            t.name_node_ids.len() > 1 && t.name.is_none() && t.value.is_none()
        }
        TagDecodeStatusV1::InvalidNameEncoding => {
            t.name_node_ids.len() == 1 && t.name.is_none() && t.value.is_none()
        }
        TagDecodeStatusV1::MissingValue => {
            t.name_node_ids.len() == 1
                && t.value_node_ids.len() <= 1
                && t.name.is_some()
                && t.value.is_none()
        }
        TagDecodeStatusV1::AmbiguousValue => {
            t.name_node_ids.len() == 1
                && t.value_node_ids.len() > 1
                && t.name.is_some()
                && t.value.is_none()
        }
        TagDecodeStatusV1::InvalidValueEncoding | TagDecodeStatusV1::InvalidEscape => {
            t.name_node_ids.len() == 1
                && t.value_node_ids.len() == 1
                && t.name.is_some()
                && t.value.is_none()
        }
        TagDecodeStatusV1::InvalidStructure => t.name.is_none() && t.value.is_none(),
    };
    if ok {
        Ok(())
    } else {
        Err("decode_status incoerente".into())
    }
}
