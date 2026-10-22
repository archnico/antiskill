use super::model::*;
use super::validation::validate_game;
use super::{lines, validation::validate_interpretation_manifest};
use crate::inspector::chess_rules::{MoveKind, NormalizedMove, PositionComponents, SideToMove};
use crate::inspector::common::{
    deserialize_logical_json_with_identity, ensure_within, logical_json_reader,
    path_to_manifest_string, prepare_directory_chain, publish_compressed_json_stream,
    resolve_workspace_path, validate_relative_path, ProgressiveCompareWriter,
};
use crate::inspector::parsing::{
    load_parse_manifest, load_streaming_parse, model::ParseTree, LoadedStreamingParse,
};
use crate::inspector::tokenization::TokenStream;
use serde::de::IgnoredAny;
use serde::ser::{SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) const INTERPRETATION_SCHEMA_VERSION: u32 = 1;
pub(super) const INTERPRETATION_PROFILE: &str = "antiskill_standard_lines_v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct InterpretationManifestV1 {
    pub(super) schema_version: u32,
    pub(super) interpretation_profile: String,
    pub(super) parse_manifest: String,
    pub(super) parse_manifest_blake3: String,
    pub(super) parse_manifest_bytes: u64,
    pub(in crate::inspector) source_blake3: String,
    pub(in crate::inspector) source_bytes: u64,
    pub(in crate::inspector) games: Vec<GameRecordV1>,
}

