use serde::{Deserialize, Serialize};

pub(super) const CATALOG_SCHEMA_VERSION: u32 = 1;
pub(super) const CATALOG_PROFILE: &str = "antiskill_game_catalog_v1";

#[derive(Debug, Default, Clone)]
pub(in crate::inspector) struct CatalogCounts {
    pub(in crate::inspector) games: usize,
    pub(in crate::inspector) tags: usize,
    pub(in crate::inspector) decoded_tags: usize,
    pub(in crate::inspector) undecodable_tags: usize,
}

impl CatalogCounts {
    pub(super) fn add_game(&mut self, game: &CatalogGameRecordV1) {
        self.games += 1;
        for tag in &game.tags {
            self.tags += 1;
            if tag.decode_status == TagDecodeStatusV1::Decoded {
                self.decoded_tags += 1;
            } else {
                self.undecodable_tags += 1;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::inspector) enum CatalogInterpretationStatusV1 {
    Complete,
    Partial,
    UnsupportedVariant,
    InvalidInitialPosition,
    AmbiguousInitialPosition,
    UnexpectedStructure,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::inspector) enum CatalogVariantV1 {
    Standard,
    Chess960,
    Other,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct CatalogManifestV1 {
    pub(super) schema_version: u32,
    pub(super) catalog_profile: String,
    pub(super) interpretation_manifest: String,
    pub(super) interpretation_manifest_blake3: String,
    pub(super) interpretation_manifest_bytes: u64,
    pub(super) parse_manifest_blake3: String,
    pub(super) parse_manifest_bytes: u64,
    pub(in crate::inspector) source_blake3: String,
    pub(in crate::inspector) source_bytes: u64,
    pub(in crate::inspector) games: Vec<CatalogGameRecordV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct CatalogGameRecordV1 {
    pub(in crate::inspector) id: u64,
    pub(in crate::inspector) interpretation_game_id: u64,
    pub(in crate::inspector) parse_tree_index: u64,
    pub(in crate::inspector) segment_ordinal: u64,
    pub(in crate::inspector) interpretation_status: CatalogInterpretationStatusV1,
    pub(in crate::inspector) variant: CatalogVariantV1,
    pub(in crate::inspector) source_start: u64,
    pub(in crate::inspector) source_end: u64,
    pub(in crate::inspector) source_game_blake3: String,
    pub(in crate::inspector) tags: Vec<CatalogTagRecordV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum NodeStatusV1 {
    Valid,
    Recovered,
    Incomplete,
    Unparsed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::inspector) enum TagDecodeStatusV1 {
    Decoded,
    MissingName,
    AmbiguousName,
    MissingValue,
    AmbiguousValue,
    InvalidNameEncoding,
    InvalidValueEncoding,
    InvalidEscape,
    InvalidStructure,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct CatalogTagRecordV1 {
    pub(super) ordinal: u64,
    pub(super) section_node_id: u64,
    pub(super) section_ordinal: u64,
    pub(super) tag_pair_node_id: u64,
    pub(super) node_status: NodeStatusV1,
    pub(super) name_node_ids: Vec<u64>,
    pub(super) value_node_ids: Vec<u64>,
    pub(in crate::inspector) decode_status: TagDecodeStatusV1,
    pub(in crate::inspector) name: Option<String>,
    pub(in crate::inspector) value: Option<String>,
    pub(super) start_token: u64,
    pub(super) end_token: u64,
    pub(super) start_byte: u64,
    pub(super) end_byte: u64,
    pub(super) name_start_token: Option<u64>,
    pub(super) name_end_token: Option<u64>,
    pub(super) name_start_byte: Option<u64>,
    pub(super) name_end_byte: Option<u64>,
    pub(super) value_start_token: Option<u64>,
    pub(super) value_end_token: Option<u64>,
    pub(super) value_start_byte: Option<u64>,
    pub(super) value_end_byte: Option<u64>,
}
