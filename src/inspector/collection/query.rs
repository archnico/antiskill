use super::LoadedCollection;
use crate::inspector::catalog::{self, manifest::TagDecodeStatusV1};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CollectionQueryResult {
    pub(crate) occurrence_ids_available: bool,
    pub(crate) matches: Vec<CollectionQueryMatch>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CollectionQueryMatch {
    pub(crate) occurrence_id: Option<String>,
    pub(crate) source_id: u64,
    pub(crate) game_id: u64,
    pub(crate) source_blake3: String,
    pub(crate) observation_paths: Vec<String>,
    pub(crate) segment_ordinal: u64,
    pub(crate) source_start: u64,
    pub(crate) source_end: u64,
    pub(crate) white: Vec<String>,
    pub(crate) black: Vec<String>,
    pub(crate) event: Vec<String>,
    pub(crate) date: Vec<String>,
    pub(crate) result: Vec<String>,
}

fn values<'a>(
    game: &'a crate::inspector::catalog::manifest::CatalogGameRecordV1,
    name: &str,
) -> Vec<&'a str> {
    game.tags
        .iter()
        .filter(|t| {
            t.decode_status == TagDecodeStatusV1::Decoded
                && t.name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .filter_map(|t| t.value.as_deref())
        .collect()
}

pub(super) fn run(
    collection: &LoadedCollection,
    filters: &catalog::QueryFilters,
) -> Result<CollectionQueryResult, String> {
    let mut rows = Vec::new();
    for (source_index, (source, verified)) in collection
        .sources()
        .into_iter()
        .zip(&collection.catalogs)
        .enumerate()
    {
        let observation_paths = source
            .observations
            .iter()
            .map(|observation| observation.canonical_path.clone())
            .collect::<Vec<_>>();
        for game in &verified.manifest.games {
            if crate::inspector::catalog::query::matches_game(game, filters) {
                let occurrence_id = collection
                    .occurrence_identities
                    .as_ref()
                    .map(|sources| {
                        sources
                            .get(source_index)
                            .and_then(|identities| {
                                usize::try_from(game.id)
                                    .ok()
                                    .and_then(|game_index| identities.get(game_index))
                            })
                            .filter(|identity| identity.ordinal == game.id)
                            .map(|identity| identity.occurrence_id.clone())
                            .ok_or("Occurrence query non allineata al Catalog")
                    })
                    .transpose()?;
                let owned_values = |name| {
                    values(game, name)
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                };
                rows.push(CollectionQueryMatch {
                    occurrence_id,
                    source_id: source.id,
                    game_id: game.id,
                    source_blake3: source.source_blake3.to_owned(),
                    observation_paths: observation_paths.clone(),
                    segment_ordinal: game.segment_ordinal,
                    source_start: game.source_start,
                    source_end: game.source_end,
                    white: owned_values("White"),
                    black: owned_values("Black"),
                    event: owned_values("Event"),
                    date: owned_values("Date"),
                    result: owned_values("Result"),
                });
            }
        }
    }
    Ok(CollectionQueryResult {
        occurrence_ids_available: collection.occurrence_identities.is_some(),
        matches: rows,
    })
}

pub(super) fn format_inspector(result: &CollectionQueryResult) -> Result<String, String> {
    let mut out = format!("MATCHES\t{}\n", result.matches.len());
    for game in &result.matches {
        let json = |values: &[String]| serde_json::to_string(values).map_err(|e| e.to_string());
        out.push_str(&format!(
            "GAME\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            game.source_id,
            game.game_id,
            game.source_blake3,
            json(&game.observation_paths)?,
            game.segment_ordinal,
            game.source_start,
            game.source_end,
            json(&game.white)?,
            json(&game.black)?,
            json(&game.event)?,
            json(&game.date)?,
            json(&game.result)?
        ));
    }
    Ok(out)
}