macro_rules! persistent_enum {
    ($vis:vis $name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
        #[serde(rename_all = "snake_case")]
        $vis enum $name { $($variant),+ }
    };
}
persistent_enum!(pub(in crate::inspector) VariantKindV1 {
    Standard,
    Chess960,
    Other,
    Unknown
});
persistent_enum!(pub(in crate::inspector) VariantStatusV1 {
    Supported,
    Unsupported,
    Ambiguous
});
persistent_enum!(pub(in crate::inspector) InitialPositionStatusV1 {
    Default,
    Explicit,
    Recovered,
    Invalid,
    Ambiguous,
    UnsupportedVariant
});
persistent_enum!(pub(in crate::inspector) GameStatusV1 {
    Complete,
    Partial,
    UnsupportedVariant,
    InvalidInitialPosition,
    AmbiguousInitialPosition,
    UnexpectedStructure
});
persistent_enum!(pub(in crate::inspector) MoveStatusV1 {
    Applied,
    InvalidEncoding,
    InvalidSan,
    AmbiguousSan,
    IllegalMove,
    BlockedByPriorError,
    UnsupportedVariant,
    UnknownInitialPosition,
    AfterResult
});
persistent_enum!(pub(in crate::inspector) LineStatusV1 {
    Complete,
    Partial,
    UnsupportedVariant,
    UnknownStartPosition,
    UnexpectedStructure
});
persistent_enum!(pub(super) TagDecodeStatusV1 {
    Decoded,
    InvalidEncoding,
    InvalidEscape,
    MissingValue
});
persistent_enum!(pub(in crate::inspector) SideToMoveV1 { White, Black });
persistent_enum!(pub(in crate::inspector) MoveKindV1 {
    Normal,
    CastleKingside,
    CastleQueenside,
    EnPassant,
    Promotion
});
persistent_enum!(pub(in crate::inspector) DiagnosticCodeV1 {
    InvalidTagEncoding,
    InvalidTagEscape,
    ConflictingVariantTags,
    UnsupportedVariant,
    FenWithoutSetup,
    SetupWithoutFen,
    InvalidSetupTag,
    InvalidFen,
    NoncanonicalFen,
    ConflictingFenTags,
    AmbiguousInitialPosition,
    InvalidSanEncoding,
    InvalidSan,
    AmbiguousSan,
    IllegalMove,
    BlockedByPriorError,
    MoveAfterResult,
    UnexpectedCstStructure,
    InvalidNodeTokenRange,
    InvalidNodeByteRange,
    VariationWithoutPrecedingMove,
    VariationAfterResult,
    UnknownVariationStart,
    EmptyVariation,
    VariationDepthExceeded,
    UnexpectedVariationStructure
});

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct PositionRecordV1 {
    pub(in crate::inspector) piece_placement: String,
    pub(in crate::inspector) side_to_move: SideToMoveV1,
    pub(in crate::inspector) castling: String,
    pub(in crate::inspector) en_passant: Option<String>,
    pub(in crate::inspector) halfmove_clock: u32,
    pub(in crate::inspector) fullmove_number: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct NormalizedMoveRecordV1 {
    pub(in crate::inspector) uci: String,
    pub(in crate::inspector) from: String,
    pub(in crate::inspector) to: String,
    pub(in crate::inspector) promotion: Option<String>,
    pub(in crate::inspector) kind: MoveKindV1,
    pub(in crate::inspector) canonical_san: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct RelevantTagRecordV1 {
    pub(super) tag_pair_node_id: u64,
    pub(super) name_node_id: u64,
    pub(super) value_node_id: Option<u64>,
    pub(super) name: Option<String>,
    pub(super) value: Option<String>,
    pub(super) decode_status: TagDecodeStatusV1,
    pub(super) start_token: u64,
    pub(super) end_token: u64,
    pub(super) start_byte: u64,
    pub(super) end_byte: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct MoveRecordV1 {
    pub(in crate::inspector) id: u64,
    pub(in crate::inspector) line_id: u64,
    pub(in crate::inspector) ordinal_in_line: u64,
    pub(super) node_id: u64,
    pub(super) move_number_node_ids: Vec<u64>,
    pub(in crate::inspector) start_token: u64,
    pub(in crate::inspector) end_token: u64,
    pub(in crate::inspector) start_byte: u64,
    pub(in crate::inspector) end_byte: u64,
    pub(in crate::inspector) raw_san: Option<String>,
    pub(in crate::inspector) status: MoveStatusV1,
    pub(in crate::inspector) previous_position: Option<PositionRecordV1>,
    pub(in crate::inspector) resulting_position: Option<PositionRecordV1>,
    pub(in crate::inspector) normalized: Option<NormalizedMoveRecordV1>,
    pub(in crate::inspector) source_was_canonical_san: Option<bool>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct LineRecordV1 {
    pub(in crate::inspector) id: u64,
    pub(in crate::inspector) parent_line_id: Option<u64>,
    pub(in crate::inspector) variation_node_id: Option<u64>,
    pub(in crate::inspector) depth: u32,
    pub(in crate::inspector) status: LineStatusV1,
    pub(super) start_position: Option<PositionRecordV1>,
    pub(in crate::inspector) replaces_move_id: Option<u64>,
    pub(in crate::inspector) move_ids: Vec<u64>,
    pub(in crate::inspector) result_node_ids: Vec<u64>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct DiagnosticRecordV1 {
    pub(in crate::inspector) code: DiagnosticCodeV1,
    pub(in crate::inspector) line_id: Option<u64>,
    pub(in crate::inspector) node_id: Option<u64>,
    pub(in crate::inspector) move_id: Option<u64>,
    pub(in crate::inspector) start_token: Option<u64>,
    pub(in crate::inspector) end_token: Option<u64>,
    pub(in crate::inspector) start_byte: Option<u64>,
    pub(in crate::inspector) end_byte: Option<u64>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct GameRecordV1 {
    pub(in crate::inspector) id: u64,
    pub(in crate::inspector) parse_tree_index: u64,
    pub(in crate::inspector) segment_ordinal: u64,
    pub(in crate::inspector) status: GameStatusV1,
    pub(in crate::inspector) variant: VariantKindV1,
    pub(in crate::inspector) variant_status: VariantStatusV1,
    pub(in crate::inspector) initial_position_status: InitialPositionStatusV1,
    pub(in crate::inspector) initial_position: Option<PositionRecordV1>,
    pub(super) relevant_tags: Vec<RelevantTagRecordV1>,
    pub(in crate::inspector) moves: Vec<MoveRecordV1>,
    pub(in crate::inspector) lines: Vec<LineRecordV1>,
    pub(in crate::inspector) diagnostics: Vec<DiagnosticRecordV1>,
}

#[derive(Debug, Default)]
pub(in crate::inspector) struct InterpretationCounts {
    pub(in crate::inspector) games: usize,
    pub(in crate::inspector) complete_games: usize,
    pub(in crate::inspector) partial_games: usize,
    pub(in crate::inspector) unsupported_games: usize,
    pub(in crate::inspector) lines: usize,
    pub(in crate::inspector) variation_lines: usize,
    pub(in crate::inspector) complete_lines: usize,
    pub(in crate::inspector) partial_lines: usize,
    pub(in crate::inspector) unknown_start_lines: usize,
    pub(in crate::inspector) moves: usize,
    pub(in crate::inspector) applied_moves: usize,
    pub(in crate::inspector) failed_moves: usize,
    pub(in crate::inspector) blocked_moves: usize,
    pub(in crate::inspector) diagnostics: usize,
}

impl InterpretationCounts {
    fn add_game(&mut self, game: &GameRecordV1) {
        self.games += 1;
        match game.status {
            GameStatusV1::Complete => self.complete_games += 1,
            GameStatusV1::Partial => self.partial_games += 1,
            GameStatusV1::UnsupportedVariant => self.unsupported_games += 1,
            _ => {}
        }
        for line in &game.lines {
            self.lines += 1;
            if line.parent_line_id.is_some() {
                self.variation_lines += 1;
            }
            match line.status {
                LineStatusV1::Complete => self.complete_lines += 1,
                LineStatusV1::Partial => self.partial_lines += 1,
                LineStatusV1::UnknownStartPosition => self.unknown_start_lines += 1,
                _ => {}
            }
        }
        for m in &game.moves {
            self.moves += 1;
            match m.status {
                MoveStatusV1::Applied => self.applied_moves += 1,
                MoveStatusV1::InvalidEncoding
                | MoveStatusV1::InvalidSan
                | MoveStatusV1::AmbiguousSan
                | MoveStatusV1::IllegalMove => self.failed_moves += 1,
                MoveStatusV1::BlockedByPriorError => self.blocked_moves += 1,
                _ => {}
            }
        }
        self.diagnostics += game.diagnostics.len();
    }
}

#[derive(Debug)]
pub(in crate::inspector) struct CreatedInterpretation {
    pub(in crate::inspector) path: PathBuf,
    pub(in crate::inspector) manifest_blake3: String,
    pub(in crate::inspector) manifest_bytes: u64,
    pub(in crate::inspector) new_manifest: bool,
    pub(in crate::inspector) source_blake3: String,
    pub(in crate::inspector) source_bytes: u64,
    pub(in crate::inspector) parse_manifest_blake3: String,
    pub(in crate::inspector) parse_manifest_bytes: u64,
    pub(in crate::inspector) counts: InterpretationCounts,
}

impl CreatedInterpretation {
    pub(in crate::inspector) fn cli_counts(&self) -> Vec<(&'static str, String)> {
        let c = &self.counts;
        vec![
            ("SOURCE_BLAKE3", self.source_blake3.clone()),
            ("SOURCE_BYTES", self.source_bytes.to_string()),
            ("PARSE_BLAKE3", self.parse_manifest_blake3.clone()),
            ("PARSE_BYTES", self.parse_manifest_bytes.to_string()),
            ("GAMES", c.games.to_string()),
            ("COMPLETE_GAMES", c.complete_games.to_string()),
            ("PARTIAL_GAMES", c.partial_games.to_string()),
            ("UNSUPPORTED_GAMES", c.unsupported_games.to_string()),
            ("LINES", c.lines.to_string()),
            ("VARIATION_LINES", c.variation_lines.to_string()),
            ("COMPLETE_LINES", c.complete_lines.to_string()),
            ("PARTIAL_LINES", c.partial_lines.to_string()),
            ("UNKNOWN_START_LINES", c.unknown_start_lines.to_string()),
            ("MOVES", c.moves.to_string()),
            ("APPLIED_MOVES", c.applied_moves.to_string()),
            ("FAILED_MOVES", c.failed_moves.to_string()),
            ("BLOCKED_MOVES", c.blocked_moves.to_string()),
            ("DIAGNOSTICS", c.diagnostics.to_string()),
        ]
    }
}
#[allow(
    dead_code,
    reason = "loader identity is part of the internal verified API"
)]
pub(super) struct LoadedInterpretationManifest {
    pub(super) manifest: InterpretationManifestV1,
    pub(super) canonical_path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) bytes: Vec<u8>,
}

pub(in crate::inspector) struct VerifiedInterpretation {
    #[allow(dead_code, reason = "materialized games only serve test extract_games")]
    pub(in crate::inspector) manifest: InterpretationManifestV1,
    #[allow(
        dead_code,
        reason = "retained for symmetry with other verified wrappers"
    )]
    pub(in crate::inspector) canonical_path: PathBuf,
    pub(in crate::inspector) manifest_blake3: String,
    pub(in crate::inspector) manifest_bytes: u64,
    #[allow(
        dead_code,
        reason = "verified exact bytes are part of the reusable loader contract"
    )]
    pub(in crate::inspector) bytes: Vec<u8>,
    pub(in crate::inspector) parse: crate::inspector::parsing::LoadedParseManifest,
}

