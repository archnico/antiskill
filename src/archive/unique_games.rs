use crate::inspector;
use serde::Serialize;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const JSON_SCHEMA_V1: &str = "antiskill.unique_games.v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Summary(OutputFormat),
    ListJsonl,
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
    let result = parse_args(args).and_then(|action| {
        let root = PathBuf::from(".");
        let collection = inspector::resolve_active_collection_path(&root)?;
        match action {
            Action::Summary(format) => {
                let summary = inspector::create_unique_game_view(&root, &collection)?;
                match format {
                    OutputFormat::Human => Ok(Some(format_human(&summary))),
                    OutputFormat::Json => format_json(&summary).map(Some),
                }
            }
            Action::ListJsonl => {
                let stdout = io::stdout();
                let mut stdout = stdout.lock();
                inspector::write_unique_game_records(&root, &collection, &mut stdout)?;
                stdout
                    .flush()
                    .map_err(|error| format!("Flush listing unique-games: {error}"))?;
                Ok(None)
            }
        }
    });
    match result {
        Ok(output) => {
            if let Some(output) = output {
                print!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERRORE unique-games: {error}");
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    println!(
        "\\
Costruisce una vista non distruttiva delle partite logiche uniche.

USO:
    antiskill unique-games [--format human|json]
    antiskill unique-games --list

Le occurrence originali restano intatte e byte-esatte. Le partite confrontabili
sono raggruppate per canonical mainline; le non confrontabili restano singleton.
"
    );
}

fn parse_args(args: Vec<String>) -> Result<Action, String> {
    match args.as_slice() {
        [] => Ok(Action::Summary(OutputFormat::Human)),
        [argument] if argument == "--list" => Ok(Action::ListJsonl),
        [flag, value] if flag == "--format" => match value.as_str() {
            "human" => Ok(Action::Summary(OutputFormat::Human)),
            "json" => Ok(Action::Summary(OutputFormat::Json)),
            value => Err(format!("Formato unique-games non supportato: {value}")),
        },
        [argument, ..] => Err(format!("Argomento unique-games inatteso: {argument}")),
    }
}

fn format_human(summary: &inspector::UniqueGameViewSummaryV1) -> String {
    format!(
        "Vista partite logiche uniche\n\nOccurrence sorgente: {}\nPartite logiche uniche: {}\nOccurrence collassate: {}\nOccurrence confrontabili: {}\nSingleton non confrontabili: {}\nGruppi logici multi-occurrence: {}\nOccurrence in gruppi multi-occurrence: {}\nMassimo occurrence per partita logica: {}\n\nView: {}\nView BLAKE3: {}\nGroups: {}\nGroups BLAKE3: {}\n",
        summary.source_occurrences,
        summary.logical_games,
        summary.collapsed_occurrences,
        summary.comparable_occurrences,
        summary.noncomparable_singletons,
        summary.duplicate_logical_games,
        summary.duplicate_group_occurrences,
        summary.max_occurrences_per_logical_game,
        summary.view_path.display(),
        summary.view_blake3,
        summary.groups_path.display(),
        summary.groups_blake3,
    )
}

#[derive(Serialize)]
struct UniqueGamesDocumentV1<'a> {
    schema: &'static str,
    source_occurrences: u64,
    comparable_occurrences: u64,
    noncomparable_singletons: u64,
    comparable_logical_games: u64,
    logical_games: u64,
    collapsed_occurrences: u64,
    duplicate_logical_games: u64,
    duplicate_group_occurrences: u64,
    max_occurrences_per_logical_game: u64,
    view_path: String,
    view_blake3: &'a str,
    view_logical_bytes: u64,
    groups_path: String,
    groups_blake3: &'a str,
    groups_logical_bytes: u64,
}

fn format_json(summary: &inspector::UniqueGameViewSummaryV1) -> Result<String, String> {
    let document = UniqueGamesDocumentV1 {
        schema: JSON_SCHEMA_V1,
        source_occurrences: summary.source_occurrences,
        comparable_occurrences: summary.comparable_occurrences,
        noncomparable_singletons: summary.noncomparable_singletons,
        comparable_logical_games: summary.comparable_logical_games,
        logical_games: summary.logical_games,
        collapsed_occurrences: summary.collapsed_occurrences,
        duplicate_logical_games: summary.duplicate_logical_games,
        duplicate_group_occurrences: summary.duplicate_group_occurrences,
        max_occurrences_per_logical_game: summary.max_occurrences_per_logical_game,
        view_path: summary.view_path.to_string_lossy().into_owned(),
        view_blake3: &summary.view_blake3,
        view_logical_bytes: summary.view_bytes,
        groups_path: summary.groups_path.to_string_lossy().into_owned(),
        groups_blake3: &summary.groups_blake3,
        groups_logical_bytes: summary.groups_bytes,
    };
    let mut output = serde_json::to_string(&document)
        .map_err(|error| format!("JSON unique-games non serializzabile: {error}"))?;
    output.push('\n');
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_format_is_narrow_and_deterministic() {
        assert_eq!(
            parse_args(vec![]).unwrap(),
            Action::Summary(OutputFormat::Human)
        );
        assert_eq!(
            parse_args(vec!["--format".into(), "json".into()]).unwrap(),
            Action::Summary(OutputFormat::Json)
        );
        assert_eq!(
            parse_args(vec!["--list".into()]).unwrap(),
            Action::ListJsonl
        );
        assert!(parse_args(vec!["--format".into(), "yaml".into()]).is_err());
        assert!(parse_args(vec!["extra".into()]).is_err());
    }
}
