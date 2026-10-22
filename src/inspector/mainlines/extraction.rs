use super::manifest::*;
use crate::inspector::interpretation::manifest::{
    GameRecordV1, InitialPositionStatusV1, LineStatusV1, MoveStatusV1, SideToMoveV1, VariantKindV1,
    VariantStatusV1,
};
use crate::inspector::occurrences::GameOccurrenceV1;
use crate::inspector::parsing::model::{NodeKind, ParseTree};
use crate::inspector::tokenization::{LoadedStreamingTokenization, TokenStream};

pub(in crate::inspector) struct MainlineExtractionContext<'a> {
    pub(in crate::inspector) occurrences_manifest_blake3: &'a str,
    pub(in crate::inspector) occurrences_manifest_bytes: u64,
    pub(in crate::inspector) interpretation_manifest_blake3: &'a str,
    pub(in crate::inspector) interpretation_manifest_bytes: u64,
    pub(in crate::inspector) tokenization: &'a LoadedStreamingTokenization,
}

pub(in crate::inspector) fn extract_one(
    cx: &MainlineExtractionContext<'_>,
    occurrence: &GameOccurrenceV1,
    game: &GameRecordV1,
    tree: &ParseTree,
    stream: &TokenStream,
) -> Result<CanonicalMainlineV1, String> {
    if occurrence.ordinal != game.id || occurrence.interpretation_game_id != game.id {
        return Err("Occurrence non allineata al gioco Interpretation".into());
    }
    let line = game.lines.first().ok_or("Interpretation senza mainline")?;
    if line.id != 0
        || line.parent_line_id.is_some()
        || line.variation_node_id.is_some()
        || line.depth != 0
        || line.replaces_move_id.is_some()
    {
        return Err("Struttura fissa della mainline non valida".into());
    }
    let moves = line
        .move_ids
        .iter()
        .enumerate()
        .map(|(ordinal, id)| {
            let mv = game
                .moves
                .get(usize::try_from(*id).map_err(|_| "move_id oltre usize")?)
                .ok_or("move_id mainline inesistente")?;
            if mv.id != *id || mv.line_id != 0 || mv.ordinal_in_line != ordinal as u64 {
                return Err("Allineamento mossa mainline non valido".into());
            }
            Ok(MainlineMoveV1 {
                ordinal_in_mainline: mv.ordinal_in_line,
                interpretation_move_id: mv.id,
                start_token: mv.start_token,
                end_token: mv.end_token,
                start_byte: mv.start_byte,
                end_byte: mv.end_byte,
                raw_san: mv.raw_san.clone(),
                status: mv.status,
                previous_position: mv.previous_position.clone(),
                resulting_position: mv.resulting_position.clone(),
                normalized: mv.normalized.clone(),
                source_was_canonical_san: mv.source_was_canonical_san,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let results = extract_results(cx.tokenization, tree, stream, &line.result_node_ids)?;
    let initial_fen = game.initial_position.as_ref().map(initial_fen);
    let reasons = reasons(game, line.status, &moves);
    let comparable = reasons.is_empty();
    let canonical_mainline_id = if comparable {
        let fen = initial_fen
            .as_deref()
            .ok_or("Mainline confrontabile senza posizione iniziale")?;
        Some(canonical_mainline_id(fen, &moves)?)
    } else {
        None
    };
    Ok(CanonicalMainlineV1 {
        ordinal: occurrence.ordinal,
        occurrence_id: occurrence.occurrence_id.clone(),
        canonical_mainline_id,
        comparable,
        noncomparability_reasons: reasons,
        variant: game.variant,
        interpretation_status: game.status,
        initial_position_status: game.initial_position_status,
        initial_position: game.initial_position.clone(),
        initial_fen,
        mainline_status: line.status,
        moves,
        results,
        provenance: MainlineProvenanceV1 {
            occurrences_manifest_blake3: cx.occurrences_manifest_blake3.to_owned(),
            occurrences_manifest_bytes: cx.occurrences_manifest_bytes,
            interpretation_manifest_blake3: cx.interpretation_manifest_blake3.to_owned(),
            interpretation_manifest_bytes: cx.interpretation_manifest_bytes,
            occurrence: occurrence.provenance.clone(),
        },
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::inspector) fn extract_one_from_blob_for_probe(
    occurrences_manifest_blake3: &str,
    occurrences_manifest_bytes: u64,
    interpretation_manifest_blake3: &str,
    interpretation_manifest_bytes: u64,
    occurrence: &GameOccurrenceV1,
    game: &GameRecordV1,
    tree: &ParseTree,
    stream: &TokenStream,
    blob: &[u8],
) -> Result<CanonicalMainlineV1, String> {
    if occurrence.ordinal != game.id || occurrence.interpretation_game_id != game.id {
        return Err("Occurrence non allineata al gioco Interpretation".into());
    }
    let line = game.lines.first().ok_or("Interpretation senza mainline")?;
    if line.id != 0
        || line.parent_line_id.is_some()
        || line.variation_node_id.is_some()
        || line.depth != 0
        || line.replaces_move_id.is_some()
    {
        return Err("Struttura fissa della mainline non valida".into());
    }
    let moves = line
        .move_ids
        .iter()
        .enumerate()
        .map(|(ordinal, id)| {
            let mv = game
                .moves
                .get(usize::try_from(*id).map_err(|_| "move_id oltre usize")?)
                .ok_or("move_id mainline inesistente")?;
            if mv.id != *id || mv.line_id != 0 || mv.ordinal_in_line != ordinal as u64 {
                return Err("Allineamento mossa mainline non valido".into());
            }
            Ok(MainlineMoveV1 {
                ordinal_in_mainline: mv.ordinal_in_line,
                interpretation_move_id: mv.id,
                start_token: mv.start_token,
                end_token: mv.end_token,
                start_byte: mv.start_byte,
                end_byte: mv.end_byte,
                raw_san: mv.raw_san.clone(),
                status: mv.status,
                previous_position: mv.previous_position.clone(),
                resulting_position: mv.resulting_position.clone(),
                normalized: mv.normalized.clone(),
                source_was_canonical_san: mv.source_was_canonical_san,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let results = extract_results_from_blob(tree, stream, &line.result_node_ids, blob)?;
    let initial_fen = game.initial_position.as_ref().map(initial_fen);
    let reasons = reasons(game, line.status, &moves);
    let comparable = reasons.is_empty();
    let canonical_mainline_id = if comparable {
        let fen = initial_fen
            .as_deref()
            .ok_or("Mainline confrontabile senza posizione iniziale")?;
        Some(canonical_mainline_id(fen, &moves)?)
    } else {
        None
    };
    Ok(CanonicalMainlineV1 {
        ordinal: occurrence.ordinal,
        occurrence_id: occurrence.occurrence_id.clone(),
        canonical_mainline_id,
        comparable,
        noncomparability_reasons: reasons,
        variant: game.variant,
        interpretation_status: game.status,
        initial_position_status: game.initial_position_status,
        initial_position: game.initial_position.clone(),
        initial_fen,
        mainline_status: line.status,
        moves,
        results,
        provenance: MainlineProvenanceV1 {
            occurrences_manifest_blake3: occurrences_manifest_blake3.to_owned(),
            occurrences_manifest_bytes,
            interpretation_manifest_blake3: interpretation_manifest_blake3.to_owned(),
            interpretation_manifest_bytes,
            occurrence: occurrence.provenance.clone(),
        },
    })
}

fn extract_results_from_blob(
    tree: &ParseTree,
    stream: &TokenStream,
    ids: &[u64],
    blob: &[u8],
) -> Result<Vec<MainlineResultV1>, String> {
    ids.iter()
        .enumerate()
        .map(|(ordinal, id)| {
            let node = tree
                .nodes
                .get(usize::try_from(*id).map_err(|_| "result node_id oltre usize")?)
                .filter(|node| node.id == *id && node.kind == NodeKind::Result)
                .ok_or("result_node_id non identifica un nodo Result")?;
            let first = stream
                .tokens
                .get(usize::try_from(node.start_token).map_err(|_| "start_token oltre usize")?)
                .ok_or("start_token Result inesistente")?;
            let last_index = node
                .end_token
                .checked_sub(1)
                .ok_or("Intervallo token Result vuoto")?;
            let last = stream
                .tokens
                .get(usize::try_from(last_index).map_err(|_| "end_token oltre usize")?)
                .ok_or("end_token Result inesistente")?;
            let start = first
                .start
                .checked_sub(stream.start)
                .ok_or("Result prima del segmento")?;
            let end = last
                .end
                .checked_sub(stream.start)
                .ok_or("Result prima del segmento")?;
            let start = usize::try_from(start).map_err(|_| "Offset result oltre usize")?;
            let end = usize::try_from(end).map_err(|_| "Offset result oltre usize")?;
            let raw = blob.get(start..end).ok_or("Intervallo Result fuori blob")?;
            let value = std::str::from_utf8(raw)
                .map_err(|_| "Result non UTF-8")?
                .to_owned();
            if !matches!(value.as_str(), "1-0" | "0-1" | "1/2-1/2" | "*") {
                return Err("Valore Result non ammesso".into());
            }
            Ok(MainlineResultV1 {
                ordinal: u64::try_from(ordinal).map_err(|_| "Ordinal result oltre u64")?,
                node_id: *id,
                start_token: node.start_token,
                end_token: node.end_token,
                start_byte: first.start,
                end_byte: last.end,
                value,
            })
        })
        .collect()
}

fn extract_results(
    tokenization: &LoadedStreamingTokenization,
    tree: &ParseTree,
    stream: &TokenStream,
    ids: &[u64],
) -> Result<Vec<MainlineResultV1>, String> {
    ids.iter()
        .enumerate()
        .map(|(ordinal, id)| {
            let node = tree
                .nodes
                .get(usize::try_from(*id).map_err(|_| "result node_id oltre usize")?)
                .filter(|node| node.id == *id && node.kind == NodeKind::Result)
                .ok_or("result_node_id non identifica un nodo Result")?;
            let first = stream
                .tokens
                .get(usize::try_from(node.start_token).map_err(|_| "start_token oltre usize")?)
                .ok_or("start_token Result inesistente")?;
            let last_index = node
                .end_token
                .checked_sub(1)
                .ok_or("Intervallo token Result vuoto")?;
            let last = stream
                .tokens
                .get(usize::try_from(last_index).map_err(|_| "end_token oltre usize")?)
                .ok_or("end_token Result inesistente")?;
            let raw = tokenization.read_source_range(first.start, last.end)?;
            let value = std::str::from_utf8(&raw)
                .map_err(|_| "Result non UTF-8")?
                .to_owned();
            if !matches!(value.as_str(), "1-0" | "0-1" | "1/2-1/2" | "*") {
                return Err("Valore Result non ammesso".into());
            }
            Ok(MainlineResultV1 {
                ordinal: u64::try_from(ordinal).map_err(|_| "Ordinal result oltre u64")?,
                node_id: *id,
                start_token: node.start_token,
                end_token: node.end_token,
                start_byte: first.start,
                end_byte: last.end,
                value,
            })
        })
        .collect()
}

pub(super) fn initial_fen(
    position: &crate::inspector::interpretation::manifest::PositionRecordV1,
) -> String {
    let side = match position.side_to_move {
        SideToMoveV1::White => "w",
        SideToMoveV1::Black => "b",
    };
    let castling = if position.castling.is_empty() {
        "-"
    } else {
        &position.castling
    };
    let en_passant = position.en_passant.as_deref().unwrap_or("-");
    format!(
        "{} {side} {castling} {en_passant} {} {}",
        position.piece_placement, position.halfmove_clock, position.fullmove_number
    )
}

pub(super) fn reasons(
    game: &GameRecordV1,
    line_status: LineStatusV1,
    moves: &[MainlineMoveV1],
) -> Vec<NoncomparabilityReasonV1> {
    let mut result = Vec::new();
    if game.variant != VariantKindV1::Standard {
        result.push(NoncomparabilityReasonV1::NonStandardVariant);
    }
    if game.variant_status != VariantStatusV1::Supported {
        result.push(NoncomparabilityReasonV1::UnsupportedVariant);
    }
    if game.initial_position.is_none() {
        result.push(NoncomparabilityReasonV1::MissingInitialPosition);
    }
    match game.initial_position_status {
        InitialPositionStatusV1::Invalid => {
            result.push(NoncomparabilityReasonV1::InvalidInitialPosition)
        }
        InitialPositionStatusV1::Ambiguous => {
            result.push(NoncomparabilityReasonV1::AmbiguousInitialPosition)
        }
        _ => {}
    }
    if line_status != LineStatusV1::Complete {
        result.push(NoncomparabilityReasonV1::MainlineNotComplete);
    }
    if moves.is_empty() {
        result.push(NoncomparabilityReasonV1::EmptyMainline);
    }
    if moves.iter().any(|mv| {
        mv.status != MoveStatusV1::Applied
            || mv.previous_position.is_none()
            || mv.resulting_position.is_none()
            || mv.normalized.is_none()
            || mv.source_was_canonical_san.is_none()
    }) {
        result.push(NoncomparabilityReasonV1::NonAppliedMove);
    }
    result
}

pub(super) fn canonical_mainline_id(
    initial_fen: &str,
    moves: &[MainlineMoveV1],
) -> Result<String, String> {
    let mut preimage = b"antiskill_canonical_mainline_v1\0standard\0".to_vec();
    preimage.extend_from_slice(initial_fen.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(moves.len().to_string().as_bytes());
    preimage.push(0);
    for mv in moves {
        let uci = mv
            .normalized
            .as_ref()
            .ok_or("Mossa confrontabile senza normalized")?
            .uci
            .as_bytes();
        preimage.extend_from_slice(uci);
        preimage.push(0);
    }
    preimage.push(b'\n');
    Ok(blake3::hash(&preimage).to_hex().to_string())
}
