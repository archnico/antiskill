use pgn_reader::shakmaty::{uci::UciMove, zobrist::Zobrist128, Chess, EnPassantMode, Position};
use pgn_reader::{RawTag, Reader, SanPlus, Visitor};

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Instant;

const BUCKETS: usize = 256;
const RECORD_BYTES: u64 = 12;

#[derive(Debug)]
struct Config {
    pgn: PathBuf,
    out_dir: PathBuf,
    max_games: u64,
    max_ply: usize,
}

impl Config {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut pgn = None;
        let mut out_dir = None;
        let mut max_games = 100_000u64;
        let mut max_ply = 60usize;

        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--pgn" => {
                    pgn = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --pgn")?,
                    ));
                }

                "--out-dir" => {
                    out_dir = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --out-dir")?,
                    ));
                }

                "--max-games" => {
                    max_games = args
                        .next()
                        .ok_or("Manca il valore di --max-games")?
                        .parse()
                        .map_err(|_| "Valore non valido per --max-games")?;
                }

                "--max-ply" => {
                    max_ply = args
                        .next()
                        .ok_or("Manca il valore di --max-ply")?
                        .parse()
                        .map_err(|_| "Valore non valido per --max-ply")?;
                }

                "--help" | "-h" => {
                    return Err("USO: antiskill lab theory build-book \
                         --pgn FILE --out-dir DIR \
                         [--max-games N] [--max-ply N]"
                        .into());
                }

                other => {
                    return Err(format!("Opzione sconosciuta: {other}"));
                }
            }
        }

        let pgn = pgn.ok_or("Parametro obbligatorio: --pgn")?;
        let out_dir = out_dir.ok_or("Parametro obbligatorio: --out-dir")?;

        if max_ply == 0 {
            return Err("--max-ply deve essere maggiore di zero".into());
        }

        Ok(Self {
            pgn,
            out_dir,
            max_games,
            max_ply,
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum GameClass {
    Historical,
    Future,
    MissingDate,
    NonStandard,
}

#[derive(Default)]
struct GameTags {
    year: Option<u16>,
    has_fen: bool,
    setup: bool,
    nonstandard_variant: bool,
}

struct GameState {
    class: GameClass,
    position: Chess,
    ply: usize,
    records: Vec<(u128, u16)>,
}

struct GameOutput {
    class: GameClass,
    records: Vec<(u128, u16)>,
}

struct BookVisitor {
    max_ply: usize,
}

pub(super) fn parse_year(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 4 || !bytes[..4].iter().all(u8::is_ascii_digit) {
        return None;
    }

    std::str::from_utf8(&bytes[..4]).ok()?.parse().ok()
}

fn square_code(file: u8, rank: u8) -> Option<u16> {
    if !(b'a'..=b'h').contains(&file) || !(b'1'..=b'8').contains(&rank) {
        return None;
    }

    let file = u16::from(file - b'a');
    let rank = u16::from(rank - b'1');

    Some(rank * 8 + file)
}

pub(super) fn encode_uci(uci: &str) -> Result<u16, String> {
    let bytes = uci.as_bytes();

    if bytes.len() != 4 && bytes.len() != 5 {
        return Err(format!("Mossa UCI non codificabile: {uci}"));
    }

    let from = square_code(bytes[0], bytes[1])
        .ok_or_else(|| format!("Casa iniziale UCI non valida: {uci}"))?;

    let to = square_code(bytes[2], bytes[3])
        .ok_or_else(|| format!("Casa finale UCI non valida: {uci}"))?;

    let promotion = if bytes.len() == 5 {
        match bytes[4] {
            b'n' => 1u16,
            b'b' => 2u16,
            b'r' => 3u16,
            b'q' => 4u16,
            _ => {
                return Err(format!("Promozione UCI non valida: {uci}"));
            }
        }
    } else {
        0
    };

    Ok(from | (to << 6) | (promotion << 12))
}

impl Visitor for BookVisitor {
    type Tags = GameTags;
    type Movetext = GameState;
    type Output = Result<GameOutput, String>;

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
            }

            b"FEN" => {
                tags.has_fen = !bytes.is_empty();
            }

            b"SetUp" => {
                tags.setup = bytes == b"1";
            }

            b"Variant" => {
                let value = String::from_utf8_lossy(bytes).trim().to_ascii_lowercase();

                tags.nonstandard_variant =
                    !value.is_empty() && value != "standard" && value != "chess";
            }

            _ => {}
        }

        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, tags: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        let class = if tags.has_fen || tags.setup || tags.nonstandard_variant {
            GameClass::NonStandard
        } else {
            match tags.year {
                Some(year) if year <= 2023 => GameClass::Historical,

                Some(_) => GameClass::Future,
                None => GameClass::MissingDate,
            }
        };

        ControlFlow::Continue(GameState {
            class,
            position: Chess::default(),
            ply: 0,
            records: Vec::with_capacity(self.max_ply),
        })
    }

    fn san(&mut self, state: &mut Self::Movetext, san_plus: SanPlus) -> ControlFlow<Self::Output> {
        if !matches!(state.class, GameClass::Historical) {
            state.ply += 1;
            return ControlFlow::Continue(());
        }

        if state.ply >= self.max_ply {
            state.ply += 1;
            return ControlFlow::Continue(());
        }

        let played = match san_plus.san.to_move(&state.position) {
            Ok(played) => played,

            Err(error) => {
                return ControlFlow::Break(Err(format!(
                    "Mossa SAN illegale al ply {}: {error}",
                    state.ply
                )));
            }
        };

        let hash: Zobrist128 = state.position.zobrist_hash(EnPassantMode::Legal);

        let castling_mode = state.position.castles().mode();

        let uci = UciMove::from_move(played, castling_mode).to_string();

        let encoded_move = match encode_uci(&uci) {
            Ok(encoded) => encoded,
            Err(error) => {
                return ControlFlow::Break(Err(error));
            }
        };

        state.records.push((hash.0, encoded_move));

        state.position.play_unchecked(played);
        state.ply += 1;

        ControlFlow::Continue(())
    }

    fn end_game(&mut self, state: Self::Movetext) -> Self::Output {
        // Una posizione conta al massimo una volta per partita.
        // Una coppia posizione-mossa conta al massimo una volta per partita.
        let mut grouped: BTreeMap<u128, BTreeSet<u16>> = BTreeMap::new();

        for (hash, encoded_move) in state.records {
            grouped.entry(hash).or_default().insert(encoded_move);
        }

        let mut records = Vec::new();

        for (hash, moves) in grouped {
            let mut first_for_position = true;

            for encoded_move in moves {
                // Il bit 15 è libero nella nostra codifica UCI.
                // Lo usiamo per marcare una sola occorrenza
                // della posizione nella partita.
                let packed_move = if first_for_position {
                    encoded_move | 0x8000
                } else {
                    encoded_move
                };

                records.push((hash, packed_move));
                first_for_position = false;
            }
        }

        Ok(GameOutput {
            class: state.class,
            records,
        })
    }
}

