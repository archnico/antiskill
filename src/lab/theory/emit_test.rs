use super::build_book::{encode_uci, parse_year};

use pgn_reader::shakmaty::{uci::UciMove, zobrist::Zobrist128, Chess, EnPassantMode, Position};
use pgn_reader::{RawTag, Reader, SanPlus, Visitor};

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const BUCKETS: usize = 256;
const DECISION_RECORD_BYTES: u64 = 19;

#[derive(Debug)]
struct Config {
    pgn: PathBuf,
    out_dir: PathBuf,
    max_test_games: u64,
    max_ply: usize,
}

impl Config {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut pgn = None;
        let mut out_dir = None;
        let mut max_test_games = 10_000_u64;
        let mut max_ply = 60_usize;

        let mut args = args.into_iter();

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--pgn" => {
                    pgn = Some(PathBuf::from(
                        args.next()
                            .ok_or_else(|| "Manca il valore di --pgn".to_owned())?,
                    ));
                }

                "--out-dir" => {
                    out_dir = Some(PathBuf::from(
                        args.next()
                            .ok_or_else(|| "Manca il valore di --out-dir".to_owned())?,
                    ));
                }

                "--max-test-games" => {
                    max_test_games = args
                        .next()
                        .ok_or_else(|| "Manca il valore di --max-test-games".to_owned())?
                        .parse()
                        .map_err(|_| "Valore non valido per --max-test-games".to_owned())?;
                }

                "--max-ply" => {
                    max_ply = args
                        .next()
                        .ok_or_else(|| "Manca il valore di --max-ply".to_owned())?
                        .parse()
                        .map_err(|_| "Valore non valido per --max-ply".to_owned())?;
                }

                "--help" | "-h" => {
                    return Err("USO: antiskill lab theory emit-test \
                         --pgn FILE --out-dir DIR \
                         [--max-test-games N] [--max-ply N]"
                        .to_owned());
                }

                other => {
                    return Err(format!("Opzione sconosciuta: {other}"));
                }
            }
        }

        if max_ply == 0 || max_ply > 255 {
            return Err("--max-ply deve essere compreso tra 1 e 255".to_owned());
        }

        Ok(Self {
            pgn: pgn.ok_or_else(|| "Parametro obbligatorio: --pgn".to_owned())?,
            out_dir: out_dir.ok_or_else(|| "Parametro obbligatorio: --out-dir".to_owned())?,
            max_test_games,
            max_ply,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GameClass {
    Test,
    OtherYear,
    MissingDate,
    NonStandard,
    NoRating,
}

impl GameClass {
    fn label(self) -> &'static str {
        match self {
            Self::Test => "test_2024",
            Self::OtherYear => "other_year",
            Self::MissingDate => "missing_date",
            Self::NonStandard => "nonstandard",
            Self::NoRating => "no_valid_rating",
        }
    }
}

#[derive(Default)]
struct GameTags {
    year: Option<u16>,
    has_fen: bool,
    setup: bool,
    nonstandard_variant: bool,

    white_elo: Option<u16>,
    black_elo: Option<u16>,

    date: String,
    white: String,
    black: String,
    event: String,
    site: String,
    round: String,
    result: String,
}

#[derive(Clone, Copy, Debug)]
struct Decision {
    hash: u128,
    ply: u8,
    move_code: u16,
    rating: u16,
}

struct GameState {
    class: GameClass,
    tags: GameTags,
    position: Chess,
    ply: usize,
    decisions: Vec<Decision>,
}

struct GameOutput {
    class: GameClass,
    tags: GameTags,
    total_plies: usize,
    decisions: Vec<Decision>,
}

enum VisitOutput {
    Game(GameOutput),
    Invalid { class: GameClass, error: String },
}

struct TestVisitor {
    max_ply: usize,
}

fn parse_rating(bytes: &[u8]) -> Option<u16> {
    let value = std::str::from_utf8(bytes)
        .ok()?
        .trim()
        .parse::<u16>()
        .ok()?;

    (100..=4000).contains(&value).then_some(value)
}

fn tag_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}

