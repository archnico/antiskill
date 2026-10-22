use crate::archive::game_id::PublicGameIdV1;
use crate::inspector;
use serde::Serialize;
use std::path::PathBuf;
use std::process::ExitCode;

const JSON_SCHEMA_V1: &str = "antiskill.search.v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OutputFormat {
    #[default]
    Human,
    Json,
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
    let result = parse_args(args).and_then(|(format, filters)| {
        let result = inspector::query_active_collection(&PathBuf::from("."), filters)?;
        match format {
            OutputFormat::Human => format_human(&result),
            OutputFormat::Json => format_json(&result),
        }
    });
    match result {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERRORE search: {error}");
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    println!(
        "\
Cerca partite nella Collection attiva del workspace corrente.

USO:
    antiskill search [--player TEXT] [--event TEXT] [--date YYYY-MM-DD] [--date-from YYYY-MM-DD] [--date-to YYYY-MM-DD] [--format human|json]
"
    );
}

fn parse_args(args: Vec<String>) -> Result<(OutputFormat, Vec<String>), String> {
    let mut format = OutputFormat::Human;
    let mut format_seen = false;
    let mut filters = Vec::new();
    let mut arguments = args.into_iter();
    while let Some(argument) = arguments.next() {
        if argument == "--format" {
            if format_seen {
                return Err("--format specificato più volte".into());
            }
            format_seen = true;
            format = match arguments.next().as_deref() {
                Some("human") => OutputFormat::Human,
                Some("json") => OutputFormat::Json,
                Some(value) => return Err(format!("Formato search non supportato: {value}")),
                None => return Err("Manca valore per --format".into()),
            };
        } else {
            filters.push(argument);
        }
    }
    Ok((format, filters))
}

pub(crate) fn human_values(values: &[String]) -> String {
    if values.is_empty() {
        "—".to_owned()
    } else {
        values.join(" | ")
    }
}

fn format_human(result: &inspector::CollectionQueryResult) -> Result<String, String> {
    if !result.occurrence_ids_available {
        return Err("La Collection attiva legacy v1 non supporta Game ID pubblici".into());
    }
    let count = result.matches.len();
    if count == 0 {
        return Ok("0 partite trovate.\n".to_owned());
    }
    let noun = if count == 1 {
        "partita trovata"
    } else {
        "partite trovate"
    };
    let mut output = format!("{count} {noun}.\n\nID\tDate\tWhite\tBlack\tResult\tEvent\n");
    for game in &result.matches {
        let occurrence_id = game
            .occurrence_id
            .as_deref()
            .ok_or("La Collection attiva legacy v1 non supporta Game ID pubblici")?;
        let public_id = PublicGameIdV1::from_occurrence_id(occurrence_id)?;
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            public_id,
            human_values(&game.date),
            human_values(&game.white),
            human_values(&game.black),
            human_values(&game.result),
            human_values(&game.event),
        ));
    }
    Ok(output)
}

#[derive(Serialize)]
struct SearchDocumentV1 {
    schema: &'static str,
    games: Vec<SearchGameV1>,
}

#[derive(Serialize)]
struct SearchGameV1 {
    id: String,
    date: Vec<String>,
    white: Vec<String>,
    black: Vec<String>,
    result: Vec<String>,
    event: Vec<String>,
}

fn format_json(result: &inspector::CollectionQueryResult) -> Result<String, String> {
    if !result.occurrence_ids_available {
        return Err("La Collection attiva legacy v1 non supporta Game ID pubblici".into());
    }
    let games = result
        .matches
        .iter()
        .map(|game| {
            let occurrence_id = game
                .occurrence_id
                .as_deref()
                .ok_or("La Collection attiva legacy v1 non supporta Game ID pubblici")?;
            Ok(SearchGameV1 {
                id: PublicGameIdV1::from_occurrence_id(occurrence_id)?.to_string(),
                date: game.date.clone(),
                white: game.white.clone(),
                black: game.black.clone(),
                result: game.result.clone(),
                event: game.event.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut output = serde_json::to_string(&SearchDocumentV1 {
        schema: JSON_SCHEMA_V1,
        games,
    })
    .map_err(|error| format!("JSON search non serializzabile: {error}"))?;
    output.push('\n');
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspector::CollectionQueryMatch;

    #[test]
    fn human_formatter_depends_only_on_structured_matches() {
        let result = inspector::CollectionQueryResult {
            occurrence_ids_available: true,
            matches: vec![CollectionQueryMatch {
                occurrence_id: Some("a".repeat(64)),
                source_id: 7,
                game_id: 3,
                source_blake3: "technical-hash".into(),
                observation_paths: vec!["technical/path.pgn".into()],
                segment_ordinal: 9,
                source_start: 11,
                source_end: 22,
                white: vec!["Alice".into(), "A. Example".into()],
                black: vec![],
                event: vec!["Open".into()],
                date: vec!["2026.08.11".into()],
                result: vec!["1-0".into()],
            }],
        };

        assert_eq!(
            format_human(&result).unwrap(),
            format!(
                "1 partita trovata.\n\nID\tDate\tWhite\tBlack\tResult\tEvent\ng1-{}\t2026.08.11\tAlice | A. Example\t—\t1-0\tOpen\n",
                "a".repeat(64)
            )
        );
    }

    #[test]
    fn json_formatter_freezes_v1_public_projection() {
        let result = inspector::CollectionQueryResult {
            occurrence_ids_available: true,
            matches: vec![CollectionQueryMatch {
                occurrence_id: Some("a".repeat(64)),
                source_id: 7,
                game_id: 3,
                source_blake3: "technical-hash".into(),
                observation_paths: vec!["technical/path.pgn".into()],
                segment_ordinal: 9,
                source_start: 11,
                source_end: 22,
                white: vec!["Alice".into(), "A. Example".into()],
                black: vec![],
                event: vec!["Open".into(), "Side event".into()],
                date: vec!["2026.08.11".into()],
                result: vec!["1-0".into()],
            }],
        };

        assert_eq!(
            format_json(&result).unwrap(),
            format!(
                "{{\"schema\":\"antiskill.search.v1\",\"games\":[{{\"id\":\"g1-{}\",\"date\":[\"2026.08.11\"],\"white\":[\"Alice\",\"A. Example\"],\"black\":[],\"result\":[\"1-0\"],\"event\":[\"Open\",\"Side event\"]}}]}}\n",
                "a".repeat(64)
            )
        );
    }

    #[test]
    fn json_formatter_emits_empty_games_array() {
        let result = inspector::CollectionQueryResult {
            occurrence_ids_available: true,
            matches: Vec::new(),
        };

        assert_eq!(
            format_json(&result).unwrap(),
            "{\"schema\":\"antiskill.search.v1\",\"games\":[]}\n"
        );
    }

    #[test]
    fn format_argument_is_removed_without_reimplementing_filters() {
        assert_eq!(
            parse_args(vec![
                "--player".into(),
                "Alice".into(),
                "--format".into(),
                "json".into(),
                "--event".into(),
                "Open".into(),
            ])
            .unwrap(),
            (
                OutputFormat::Json,
                vec!["--player", "Alice", "--event", "Open"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            )
        );
    }
}
