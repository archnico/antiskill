use crate::inspector::chess_rules::{NormalizedMove, PositionComponents};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inspector) enum VariantKind {
    Standard,
    Chess960,
    Other,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inspector) enum VariantStatus {
    Supported,
    Unsupported,
    Ambiguous,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inspector) enum InitialPositionStatus {
    Default,
    Explicit,
    Recovered,
    Invalid,
    Ambiguous,
    UnsupportedVariant,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inspector) enum GameInterpretationStatus {
    Complete,
    Partial,
    UnsupportedVariant,
    InvalidInitialPosition,
    AmbiguousInitialPosition,
    UnexpectedStructure,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inspector) enum MoveInterpretationStatus {
    Applied,
    InvalidEncoding,
    InvalidSan,
    AmbiguousSan,
    IllegalMove,
    BlockedByPriorError,
    UnsupportedVariant,
    UnknownInitialPosition,
    AfterResult,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inspector) enum LineStatus {
    Complete,
    Partial,
    UnsupportedVariant,
    UnknownStartPosition,
    UnexpectedStructure,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inspector) enum TagDecodeStatus {
    Decoded,
    InvalidEncoding,
    InvalidEscape,
    MissingValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) struct RelevantTag {
    pub(super) tag_pair_node_id: u64,
    pub(super) name_node_id: u64,
    pub(super) value_node_id: Option<u64>,
    pub(super) name: Option<String>,
    pub(super) value: Option<String>,
    pub(super) decode_status: TagDecodeStatus,
    pub(super) start_token: u64,
    pub(super) end_token: u64,
    pub(super) start_byte: u64,
    pub(super) end_byte: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) struct InterpretedMove {
    pub(super) id: u64,
    pub(super) line_id: u64,
    pub(super) ordinal_in_line: u64,
    pub(super) node_id: u64,
    pub(super) move_number_node_ids: Vec<u64>,
    pub(super) start_token: u64,
    pub(super) end_token: u64,
    pub(super) start_byte: u64,
    pub(super) end_byte: u64,
    pub(super) raw_san: Option<String>,
    pub(super) status: MoveInterpretationStatus,
    pub(super) previous_position: Option<PositionComponents>,
    pub(super) resulting_position: Option<PositionComponents>,
    pub(super) normalized: Option<NormalizedMove>,
    pub(super) source_was_canonical_san: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inspector) enum InterpretationDiagnosticCode {
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
    UnexpectedVariationStructure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) struct InterpretationDiagnostic {
    pub(super) code: InterpretationDiagnosticCode,
    pub(super) line_id: Option<u64>,
    pub(super) node_id: Option<u64>,
    pub(super) move_id: Option<u64>,
    pub(super) start_token: Option<u64>,
    pub(super) end_token: Option<u64>,
    pub(super) start_byte: Option<u64>,
    pub(super) end_byte: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) struct InterpretedLine {
    pub(super) id: u64,
    pub(super) parent_line_id: Option<u64>,
    pub(super) variation_node_id: Option<u64>,
    pub(super) depth: u32,
    pub(super) status: LineStatus,
    pub(super) start_position: Option<PositionComponents>,
    pub(super) replaces_move_id: Option<u64>,
    pub(super) move_ids: Vec<u64>,
    pub(super) result_node_ids: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) struct InterpretedGame {
    pub(super) id: u64,
    pub(super) parse_tree_index: u64,
    pub(super) segment_ordinal: u64,
    pub(super) status: GameInterpretationStatus,
    pub(super) variant: VariantKind,
    pub(super) variant_status: VariantStatus,
    pub(super) initial_position_status: InitialPositionStatus,
    pub(super) initial_position: Option<PositionComponents>,
    pub(super) relevant_tags: Vec<RelevantTag>,
    pub(super) moves: Vec<InterpretedMove>,
    pub(super) lines: Vec<InterpretedLine>,
    pub(super) diagnostics: Vec<InterpretationDiagnostic>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) struct LineInterpretation {
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) games: Vec<InterpretedGame>,
}