macro_rules! map_enum { ($value:expr, $from:ident, $to:ident, [$($v:ident),+]) => { match $value { $($from::$v => $to::$v),+ } }; }
fn position(p: &PositionComponents) -> PositionRecordV1 {
    PositionRecordV1 {
        piece_placement: p.piece_placement.clone(),
        side_to_move: map_enum!(p.side_to_move, SideToMove, SideToMoveV1, [White, Black]),
        castling: p.castling.clone(),
        en_passant: p.en_passant.clone(),
        halfmove_clock: p.halfmove_clock,
        fullmove_number: p.fullmove_number,
    }
}
fn normalized(n: &NormalizedMove) -> NormalizedMoveRecordV1 {
    NormalizedMoveRecordV1 {
        uci: n.uci.clone(),
        from: n.from.clone(),
        to: n.to.clone(),
        promotion: n.promotion.clone(),
        kind: map_enum!(
            n.kind,
            MoveKind,
            MoveKindV1,
            [
                Normal,
                CastleKingside,
                CastleQueenside,
                EnPassant,
                Promotion
            ]
        ),
        canonical_san: n.canonical_san.clone(),
    }
}

pub(in crate::inspector) fn validate_game_record_for_probe(
    record: &GameRecordV1,
    expected: u64,
) -> Result<(), String> {
    validate_game(record, expected)
}

pub(super) fn to_game_record(g: &InterpretedGame) -> GameRecordV1 {
    GameRecordV1 {
        id: g.id,
        parse_tree_index: g.parse_tree_index,
        segment_ordinal: g.segment_ordinal,
        status: map_enum!(
            g.status,
            GameInterpretationStatus,
            GameStatusV1,
            [
                Complete,
                Partial,
                UnsupportedVariant,
                InvalidInitialPosition,
                AmbiguousInitialPosition,
                UnexpectedStructure
            ]
        ),
        variant: map_enum!(
            g.variant,
            VariantKind,
            VariantKindV1,
            [Standard, Chess960, Other, Unknown]
        ),
        variant_status: map_enum!(
            g.variant_status,
            VariantStatus,
            VariantStatusV1,
            [Supported, Unsupported, Ambiguous]
        ),
        initial_position_status: map_enum!(
            g.initial_position_status,
            InitialPositionStatus,
            InitialPositionStatusV1,
            [
                Default,
                Explicit,
                Recovered,
                Invalid,
                Ambiguous,
                UnsupportedVariant
            ]
        ),
        initial_position: g.initial_position.as_ref().map(position),
        relevant_tags: g
            .relevant_tags
            .iter()
            .map(|t| RelevantTagRecordV1 {
                tag_pair_node_id: t.tag_pair_node_id,
                name_node_id: t.name_node_id,
                value_node_id: t.value_node_id,
                name: t.name.clone(),
                value: t.value.clone(),
                decode_status: map_enum!(
                    t.decode_status,
                    TagDecodeStatus,
                    TagDecodeStatusV1,
                    [Decoded, InvalidEncoding, InvalidEscape, MissingValue]
                ),
                start_token: t.start_token,
                end_token: t.end_token,
                start_byte: t.start_byte,
                end_byte: t.end_byte,
            })
            .collect(),
        moves: g
            .moves
            .iter()
            .map(|m| MoveRecordV1 {
                id: m.id,
                line_id: m.line_id,
                ordinal_in_line: m.ordinal_in_line,
                node_id: m.node_id,
                move_number_node_ids: m.move_number_node_ids.clone(),
                start_token: m.start_token,
                end_token: m.end_token,
                start_byte: m.start_byte,
                end_byte: m.end_byte,
                raw_san: m.raw_san.clone(),
                status: map_enum!(
                    m.status,
                    MoveInterpretationStatus,
                    MoveStatusV1,
                    [
                        Applied,
                        InvalidEncoding,
                        InvalidSan,
                        AmbiguousSan,
                        IllegalMove,
                        BlockedByPriorError,
                        UnsupportedVariant,
                        UnknownInitialPosition,
                        AfterResult
                    ]
                ),
                previous_position: m.previous_position.as_ref().map(position),
                resulting_position: m.resulting_position.as_ref().map(position),
                normalized: m.normalized.as_ref().map(normalized),
                source_was_canonical_san: m.source_was_canonical_san,
            })
            .collect(),
        lines: g
            .lines
            .iter()
            .map(|l| LineRecordV1 {
                id: l.id,
                parent_line_id: l.parent_line_id,
                variation_node_id: l.variation_node_id,
                depth: l.depth,
                status: map_enum!(
                    l.status,
                    LineStatus,
                    LineStatusV1,
                    [
                        Complete,
                        Partial,
                        UnsupportedVariant,
                        UnknownStartPosition,
                        UnexpectedStructure
                    ]
                ),
                start_position: l.start_position.as_ref().map(position),
                replaces_move_id: l.replaces_move_id,
                move_ids: l.move_ids.clone(),
                result_node_ids: l.result_node_ids.clone(),
            })
            .collect(),
        diagnostics: g
            .diagnostics
            .iter()
            .map(|d| DiagnosticRecordV1 {
                code: map_enum!(
                    d.code,
                    InterpretationDiagnosticCode,
                    DiagnosticCodeV1,
                    [
                        InvalidTagEncoding,
                        InvalidTagEscape,
                        ConflictingVariantTags,
                        UnsupportedVariant,
                        FenWithoutSetup,
                        SetupWithoutFen,
                        InvalidSetupTag,
                        InvalidFen,
                        NoncanonicalFen,
                        ConflictingFenTags,
                        AmbiguousInitialPosition,
                        InvalidSanEncoding,
                        InvalidSan,
                        AmbiguousSan,
                        IllegalMove,
                        BlockedByPriorError,
                        MoveAfterResult,
                        UnexpectedCstStructure,
                        InvalidNodeTokenRange,
                        InvalidNodeByteRange,
                        VariationWithoutPrecedingMove,
                        VariationAfterResult,
                        UnknownVariationStart,
                        EmptyVariation,
                        VariationDepthExceeded,
                        UnexpectedVariationStructure
                    ]
                ),
                line_id: d.line_id,
                node_id: d.node_id,
                move_id: d.move_id,
                start_token: d.start_token,
                end_token: d.end_token,
                start_byte: d.start_byte,
                end_byte: d.end_byte,
            })
            .collect(),
    }
}

