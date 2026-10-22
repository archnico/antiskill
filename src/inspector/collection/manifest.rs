use serde::{Deserialize, Serialize};

pub(super) const COLLECTION_SCHEMA_VERSION: u32 = 1;
pub(super) const COLLECTION_PROFILE: &str = "antiskill_workspace_collection_v1";
pub(super) const COLLECTION_SCHEMA_VERSION_V2: u32 = 2;
pub(super) const COLLECTION_PROFILE_V2: &str = "antiskill_workspace_collection_v2";
pub(super) const COLLECTION_SCHEMA_VERSION_V3: u32 = 3;
pub(super) const COLLECTION_PROFILE_V3: &str = "antiskill_workspace_collection_v3";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct CollectionManifestV1 {
    pub(super) schema_version: u32,
    pub(super) collection_profile: String,
    pub(in crate::inspector) sources: Vec<CollectionSourceRecordV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct CollectionSourceRecordV1 {
    pub(super) id: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) catalog: String,
    pub(super) catalog_blake3: String,
    pub(super) catalog_bytes: u64,
    pub(in crate::inspector) games: u64,
    pub(super) observations: Vec<SourceObservationRecordV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceObservationRecordV1 {
    pub(super) canonical_path: String,
    pub(super) input_relative_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct CollectionManifestV2 {
    pub(super) schema_version: u32,
    pub(super) collection_profile: String,
    pub(in crate::inspector) sources: Vec<CollectionSourceRecordV2>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct CollectionSourceRecordV2 {
    pub(super) id: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) catalog: String,
    pub(super) catalog_blake3: String,
    pub(super) catalog_bytes: u64,
    pub(super) occurrences: String,
    pub(super) occurrences_blake3: String,
    pub(super) occurrences_bytes: u64,
    pub(super) mainlines: String,
    pub(super) mainlines_blake3: String,
    pub(super) mainlines_bytes: u64,
    pub(in crate::inspector) games: u64,
    pub(super) observations: Vec<SourceObservationRecordV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct CollectionManifestV3 {
    pub(super) schema_version: u32,
    pub(super) collection_profile: String,
    pub(in crate::inspector) sources: Vec<CollectionSourceRecordV3>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct CollectionSourceRecordV3 {
    pub(super) id: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) source_index: String,
    pub(super) source_index_blake3: String,
    pub(super) source_index_bytes: u64,
    pub(in crate::inspector) games: u64,
    pub(super) observations: Vec<SourceObservationRecordV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) enum CollectionManifest {
    V1(CollectionManifestV1),
    V2(CollectionManifestV2),
    V3(CollectionManifestV3),
}

impl CollectionManifest {
    pub(in crate::inspector) fn source_count(&self) -> usize {
        match self {
            Self::V1(m) => m.sources.len(),
            Self::V2(m) => m.sources.len(),
            Self::V3(m) => m.sources.len(),
        }
    }

    pub(in crate::inspector) fn games(&self) -> u64 {
        match self {
            Self::V1(m) => m.sources.iter().map(|source| source.games).sum(),
            Self::V2(m) => m.sources.iter().map(|source| source.games).sum(),
            Self::V3(m) => m.sources.iter().map(|source| source.games).sum(),
        }
    }
}
