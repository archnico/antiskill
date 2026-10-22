use crate::inspector::interpretation::manifest::{
    GameStatusV1, InitialPositionStatusV1, LineStatusV1, MoveStatusV1, NormalizedMoveRecordV1,
    PositionRecordV1, VariantKindV1,
};
use crate::inspector::occurrences::OccurrenceProvenanceV1;
use serde::{Deserialize, Serialize};

pub(super) const MAINLINE_SCHEMA_VERSION: u32 = 1;
pub(super) const MAINLINE_PROFILE: &str = "antiskill_canonical_mainline_v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct MainlineManifestV1 {
    pub(super) schema_version: u32,
    pub(super) mainline_profile: String,
    pub(super) occurrences_manifest: String,
    pub(super) occurrences_manifest_blake3: String,
    pub(super) occurrences_manifest_bytes: u64,
    pub(super) interpretation_manifest: String,
    pub(super) interpretation_manifest_blake3: String,
    pub(super) interpretation_manifest_bytes: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) mainlines: Vec<CanonicalMainlineV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct CanonicalMainlineV1 {
    pub(super) ordinal: u64,
    pub(super) occurrence_id: String,
    pub(super) canonical_mainline_id: Option<String>,
    pub(super) comparable: bool,
    pub(super) noncomparability_reasons: Vec<NoncomparabilityReasonV1>,
    pub(super) variant: VariantKindV1,
    pub(super) interpretation_status: GameStatusV1,
    pub(super) initial_position_status: InitialPositionStatusV1,
    pub(super) initial_position: Option<PositionRecordV1>,
    pub(super) initial_fen: Option<String>,
    pub(super) mainline_status: LineStatusV1,
    pub(super) moves: Vec<MainlineMoveV1>,
    pub(super) results: Vec<MainlineResultV1>,
    pub(super) provenance: MainlineProvenanceV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum NoncomparabilityReasonV1 {
    NonStandardVariant,
    UnsupportedVariant,
    MissingInitialPosition,
    InvalidInitialPosition,
    AmbiguousInitialPosition,
    MainlineNotComplete,
    EmptyMainline,
    NonAppliedMove,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct MainlineMoveV1 {
    pub(super) ordinal_in_mainline: u64,
    pub(super) interpretation_move_id: u64,
    pub(super) start_token: u64,
    pub(super) end_token: u64,
    pub(super) start_byte: u64,
    pub(super) end_byte: u64,
    pub(super) raw_san: Option<String>,
    pub(super) status: MoveStatusV1,
    pub(super) previous_position: Option<PositionRecordV1>,
    pub(super) resulting_position: Option<PositionRecordV1>,
    pub(super) normalized: Option<NormalizedMoveRecordV1>,
    pub(super) source_was_canonical_san: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct MainlineResultV1 {
    pub(super) ordinal: u64,
    pub(super) node_id: u64,
    pub(super) start_token: u64,
    pub(super) end_token: u64,
    pub(super) start_byte: u64,
    pub(super) end_byte: u64,
    pub(super) value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct MainlineProvenanceV1 {
    pub(super) occurrences_manifest_blake3: String,
    pub(super) occurrences_manifest_bytes: u64,
    pub(super) interpretation_manifest_blake3: String,
    pub(super) interpretation_manifest_bytes: u64,
    pub(super) occurrence: OccurrenceProvenanceV1,
}
