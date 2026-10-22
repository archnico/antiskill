use crate::inspector::catalog::{self, QueryFilters};
use serde::{Deserialize, Serialize};

pub(super) const ARCHIVE_VIEW_SCHEMA_VERSION: u32 = 1;
pub(super) const ARCHIVE_VIEW_PROFILE: &str = "antiskill_archive_view_v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ArchiveViewManifestV1 {
    pub(super) schema_version: u32,
    pub(super) archive_view_profile: String,
    pub(super) collection: ArchiveViewCollectionV1,
    pub(super) filters: ArchiveViewFiltersV1,
    pub(super) occurrences: Vec<ArchiveViewOccurrenceV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ArchiveViewCollectionV1 {
    pub(super) path: String,
    pub(super) blake3: String,
    pub(super) bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ArchiveViewFiltersV1 {
    pub(super) player: Option<String>,
    pub(super) event: Option<String>,
    pub(super) date: Option<String>,
    pub(super) date_from: Option<String>,
    pub(super) date_to: Option<String>,
}

impl ArchiveViewFiltersV1 {
    pub(super) fn from_query(filters: &QueryFilters) -> Self {
        let canonical = filters.canonicalized();
        Self {
            player: canonical.player,
            event: canonical.event,
            date: canonical.date.map(|date| date.canonical_string()),
            date_from: canonical.date_from.map(|date| date.canonical_string()),
            date_to: canonical.date_to.map(|date| date.canonical_string()),
        }
    }

    pub(super) fn to_query(&self) -> Result<QueryFilters, String> {
        let filters = QueryFilters {
            player: self.player.clone(),
            event: self.event.clone(),
            date: self
                .date
                .as_deref()
                .map(catalog::query::parse_cli_date)
                .transpose()?,
            date_from: self
                .date_from
                .as_deref()
                .map(catalog::query::parse_cli_date)
                .transpose()?,
            date_to: self
                .date_to
                .as_deref()
                .map(catalog::query::parse_cli_date)
                .transpose()?,
        };
        catalog::query::validate_filters(&filters)?;
        Ok(filters)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ArchiveViewOccurrenceV1 {
    pub(super) source_id: u64,
    pub(super) game_id: u64,
    pub(super) occurrence_id: String,
    pub(super) canonical_mainline_id: Option<String>,
}
