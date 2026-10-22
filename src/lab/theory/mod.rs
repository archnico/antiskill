mod build_book;
mod emit_test;
mod reduce_bucket;

use std::process::ExitCode;

fn print_help() {
    println!(
        "\
antiskill lab theory

Misura l'aderenza delle mosse a un libro storico precedente.

USO:
    antiskill lab theory <COMANDO> [OPZIONI]

COMANDI:
    build-book    Costruisce i bucket del libro storico
    emit-test     Emette le decisioni del campione 2024
    reduce-bucket Riduce un bucket grezzo in conteggi ordinati
    analyze       Analizza il campione di test del 2024
    help          Mostra questo messaggio

SPECIFICA:
    docs/theory-spec-v1.md
"
    );
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

        Some("build-book") => match build_book::run(args) {
            Ok(()) => ExitCode::SUCCESS,

            Err(error) => {
                eprintln!("ERRORE build-book: {error}");

                ExitCode::from(1)
            }
        },

        Some("emit-test") => match emit_test::run(args) {
            Ok(()) => ExitCode::SUCCESS,

            Err(error) => {
                eprintln!("ERRORE emit-test: {error}");
                ExitCode::from(1)
            }
        },

        Some("reduce-bucket") => match reduce_bucket::run(args) {
            Ok(()) => ExitCode::SUCCESS,

            Err(error) => {
                eprintln!("ERRORE reduce-bucket: {error}");
                ExitCode::from(1)
            }
        },

        Some("analyze") => {
            eprintln!(
                "antiskill lab theory analyze: \
                 implementazione non ancora presente."
            );

            ExitCode::from(2)
        }

        Some(command) => {
            eprintln!(
                "Sottocomando theory sconosciuto: \
                 {command}\n"
            );

            print_help();
            ExitCode::from(2)
        }
    }
}
