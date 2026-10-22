use crate::inspector;
use std::path::Path;
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
    if let Err(error) = parse_args(&args) {
        eprintln!("ERRORE doctor: {error}");
        return ExitCode::from(1);
    }
    match inspector::diagnose_workspace(Path::new(".")) {
        Ok(diagnosis) => {
            print!("{}", format_success(&diagnosis));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERRORE doctor: {error}");
            if error.starts_with("Directory stato workspace") {
                eprintln!("Per creare un workspace: antiskill init DIR");
            }
            ExitCode::from(1)
        }
    }
}

fn parse_args(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [argument] => Err(format!(
            "Argomento inatteso: {argument}. Uso: antiskill doctor"
        )),
        _ => Err("Il comando non accetta argomenti. Uso: antiskill doctor".into()),
    }
}

fn print_help() {
    println!(
        "\
Verifica il workspace Antiskill corrente e il suo snapshot attivo.

USO:
    antiskill doctor

La diagnostica è strettamente read-only e non esegue riparazioni.
"
    );
}

fn format_success(diagnosis: &inspector::WorkspaceDiagnosis) -> String {
    match diagnosis {
        inspector::WorkspaceDiagnosis::InitializedNoSnapshot => {
            "Workspace OK.\nNessuno snapshot attivo.\nProssimo passo: antiskill ingest DIR\n".into()
        }
        inspector::WorkspaceDiagnosis::Active { sources, games } => format!(
            "Workspace OK.\nSnapshot attivo verificato: {sources} {} · {games} {}.\n",
            noun(*sources, "sorgente", "sorgenti"),
            noun(*games, "partita", "partite")
        ),
    }
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
    fn formatter_is_product_facing_with_correct_number() {
        assert_eq!(
            format_success(&inspector::WorkspaceDiagnosis::InitializedNoSnapshot),
            "Workspace OK.\nNessuno snapshot attivo.\nProssimo passo: antiskill ingest DIR\n"
        );
        assert_eq!(
            format_success(&inspector::WorkspaceDiagnosis::Active {
                sources: 1,
                games: 1,
            }),
            "Workspace OK.\nSnapshot attivo verificato: 1 sorgente · 1 partita.\n"
        );
        assert_eq!(
            format_success(&inspector::WorkspaceDiagnosis::Active {
                sources: 12,
                games: 8538,
            }),
            "Workspace OK.\nSnapshot attivo verificato: 12 sorgenti · 8538 partite.\n"
        );
    }

    #[test]
    fn product_uses_only_the_structured_read_only_api() {
        let source = include_str!("doctor.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("inspector::diagnose_workspace"));
        for forbidden in [
            "Command::new",
            "fs::write",
            "create_dir",
            "remove_",
            "rename",
            "publish",
            "ureq",
        ] {
            assert!(!production.contains(forbidden));
        }
    }
}