fn normalized_parse_manifest(
    canonical_root: &Path,
    canonical_parse: &Path,
) -> Result<String, String> {
    let relative = canonical_parse
        .strip_prefix(canonical_root)
        .map_err(|_| "Parse fuori dal workspace".to_owned())?;
    let value = relative
        .to_str()
        .ok_or("Il percorso Parse non è UTF-8 valido")?;
    validate_relative_path(value, "parse_manifest")?;
    Ok(path_to_manifest_string(relative))
}

fn prepare_interpretations_directory(
    root: &Path,
    canonical_root: &Path,
) -> Result<PathBuf, String> {
    prepare_directory_chain(
        root,
        canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("interpretations", "directory interpretazioni"),
        ],
    )
}

pub(in crate::inspector) fn create_interpretation(
    root: &Path,
    parse_path: &Path,
) -> Result<CreatedInterpretation, String> {
    let loaded = load_streaming_parse(root, parse_path, None)?;
    let canonical_root = fs::canonicalize(root)
        .map_err(|e| format!("Workspace non valido {}: {e}", root.display()))?;
    let parse_manifest = normalized_parse_manifest(&canonical_root, &loaded.canonical_path)?;
    let directory = prepare_interpretations_directory(root, &canonical_root)?;
    let counts = RefCell::new(InterpretationCounts::default());
    let (target, new_manifest, hash, size) =
        publish_compressed_json_stream(&canonical_root, &directory, "Interpretation", |writer| {
            let data =
                serialize_interpretation_into(writer, &parse_manifest, &loaded, |_, _, _| Ok(()))?;
            *counts.borrow_mut() = data;
            Ok(())
        })?;
    let counts = counts.into_inner();
    Ok(CreatedInterpretation {
        path: PathBuf::from("inspector/interpretations").join(target.file_name().unwrap()),
        manifest_blake3: hash,
        manifest_bytes: size,
        new_manifest,
        source_blake3: loaded.manifest.source_blake3.clone(),
        source_bytes: loaded.manifest.source_bytes,
        parse_manifest_blake3: loaded.manifest_blake3.clone(),
        parse_manifest_bytes: loaded.manifest_bytes,
        counts,
    })
}
#[cfg(test)]
pub(super) fn load_interpretation_manifest(
    root: &Path,
    path: &Path,
    expected: Option<(&str, u64)>,
) -> Result<LoadedInterpretationManifest, String> {
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let canonical = fs::canonicalize(&resolved)
        .map_err(|e| format!("Interpretazione non valida {}: {e}", resolved.display()))?;
    ensure_within(&canonical_root, &canonical, "interpretation")?;
    let data = crate::inspector::common::logical_json_bytes(&canonical, "Interpretation")?;
    let size = data.len() as u64;
    let hash = blake3::hash(&data).to_hex().to_string();
    if let Some((h, s)) = expected {
        if h != hash || s != size {
            return Err("L'interpretazione non corrisponde all'identità attesa".to_owned());
        }
    }
    let manifest = serde_json::from_slice(&data)
        .map_err(|e| format!("Manifest Interpretation non valido: {e}"))?;
    validate_interpretation_manifest(&manifest)?;
    Ok(LoadedInterpretationManifest {
        manifest,
        canonical_path: canonical,
        manifest_blake3: hash,
        manifest_bytes: size,
        bytes: data,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct StreamingInterpretationHeader {
    pub(in crate::inspector) schema_version: u32,
    pub(in crate::inspector) interpretation_profile: String,
    pub(in crate::inspector) parse_manifest: String,
    pub(in crate::inspector) parse_manifest_blake3: String,
    pub(in crate::inspector) parse_manifest_bytes: u64,
    pub(in crate::inspector) source_blake3: String,
    pub(in crate::inspector) source_bytes: u64,
    #[serde(rename = "games")]
    _games: IgnoredAny,
}

pub(in crate::inspector) struct LoadedStreamingInterpretation {
    pub(in crate::inspector) manifest: StreamingInterpretationHeader,
    pub(in crate::inspector) canonical_path: PathBuf,
    pub(in crate::inspector) manifest_blake3: String,
    pub(in crate::inspector) manifest_bytes: u64,
    pub(in crate::inspector) parse: LoadedStreamingParse,
}

pub(in crate::inspector) fn load_streaming_interpretation(
    root: &Path,
    path: &Path,
    expected: Option<(&str, u64)>,
) -> Result<LoadedStreamingInterpretation, String> {
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let canonical_path = fs::canonicalize(&resolved)
        .map_err(|e| format!("Interpretazione non valida {}: {e}", resolved.display()))?;
    ensure_within(&canonical_root, &canonical_path, "interpretation")?;
    let (manifest, manifest_blake3, manifest_bytes): (StreamingInterpretationHeader, _, _) =
        deserialize_logical_json_with_identity(&canonical_path, "Interpretation")
            .map_err(|error| format!("Manifest Interpretation non valido: {error}"))?;
    if let Some((h, s)) = expected {
        if h != manifest_blake3 || s != manifest_bytes {
            return Err("L'interpretazione non corrisponde all'identità attesa".to_owned());
        }
    }
    super::validation::validate_interpretation_header(
        manifest.schema_version,
        &manifest.interpretation_profile,
        &manifest.parse_manifest,
        &manifest.parse_manifest_blake3,
        manifest.parse_manifest_bytes,
        &manifest.source_blake3,
        manifest.source_bytes,
    )?;
    let parse_path = validate_relative_path(&manifest.parse_manifest, "parse_manifest")?;
    let parse = load_streaming_parse(
        root,
        &parse_path,
        Some((
            &manifest.parse_manifest_blake3,
            manifest.parse_manifest_bytes,
        )),
    )?;
    let normalized_parse = normalized_parse_manifest(&canonical_root, &parse.canonical_path)?;
    if normalized_parse != manifest.parse_manifest {
        return Err("Il percorso Parse non è canonico".to_owned());
    }
    if manifest.source_blake3 != parse.tokenization.manifest.source_blake3
        || manifest.source_bytes != parse.tokenization.manifest.source_bytes
    {
        return Err("Interpretation incoerente con la parse".to_owned());
    }
    Ok(LoadedStreamingInterpretation {
        manifest,
        canonical_path,
        manifest_blake3,
        manifest_bytes,
        parse,
    })
}

#[derive(Serialize)]
struct StreamingInterpretationManifest<'a, T> {
    schema_version: u32,
    interpretation_profile: &'a str,
    parse_manifest: &'a str,
    parse_manifest_blake3: &'a str,
    parse_manifest_bytes: u64,
    source_blake3: &'a str,
    source_bytes: u64,
    games: T,
}

pub(super) fn serialize_interpretation_into<W, F>(
    writer: &mut W,
    parse_manifest: &str,
    parse: &LoadedStreamingParse,
    on_game: F,
) -> Result<InterpretationCounts, String>
where
    W: Write + ?Sized,
    F: FnMut(GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    let counts = RefCell::new(InterpretationCounts::default());
    let on_game = RefCell::new(on_game);
    let manifest = StreamingInterpretationManifest {
        schema_version: INTERPRETATION_SCHEMA_VERSION,
        interpretation_profile: INTERPRETATION_PROFILE,
        parse_manifest,
        parse_manifest_blake3: &parse.manifest_blake3,
        parse_manifest_bytes: parse.manifest_bytes,
        source_blake3: &parse.tokenization.manifest.source_blake3,
        source_bytes: parse.tokenization.manifest.source_bytes,
        games: ChangedGames {
            parse,
            counts: &counts,
            on_game: &on_game,
        },
    };
    serde_json::to_writer_pretty(&mut *writer, &manifest)
        .map_err(|error| format!("Errore serializzando l'interpretazione: {error}"))?;
    writer
        .write_all(b"\n")
        .map_err(|error| format!("Errore serializzando l'interpretazione: {error}"))?;
    Ok(counts.into_inner())
}

struct ChangedGames<'a, F> {
    parse: &'a LoadedStreamingParse,
    counts: &'a RefCell<InterpretationCounts>,
    on_game: &'a RefCell<F>,
}

impl<F> Serialize for ChangedGames<'_, F>
where
    F: FnMut(GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(None)?;
        let result = for_each_interpreted_game(self.parse, |record, tree, stream| {
            sequence
                .serialize_element(&record)
                .map_err(|error| error.to_string())?;
            (self.on_game.borrow_mut())(record, tree, stream).map_err(|error| error.to_string())
        });
        let counts = result.map_err(serde::ser::Error::custom)?;
        *self.counts.borrow_mut() = counts;
        sequence.end()
    }
}

