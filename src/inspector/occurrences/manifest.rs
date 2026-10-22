use crate::inspector::catalog::manifest::{
    CatalogInterpretationStatusV1, CatalogTagRecordV1, CatalogVariantV1,
};
use crate::inspector::interpretation::manifest::DiagnosticCodeV1;
use crate::inspector::parsing::model::DiagnosticCode;
use serde::{Deserialize, Serialize};

pub(super) const OCCURRENCE_SCHEMA_VERSION: u32 = 1;
pub(super) const OCCURRENCE_PROFILE: &str = "antiskill_game_occurrence_v1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum SyntaxStatusV1 {
    Valid,
    Recovered,
    Incomplete,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct OccurrenceManifestV1 {
    pub(super) schema_version: u32,
    pub(super) occurrence_profile: String,
    pub(super) catalog_manifest: String,
    pub(super) catalog_manifest_blake3: String,
    pub(super) catalog_manifest_bytes: u64,
    pub(in crate::inspector) interpretation_manifest: String,
    pub(in crate::inspector) interpretation_manifest_blake3: String,
    pub(in crate::inspector) interpretation_manifest_bytes: u64,
    pub(super) parse_manifest: String,
    pub(super) parse_manifest_blake3: String,
    pub(super) parse_manifest_bytes: u64,
    pub(super) tokenization_manifest: String,
    pub(super) tokenization_manifest_blake3: String,
    pub(super) tokenization_manifest_bytes: u64,
    pub(super) segmentation_manifest: String,
    pub(super) segmentation_manifest_blake3: String,
    pub(super) segmentation_manifest_bytes: u64,
    pub(super) source_manifest: String,
    pub(super) source_manifest_blake3: String,
    pub(super) source_manifest_bytes: u64,
    pub(in crate::inspector) source_blake3: String,
    pub(in crate::inspector) source_bytes: u64,
    pub(in crate::inspector) occurrences: Vec<GameOccurrenceV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct GameOccurrenceV1 {
    pub(in crate::inspector) ordinal: u64,
    pub(in crate::inspector) occurrence_id: String,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) source_start: u64,
    pub(super) source_end: u64,
    pub(super) source_game_blake3: String,
    pub(super) segment_ordinal: u64,
    pub(super) parse_tree_index: u64,
    pub(in crate::inspector) interpretation_game_id: u64,
    pub(super) catalog_game_id: u64,
    pub(super) syntax_status: SyntaxStatusV1,
    pub(super) interpretation_status: CatalogInterpretationStatusV1,
    pub(super) variant: CatalogVariantV1,
    pub(super) tags: Vec<CatalogTagRecordV1>,
    pub(super) syntax_diagnostics: Vec<SyntaxDiagnosticV1>,
    pub(super) interpretation_diagnostics: Vec<InterpretationDiagnosticV1>,
    pub(in crate::inspector) provenance: OccurrenceProvenanceV1,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SyntaxDiagnosticV1 {
    pub(super) ordinal: u64,
    pub(super) code: DiagnosticCode,
    pub(super) node_id: u64,
    pub(super) start_token: u64,
    pub(super) end_token: u64,
    pub(super) start_byte: u64,
    pub(super) end_byte: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct InterpretationDiagnosticV1 {
    pub(super) code: DiagnosticCodeV1,
    pub(super) line_id: Option<u64>,
    pub(super) node_id: Option<u64>,
    pub(super) move_id: Option<u64>,
    pub(super) start_token: Option<u64>,
    pub(super) end_token: Option<u64>,
    pub(super) start_byte: Option<u64>,
    pub(super) end_byte: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct OccurrenceProvenanceV1 {
    pub(super) source_manifest_blake3: String,
    pub(super) source_manifest_bytes: u64,
    pub(super) segmentation_manifest_blake3: String,
    pub(super) segmentation_manifest_bytes: u64,
    pub(super) tokenization_manifest_blake3: String,
    pub(super) tokenization_manifest_bytes: u64,
    pub(super) parse_manifest_blake3: String,
    pub(super) parse_manifest_bytes: u64,
    pub(super) interpretation_manifest_blake3: String,
    pub(super) interpretation_manifest_bytes: u64,
    pub(super) catalog_manifest_blake3: String,
    pub(super) catalog_manifest_bytes: u64,
}