fn create_bucket_writers(directory: &Path) -> Result<Vec<BufWriter<File>>, String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("Impossibile creare {}: {error}", directory.display()))?;

    let mut writers = Vec::with_capacity(BUCKETS);

    for bucket in 0..BUCKETS {
        let path = directory.join(format!("{bucket:02x}.bin"));

        let file = File::create(&path)
            .map_err(|error| format!("Impossibile creare {}: {error}", path.display()))?;

        writers.push(BufWriter::with_capacity(64 * 1024, file));
    }

    Ok(writers)
}

fn write_record(writer: &mut BufWriter<File>, hash: u128, packed_move: u16) -> Result<(), String> {
    let hash_bytes = hash.to_le_bytes();

    writer
        .write_all(&hash_bytes[..10])
        .and_then(|_| writer.write_all(&packed_move.to_le_bytes()))
        .map_err(|error| format!("Errore scrivendo record binario: {error}"))
}

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let cfg = Config::from_args(args)?;

    if !cfg.pgn.is_file() {
        return Err(format!("PGN non trovato: {}", cfg.pgn.display()));
    }

    fs::create_dir_all(&cfg.out_dir)
        .map_err(|error| format!("Impossibile creare output: {error}"))?;

    let bucket_dir = cfg.out_dir.join("raw-buckets");

    if bucket_dir.exists() {
        fs::remove_dir_all(&bucket_dir)
            .map_err(|error| format!("Impossibile pulire i bucket: {error}"))?;
    }

    let mut writers = create_bucket_writers(&bucket_dir)?;

    let file =
        File::open(&cfg.pgn).map_err(|error| format!("Impossibile aprire il PGN: {error}"))?;

    let mut reader = Reader::new(BufReader::with_capacity(4 * 1024 * 1024, file));

    let mut visitor = BookVisitor {
        max_ply: cfg.max_ply,
    };

    let started = Instant::now();

    let mut games_read = 0u64;
    let mut historical_games = 0u64;
    let mut future_games = 0u64;
    let mut missing_date_games = 0u64;
    let mut nonstandard_games = 0u64;
    let mut invalid_games = 0u64;
    let mut records_written = 0u64;

    loop {
        if cfg.max_games > 0 && games_read >= cfg.max_games {
            break;
        }

        let output = reader
            .read_game(&mut visitor)
            .map_err(|error| format!("Errore leggendo il PGN: {error}"))?;

        let Some(output) = output else {
            break;
        };

        games_read += 1;

        match output {
            Ok(game) => match game.class {
                GameClass::Historical => {
                    historical_games += 1;

                    for (hash, encoded_move) in game.records {
                        let bucket = ((hash >> 120) & 0xff) as usize;

                        write_record(&mut writers[bucket], hash, encoded_move)?;

                        records_written += 1;
                    }
                }

                GameClass::Future => {
                    future_games += 1;
                }

                GameClass::MissingDate => {
                    missing_date_games += 1;
                }

                GameClass::NonStandard => {
                    nonstandard_games += 1;
                }
            },

            Err(_) => {
                invalid_games += 1;
            }
        }

        if games_read.is_multiple_of(25_000) {
            let elapsed = started.elapsed().as_secs_f64().max(0.001);

            eprintln!(
                "[PILOT] {:>9} partite | \
                 {:>9} storiche | \
                 {:>11} record | \
                 {:>8.0} partite/s",
                games_read,
                historical_games,
                records_written,
                games_read as f64 / elapsed,
            );
        }
    }

    for writer in &mut writers {
        writer
            .flush()
            .map_err(|error| format!("Errore flush bucket: {error}"))?;
    }

    drop(writers);

    let raw_bytes = fs::read_dir(&bucket_dir)
        .map_err(|error| format!("Impossibile leggere bucket: {error}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum::<u64>();

    let elapsed = started.elapsed().as_secs_f64();

    let bytes_per_game = if historical_games > 0 {
        raw_bytes as f64 / historical_games as f64
    } else {
        0.0
    };

    let records_per_game = if historical_games > 0 {
        records_written as f64 / historical_games as f64
    } else {
        0.0
    };

    let summary = format!(
        "\
ANTISKILL THEORY — PILOT EMISSIONE BUCKET
==========================================
PGN: {pgn}
Output: {out}
Massimo partite: {max_games}
Massimo ply: {max_ply}
Impronta posizione: 88 bit effettivi
Deduplicazione intra-partita: attiva

Partite lette: {games_read}
Partite storiche incluse: {historical_games}
Partite future escluse: {future_games}
Partite senza anno escluse: {missing_date_games}
Partite non standard escluse: {nonstandard_games}
Partite invalide: {invalid_games}

Record scritti: {records_written}
Dimensione record: {record_bytes} byte
Dimensione bucket grezzi: {raw_bytes} byte
Record per partita storica: {records_per_game:.3}
Byte per partita storica: {bytes_per_game:.3}

Tempo: {elapsed:.3} secondi
Velocità: {rate:.1} partite/s
",
        pgn = cfg.pgn.display(),
        out = cfg.out_dir.display(),
        max_games = cfg.max_games,
        max_ply = cfg.max_ply,
        rate = games_read as f64 / elapsed.max(0.001),
        record_bytes = RECORD_BYTES,
    );

    fs::write(cfg.out_dir.join("summary.txt"), &summary)
        .map_err(|error| format!("Impossibile scrivere summary.txt: {error}"))?;

    println!("{summary}");

    Ok(())
}