fn escape_tsv(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            other => escaped.push(other),
        }
    }

    escaped
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other if other.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(escaped, "\\u{:04x}", other as u32);
            }
            other => escaped.push(other),
        }
    }

    escaped
}

fn optional_rating(value: Option<u16>) -> String {
    value.map(|rating| rating.to_string()).unwrap_or_default()
}

impl Visitor for TestVisitor {
    type Tags = GameTags;
    type Movetext = GameState;
    type Output = VisitOutput;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(GameTags::default())
    }

    fn tag(
        &mut self,
        tags: &mut Self::Tags,
        name: &[u8],
        value: RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        let bytes = value.as_bytes();

        match name {
            b"Date" => {
                tags.year = parse_year(bytes);
                tags.date = tag_text(bytes);
            }

            b"WhiteElo" => {
                tags.white_elo = parse_rating(bytes);
            }

            b"BlackElo" => {
                tags.black_elo = parse_rating(bytes);
            }

            b"White" => tags.white = tag_text(bytes),
            b"Black" => tags.black = tag_text(bytes),
            b"Event" => tags.event = tag_text(bytes),
            b"Site" => tags.site = tag_text(bytes),
            b"Round" => tags.round = tag_text(bytes),
            b"Result" => tags.result = tag_text(bytes),

            b"FEN" => {
                tags.has_fen = !bytes.is_empty();
            }

            b"SetUp" => {
                tags.setup = bytes == b"1";
            }

            b"Variant" => {
                let variant = tag_text(bytes).to_ascii_lowercase();

                tags.nonstandard_variant =
                    !variant.is_empty() && variant != "standard" && variant != "chess";
            }

            _ => {}
        }

        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, tags: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        let class = match tags.year {
            None => GameClass::MissingDate,

            Some(2024) if tags.has_fen || tags.setup || tags.nonstandard_variant => {
                GameClass::NonStandard
            }

            Some(2024) if tags.white_elo.is_none() && tags.black_elo.is_none() => {
                GameClass::NoRating
            }

            Some(2024) => GameClass::Test,

            Some(_) => GameClass::OtherYear,
        };

        ControlFlow::Continue(GameState {
            class,
            tags,
            position: Chess::default(),
            ply: 0,
            decisions: Vec::with_capacity(self.max_ply),
        })
    }

    fn san(&mut self, state: &mut Self::Movetext, san_plus: SanPlus) -> ControlFlow<Self::Output> {
        if state.class != GameClass::Test {
            state.ply += 1;
            return ControlFlow::Continue(());
        }

        let played = match san_plus.san.to_move(&state.position) {
            Ok(played) => played,

            Err(error) => {
                return ControlFlow::Break(VisitOutput::Invalid {
                    class: state.class,
                    error: format!("Mossa SAN illegale al ply {}: {error}", state.ply),
                });
            }
        };

        if state.ply < self.max_ply {
            let hash: Zobrist128 = state.position.zobrist_hash(EnPassantMode::Legal);

            let castling_mode = state.position.castles().mode();

            let uci = UciMove::from_move(played, castling_mode).to_string();

            let move_code = match encode_uci(&uci) {
                Ok(move_code) => move_code,

                Err(error) => {
                    return ControlFlow::Break(VisitOutput::Invalid {
                        class: state.class,
                        error,
                    });
                }
            };

            let rating = if state.ply % 2 == 0 {
                state.tags.white_elo.unwrap_or(0)
            } else {
                state.tags.black_elo.unwrap_or(0)
            };

            state.decisions.push(Decision {
                hash: hash.0,
                ply: state.ply as u8,
                move_code,
                rating,
            });
        }

        state.position.play_unchecked(played);
        state.ply += 1;

        ControlFlow::Continue(())
    }

    fn end_game(&mut self, state: Self::Movetext) -> Self::Output {
        VisitOutput::Game(GameOutput {
            class: state.class,
            tags: state.tags,
            total_plies: state.ply,
            decisions: state.decisions,
        })
    }
}

