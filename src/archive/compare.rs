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
            eprintln!("ERRORE compare: {error}");
            return ExitCode::from(1);
        }
    };
    match inspector::compare_active_snapshot(Path::new("."), &input) {
        Ok(result) => {
            print!("{}", format_success(&result));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERRORE compare: {error}");
            ExitCode::from(1)
        }
    }
}

fn parse_args(args: &[String]) -> Result<PathBuf, String> {
    match args {
        [] => Err("Manca DIR. Uso: antiskill compare DIR".into()),
        [directory] if !directory.starts_with('-') => Ok(PathBuf::from(directory)),
        [argument] => Err(format!(
            "Argomento inatteso: {argument}. Uso: antiskill compare DIR"
        )),
        _ => Err("Sono ammessi esattamente un argomento DIR. Uso: antiskill compare DIR".into()),
    }
}

fn print_help() {
    println!(
        "\
Confronta una directory PGN con lo snapshot attivo del workspace corrente.

USO:
    antiskill compare DIR

Il confronto è strettamente read-only e usa i path relativi dei file PGN.
"
    );
}

fn format_success(result: &inspector::SnapshotComparison) -> String {
    if result.modified.is_empty() && result.added.is_empty() && result.removed.is_empty() {
        return format!(
            "Nessuna differenza: {} file PGN {}.\n",
            result.unchanged,
            noun_u64(result.unchanged, "invariato", "invariati")
        );
    }
    let mut output = format!(
        "Confronto snapshot:\n{} {} · {} {} · {} {} · {} {}\n",
        result.modified.len(),
        noun(result.modified.len(), "modificato", "modificati"),
        result.added.len(),
        noun(result.added.len(), "aggiunto", "aggiunti"),
        result.removed.len(),
        noun(result.removed.len(), "rimosso", "rimossi"),
        result.unchanged,
        noun_u64(result.unchanged, "invariato", "invariati")
    );
    section(&mut output, "Modificati", &result.modified);
    section(&mut output, "Aggiunti", &result.added);
    section(&mut output, "Rimossi", &result.removed);
    output
}

fn noun<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

fn noun_u64<'a>(count: u64, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

fn section(output: &mut String, title: &str, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    output.push_str(&format!("\n{title}\n"));
    for path in paths {
        output.push_str(&format!("- {path}\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_human_output_uses_only_product_fields() {
        let result = inspector::SnapshotComparison {
            unchanged: 37,
            modified: vec!["torneo/2026.pgn".into()],
            added: vec!["nuovi/agosto.pgn".into()],
            removed: vec!["vecchi/luglio.pgn".into()],
        };
        assert_eq!(
            format_success(&result),
            "Confronto snapshot:\n1 modificato · 1 aggiunto · 1 rimosso · 37 invariati\n\nModificati\n- torneo/2026.pgn\n\nAggiunti\n- nuovi/agosto.pgn\n\nRimossi\n- vecchi/luglio.pgn\n"
        );
        assert_eq!(
            format_success(&inspector::SnapshotComparison {
                unchanged: 40,
                modified: vec![],
                added: vec![],
                removed: vec![],
            }),
            "Nessuna differenza: 40 file PGN invariati.\n"
        );
    }

    #[test]
    fn product_uses_structured_rust_api_without_subprocesses_or_technical_output() {
        let source = include_str!("compare.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("inspector::compare_active_snapshot"));
        for forbidden in ["Command::new", "workspace-state", "COLLECTION", "BLAKE3"] {
            assert!(!production.contains(forbidden));
        }
    }
}
