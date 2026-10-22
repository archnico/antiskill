use crate::inspector;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
    let input = match parse_args(&args) {
        Ok(input) => input,
        Err(error) => {
            eprintln!("ERRORE ingest: {error}");
            return ExitCode::from(1);
        }
    };
    match inspector::ingest_snapshot(Path::new("."), &input) {
        Ok(result) if result.failures.is_empty() => {
            print!("{}", format_success(&result));
            ExitCode::SUCCESS
        }
        Ok(result) => {
            eprint!("{}", format_failure(&result));
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("ERRORE ingest: {error}");
            ExitCode::from(1)
        }
    }
}

fn parse_args(args: &[String]) -> Result<PathBuf, String> {
    match args {
        [] => Err("Manca DIR. Uso: antiskill ingest DIR".into()),
        [directory] if !directory.starts_with('-') => Ok(PathBuf::from(directory)),
        [argument] => Err(format!(
            "Argomento inatteso: {argument}. Uso: antiskill ingest DIR"
        )),
        _ => Err("Sono ammessi esattamente un argomento DIR. Uso: antiskill ingest DIR".into()),
    }
}

fn print_help() {
    println!(
        "\
Ingerisce una directory sorgente PGN nel workspace corrente.

USO:
    antiskill ingest DIR

DIR rappresenta lo snapshot corrente delle sorgenti PGN: a ogni esecuzione la
Collection attiva viene ricostruita dai PGN presenti in DIR. Il workspace è la
directory corrente.
"
    );
}

fn format_success(result: &inspector::IngestSnapshotResult) -> String {
    let mut output = format!(
        "Ingest completato.\n{} file PGN · {} {} · {} {} · 0 errori\n",
        result.pgn_files,
        result.sources,
        noun(result.sources, "sorgente", "sorgenti"),
        result.games,
        noun(result.games, "partita", "partite")
    );
    if result.reused_sources > 0 {
        output.push_str(&format!(
            "{} {} dal workspace\n",
            result.reused_sources,
            noun(
                result.reused_sources,
                "sorgente riusata",
                "sorgenti riusate"
            )
        ));
    }
    output
}

fn format_failure(result: &inspector::IngestSnapshotResult) -> String {
    let error_noun = if result.failures.len() == 1 {
        "errore"
    } else {
        "errori"
    };
    let mut output = format!(
        "Ingest non completato.\n{} file PGN · {} {} · {} {} · {} {}\n",
        result.pgn_files,
        result.sources,
        noun(result.sources, "sorgente", "sorgenti"),
        result.games,
        noun(result.games, "partita", "partite"),
        result.failures.len(),
        error_noun
    );
    for failure in &result.failures {
        output.push_str(&format!("- {}: {}\n", failure.file, failure.cause));
    }
    output
}

fn noun<'a>(count: u64, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_formatters_depend_only_on_structured_results() {
        let success = inspector::IngestSnapshotResult {
            pgn_files: 3,
            sources: 2,
            games: 427,
            reused_sources: 1,
            failures: vec![],
        };
        assert_eq!(
            format_success(&success),
            "Ingest completato.\n3 file PGN · 2 sorgenti · 427 partite · 0 errori\n1 sorgente riusata dal workspace\n"
        );

        let failure = inspector::IngestSnapshotResult {
            pgn_files: 2,
            sources: 1,
            games: 1,
            reused_sources: 0,
            failures: vec![inspector::IngestSnapshotFailure {
                file: "bad.pgn".into(),
                cause: "Il SourceBlob candidato è vuoto".into(),
            }],
        };
        assert_eq!(
            format_failure(&failure),
            "Ingest non completato.\n2 file PGN · 1 sorgente · 1 partita · 1 errore\n- bad.pgn: Il SourceBlob candidato è vuoto\n"
        );
    }

    #[test]
    fn product_uses_the_structured_rust_api_not_inspector_output() {
        let source = include_str!("ingest.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("inspector::ingest_snapshot"));
        assert!(!production.contains("Command::new"));
        assert!(!production.contains("INGEST_RESULT"));
        assert!(!production.contains("REUSED_WORKSPACE"));
    }

    #[test]
    fn product_streaming_serializers_have_no_corpus_wide_hash_sets() {
        for (name, source, marker) in [
            (
                "catalog",
                include_str!("../inspector/catalog/mod.rs"),
                "impl<F> Serialize for ChangedCatalogs",
            ),
            (
                "occurrences",
                include_str!("../inspector/occurrences/mod.rs"),
                "impl<F> Serialize for ChangedOccurrences",
            ),
            (
                "mainlines",
                include_str!("../inspector/mainlines/mod.rs"),
                "impl<F> Serialize for ChangedMainlines",
            ),
        ] {
            let serializer = source
                .split(marker)
                .nth(1)
                .unwrap_or_else(|| panic!("serializer {name} assente"))
                .split("\n}\n\nfn ")
                .next()
                .unwrap();
            assert!(!serializer.contains("HashSet"), "serializer {name}");
            assert!(
                !serializer.contains("std::collections"),
                "serializer {name}"
            );
        }
    }
}