fn create_bucket_writers(directory: &Path) -> Result<Vec<BufWriter<File>>, String> {
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "Impossibile creare la directory {}: {error}",
            directory.display()
        )
    })?;

    let mut writers = Vec::with_capacity(BUCKETS);

    for bucket in 0..BUCKETS {
        let path = directory.join(format!("{bucket:02x}.bin"));

        let file = File::create(&path)
            .map_err(|error| format!("Impossibile creare {}: {error}", path.display()))?;

        writers.push(BufWriter::with_capacity(64 * 1024, file));
    }

    Ok(writers)
}

fn write_decision_record(
    writer: &mut impl Write,
    hash: u128,
    game_id: u32,
    ply: u8,
    move_code: u16,
    rating: u16,
) -> Result<(), String> {
    let hash_bytes = hash.to_le_bytes();

    writer
        .write_all(&hash_bytes[..10])
        .and_then(|_| writer.write_all(&game_id.to_le_bytes()))
        .and_then(|_| writer.write_all(&[ply]))
        .and_then(|_| writer.write_all(&move_code.to_le_bytes()))
        .and_then(|_| writer.write_all(&rating.to_le_bytes()))
        .map_err(|error| format!("Errore scrivendo decisione: {error}"))
}

fn write_game_row(
    writer: &mut impl Write,
    game_id: u32,
    output: &GameOutput,
) -> Result<(), String> {
    writeln!(
        writer,
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        game_id,
        escape_tsv(&output.tags.date),
        escape_tsv(&output.tags.white),
        escape_tsv(&output.tags.black),
        optional_rating(output.tags.white_elo),
        optional_rating(output.tags.black_elo),
        escape_tsv(&output.tags.result),
        escape_tsv(&output.tags.event),
        escape_tsv(&output.tags.site),
        escape_tsv(&output.tags.round),
        output.decisions.len(),
        output.total_plies,
    )
    .map_err(|error| format!("Errore scrivendo games.tsv: {error}"))
}

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let config = Config::from_args(args)?;

    if !config.pgn.is_file() {
        return Err(format!("PGN non trovato: {}", config.pgn.display()));
    }

    if config.out_dir.exists() {
        return Err(format!(
            "La directory output esiste già: {}",
            config.out_dir.display()
        ));
    }

    fs::create_dir_all(&config.out_dir)
        .map_err(|error| format!("Impossibile creare {}: {error}", config.out_dir.display()))?;

    let bucket_dir = config.out_dir.join("decision-buckets");

    let mut bucket_writers = create_bucket_writers(&bucket_dir)?;

    let games_path = config.out_dir.join("games.tsv");
    let errors_path = config.out_dir.join("errors.tsv");

    let mut games_writer = BufWriter::new(
        File::create(&games_path)
            .map_err(|error| format!("Impossibile creare games.tsv: {error}"))?,
    );

    let mut errors_writer = BufWriter::new(
        File::create(&errors_path)
            .map_err(|error| format!("Impossibile creare errors.tsv: {error}"))?,
    );

    writeln!(
        games_writer,
        "game_id\tdate\twhite\tblack\twhite_elo\tblack_elo\tresult\t\
         event\tsite\tround\trecorded_decisions\ttotal_plies"
    )
    .map_err(|error| format!("Errore intestazione games.tsv: {error}"))?;

    writeln!(errors_writer, "source_index\tclass\terror")
        .map_err(|error| format!("Errore intestazione errors.tsv: {error}"))?;

    let file =
        File::open(&config.pgn).map_err(|error| format!("Impossibile aprire il PGN: {error}"))?;

    let mut reader = Reader::new(BufReader::with_capacity(4 * 1024 * 1024, file));

    let mut visitor = TestVisitor {
        max_ply: config.max_ply,
    };

    let started = Instant::now();

    let mut games_read = 0_u64;
    let mut test_games = 0_u64;
    let mut other_year_games = 0_u64;
    let mut missing_date_games = 0_u64;
    let mut nonstandard_2024 = 0_u64;
    let mut no_rating_2024 = 0_u64;
    let mut invalid_2024 = 0_u64;
    let mut reader_errors = 0_u64;

    let mut both_ratings_2024 = 0_u64;
    let mut one_rating_2024 = 0_u64;

    let mut decisions_written = 0_u64;
    let mut decisions_with_rating = 0_u64;
    let mut decisions_without_rating = 0_u64;

    loop {
        if config.max_test_games > 0 && test_games >= config.max_test_games {
            break;
        }

        let output = match reader.read_game(&mut visitor) {
            Ok(Some(output)) => {
                games_read += 1;
                output
            }

            Ok(None) => break,

            Err(error) => {
                games_read += 1;
                reader_errors += 1;

                writeln!(
                    errors_writer,
                    "{}\treader_error\t{}",
                    games_read,
                    escape_tsv(&error.to_string()),
                )
                .map_err(|write_error| format!("Errore scrivendo errors.tsv: {write_error}"))?;

                continue;
            }
        };

        match output {
            VisitOutput::Invalid { class, error } => {
                if class == GameClass::Test {
                    invalid_2024 += 1;
                }

                writeln!(
                    errors_writer,
                    "{}\t{}\t{}",
                    games_read,
                    class.label(),
                    escape_tsv(&error),
                )
                .map_err(|write_error| format!("Errore scrivendo errors.tsv: {write_error}"))?;
            }

            VisitOutput::Game(output) => match output.class {
                GameClass::OtherYear => {
                    other_year_games += 1;
                }

                GameClass::MissingDate => {
                    missing_date_games += 1;
                }

                GameClass::NonStandard => {
                    nonstandard_2024 += 1;
                }

                GameClass::NoRating => {
                    no_rating_2024 += 1;
                }

                GameClass::Test => {
                    test_games += 1;

                    if output.tags.white_elo.is_some() && output.tags.black_elo.is_some() {
                        both_ratings_2024 += 1;
                    } else {
                        one_rating_2024 += 1;
                    }

                    let game_id = u32::try_from(test_games)
                        .map_err(|_| "Troppi game_id per u32".to_owned())?;

                    write_game_row(&mut games_writer, game_id, &output)?;

                    for decision in output.decisions {
                        let bucket = ((decision.hash >> 120) & 0xff) as usize;

                        write_decision_record(
                            &mut bucket_writers[bucket],
                            decision.hash,
                            game_id,
                            decision.ply,
                            decision.move_code,
                            decision.rating,
                        )?;

                        decisions_written += 1;

                        if decision.rating == 0 {
                            decisions_without_rating += 1;
                        } else {
                            decisions_with_rating += 1;
                        }
                    }
                }
            },
        }

        if games_read.is_multiple_of(100_000) {
            let elapsed = started.elapsed().as_secs_f64();

            eprintln!(
                "[EMIT] {games_read:>10} lette | \
                 {test_games:>7} test 2024 | \
                 {decisions_written:>10} decisioni | \
                 {:>9.0} partite/s",
                games_read as f64 / elapsed.max(0.001),
            );
        }
    }

    for writer in &mut bucket_writers {
        writer
            .flush()
            .map_err(|error| format!("Errore flush bucket: {error}"))?;
    }

    games_writer
        .flush()
        .map_err(|error| format!("Errore flush games.tsv: {error}"))?;

    errors_writer
        .flush()
        .map_err(|error| format!("Errore flush errors.tsv: {error}"))?;

    drop(bucket_writers);
    drop(games_writer);
    drop(errors_writer);

    let decision_bytes = fs::read_dir(&bucket_dir)
        .map_err(|error| format!("Impossibile leggere i bucket: {error}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum::<u64>();

    let elapsed = started.elapsed().as_secs_f64();

    let summary = format!(
        "\
ANTISKILL THEORY — EMISSIONE TEST 2024
=======================================

PGN: {pgn}
Output: {out}
Massimo partite test: {max_test_games}
Massimo ply: {max_ply}

Partite sorgente lette: {games_read}
Partite 2024 valide emesse: {test_games}
Partite 2024 con entrambi i rating: {both_ratings_2024}
Partite 2024 con un solo rating: {one_rating_2024}
Partite 2024 senza rating escluse: {no_rating_2024}
Partite 2024 non standard escluse: {nonstandard_2024}
Partite 2024 invalide: {invalid_2024}

Partite di altri anni attraversate: {other_year_games}
Partite senza anno attraversate: {missing_date_games}
Errori del lettore PGN: {reader_errors}

Decisioni scritte: {decisions_written}
Decisioni con rating valido: {decisions_with_rating}
Decisioni senza rating valido: {decisions_without_rating}

Bucket: 256
Dimensione record decisione: {record_bytes} byte
Dimensione bucket decisioni: {decision_bytes} byte

Tempo: {elapsed:.3} secondi
Velocità: {rate:.1} partite sorgente/s

Nota metodologica:
il pilot non applica ancora la deduplicazione globale del campione 2024.
",
        pgn = config.pgn.display(),
        out = config.out_dir.display(),
        max_test_games = config.max_test_games,
        max_ply = config.max_ply,
        record_bytes = DECISION_RECORD_BYTES,
        rate = games_read as f64 / elapsed.max(0.001),
    );

    fs::write(config.out_dir.join("summary.txt"), &summary)
        .map_err(|error| format!("Errore scrivendo summary.txt: {error}"))?;

    let unix_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Errore orologio di sistema: {error}"))?
        .as_secs();

    let metadata = format!(
        concat!(
            "{{\n",
            "  \"program\": \"antiskill\",\n",
            "  \"version\": \"{}\",\n",
            "  \"stage\": \"emit-test\",\n",
            "  \"unix_time\": {},\n",
            "  \"pgn\": \"{}\",\n",
            "  \"output\": \"{}\",\n",
            "  \"max_test_games\": {},\n",
            "  \"max_ply\": {},\n",
            "  \"decision_record_bytes\": {},\n",
            "  \"global_test_deduplication\": false\n",
            "}}\n"
        ),
        env!("CARGO_PKG_VERSION"),
        unix_time,
        escape_json(&config.pgn.display().to_string()),
        escape_json(&config.out_dir.display().to_string()),
        config.max_test_games,
        config.max_ply,
        DECISION_RECORD_BYTES,
    );

    fs::write(config.out_dir.join("run-metadata.json"), metadata)
        .map_err(|error| format!("Errore scrivendo run-metadata.json: {error}"))?;

    fs::write(config.out_dir.join("EMIT-COMPLETE"), b"")
        .map_err(|error| format!("Errore creando EMIT-COMPLETE: {error}"))?;

    println!("{summary}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_valid_ratings() {
        assert_eq!(parse_rating(b"100"), Some(100));
        assert_eq!(parse_rating(b" 2400 "), Some(2400));
        assert_eq!(parse_rating(b"4000"), Some(4000));

        assert_eq!(parse_rating(b"99"), None);
        assert_eq!(parse_rating(b"4001"), None);
        assert_eq!(parse_rating(b"?"), None);
    }

    #[test]
    fn decision_record_is_exactly_nineteen_bytes() {
        let mut bytes = Vec::new();

        write_decision_record(&mut bytes, 123_u128, 42, 17, 321, 1800).expect("scrittura valida");

        assert_eq!(bytes.len(), DECISION_RECORD_BYTES as usize);
    }

    #[test]
    fn tsv_escaping_is_unambiguous() {
        assert_eq!(escape_tsv("a\tb\nc\\d"), "a\\tb\\nc\\\\d");
    }
}