pub(in crate::inspector) fn for_each_interpreted_game<F>(
    parse: &LoadedStreamingParse,
    mut on_game: F,
) -> Result<InterpretationCounts, String>
where
    F: FnMut(GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    let mut index = 0_u64;
    let mut counts = InterpretationCounts::default();
    parse.tokenization.verify_source()?;
    parse.for_each_tree(|tree, stream| {
        let id = index;
        index = index.checked_add(1).ok_or("Indice partita oltre u64")?;
        let blob = parse
            .tokenization
            .read_source_range(stream.start, stream.end)?;
        let game = lines::interpret_line_tree(id, &tree, Some(&stream), stream.start, &blob);
        let record = to_game_record(&game);
        validate_game(&record, id)?;
        counts.add_game(&record);
        on_game(record, &tree, &stream)?;
        Ok(())
    })?;
    parse.tokenization.verify_source()?;
    Ok(counts)
}

impl LoadedStreamingInterpretation {
    pub(in crate::inspector) fn for_each_fresh_game<F>(&self, consume: F) -> Result<usize, String>
    where
        F: FnMut(GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
    {
        Ok(for_each_interpreted_game(&self.parse, consume)?.games)
    }

    pub(in crate::inspector) fn for_each_verified_game<F>(
        &self,
        consume: F,
    ) -> Result<usize, String>
    where
        F: FnMut(GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
    {
        self.parse.tokenization.verify_source()?;
        let mut stored = logical_json_reader(&self.canonical_path, "Interpretation")?;
        let mut compare = ProgressiveCompareWriter::new(&mut stored);
        let counts = serialize_interpretation_into(
            &mut compare,
            &self.manifest.parse_manifest,
            &self.parse,
            consume,
        )?;
        compare.finish("Interpretation", &self.manifest_blake3, self.manifest_bytes)?;
        self.parse.tokenization.verify_source()?;
        Ok(counts.games)
    }
}

pub(in crate::inspector) fn load_verified_interpretation(
    root: &Path,
    path: &Path,
) -> Result<VerifiedInterpretation, String> {
    let loaded = load_streaming_interpretation(root, path, None)?;
    let parse_path = validate_relative_path(&loaded.manifest.parse_manifest, "parse_manifest")?;
    let parse = load_parse_manifest(
        root,
        &parse_path,
        Some((
            &loaded.manifest.parse_manifest_blake3,
            loaded.manifest.parse_manifest_bytes,
        )),
    )?;
    let mut games = Vec::new();
    loaded.for_each_verified_game(|game, _, _| {
        games.push(game);
        Ok(())
    })?;
    let manifest = InterpretationManifestV1 {
        schema_version: loaded.manifest.schema_version,
        interpretation_profile: loaded.manifest.interpretation_profile,
        parse_manifest: loaded.manifest.parse_manifest,
        parse_manifest_blake3: loaded.manifest.parse_manifest_blake3,
        parse_manifest_bytes: loaded.manifest.parse_manifest_bytes,
        source_blake3: loaded.manifest.source_blake3,
        source_bytes: loaded.manifest.source_bytes,
        games,
    };
    validate_interpretation_manifest(&manifest)?;
    let bytes =
        crate::inspector::common::logical_json_bytes(&loaded.canonical_path, "Interpretation")?;
    Ok(VerifiedInterpretation {
        manifest,
        canonical_path: loaded.canonical_path,
        manifest_blake3: loaded.manifest_blake3,
        manifest_bytes: loaded.manifest_bytes,
        bytes,
        parse,
    })
}

pub(in crate::inspector) fn verify_interpretation(root: &Path, path: &Path) -> Result<(), String> {
    let loaded = load_streaming_interpretation(root, path, None)?;
    loaded.for_each_verified_game(|_, _, _| Ok(()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspector::{parsing, segmentation, source, tokenization};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> (PathBuf, PathBuf) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("antiskill-manifest-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let pgn = root.join("source.pgn");
        fs::write(
            &pgn,
            b"[Variant \"Standard\"]\n\n1. e4 (1. d4 (1. Nf3)) e5 *\n",
        )
        .unwrap();
        let imported = source::import_source(&root, &pgn).unwrap();
        let segmented = segmentation::segment_source(&root, &imported.manifest_path).unwrap();
        let tokenized = tokenization::tokenize(&root, &segmented.path).unwrap();
        let parsed = parsing::parse(&root, &tokenized.path).unwrap();
        (root, parsed.path)
    }

    #[test]
    fn persistent_conversion_covers_mainline_and_nested_rav() {
        let (root, parse) = fixture();
        let created = create_interpretation(&root, &parse).unwrap();
        let manifest = load_interpretation_manifest(&root, &created.path, None)
            .unwrap()
            .manifest;
        assert_eq!(manifest.games.len(), 1);
        assert_eq!(manifest.games[0].lines.len(), 3);
        assert!(manifest.games[0]
            .moves
            .iter()
            .all(|m| m.previous_position.is_some()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persistent_json_is_deterministic_snake_case_null_and_has_no_timestamp() {
        let (root, parse) = fixture();
        let first = create_interpretation(&root, &parse).unwrap();
        let bytes = crate::inspector::common::logical_json_bytes(
            &root.join(&first.path),
            "interpretation test",
        )
        .unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("\"status\": \"applied\""));
        assert!(text.contains("\"promotion\": null"));
        assert!(!text.contains("timestamp"));
        assert!(!text.contains("unix_ms"));
        let second = create_interpretation(&root, &parse).unwrap();
        assert!(!second.new_manifest);
        assert_eq!(first.path, second.path);
        assert_eq!(first.manifest_blake3, second.manifest_blake3);
        assert_eq!(first.manifest_bytes, second.manifest_bytes);
        assert_eq!(
            bytes,
            crate::inspector::common::logical_json_bytes(
                &root.join(second.path),
                "interpretation test"
            )
            .unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn valid_manifest_loads_and_verifies_by_exact_recalculation() {
        let (root, parse) = fixture();
        let created = create_interpretation(&root, &parse).unwrap();
        let loaded = load_interpretation_manifest(
            &root,
            &created.path,
            Some((&created.manifest_blake3, created.manifest_bytes)),
        )
        .unwrap();
        let loaded_again = load_interpretation_manifest(&root, &created.path, None).unwrap();
        assert_eq!(loaded.manifest, loaded_again.manifest);
        assert_eq!(loaded.manifest_blake3, created.manifest_blake3);
        assert_eq!(loaded.manifest_bytes, created.manifest_bytes);
        verify_interpretation(&root, &created.path).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn structural_validation_rejects_schema_profile_ids_and_move_shape() {
        let (root, parse) = fixture();
        let created = create_interpretation(&root, &parse).unwrap();
        let mut m = load_interpretation_manifest(&root, &created.path, None)
            .unwrap()
            .manifest;
        m.schema_version = 2;
        assert!(validate_interpretation_manifest(&m).is_err());
        m.schema_version = 1;
        m.interpretation_profile = "other".to_owned();
        assert!(validate_interpretation_manifest(&m).is_err());
        m.interpretation_profile = INTERPRETATION_PROFILE.to_owned();
        m.games[0].id = 1;
        assert!(validate_interpretation_manifest(&m).is_err());
        m.games[0].id = 0;
        m.games[0].moves[0].normalized = None;
        assert!(validate_interpretation_manifest(&m).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    fn diagnostic() -> DiagnosticRecordV1 {
        DiagnosticRecordV1 {
            code: DiagnosticCodeV1::IllegalMove,
            line_id: Some(0),
            node_id: Some(1),
            move_id: Some(0),
            start_token: Some(1),
            end_token: Some(2),
            start_byte: Some(1),
            end_byte: Some(2),
        }
    }

    #[test]
    fn validation_rejects_every_named_identity_line_move_tag_and_diagnostic_corruption() {
        type Mutation = (&'static str, fn(&mut InterpretationManifestV1));
        let (root, parse) = fixture();
        let base = load_interpretation_manifest(
            &root,
            &create_interpretation(&root, &parse).unwrap().path,
            None,
        )
        .unwrap()
        .manifest;
        let mutations: Vec<Mutation> = vec![
            ("schema errato", |m| m.schema_version = 2),
            ("profilo errato", |m| {
                m.interpretation_profile = "bad".into()
            }),
            ("hash Parse malformato", |m| {
                m.parse_manifest_blake3 = "x".into()
            }),
            ("byte Parse zero", |m| m.parse_manifest_bytes = 0),
            ("hash Source malformato", |m| m.source_blake3 = "x".into()),
            ("byte Source zero", |m| m.source_bytes = 0),
            ("path Parse assoluto", |m| {
                m.parse_manifest = "/tmp/parse.json".into()
            }),
            ("path Parse traversal", |m| {
                m.parse_manifest = "../parse.json".into()
            }),
            ("game id non consecutivo", |m| m.games[0].id = 1),
            ("parse_tree_index incoerente", |m| {
                m.games[0].parse_tree_index = 1
            }),
            ("partita senza linee", |m| m.games[0].lines.clear()),
            ("mainline con parent", |m| {
                m.games[0].lines[0].parent_line_id = Some(0)
            }),
            ("mainline con variation", |m| {
                m.games[0].lines[0].variation_node_id = Some(1)
            }),
            ("mainline depth", |m| m.games[0].lines[0].depth = 1),
            ("mainline replaces", |m| {
                m.games[0].lines[0].replaces_move_id = Some(0)
            }),
            ("line id non consecutivo", |m| m.games[0].lines[1].id = 9),
            ("parent inesistente", |m| {
                m.games[0].lines[1].parent_line_id = Some(9)
            }),
            ("parent successivo", |m| {
                m.games[0].lines[1].parent_line_id = Some(2)
            }),
            ("depth incoerente", |m| m.games[0].lines[1].depth = 7),
            ("RAV senza variation", |m| {
                m.games[0].lines[1].variation_node_id = None
            }),
            ("replaces inesistente", |m| {
                m.games[0].lines[1].replaces_move_id = Some(999)
            }),
            ("replaces linea sbagliata", |m| {
                m.games[0].lines[1].replaces_move_id = Some(2)
            }),
            ("move_ids duplicati", |m| {
                let id = m.games[0].lines[0].move_ids[0];
                m.games[0].lines[0].move_ids.push(id)
            }),
            ("result ids duplicati", |m| {
                m.games[0].lines[0].result_node_ids = vec![1, 1]
            }),
            ("move id non consecutivo", |m| m.games[0].moves[0].id = 9),
            ("line_id invalido", |m| m.games[0].moves[0].line_id = 99),
            ("ordinal incoerente", |m| {
                m.games[0].moves[0].ordinal_in_line = 9
            }),
            ("mossa in due linee", |m| {
                m.games[0].lines[1].move_ids.push(0)
            }),
            ("mossa non elencata", |m| {
                m.games[0].lines[0].move_ids.remove(0);
            }),
            ("riferimento mossa inesistente", |m| {
                m.games[0].lines[0].move_ids[0] = 999
            }),
            ("Applied senza previous", |m| {
                m.games[0].moves[0].previous_position = None
            }),
            ("Applied senza resulting", |m| {
                m.games[0].moves[0].resulting_position = None
            }),
            ("Applied senza normalized", |m| {
                m.games[0].moves[0].normalized = None
            }),
            ("Applied senza canonicalita", |m| {
                m.games[0].moves[0].source_was_canonical_san = None
            }),
            ("non Applied con resulting", |m| {
                m.games[0].moves[0].status = MoveStatusV1::InvalidSan;
            }),
            ("non Applied con normalized", |m| {
                m.games[0].moves[0].status = MoveStatusV1::InvalidSan;
                m.games[0].moves[0].resulting_position = None;
                m.games[0].moves[0].source_was_canonical_san = None;
            }),
            ("non Applied con canonicalita", |m| {
                m.games[0].moves[0].status = MoveStatusV1::InvalidSan;
                m.games[0].moves[0].resulting_position = None;
                m.games[0].moves[0].normalized = None;
            }),
            ("Blocked con previous", |m| {
                m.games[0].moves[0].status = MoveStatusV1::BlockedByPriorError;
                m.games[0].moves[0].resulting_position = None;
                m.games[0].moves[0].normalized = None;
                m.games[0].moves[0].source_was_canonical_san = None;
            }),
            ("AfterResult con previous", |m| {
                m.games[0].moves[0].status = MoveStatusV1::AfterResult;
                m.games[0].moves[0].resulting_position = None;
                m.games[0].moves[0].normalized = None;
                m.games[0].moves[0].source_was_canonical_san = None;
            }),
            ("token move vuoto", |m| {
                m.games[0].moves[0].end_token = m.games[0].moves[0].start_token
            }),
            ("byte move vuoto", |m| {
                m.games[0].moves[0].end_byte = m.games[0].moves[0].start_byte
            }),
            ("Decoded senza nodo", |m| {
                m.games[0].relevant_tags[0].value_node_id = None
            }),
            ("Decoded senza value", |m| {
                m.games[0].relevant_tags[0].value = None
            }),
            ("InvalidEncoding con value", |m| {
                m.games[0].relevant_tags[0].decode_status = TagDecodeStatusV1::InvalidEncoding
            }),
            ("InvalidEscape con value", |m| {
                m.games[0].relevant_tags[0].decode_status = TagDecodeStatusV1::InvalidEscape
            }),
            ("tag token vuoto", |m| {
                m.games[0].relevant_tags[0].end_token = m.games[0].relevant_tags[0].start_token
            }),
            ("diagnostica token incompleto", |m| {
                let mut d = diagnostic();
                d.end_token = None;
                m.games[0].diagnostics.push(d)
            }),
            ("diagnostica intervallo invertito", |m| {
                let mut d = diagnostic();
                d.end_token = d.start_token;
                m.games[0].diagnostics.push(d)
            }),
            ("diagnostica line inesistente", |m| {
                let mut d = diagnostic();
                d.line_id = Some(99);
                m.games[0].diagnostics.push(d)
            }),
            ("diagnostica move inesistente", |m| {
                let mut d = diagnostic();
                d.move_id = Some(99);
                m.games[0].diagnostics.push(d)
            }),
            ("diagnostica line incoerente", |m| {
                let mut d = diagnostic();
                d.line_id = Some(1);
                m.games[0].diagnostics.push(d)
            }),
        ];
        for (name, mutate) in mutations {
            let mut candidate = base.clone();
            mutate(&mut candidate);
            assert!(
                validate_interpretation_manifest(&candidate).is_err(),
                "corruzione accettata: {name}"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_value_with_preserved_value_node_is_persisted_and_verified() {
        let root = test_root_for("missing-value");
        fs::create_dir_all(&root).unwrap();
        let pgn = root.join("source.pgn");
        fs::write(&pgn, b"[Event \"x\"]\n[Variant \"Standard\n").unwrap();
        let imported = source::import_source(&root, &pgn).unwrap();
        let segmented = segmentation::segment_source(&root, &imported.manifest_path).unwrap();
        let tokenized = tokenization::tokenize(&root, &segmented.path).unwrap();
        let parsed = parsing::parse(&root, &tokenized.path).unwrap();
        let created = create_interpretation(&root, &parsed.path).unwrap();
        let tag = &load_interpretation_manifest(&root, &created.path, None)
            .unwrap()
            .manifest
            .games[0]
            .relevant_tags[0];
        assert_eq!(tag.decode_status, TagDecodeStatusV1::MissingValue);
        assert!(tag.value_node_id.is_some());
        assert!(tag.value.is_none());
        verify_interpretation(&root, &created.path).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    fn test_root_for(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("antiskill-{label}-{}-{nonce}", std::process::id()))
    }

    fn write_manifest(root: &Path, path: &Path, manifest: &InterpretationManifestV1) {
        let mut bytes = serde_json::to_vec_pretty(manifest).unwrap();
        bytes.push(b'\n');
        fs::write(root.join(path), bytes).unwrap();
    }

    #[test]
    fn verify_rejects_unknown_fields_and_every_noncanonical_json_representation() {
        let (root, parse) = fixture();
        let created = create_interpretation(&root, &parse).unwrap();
        let path = root.join(&created.path);
        let manifest = load_interpretation_manifest(&root, &created.path, None)
            .unwrap()
            .manifest;
        let canonical =
            crate::inspector::common::logical_json_bytes(&path, "interpretation test").unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), serde_json::Value::Bool(true));
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        assert!(
            verify_interpretation(&root, &created.path).is_err(),
            "campo sconosciuto accettato"
        );
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(
            verify_interpretation(&root, &created.path).is_err(),
            "whitespace/newline non canonici accettati"
        );
        let mut reordered = String::from_utf8(canonical.clone()).unwrap();
        let schema = reordered.find("  \"schema_version\"").unwrap();
        let profile = reordered.find("  \"interpretation_profile\"").unwrap();
        assert!(schema < profile);
        reordered=reordered.replacen("  \"schema_version\": 1,\n  \"interpretation_profile\": \"antiskill_standard_lines_v1\",", "  \"interpretation_profile\": \"antiskill_standard_lines_v1\",\n  \"schema_version\": 1,",1);
        fs::write(&path, reordered).unwrap();
        assert!(
            verify_interpretation(&root, &created.path).is_err(),
            "ordine campi non canonico accettato"
        );
        fs::write(&path, &canonical[..canonical.len() - 1]).unwrap();
        assert!(
            verify_interpretation(&root, &created.path).is_err(),
            "newline finale assente accettata"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verify_rejects_every_named_semantic_recalculation_difference() {
        type Mutation = (&'static str, fn(&mut InterpretationManifestV1));
        let (root, parse) = fixture();
        let created = create_interpretation(&root, &parse).unwrap();
        let base = load_interpretation_manifest(&root, &created.path, None)
            .unwrap()
            .manifest;
        let mutations: Vec<Mutation> = vec![
            ("raw_san", |m| {
                m.games[0].moves[0].raw_san = Some("d4".into())
            }),
            ("canonical_san", |m| {
                m.games[0].moves[0]
                    .normalized
                    .as_mut()
                    .unwrap()
                    .canonical_san = "d4".into()
            }),
            ("uci", |m| {
                m.games[0].moves[0].normalized.as_mut().unwrap().uci = "d2d4".into()
            }),
            ("previous_position", |m| {
                m.games[0].moves[0]
                    .previous_position
                    .as_mut()
                    .unwrap()
                    .halfmove_clock += 1
            }),
            ("resulting_position", |m| {
                m.games[0].moves[0]
                    .resulting_position
                    .as_mut()
                    .unwrap()
                    .halfmove_clock += 1
            }),
            ("line start_position", |m| {
                m.games[0].lines[0]
                    .start_position
                    .as_mut()
                    .unwrap()
                    .halfmove_clock += 1
            }),
            ("parent_line_id", |m| {
                m.games[0].lines[2].parent_line_id = Some(0)
            }),
            ("replaces_move_id", |m| {
                m.games[0].lines[1].replaces_move_id = None
            }),
            ("ordine lines", |m| m.games[0].lines.swap(1, 2)),
            ("ordine moves", |m| m.games[0].moves.swap(0, 1)),
            ("ordine diagnostiche", |m| {
                m.games[0].diagnostics = vec![
                    diagnostic(),
                    DiagnosticRecordV1 {
                        code: DiagnosticCodeV1::InvalidSan,
                        ..diagnostic()
                    },
                ];
                m.games[0].diagnostics.swap(0, 1)
            }),
            ("hash Parse", |m| m.parse_manifest_blake3 = "0".repeat(64)),
            ("byte Parse", |m| m.parse_manifest_bytes += 1),
            ("hash Source", |m| m.source_blake3 = "0".repeat(64)),
            ("byte Source", |m| m.source_bytes += 1),
        ];
        for (name, mutate) in mutations {
            let mut candidate = base.clone();
            mutate(&mut candidate);
            write_manifest(&root, &created.path, &candidate);
            assert!(
                verify_interpretation(&root, &created.path).is_err(),
                "differenza ricalcolo accettata: {name}"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn canonical_parse_path_alias_and_external_manifest_symlink_are_rejected() {
        use std::os::unix::fs::symlink;
        let (root, parse) = fixture();
        let created = create_interpretation(&root, &parse).unwrap();
        let mut alias = load_interpretation_manifest(&root, &created.path, None)
            .unwrap()
            .manifest
            .clone();
        let outside_manifest = alias.clone();
        let link = root.join("parse-alias.json");
        symlink(root.join(&parse), &link).unwrap();
        alias.parse_manifest = "parse-alias.json".into();
        write_manifest(&root, &created.path, &alias);
        assert!(
            verify_interpretation(&root, &created.path).is_err(),
            "alias Parse accettato"
        );
        let outside = root.with_extension("outside.json");
        let mut outside_bytes = serde_json::to_vec_pretty(&outside_manifest).unwrap();
        outside_bytes.push(b'\n');
        fs::write(&outside, outside_bytes).unwrap();
        let manifest_link = root.join("manifest-link.json");
        symlink(&outside, &manifest_link).unwrap();
        assert!(
            load_interpretation_manifest(&root, Path::new("manifest-link.json"), None).is_err()
        );
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(outside).unwrap();
    }

    #[test]
    fn publication_reuses_identical_rejects_collision_and_leaves_no_temporary() {
        let (root, parse) = fixture();
        let first = create_interpretation(&root, &parse).unwrap();
        let second = create_interpretation(&root, &parse).unwrap();
        assert!(!second.new_manifest);
        fs::write(root.join(&first.path), b"different").unwrap();
        assert!(
            create_interpretation(&root, &parse).is_err(),
            "collisione accettata"
        );
        let leftovers = fs::read_dir(root.join("inspector/interpretations"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|name| name.contains(".tmp"))
            })
            .count();
        assert_eq!(leftovers, 0, "temporaneo residuo");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn publication_rejects_target_and_directory_symlinks_without_external_writes() {
        use std::os::unix::fs::symlink;
        let (root, parse) = fixture();
        let created = create_interpretation(&root, &parse).unwrap();
        let target = root.join(&created.path);
        let outside = root.with_extension("target-outside");
        let outside_bytes = fs::read(&target).unwrap();
        fs::write(&outside, &outside_bytes).unwrap();
        fs::remove_file(&target).unwrap();
        symlink(&outside, &target).unwrap();
        assert!(
            create_interpretation(&root, &parse).is_err(),
            "target symlink accettato"
        );
        assert_eq!(fs::read(&outside).unwrap(), outside_bytes);
        fs::remove_file(&target).unwrap();
        let outside_dir = root.with_extension("interpretations-outside");
        fs::create_dir(&outside_dir).unwrap();
        fs::remove_dir(root.join("inspector/interpretations")).unwrap();
        symlink(&outside_dir, root.join("inspector/interpretations")).unwrap();
        assert!(
            create_interpretation(&root, &parse).is_err(),
            "directory symlink accettata"
        );
        assert_eq!(fs::read_dir(&outside_dir).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(outside).unwrap();
        fs::remove_dir(outside_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn inspector_component_symlink_is_rejected_before_external_creation() {
        use std::os::unix::fs::symlink;
        let root = test_root_for("inspector-link");
        let outside = root.with_extension("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.join("inspector")).unwrap();
        let canonical = fs::canonicalize(&root).unwrap();
        assert!(prepare_interpretations_directory(&root, &canonical).is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
