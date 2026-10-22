mod bounded;
use crate::archive::game_id::PublicGameIdV1;
#[cfg(test)]
use crate::archive::search::human_values;
#[cfg(test)]
use crate::dedup::{ProductCanonicalGroup, ProductCanonicalReport};
#[cfg(test)]
use crate::inspector::{CollectionQueryResult, ProductExactDuplicateReport};
#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Clone, Debug, PartialEq, Eq)]
struct HumanGame {
    id: PublicGameIdV1,
    date: String,
    white: String,
    black: String,
    result: String,
    event: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HumanExactGroup {
    members: Vec<HumanGame>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HumanCanonicalGroup {
    members: Vec<HumanGame>,
    byte_variant_count: u64,
    result_conflict: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HumanDuplicatesReport {
    exact_groups: Vec<HumanExactGroup>,
    canonical_groups: Vec<HumanCanonicalGroup>,
    noncomparable_occurrences: u64,
}

pub(crate) fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if matches!(args.as_slice(), [argument] if matches!(argument.as_str(), "help" | "--help" | "-h"))
    {
        print_help();
        return ExitCode::SUCCESS;
    }
    if !args.is_empty() {
        eprintln!("ERRORE duplicates: argomento inatteso: {}", args[0]);
        return ExitCode::from(1);
    }

    match build(&PathBuf::from("."))
        .map(|report| format_human(&report))
        .map_err(product_error)
    {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERRORE duplicates: {error}");
            ExitCode::from(1)
        }
    }
}

fn product_error(error: String) -> String {
    if error.starts_with("Stato workspace mancante:") {
        "Stato workspace mancante: eseguire antiskill ingest DIR".into()
    } else {
        error
    }
}

fn print_help() {
    println!(
        "\
Mostra i duplicati nella Collection attiva del workspace corrente.

USO:
    antiskill duplicates

Il comando distingue le copie byte-identiche dai PGN diversi con la stessa
sequenza di mosse. È solo diagnostico: non cancella e non modifica nulla.
Gli ID mostrati possono essere usati con `antiskill export`.
"
    );
}

fn build(root: &Path) -> Result<HumanDuplicatesReport, String> {
    let collection = crate::inspector::resolve_active_collection_path(root)?;
    bounded::build(root, &collection)
}

#[cfg(test)]
fn metadata_map(result: &CollectionQueryResult) -> Result<BTreeMap<String, HumanGame>, String> {
    if !result.occurrence_ids_available {
        return Err(
            "La Collection attiva legacy v1 non supporta l'analisi duplicati prodotto".into(),
        );
    }
    let mut metadata = BTreeMap::new();
    for game in &result.matches {
        let occurrence_id = game
            .occurrence_id
            .as_deref()
            .ok_or("Occurrence senza identità nella proiezione metadata")?;
        let human = HumanGame {
            id: PublicGameIdV1::from_occurrence_id(occurrence_id)?,
            date: human_values(&game.date),
            white: human_values(&game.white),
            black: human_values(&game.black),
            result: human_values(&game.result),
            event: human_values(&game.event),
        };
        if metadata.insert(occurrence_id.to_owned(), human).is_some() {
            return Err("occurrence_id duplicato nella proiezione metadata".into());
        }
    }
    Ok(metadata)
}

#[cfg(test)]
fn join_members(
    occurrence_ids: &[String],
    metadata: &BTreeMap<String, HumanGame>,
) -> Result<Vec<HumanGame>, String> {
    let mut seen = BTreeSet::new();
    let mut members = Vec::with_capacity(occurrence_ids.len());
    for occurrence_id in occurrence_ids {
        if !seen.insert(occurrence_id) {
            return Err("occurrence_id duplicato in un gruppo duplicati".into());
        }
        members.push(
            metadata
                .get(occurrence_id)
                .cloned()
                .ok_or("Occurrence del report assente dalla proiezione metadata")?,
        );
    }
    members.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(members)
}

#[cfg(test)]
fn join_reports(
    metadata: &CollectionQueryResult,
    exact: &ProductExactDuplicateReport,
    canonical: &ProductCanonicalReport,
) -> Result<HumanDuplicatesReport, String> {
    let metadata = metadata_map(metadata)?;
    let exact_groups = exact
        .groups
        .iter()
        .map(|group| {
            Ok(HumanExactGroup {
                members: join_members(&group.occurrence_ids, &metadata)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let canonical_groups = canonical
        .groups
        .iter()
        .map(
            |ProductCanonicalGroup {
                 occurrence_ids,
                 byte_variant_count,
                 result_conflict,
             }| {
                Ok(HumanCanonicalGroup {
                    members: join_members(occurrence_ids, &metadata)?,
                    byte_variant_count: *byte_variant_count,
                    result_conflict: *result_conflict,
                })
            },
        )
        .collect::<Result<Vec<_>, String>>()?;
    Ok(HumanDuplicatesReport {
        exact_groups,
        canonical_groups,
        noncomparable_occurrences: canonical.noncomparable_occurrences,
    })
}

fn noun(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

fn member_line(game: &HumanGame) -> String {
    format!(
        "  {}\t{}\t{} – {}\t{}\t{}\n",
        game.id, game.date, game.white, game.black, game.result, game.event
    )
}

fn format_human(report: &HumanDuplicatesReport) -> String {
    let mut output = format!(
        "Duplicati nel corpus attivo\n\nCopie byte-identiche: {}\n",
        noun(report.exact_groups.len(), "gruppo", "gruppi")
    );
    for (index, group) in report.exact_groups.iter().enumerate() {
        output.push_str(&format!(
            "\nGruppo {} · {}\n",
            index + 1,
            noun(group.members.len(), "copia", "copie")
        ));
        for member in &group.members {
            output.push_str(&member_line(member));
        }
    }
    output.push_str(&format!(
        "\nStessa sequenza di mosse, PGN diversi: {}\n",
        noun(report.canonical_groups.len(), "gruppo", "gruppi")
    ));
    for (index, group) in report.canonical_groups.iter().enumerate() {
        output.push_str(&format!(
            "\nGruppo {} · {} · {} varianti byte\n",
            index + 1,
            noun(group.members.len(), "occurrence", "occurrence"),
            group.byte_variant_count
        ));
        for member in &group.members {
            output.push_str(&member_line(member));
        }
        if group.result_conflict {
            output.push_str("  Risultati discordanti\n");
        }
    }
    output.push_str(&format!(
        "\nNon confrontabili per mainline: {}\n",
        report.noncomparable_occurrences
    ));
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspector::{CollectionQueryMatch, ProductExactDuplicateGroup};

    fn metadata(id: char) -> CollectionQueryMatch {
        CollectionQueryMatch {
            occurrence_id: Some(id.to_string().repeat(64)),
            source_id: 0,
            game_id: 0,
            source_blake3: "hidden".into(),
            observation_paths: Vec::new(),
            segment_ordinal: 0,
            source_start: 0,
            source_end: 1,
            white: vec!["Alice".into(), "A. Example".into()],
            black: Vec::new(),
            event: vec!["Open".into()],
            date: vec!["2026.08.11".into()],
            result: vec!["1-0".into()],
        }
    }

    #[test]
    fn joins_only_by_occurrence_id_and_formats_human_values() {
        let query = CollectionQueryResult {
            occurrence_ids_available: true,
            matches: vec![metadata('b'), metadata('a')],
        };
        let exact = ProductExactDuplicateReport {
            groups: vec![ProductExactDuplicateGroup {
                occurrence_ids: vec!["b".repeat(64), "a".repeat(64)],
            }],
        };
        let canonical = ProductCanonicalReport {
            groups: Vec::new(),
            noncomparable_occurrences: 2,
        };
        let report = join_reports(&query, &exact, &canonical).unwrap();
        let output = format_human(&report);
        assert!(output.contains("Alice | A. Example – —"));
        assert!(
            output.find(&format!("g1-{}", "a".repeat(64))).unwrap()
                < output.find(&format!("g1-{}", "b".repeat(64))).unwrap()
        );
        assert!(output.contains("Non confrontabili per mainline: 2"));
    }

    #[test]
    fn duplicate_or_missing_metadata_fails_closed() {
        let exact = ProductExactDuplicateReport {
            groups: vec![ProductExactDuplicateGroup {
                occurrence_ids: vec!["a".repeat(64), "b".repeat(64)],
            }],
        };
        let canonical = ProductCanonicalReport {
            groups: Vec::new(),
            noncomparable_occurrences: 0,
        };
        let duplicate = CollectionQueryResult {
            occurrence_ids_available: true,
            matches: vec![metadata('a'), metadata('a')],
        };
        assert!(join_reports(&duplicate, &exact, &canonical)
            .unwrap_err()
            .contains("duplicato"));
        let missing = CollectionQueryResult {
            occurrence_ids_available: true,
            matches: vec![metadata('a')],
        };
        assert!(join_reports(&missing, &exact, &canonical)
            .unwrap_err()
            .contains("assente"));
    }

    #[test]
    fn product_module_uses_structured_apis_without_subprocess_or_renderer_parsing() {
        let source = include_str!("duplicates.rs");
        let production = source
            .split_once("mod tests {")
            .map(|(production, _)| production)
            .expect("modulo test duplicates assente");
        assert!(production.contains("bounded::build"));
        assert!(!production.contains("exact_duplicates_product_report"));
        assert!(!production.contains("product_report_from_collection"));
        assert!(!production.contains("query_collection_metadata"));
        assert!(!production.contains("Command::new"));
        assert!(!production.contains("jsonl"));
        assert!(!production.contains("DUPLICATE_PROFILE"));
        assert!(!production.contains("DEDUP_PROFILE"));
    }

    #[test]
    fn missing_state_error_does_not_suggest_inspector_options() {
        assert_eq!(
            product_error(
                "Stato workspace mancante: eseguire ingest-directory o usare --collection".into()
            ),
            "Stato workspace mancante: eseguire antiskill ingest DIR"
        );
    }
}
