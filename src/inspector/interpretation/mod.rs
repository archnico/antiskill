mod lines;
pub(in crate::inspector) mod manifest;
mod model;
mod validation;

#[cfg(test)]
use super::parsing::LoadedParseManifest;
#[cfg(test)]
use std::path::Path;

pub(super) use manifest::{
    create_interpretation, load_streaming_interpretation, load_verified_interpretation,
    verify_interpretation,
};

#[derive(Debug)]
pub(in crate::inspector) struct ProbeLeanMainlineIdentity {
    pub(in crate::inspector) comparable: bool,
    pub(in crate::inspector) moves: u64,
    pub(in crate::inspector) results: u64,
    pub(in crate::inspector) identity: Option<String>,
}

pub(in crate::inspector) fn lean_mainline_identity_direct(
    id: u64,
    tree: &super::parsing::model::ParseTree,
    stream: &super::tokenization::TokenStream,
    blob: &[u8],
) -> ProbeLeanMainlineIdentity {
    let result = lines::lean_mainline_identity(id, tree, stream, blob);
    ProbeLeanMainlineIdentity {
        comparable: result.comparable,
        moves: result.moves,
        results: result.results,
        identity: result.identity,
    }
}

pub(in crate::inspector) fn lean_mainline_identity_tokens_fast_direct(
    stream: &super::tokenization::TokenStream,
    blob: &[u8],
) -> Option<(ProbeLeanMainlineIdentity, u64, u64)> {
    let (result, nodes, moves) = lines::lean_mainline_identity_tokens_fast(stream, blob)?;
    Some((
        ProbeLeanMainlineIdentity {
            comparable: result.comparable,
            moves: result.moves,
            results: result.results,
            identity: result.identity,
        },
        nodes,
        moves,
    ))
}

#[derive(Debug)]
pub(in crate::inspector) struct ProbeInterpretProfile {
    pub(in crate::inspector) record: manifest::GameRecordV1,
    pub(in crate::inspector) core: std::time::Duration,
    pub(in crate::inspector) core_view: std::time::Duration,
    pub(in crate::inspector) core_setup: std::time::Duration,
    pub(in crate::inspector) core_movetext: std::time::Duration,
    pub(in crate::inspector) core_finalize: std::time::Duration,
    pub(in crate::inspector) move_total: std::time::Duration,
    pub(in crate::inspector) move_range_raw: std::time::Duration,
    pub(in crate::inspector) move_position_clone: std::time::Duration,
    pub(in crate::inspector) move_apply_san: std::time::Duration,
    pub(in crate::inspector) move_count: u64,
    pub(in crate::inspector) move_apply_attempts: u64,
    pub(in crate::inspector) convert: std::time::Duration,
    pub(in crate::inspector) validate: std::time::Duration,
}

pub(in crate::inspector) fn interpret_one_direct_profiled(
    id: u64,
    tree: &super::parsing::model::ParseTree,
    stream: &super::tokenization::TokenStream,
    blob: &[u8],
) -> Result<ProbeInterpretProfile, String> {
    let started = std::time::Instant::now();
    let profiled = lines::interpret_line_tree_profiled(id, tree, Some(stream), stream.start, blob);
    let core = started.elapsed();
    let started = std::time::Instant::now();
    let record = manifest::to_game_record(&profiled.game);
    let convert = started.elapsed();
    let started = std::time::Instant::now();
    manifest::validate_game_record_for_probe(&record, id)?;
    let validate = started.elapsed();
    Ok(ProbeInterpretProfile {
        record,
        core,
        core_view: profiled.view,
        core_setup: profiled.setup,
        core_movetext: profiled.movetext,
        core_finalize: profiled.finalize,
        move_total: profiled.move_total,
        move_range_raw: profiled.move_range_raw,
        move_position_clone: profiled.move_position_clone,
        move_apply_san: profiled.move_apply_san,
        move_count: profiled.move_count,
        move_apply_attempts: profiled.move_apply_attempts,
        convert,
        validate,
    })
}

#[cfg(test)]
use model::*;

#[cfg(test)]
pub(super) fn interpret_lines(
    root: &Path,
    parse_path: &Path,
) -> Result<LineInterpretation, String> {
    let loaded = super::parsing::load_parse_manifest(root, parse_path, None)?;
    interpret_loaded_parse(&loaded)
}

#[cfg(test)]
pub(super) fn interpret_loaded_parse(
    loaded: &LoadedParseManifest,
) -> Result<LineInterpretation, String> {
    loaded.tokenization.verify_source()?;
    let games = loaded
        .manifest
        .trees
        .iter()
        .zip(&loaded.tokenization.manifest.streams)
        .enumerate()
        .map(|(index, (tree, stream))| {
            let blob = loaded
                .tokenization
                .read_source_range(stream.start, stream.end)?;
            Ok(lines::interpret_line_tree(
                index as u64,
                tree,
                Some(stream),
                stream.start,
                &blob,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    loaded.tokenization.verify_source()?;
    Ok(LineInterpretation {
        source_blake3: loaded.manifest.source_blake3.clone(),
        source_bytes: loaded.manifest.source_bytes,
        games,
    })
}

#[cfg(test)]
mod tests;
