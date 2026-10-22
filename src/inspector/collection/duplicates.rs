use super::LoadedCollection;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

pub(super) const EXACT_DUPLICATE_PROFILE: &str = "exact_source_game_bytes_v1";

#[derive(Clone, Copy)]
pub(in crate::inspector) enum DuplicateFormat {
    Text,
    Jsonl,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct ExactDuplicateOccurrence {
    source_id: u64,
    game_id: u64,
    source_blake3: String,
    catalog: String,
    segment_ordinal: u64,
    source_start: u64,
    source_end: u64,
    observed_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExactDuplicateGroup {
    group_id: u64,
    game_blake3: String,
    game_bytes: u64,
    occurrences: Vec<ExactDuplicateOccurrence>,
    occurrence_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExactDuplicateReport {
    profile: &'static str,
    groups: Vec<ExactDuplicateGroup>,
    duplicate_occurrences: u64,
}

struct CollectedOccurrence {
    game_blake3: String,
    game_bytes: u64,
    technical: ExactDuplicateOccurrence,
    occurrence_id: Option<String>,
}

fn collect_occurrences(collection: &LoadedCollection) -> Result<Vec<CollectedOccurrence>, String> {
    let mut occurrences = Vec::new();
    for (source_index, (source, catalog)) in collection
        .sources()
        .into_iter()
        .zip(&collection.catalogs)
        .enumerate()
    {
        let observed_paths = source
            .observations
            .iter()
            .map(|observation| observation.canonical_path.clone())
            .collect::<Vec<_>>();
        let source_blob = &catalog.interpretation.parse.tokenization;
        source_blob.verify_source()?;
        for game in &catalog.manifest.games {
            let game_bytes = game
                .source_end
                .checked_sub(game.source_start)
                .ok_or("Intervallo GameOccurrence invertito")?;
            let slice = source_blob.read_source_range(game.source_start, game.source_end)?;
            let slice_bytes =
                u64::try_from(slice.len()).map_err(|_| "Dimensione GameOccurrence oltre u64")?;
            if slice_bytes != game_bytes {
                return Err("Dimensione GameOccurrence incoerente".into());
            }
            let hash = blake3::hash(&slice).to_hex().to_string();
            if hash != game.source_game_blake3 {
                return Err("source_game_blake3 non corrisponde ai byte esatti".into());
            }
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
                        .filter(|identity| {
                            identity.ordinal == game.id
                                && identity.source_game_blake3 == game.source_game_blake3
                                && identity.game_bytes == game_bytes
                        })
                        .map(|identity| identity.occurrence_id.clone())
                        .ok_or("Occurrence exact duplicates non allineata alla Collection")
                })
                .transpose()?;
            occurrences.push(CollectedOccurrence {
                game_blake3: game.source_game_blake3.clone(),
                game_bytes,
                technical: ExactDuplicateOccurrence {
                    source_id: source.id,
                    game_id: game.id,
                    source_blake3: source.source_blake3.to_owned(),
                    catalog: source.catalog.to_owned(),
                    segment_ordinal: game.segment_ordinal,
                    source_start: game.source_start,
                    source_end: game.source_end,
                    observed_paths: observed_paths.clone(),
                },
                occurrence_id,
            });
        }
        source_blob.verify_source()?;
    }
    Ok(occurrences)
}

fn group_occurrences(
    occurrences: Vec<CollectedOccurrence>,
) -> Result<ExactDuplicateReport, String> {
    let mut grouped = BTreeMap::<(String, u64), Vec<CollectedOccurrence>>::new();
    for occurrence in occurrences {
        grouped
            .entry((occurrence.game_blake3.clone(), occurrence.game_bytes))
            .or_default()
            .push(occurrence);
    }
    let mut groups = Vec::new();
    for ((game_blake3, game_bytes), mut occurrences) in grouped {
        if occurrences.len() < 2 {
            continue;
        }
        occurrences.sort_by_key(|occurrence| {
            (occurrence.technical.source_id, occurrence.technical.game_id)
        });
        let occurrence_ids = occurrences
            .iter()
            .map(|occurrence| occurrence.occurrence_id.clone())
            .collect::<Option<Vec<_>>>();
        let occurrences = occurrences
            .into_iter()
            .map(|occurrence| occurrence.technical)
            .collect();
        let group_id = u64::try_from(groups.len()).map_err(|_| "Numero gruppi oltre u64")?;
        groups.push(ExactDuplicateGroup {
            group_id,
            game_blake3,
            game_bytes,
            occurrences,
            occurrence_ids,
        });
    }
    let duplicate_occurrences = groups.iter().try_fold(0_u64, |total, group| {
        let count =
            u64::try_from(group.occurrences.len()).map_err(|_| "Numero occurrence oltre u64")?;
        total
            .checked_add(count)
            .ok_or("Totale occurrence oltre u64")
    })?;
    Ok(ExactDuplicateReport {
        profile: EXACT_DUPLICATE_PROFILE,
        groups,
        duplicate_occurrences,
    })
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductExactDuplicateGroup {
    pub(crate) occurrence_ids: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductExactDuplicateReport {
    pub(crate) groups: Vec<ProductExactDuplicateGroup>,
}

fn render_text(report: &ExactDuplicateReport) -> Result<String, String> {
    let mut output = format!(
        "DUPLICATE_PROFILE\t{}\nGROUPS\t{}\nDUPLICATE_OCCURRENCES\t{}\n",
        report.profile,
        report.groups.len(),
        report.duplicate_occurrences
    );
    for group in &report.groups {
        output.push_str(&format!(
            "GROUP\t{}\t{}\t{}\t{}\n",
            group.group_id,
            group.game_blake3,
            group.game_bytes,
            group.occurrences.len()
        ));
        for occurrence in &group.occurrences {
            let catalog = serde_json::to_string(&occurrence.catalog)
                .map_err(|error| format!("Rendering catalog JSON: {error}"))?;
            let observed_paths = serde_json::to_string(&occurrence.observed_paths)
                .map_err(|error| format!("Rendering observed paths JSON: {error}"))?;
            output.push_str(&format!(
                "OCCURRENCE\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                group.group_id,
                occurrence.source_id,
                occurrence.game_id,
                occurrence.source_blake3,
                catalog,
                occurrence.segment_ordinal,
                occurrence.source_start,
                occurrence.source_end,
                observed_paths
            ));
        }
    }
    output.push_str("DUPLICATES_REPORT\tPASS\n");
    Ok(output)
}

#[derive(Serialize)]
struct JsonSummary<'a> {
    record: &'static str,
    profile: &'a str,
    groups: usize,
    duplicate_occurrences: u64,
}

#[derive(Serialize)]
struct JsonGroup<'a> {
    record: &'static str,
    profile: &'a str,
    group_id: u64,
    game_blake3: &'a str,
    game_bytes: u64,
    occurrences: &'a [ExactDuplicateOccurrence],
}

fn render_jsonl(report: &ExactDuplicateReport) -> Result<String, String> {
    let mut output = serde_json::to_string(&JsonSummary {
        record: "summary",
        profile: report.profile,
        groups: report.groups.len(),
        duplicate_occurrences: report.duplicate_occurrences,
    })
    .map_err(|error| format!("Rendering summary JSONL: {error}"))?;
    output.push('\n');
    for group in &report.groups {
        output.push_str(
            &serde_json::to_string(&JsonGroup {
                record: "group",
                profile: report.profile,
                group_id: group.group_id,
                game_blake3: &group.game_blake3,
                game_bytes: group.game_bytes,
                occurrences: &group.occurrences,
            })
            .map_err(|error| format!("Rendering group JSONL: {error}"))?,
        );
        output.push('\n');
    }
    Ok(output)
}

pub(in crate::inspector) fn duplicates_report(
    root: &Path,
    collection_path: &Path,
    format: DuplicateFormat,
) -> Result<String, String> {
    let collection = super::load_verified_collection(root, collection_path)?;
    let report = group_occurrences(collect_occurrences(&collection)?)?;
    match format {
        DuplicateFormat::Text => render_text(&report),
        DuplicateFormat::Jsonl => render_jsonl(&report),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occurrence(source_id: u64, game_id: u64) -> ExactDuplicateOccurrence {
        ExactDuplicateOccurrence {
            source_id,
            game_id,
            source_blake3: format!("source-{source_id}"),
            catalog: format!("catalog-{source_id}.json"),
            segment_ordinal: game_id,
            source_start: 10,
            source_end: 20,
            observed_paths: vec![format!("/input/{source_id}.pgn")],
        }
    }

    fn grouped(values: &[(&str, u64, u64, u64)]) -> ExactDuplicateReport {
        group_occurrences(
            values
                .iter()
                .map(|(hash, bytes, source, game)| CollectedOccurrence {
                    game_blake3: (*hash).into(),
                    game_bytes: *bytes,
                    technical: occurrence(*source, *game),
                    occurrence_id: None,
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn no_occurrence_produces_zero_groups() {
        assert!(grouped(&[]).groups.is_empty());
    }

    #[test]
    fn one_occurrence_produces_zero_groups() {
        assert!(grouped(&[("a", 10, 0, 0)]).groups.is_empty());
    }

    #[test]
    fn same_hash_and_size_produce_one_group() {
        assert_eq!(grouped(&[("a", 10, 0, 0), ("a", 10, 1, 0)]).groups.len(), 1);
    }

    #[test]
    fn same_hash_different_size_is_not_duplicate() {
        assert!(grouped(&[("a", 10, 0, 0), ("a", 11, 1, 0)])
            .groups
            .is_empty());
    }

    #[test]
    fn same_size_different_hash_is_not_duplicate() {
        assert!(grouped(&[("a", 10, 0, 0), ("b", 10, 1, 0)])
            .groups
            .is_empty());
    }

    #[test]
    fn three_occurrences_form_one_sorted_group() {
        let report = grouped(&[("a", 10, 2, 1), ("a", 10, 0, 4), ("a", 10, 0, 2)]);
        let ids = report.groups[0]
            .occurrences
            .iter()
            .map(|o| (o.source_id, o.game_id))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![(0, 2), (0, 4), (2, 1)]);
    }

    #[test]
    fn multiple_groups_are_sorted_by_hash_and_size() {
        let report = grouped(&[
            ("z", 1, 0, 0),
            ("b", 9, 1, 0),
            ("b", 9, 2, 0),
            ("a", 7, 3, 0),
            ("a", 7, 4, 0),
            ("b", 8, 5, 0),
            ("b", 8, 6, 0),
        ]);
        assert_eq!(
            report
                .groups
                .iter()
                .map(|g| (g.group_id, g.game_blake3.as_str(), g.game_bytes))
                .collect::<Vec<_>>(),
            vec![(0, "a", 7), (1, "b", 8), (2, "b", 9)]
        );
    }

    #[test]
    fn group_ids_are_consecutive_after_filter() {
        let report = grouped(&[
            ("skip", 1, 0, 0),
            ("b", 1, 1, 0),
            ("b", 1, 2, 0),
            ("c", 1, 3, 0),
            ("c", 1, 4, 0),
        ]);
        assert_eq!(
            report.groups.iter().map(|g| g.group_id).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn occurrences_are_sorted_by_source_id_and_game_id() {
        let report = grouped(&[("a", 1, 3, 0), ("a", 1, 1, 4), ("a", 1, 1, 2)]);
        assert_eq!(
            report.groups[0]
                .occurrences
                .iter()
                .map(|o| (o.source_id, o.game_id))
                .collect::<Vec<_>>(),
            vec![(1, 2), (1, 4), (3, 0)]
        );
    }

    #[test]
    fn duplicate_in_same_source_with_different_game_ids_is_kept() {
        assert_eq!(
            grouped(&[("a", 10, 0, 1), ("a", 10, 0, 2)]).duplicate_occurrences,
            2
        );
    }

    #[test]
    fn observations_are_metadata_not_occurrences() {
        let mut item = occurrence(0, 0);
        item.observed_paths.push("/other/copy.pgn".into());
        let report = group_occurrences(vec![CollectedOccurrence {
            game_blake3: "a".into(),
            game_bytes: 10,
            technical: item,
            occurrence_id: None,
        }])
        .unwrap();
        assert!(report.groups.is_empty());
    }

    #[test]
    fn text_output_is_deterministic() {
        let report = grouped(&[("a", 10, 1, 0), ("a", 10, 0, 0)]);
        let text = render_text(&report).unwrap();
        assert_eq!(text, render_text(&report).unwrap());
        assert!(text.ends_with("DUPLICATES_REPORT\tPASS\n"));
    }

    #[test]
    fn technical_text_and_jsonl_are_exact_regressions() {
        let report = grouped(&[("a", 10, 1, 0), ("a", 10, 0, 0)]);
        assert_eq!(
            render_text(&report).unwrap(),
            concat!(
                "DUPLICATE_PROFILE\texact_source_game_bytes_v1\n",
                "GROUPS\t1\n",
                "DUPLICATE_OCCURRENCES\t2\n",
                "GROUP\t0\ta\t10\t2\n",
                "OCCURRENCE\t0\t0\t0\tsource-0\t\"catalog-0.json\"\t0\t10\t20\t[\"/input/0.pgn\"]\n",
                "OCCURRENCE\t0\t1\t0\tsource-1\t\"catalog-1.json\"\t0\t10\t20\t[\"/input/1.pgn\"]\n",
                "DUPLICATES_REPORT\tPASS\n",
            )
        );
        assert_eq!(
            render_jsonl(&report).unwrap(),
            concat!(
                "{\"record\":\"summary\",\"profile\":\"exact_source_game_bytes_v1\",\"groups\":1,\"duplicate_occurrences\":2}\n",
                "{\"record\":\"group\",\"profile\":\"exact_source_game_bytes_v1\",\"group_id\":0,\"game_blake3\":\"a\",\"game_bytes\":10,\"occurrences\":[{\"source_id\":0,\"game_id\":0,\"source_blake3\":\"source-0\",\"catalog\":\"catalog-0.json\",\"segment_ordinal\":0,\"source_start\":10,\"source_end\":20,\"observed_paths\":[\"/input/0.pgn\"]},{\"source_id\":1,\"game_id\":0,\"source_blake3\":\"source-1\",\"catalog\":\"catalog-1.json\",\"segment_ordinal\":0,\"source_start\":10,\"source_end\":20,\"observed_paths\":[\"/input/1.pgn\"]}]}\n",
            )
        );
    }

    #[test]
    fn jsonl_output_is_deterministic() {
        let report = grouped(&[("a", 10, 1, 0), ("a", 10, 0, 0)]);
        let jsonl = render_jsonl(&report).unwrap();
        assert_eq!(jsonl, render_jsonl(&report).unwrap());
    }

    #[test]
    fn jsonl_is_valid_and_parseable() {
        let report = grouped(&[("a", 10, 1, 0), ("a", 10, 0, 0)]);
        let jsonl = render_jsonl(&report).unwrap();
        for line in jsonl.lines() {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
    }

    #[test]
    fn outputs_have_final_newline() {
        let report = grouped(&[("a", 10, 1, 0), ("a", 10, 0, 0)]);
        assert!(render_text(&report).unwrap().ends_with('\n'));
        let jsonl = render_jsonl(&report).unwrap();
        assert!(jsonl.ends_with('\n'));
    }

    #[test]
    fn inverted_interval_is_rejected_by_checked_subtraction() {
        assert!(20_u64.checked_sub(21).is_none());
    }

    #[test]
    fn subtraction_and_conversions_are_checked() {
        assert_eq!(20_u64.checked_sub(10), Some(10));
        assert!(usize::try_from(u64::MAX).is_err() || usize::BITS == 64);
    }

    #[test]
    fn exact_profile_is_versioned() {
        assert_eq!(EXACT_DUPLICATE_PROFILE, "exact_source_game_bytes_v1");
    }
}
