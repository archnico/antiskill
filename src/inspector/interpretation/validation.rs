use super::manifest::*;
use crate::inspector::common::validate_relative_path;
use std::collections::HashSet;

pub(super) fn validate_interpretation_manifest(m: &InterpretationManifestV1) -> Result<(), String> {
    validate_interpretation_header(
        m.schema_version,
        &m.interpretation_profile,
        &m.parse_manifest,
        &m.parse_manifest_blake3,
        m.parse_manifest_bytes,
        &m.source_blake3,
        m.source_bytes,
    )?;
    for (index, game) in m.games.iter().enumerate() {
        validate_game(game, index as u64)?;
    }
    Ok(())
}

pub(super) fn validate_interpretation_header(
    schema_version: u32,
    interpretation_profile: &str,
    parse_manifest: &str,
    parse_manifest_blake3: &str,
    parse_manifest_bytes: u64,
    source_blake3: &str,
    source_bytes: u64,
) -> Result<(), String> {
    if schema_version != INTERPRETATION_SCHEMA_VERSION {
        return Err(format!(
            "Versione Interpretation non supportata: {}",
            schema_version
        ));
    }
    if interpretation_profile != INTERPRETATION_PROFILE {
        return Err("Profilo Interpretation non supportato".to_owned());
    }
    validate_relative_path(parse_manifest, "parse_manifest")?;
    valid_hash(parse_manifest_blake3)?;
    valid_hash(source_blake3)?;
    if parse_manifest_bytes == 0 || source_bytes == 0 {
        return Err("Identità Parse o Source vuota".to_owned());
    }
    Ok(())
}

fn valid_hash(value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err("Hash BLAKE3 non valido".to_owned())
    }
}

pub(super) fn validate_game(g: &GameRecordV1, expected: u64) -> Result<(), String> {
    if g.id != expected || g.parse_tree_index != expected {
        return Err("ID partita o parse_tree_index non consecutivo".to_owned());
    }
    let main = g
        .lines
        .first()
        .ok_or("Ogni partita deve avere almeno una linea")?;
    if main.id != 0
        || main.parent_line_id.is_some()
        || main.variation_node_id.is_some()
        || main.depth != 0
        || main.replaces_move_id.is_some()
    {
        return Err("Mainline non valida".to_owned());
    }
    for (i, m) in g.moves.iter().enumerate() {
        if m.id != i as u64 {
            return Err("ID mosse non consecutivi".to_owned());
        }
        if m.line_id as usize >= g.lines.len() {
            return Err("line_id della mossa non valido".to_owned());
        }
        if m.start_token >= m.end_token || m.start_byte >= m.end_byte {
            return Err("Intervallo mossa vuoto o invertito".to_owned());
        }
        match m.status {
            MoveStatusV1::Applied
                if m.previous_position.is_none()
                    || m.resulting_position.is_none()
                    || m.normalized.is_none()
                    || m.source_was_canonical_san.is_none() =>
            {
                return Err("Mossa applied incompleta".to_owned())
            }
            MoveStatusV1::Applied => {}
            _ if m.resulting_position.is_some()
                || m.normalized.is_some()
                || m.source_was_canonical_san.is_some() =>
            {
                return Err("Mossa non applied con risultato".to_owned())
            }
            MoveStatusV1::BlockedByPriorError | MoveStatusV1::AfterResult
                if m.previous_position.is_some() =>
            {
                return Err("Mossa bloccata con previous_position".to_owned())
            }
            _ => {}
        }
    }
    let mut listed = vec![0_u8; g.moves.len()];
    for (i, line) in g.lines.iter().enumerate() {
        if line.id != i as u64 {
            return Err("ID linee non consecutivi".to_owned());
        }
        if i > 0 {
            let parent = line.parent_line_id.ok_or("RAV senza parent_line_id")?;
            if parent >= line.id {
                return Err("Parent linea inesistente o successivo".to_owned());
            }
            if line.depth != g.lines[parent as usize].depth + 1 {
                return Err("Depth figlio incoerente".to_owned());
            }
            if line.variation_node_id.is_none() {
                return Err("RAV senza variation_node_id".to_owned());
            }
            if let Some(replaced) = line.replaces_move_id {
                let mv = g
                    .moves
                    .get(replaced as usize)
                    .ok_or("replaces_move_id inesistente")?;
                if mv.line_id != parent {
                    return Err("Mossa sostituita non appartiene al parent".to_owned());
                }
            }
        }
        let mut ids = HashSet::new();
        for (ordinal, id) in line.move_ids.iter().enumerate() {
            if !ids.insert(*id) {
                return Err("Mossa duplicata nella linea".to_owned());
            }
            let mv = g
                .moves
                .get(*id as usize)
                .ok_or("move_ids riferisce mossa inesistente")?;
            if mv.line_id != line.id || mv.ordinal_in_line != ordinal as u64 {
                return Err("Appartenenza o ordinal_in_line incoerente".to_owned());
            }
            listed[*id as usize] += 1;
        }
        if line.result_node_ids.iter().collect::<HashSet<_>>().len() != line.result_node_ids.len() {
            return Err("result_node_ids duplicati".to_owned());
        }
    }
    if listed.iter().any(|n| *n != 1) {
        return Err("Ogni mossa deve apparire esattamente in una linea".to_owned());
    }
    for tag in &g.relevant_tags {
        if tag.start_token >= tag.end_token || tag.start_byte >= tag.end_byte {
            return Err("Intervallo tag vuoto o invertito".to_owned());
        }
        match tag.decode_status {
            TagDecodeStatusV1::Decoded if tag.value_node_id.is_none() || tag.value.is_none() => {
                return Err("Tag decoded senza nodo o valore".to_owned())
            }
            TagDecodeStatusV1::InvalidEncoding | TagDecodeStatusV1::InvalidEscape
                if tag.value_node_id.is_none() || tag.value.is_some() =>
            {
                return Err("Tag non decodificato incoerente".to_owned())
            }
            TagDecodeStatusV1::MissingValue if tag.value.is_some() => {
                return Err("Tag MissingValue con valore decodificato".to_owned())
            }
            _ => {}
        }
    }
    for d in &g.diagnostics {
        if d.start_token.is_some() != d.end_token.is_some()
            || d.start_byte.is_some() != d.end_byte.is_some()
        {
            return Err("Intervallo diagnostica incompleto".to_owned());
        }
        if d.start_token.zip(d.end_token).is_some_and(|(a, b)| a >= b)
            || d.start_byte.zip(d.end_byte).is_some_and(|(a, b)| a >= b)
        {
            return Err("Intervallo diagnostica vuoto".to_owned());
        }
        if let Some(line) = d.line_id {
            if line as usize >= g.lines.len() {
                return Err("line_id diagnostica inesistente".to_owned());
            }
        }
        if let Some(id) = d.move_id {
            let mv = g
                .moves
                .get(id as usize)
                .ok_or("move_id diagnostica inesistente")?;
            if d.line_id.is_some_and(|line| line != mv.line_id) {
                return Err("line_id diagnostica incoerente con move_id".to_owned());
            }
        }
    }
    Ok(())
}
