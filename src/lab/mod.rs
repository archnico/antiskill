#[allow(clippy::clone_on_copy, clippy::manual_is_multiple_of)]
mod theory;

use std::process::ExitCode;

fn print_help() {
    println!(
        "\
antiskill lab

Laboratorio per strumenti sperimentali non appartenenti alla superficie stabile del prodotto.

USO:
    antiskill lab <COMANDO> [OPZIONI]

COMANDI:
    theory       Analisi sperimentale dell'aderenza alla teoria storica
    help         Mostra questo messaggio

Gli strumenti del Laboratorio possono cambiare e non definiscono i contratti stabili di Archivio."
    );
}

pub(crate) fn run_theory<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    theory::run(args)
}

pub fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();

    match args.next().as_deref() {
        None | Some("help" | "--help" | "-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some("theory") => run_theory(args),
        Some(command) => {
            eprintln!("Sottocomando lab sconosciuto: {command}\n");
            print_help();
            ExitCode::from(2)
        }
    }
}
