mod archive;
mod coverage;
mod dedup;
mod identity;
mod inspector;
mod lab;
mod object_store;
mod players;
mod sources;
use std::env;
use std::process::ExitCode;

fn print_help() {
    println!(
        "\
antiskill {version}

Archivio scacchistico locale, verificabile e attento alla provenienza.

USO:
    antiskill <COMANDO> [OPZIONI]

ARCHIVIO:
    init         Crea un nuovo workspace
    doctor       Verifica il workspace e lo snapshot attivo
    ingest       Attiva lo snapshot PGN nel workspace corrente
    compare      Confronta una directory PGN con lo snapshot attivo
    search       Cerca partite nella Collection attiva del workspace corrente
    export       Esporta una partita trovata da search
    duplicates   Mostra copie identiche e PGN con la stessa sequenza di mosse
    unique-games Costruisce la vista non distruttiva delle partite logiche uniche
    coverage     Misura il vettore di copertura senza inventare denominatori
    players      Interroga identità giocatore da identificatori FIDE verificati

FONTI:
    sources      Catalogo e stato delle fonti esterne

TECNICI:
    inspector    Importazione ed esportazione lossless di sorgenti PGN

LABORATORIO:
    lab          Strumenti sperimentali separati dal prodotto stabile

ALTRO:
    version      Mostra la versione
    help         Mostra questo messaggio

Per i dettagli:
    antiskill init --help
    antiskill doctor --help
    antiskill ingest --help
    antiskill compare --help
    antiskill search --help
    antiskill export --help
    antiskill duplicates --help
    antiskill unique-games --help
    antiskill coverage --help
    antiskill players --help
    antiskill sources --help
    antiskill inspector --help
    antiskill lab --help
",
        version = env!("CARGO_PKG_VERSION")
    );
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);

    match args.next().as_deref() {
        None | Some("help" | "--help" | "-h") => {
            print_help();
            ExitCode::SUCCESS
        }

        Some("version" | "--version" | "-V") => {
            println!("antiskill {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }

        Some("sources") => sources::run(args),

        Some("init") => archive::init::run(args),

        Some("doctor") => archive::doctor::run(args),

        Some("ingest") => archive::ingest::run(args),

        Some("compare") => archive::compare::run(args),

        Some("search") => archive::search::run(args),

        Some("export") => archive::export::run(args),

        Some("duplicates") => archive::duplicates::run(args),

        Some("unique-games") => archive::unique_games::run(args),

        Some("players") => players::run(args),

        Some("coverage") => coverage::run(args),

        Some("dedup") => dedup::run(args),

        Some("inspector") => inspector::run(args),

        Some("lab") => lab::run(args),

        Some("theory") => {
            eprintln!("AVVISO: `antiskill theory` è un alias legacy; usa `antiskill lab theory`.");
            lab::run_theory(args)
        }

        Some(command) => {
            eprintln!("Comando sconosciuto: {command}\n");
            print_help();
            ExitCode::from(2)
        }
    }
}
