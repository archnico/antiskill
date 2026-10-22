use super::manifest::*;
use crate::inspector::catalog::manifest::CatalogGameRecordV1;
use crate::inspector::interpretation::manifest::GameRecordV1;
use crate::inspector::parsing::model::{NodeStatus, ParseTree};
use crate::inspector::tokenization::TokenStream;

pub(super) fn occurrence_id(
    source_blake3: &str,
    source_bytes: u64,
    source_start: u64,
    source_end: u64,
    source_game_blake3: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"antiskill_game_occurrence_v1\0");
    hasher.update(source_blake3.as_bytes());
    hasher.update(b"\0");
    hasher.update(source_bytes.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(source_start.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(source_end.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(source_game_blake3.as_bytes());
    hasher.update(b"\n");
    hasher.finalize().to_hex().to_string()
}

#[allow(clippy::too_many_arguments)]
pub(in crate::inspector) fn occurrence(
    ordinal: u64,
    source: (&str, u64),
    catalog_game: &CatalogGameRecordV1,
    interpretation_game: &GameRecordV1,
    tree: &ParseTree,
    stream: &TokenStream,
    provenance: &OccurrenceProvenanceV1,
) -> Result<GameOccurrenceV1, String> {
    if catalog_game.id != ordinal
        || catalog_game.interpretation_game_id != interpretation_game.id
        || catalog_game.parse_tree_index != interpretation_game.parse_tree_index
        || catalog_game.parse_tree_index != ordinal
        || tree.segment_ordinal != catalog_game.segment_ordinal
        || stream.segment_ordinal != catalog_game.segment_ordinal
        || interpretation_game.segment_ordinal != catalog_game.segment_ordinal
        || stream.start != catalog_game.source_start
        || stream.end != catalog_game.source_end
    {
        return Err("Allineamento Catalog/Interpretation/Parse/TokenStream non valido".into());
    }
    let root = tree
        .nodes
        .first()
        .ok_or("ParseTree occurrence senza radice")?;
    let syntax_diagnostics = syntax_diagnostics(stream, tree)?;
    let interpretation_diagnostics = interpretation_game
        .diagnostics
        .iter()
        .map(|diagnostic| InterpretationDiagnosticV1 {
            code: diagnostic.code,
            line_id: diagnostic.line_id,
            node_id: diagnostic.node_id,
            move_id: diagnostic.move_id,
            start_token: diagnostic.start_token,
            end_token: diagnostic.end_token,
            start_byte: diagnostic.start_byte,
            end_byte: diagnostic.end_byte,
        })
        .collect();
    Ok(GameOccurrenceV1 {
        ordinal,
        occurrence_id: occurrence_id(
            source.0,
            source.1,
            catalog_game.source_start,
            catalog_game.source_end,
            &catalog_game.source_game_blake3,
        ),
        source_blake3: source.0.to_owned(),
        source_bytes: source.1,
        source_start: catalog_game.source_start,
        source_end: catalog_game.source_end,
        source_game_blake3: catalog_game.source_game_blake3.clone(),
        segment_ordinal: catalog_game.segment_ordinal,
        parse_tree_index: catalog_game.parse_tree_index,
        interpretation_game_id: catalog_game.interpretation_game_id,
        catalog_game_id: catalog_game.id,
        syntax_status: syntax_status(root.status)?,
        interpretation_status: catalog_game.interpretation_status,
        variant: catalog_game.variant,
        tags: catalog_game.tags.clone(),
        syntax_diagnostics,
        interpretation_diagnostics,
        provenance: provenance.clone(),
    })
}

fn syntax_status(status: NodeStatus) -> Result<SyntaxStatusV1, String> {
    match status {
        NodeStatus::Valid => Ok(SyntaxStatusV1::Valid),
        NodeStatus::Recovered => Ok(SyntaxStatusV1::Recovered),
        NodeStatus::Incomplete => Ok(SyntaxStatusV1::Incomplete),
        NodeStatus::Unparsed => Err("Radice GameCandidate con status unparsed non valida".into()),
    }
}

fn syntax_diagnostics(
    stream: &TokenStream,
    tree: &crate::inspector::parsing::model::ParseTree,
) -> Result<Vec<SyntaxDiagnosticV1>, String> {
    tree.diagnostics
        .iter()
        .map(|diagnostic| {
            let start = usize::try_from(diagnostic.start_token)
                .map_err(|_| "start_token diagnostica oltre usize".to_owned())?;
            let end = usize::try_from(diagnostic.end_token)
                .map_err(|_| "end_token diagnostica oltre usize".to_owned())?;
            let last = end
                .checked_sub(1)
                .ok_or("Intervallo token diagnostica vuoto")?;
            let first_token = stream
                .tokens
                .get(start)
                .ok_or("start_token diagnostica inesistente")?;
            let last_token = stream
                .tokens
                .get(last)
                .ok_or("end_token diagnostica inesistente")?;
            Ok(SyntaxDiagnosticV1 {
                ordinal: diagnostic.ordinal,
                code: diagnostic.code,
                node_id: diagnostic.node_id,
                start_token: diagnostic.start_token,
                end_token: diagnostic.end_token,
                start_byte: first_token.start,
                end_byte: last_token.end,
            })
        })
        .collect()
}
