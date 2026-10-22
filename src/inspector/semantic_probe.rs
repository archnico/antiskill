use super::{
    catalog, interpretation, mainlines, occurrences, parsing, segmentation, source, tokenization,
};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct SemanticProbeResult {
    pub(super) games: u64,
    pub(super) tokens: u64,
    pub(super) nodes: u64,
    pub(super) moves: u64,
    pub(super) comparable: u64,
    pub(super) mainline_results: u64,
    pub(super) digest: String,
    pub(super) mainline_digest: String,
    pub(super) total: Duration,
    pub(super) tokenize: Duration,
    pub(super) parse: Duration,
    pub(super) interpret: Duration,
    pub(super) interpret_core: Duration,
    pub(super) interpret_core_view: Duration,
    pub(super) interpret_core_setup: Duration,
    pub(super) interpret_core_movetext: Duration,
    pub(super) interpret_core_finalize: Duration,
    pub(super) move_total: Duration,
    pub(super) move_range_raw: Duration,
    pub(super) move_position_clone: Duration,
    pub(super) move_apply_san: Duration,
    pub(super) move_count_profiled: u64,
    pub(super) move_apply_attempts: u64,
    pub(super) interpret_convert: Duration,
    pub(super) interpret_validate: Duration,
    pub(super) catalog: Duration,
    pub(super) occurrence: Duration,
    pub(super) mainline: Duration,
}

#[derive(Debug)]
struct WorkItem {
    ordinal: u64,
    stream: tokenization::TokenStream,
    blob: Vec<u8>,
}

#[derive(Debug)]
pub(in crate::inspector) struct GameSummary {
    pub(in crate::inspector) ordinal: u64,
    tokens: u64,
    nodes: u64,
    moves: u64,
    mainline_moves: u64,
    pub(in crate::inspector) comparable: bool,
    results: u64,
    pub(in crate::inspector) identity: Option<String>,
    pub(in crate::inspector) source_start: u64,
    pub(in crate::inspector) source_end: u64,
    pub(in crate::inspector) source_game_blake3: String,
    pub(in crate::inspector) occurrence_id: String,
    pub(in crate::inspector) direct_fast: bool,
    parse: Duration,
    interpret: Duration,
    interpret_core: Duration,
    interpret_core_view: Duration,
    interpret_core_setup: Duration,
    interpret_core_movetext: Duration,
    interpret_core_finalize: Duration,
    move_total: Duration,
    move_range_raw: Duration,
    move_position_clone: Duration,
    move_apply_san: Duration,
    move_count_profiled: u64,
    move_apply_attempts: u64,
    interpret_convert: Duration,
    interpret_validate: Duration,
    catalog: Duration,
    occurrence: Duration,
    mainline: Duration,
}

const COMPACT_HEADER_BYTES: u64 = 56;
const COMPACT_RECORD_BYTES: u64 = 128;
const COMPACT80_RECORD_BYTES: u64 = 80;

struct Compact80Writer {
    writer: BufWriter<File>,
    records: u64,
}

impl Compact80Writer {
    fn create(path: &Path, source_blake3: &str, source_bytes: u64) -> Result<Self, String> {
        let file = File::create(path).map_err(|error| {
            format!(
                "Impossibile creare compact80 probe {}: {error}",
                path.display()
            )
        })?;
        let writer_capacity = std::env::var("ANTISKILL_PROBE_INDEX_BUFFER")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(8192)
            .max(1024);
        let mut writer = BufWriter::with_capacity(writer_capacity, file);
        writer.write_all(b"ASKCP80\0").map_err(|e| e.to_string())?;
        writer
            .write_all(&0_u32.to_le_bytes())
            .map_err(|e| e.to_string())?;
        writer
            .write_all(&(COMPACT80_RECORD_BYTES as u32).to_le_bytes())
            .map_err(|e| e.to_string())?;
        writer
            .write_all(&source_bytes.to_le_bytes())
            .map_err(|e| e.to_string())?;
        writer
            .write_all(&decode_hash(source_blake3)?)
            .map_err(|e| e.to_string())?;
        Ok(Self { writer, records: 0 })
    }

    fn write_game(&mut self, game: &GameSummary) -> Result<(), String> {
        let game_bytes = game
            .source_end
            .checked_sub(game.source_start)
            .ok_or("Intervallo compact80 invertito")?;
        let game_bytes = u32::try_from(game_bytes)
            .map_err(|_| "Partita oltre 4 GiB non rappresentabile in compact80".to_owned())?;
        self.writer
            .write_all(&game.source_start.to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(&game_bytes.to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(&decode_hash(&game.source_game_blake3)?)
            .map_err(|e| e.to_string())?;
        let canonical = game
            .identity
            .as_deref()
            .map(decode_hash)
            .transpose()?
            .unwrap_or([0_u8; 32]);
        self.writer
            .write_all(&canonical)
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(&[u8::from(game.comparable), 0, 0, 0])
            .map_err(|e| e.to_string())?;
        self.records = self
            .records
            .checked_add(1)
            .ok_or("Record compact80 probe oltre u64")?;
        Ok(())
    }

    fn finish(mut self) -> Result<u64, String> {
        self.writer.flush().map_err(|e| e.to_string())?;
        COMPACT_HEADER_BYTES
            .checked_add(
                self.records
                    .checked_mul(COMPACT80_RECORD_BYTES)
                    .ok_or("Dimensione compact80 probe oltre u64")?,
            )
            .ok_or("Dimensione compact80 probe oltre u64".to_owned())
    }
}

struct CompactWriter {
    writer: BufWriter<File>,
    records: u64,
}

fn decode_hash(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 {
        return Err("Hash compact probe non lungo 64 caratteri".to_owned());
    }
    fn nibble(value: u8) -> Result<u8, String> {
        match value {
            b'0'..=b'9' => Ok(value - b'0'),
            b'a'..=b'f' => Ok(value - b'a' + 10),
            _ => Err("Hash compact probe non esadecimale lowercase".to_owned()),
        }
    }
    let bytes = value.as_bytes();
    let mut output = [0_u8; 32];
    for index in 0..32 {
        output[index] = (nibble(bytes[index * 2])? << 4) | nibble(bytes[index * 2 + 1])?;
    }
    Ok(output)
}

impl CompactWriter {
    fn create(path: &Path, source_blake3: &str, source_bytes: u64) -> Result<Self, String> {
        let file = File::create(path).map_err(|error| {
            format!(
                "Impossibile creare compact probe {}: {error}",
                path.display()
            )
        })?;
        let writer_capacity = std::env::var("ANTISKILL_PROBE_INDEX_BUFFER")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(8192)
            .max(1024);
        let mut writer = BufWriter::with_capacity(writer_capacity, file);
        writer.write_all(b"ASKCPV0\0").map_err(|e| e.to_string())?;
        writer
            .write_all(&0_u32.to_le_bytes())
            .map_err(|e| e.to_string())?;
        writer
            .write_all(&(COMPACT_RECORD_BYTES as u32).to_le_bytes())
            .map_err(|e| e.to_string())?;
        writer
            .write_all(&source_bytes.to_le_bytes())
            .map_err(|e| e.to_string())?;
        writer
            .write_all(&decode_hash(source_blake3)?)
            .map_err(|e| e.to_string())?;
        Ok(Self { writer, records: 0 })
    }

    fn write_game(&mut self, game: &GameSummary) -> Result<(), String> {
        let mainline_moves = u32::try_from(game.mainline_moves)
            .map_err(|_| "Troppe mosse mainline per compact probe".to_owned())?;
        let results =
            u8::try_from(game.results).map_err(|_| "Troppi result per compact probe".to_owned())?;
        self.writer
            .write_all(&game.ordinal.to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(&game.source_start.to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(&game.source_end.to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(&decode_hash(&game.source_game_blake3)?)
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(&decode_hash(&game.occurrence_id)?)
            .map_err(|e| e.to_string())?;
        let canonical = game
            .identity
            .as_deref()
            .map(decode_hash)
            .transpose()?
            .unwrap_or([0_u8; 32]);
        self.writer
            .write_all(&canonical)
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(&[u8::from(game.comparable)])
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(&mainline_moves.to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(&[results, 0, 0])
            .map_err(|e| e.to_string())?;
        self.records = self
            .records
            .checked_add(1)
            .ok_or("Record compact probe oltre u64")?;
        Ok(())
    }

    fn finish(mut self) -> Result<u64, String> {
        self.writer.flush().map_err(|e| e.to_string())?;
        COMPACT_HEADER_BYTES
            .checked_add(
                self.records
                    .checked_mul(COMPACT_RECORD_BYTES)
                    .ok_or("Dimensione compact probe oltre u64")?,
            )
            .ok_or("Dimensione compact probe oltre u64".to_owned())
    }
}

fn compact_output_path() -> Option<PathBuf> {
    std::env::var_os("ANTISKILL_PROBE_COMPACT_OUT").map(PathBuf::from)
}

fn compact80_output_path() -> Option<PathBuf> {
    std::env::var_os("ANTISKILL_PROBE_COMPACT80_OUT").map(PathBuf::from)
}

#[derive(Default)]
struct CollectorState {
    next_ordinal: u64,
    pending: BTreeMap<u64, GameSummary>,
    games: u64,
    tokens: u64,
    nodes: u64,
    moves: u64,
    comparable: u64,
    results: u64,
    digest: blake3::Hasher,
    mainline_digest: blake3::Hasher,
    parse: Duration,
    interpret: Duration,
    interpret_core: Duration,
    interpret_core_view: Duration,
    interpret_core_setup: Duration,
    interpret_core_movetext: Duration,
    interpret_core_finalize: Duration,
    move_total: Duration,
    move_range_raw: Duration,
    move_position_clone: Duration,
    move_apply_san: Duration,
    move_count_profiled: u64,
    move_apply_attempts: u64,
    interpret_convert: Duration,
    interpret_validate: Duration,
    catalog: Duration,
    occurrence: Duration,
    mainline: Duration,
}

fn add(counter: &mut Duration, started: Instant) {
    *counter += started.elapsed();
}

fn shard_config() -> Result<(u64, u64), String> {
    let count = std::env::var("ANTISKILL_PROBE_SHARDS")
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| "ANTISKILL_PROBE_SHARDS non valido".to_owned())
        })
        .transpose()?
        .unwrap_or(1);
    let index = std::env::var("ANTISKILL_PROBE_SHARD")
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| "ANTISKILL_PROBE_SHARD non valido".to_owned())
        })
        .transpose()?
        .unwrap_or(0);
    if count == 0 || index >= count {
        return Err("Configurazione shard probe non valida".to_owned());
    }
    Ok((count, index))
}

fn worker_config() -> Result<(usize, usize), String> {
    let workers = std::env::var("ANTISKILL_PROBE_WORKERS")
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "ANTISKILL_PROBE_WORKERS non valido".to_owned())
        })
        .transpose()?
        .unwrap_or(1);
    let depth = std::env::var("ANTISKILL_PROBE_QUEUE")
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "ANTISKILL_PROBE_QUEUE non valido".to_owned())
        })
        .transpose()?
        .unwrap_or(2);
    if workers == 0 || workers > 64 || depth == 0 || depth > 1024 {
        return Err("Configurazione worker/queue probe non valida".to_owned());
    }
    Ok((workers, depth))
}

fn lean_identity_enabled() -> bool {
    std::env::var_os("ANTISKILL_PROBE_LEAN_IDENTITY").is_some()
}

static DIRECT_FAST_GAMES: AtomicU64 = AtomicU64::new(0);
static DIRECT_FALLBACK_GAMES: AtomicU64 = AtomicU64::new(0);

fn direct_identity_enabled() -> bool {
    std::env::var_os("ANTISKILL_PROBE_DIRECT_IDENTITY").is_some()
}

pub(super) fn direct_identity_counts() -> (u64, u64) {
    (
        DIRECT_FAST_GAMES.load(Ordering::Relaxed),
        DIRECT_FALLBACK_GAMES.load(Ordering::Relaxed),
    )
}

fn process_item_mode(
    item: WorkItem,
    source_blake3: &str,
    source_bytes: u64,
    direct_identity: bool,
) -> Result<GameSummary, String> {
    let WorkItem {
        ordinal,
        stream,
        blob,
    } = item;
    let tokens = stream.tokens.len() as u64;

    if direct_identity {
        let started = Instant::now();
        if let Some((lean, nodes, moves)) =
            interpretation::lean_mainline_identity_tokens_fast_direct(&stream, &blob)
        {
            let interpret = started.elapsed();
            DIRECT_FAST_GAMES.fetch_add(1, Ordering::Relaxed);
            let started = Instant::now();
            let (source_game_blake3, occurrence_id) = occurrences::probe_occurrence_identity(
                source_blake3,
                source_bytes,
                stream.start,
                stream.end,
                &blob,
            );
            let occurrence_time = started.elapsed();
            return Ok(GameSummary {
                ordinal,
                tokens,
                nodes,
                moves,
                mainline_moves: lean.moves,
                comparable: lean.comparable,
                results: lean.results,
                identity: lean.identity,
                source_start: stream.start,
                source_end: stream.end,
                source_game_blake3,
                occurrence_id,
                direct_fast: true,
                parse: Duration::ZERO,
                interpret,
                interpret_core: interpret,
                interpret_core_view: Duration::ZERO,
                interpret_core_setup: Duration::ZERO,
                interpret_core_movetext: Duration::ZERO,
                interpret_core_finalize: Duration::ZERO,
                move_total: Duration::ZERO,
                move_range_raw: Duration::ZERO,
                move_position_clone: Duration::ZERO,
                move_apply_san: Duration::ZERO,
                move_count_profiled: 0,
                move_apply_attempts: 0,
                interpret_convert: Duration::ZERO,
                interpret_validate: Duration::ZERO,
                catalog: Duration::ZERO,
                occurrence: occurrence_time,
                mainline: Duration::ZERO,
            });
        }
        DIRECT_FALLBACK_GAMES.fetch_add(1, Ordering::Relaxed);
    }

    let started = Instant::now();
    let tree = parsing::parse_stream_direct(&stream)?;
    let parse = started.elapsed();
    let nodes = tree.nodes.len() as u64;

    if lean_identity_enabled() || direct_identity {
        let moves = tree
            .nodes
            .iter()
            .filter(|node| node.kind == parsing::model::NodeKind::MoveSymbolCandidate)
            .count() as u64;
        let started = Instant::now();
        let lean = interpretation::lean_mainline_identity_direct(ordinal, &tree, &stream, &blob);
        let interpret = started.elapsed();
        let started = Instant::now();
        let (source_game_blake3, occurrence_id) = occurrences::probe_occurrence_identity(
            source_blake3,
            source_bytes,
            stream.start,
            stream.end,
            &blob,
        );
        let occurrence_time = started.elapsed();
        return Ok(GameSummary {
            ordinal,
            tokens,
            nodes,
            moves,
            mainline_moves: lean.moves,
            comparable: lean.comparable,
            results: lean.results,
            identity: lean.identity,
            source_start: stream.start,
            source_end: stream.end,
            source_game_blake3,
            occurrence_id,
            direct_fast: false,
            parse,
            interpret,
            interpret_core: interpret,
            interpret_core_view: Duration::ZERO,
            interpret_core_setup: Duration::ZERO,
            interpret_core_movetext: Duration::ZERO,
            interpret_core_finalize: Duration::ZERO,
            move_total: Duration::ZERO,
            move_range_raw: Duration::ZERO,
            move_position_clone: Duration::ZERO,
            move_apply_san: Duration::ZERO,
            move_count_profiled: 0,
            move_apply_attempts: 0,
            interpret_convert: Duration::ZERO,
            interpret_validate: Duration::ZERO,
            catalog: Duration::ZERO,
            occurrence: occurrence_time,
            mainline: Duration::ZERO,
        });
    }

    let started = Instant::now();
    let profiled = interpretation::interpret_one_direct_profiled(ordinal, &tree, &stream, &blob)?;
    let interpret = started.elapsed();
    let interpret_core = profiled.core;
    let interpret_core_view = profiled.core_view;
    let interpret_core_setup = profiled.core_setup;
    let interpret_core_movetext = profiled.core_movetext;
    let interpret_core_finalize = profiled.core_finalize;
    let move_total = profiled.move_total;
    let move_range_raw = profiled.move_range_raw;
    let move_position_clone = profiled.move_position_clone;
    let move_apply_san = profiled.move_apply_san;
    let move_count_profiled = profiled.move_count;
    let move_apply_attempts = profiled.move_apply_attempts;
    let interpret_convert = profiled.convert;
    let interpret_validate = profiled.validate;
    let game = profiled.record;
    let moves = game.moves.len() as u64;

    let started = Instant::now();
    let catalog_game =
        catalog::probe_catalog_game(ordinal, &game, &tree, &stream, &blob, source_bytes)?;
    let catalog = started.elapsed();

    let started = Instant::now();
    let occurrence = occurrences::probe_occurrence(
        ordinal,
        source_blake3,
        source_bytes,
        &catalog_game,
        &game,
        &tree,
        &stream,
    )?;
    let occurrence_time = started.elapsed();
    let compact = occurrences::probe_occurrence_compact_fields(&occurrence);
    let occurrence_id = occurrence.occurrence_id.clone();

    let started = Instant::now();
    let mainline = mainlines::probe_mainline(ordinal, &occurrence, &game, &tree, &stream, &blob)?;
    let mainline_time = started.elapsed();

    Ok(GameSummary {
        ordinal,
        tokens,
        nodes,
        moves,
        mainline_moves: mainline.moves as u64,
        comparable: mainline.comparable,
        results: mainline.results as u64,
        identity: mainline.identity,
        source_start: compact.source_start,
        source_end: compact.source_end,
        source_game_blake3: compact.source_game_blake3,
        occurrence_id,
        direct_fast: false,
        parse,
        interpret,
        interpret_core,
        interpret_core_view,
        interpret_core_setup,
        interpret_core_movetext,
        interpret_core_finalize,
        move_total,
        move_range_raw,
        move_position_clone,
        move_apply_san,
        move_count_profiled,
        move_apply_attempts,
        interpret_convert,
        interpret_validate,
        catalog,
        occurrence: occurrence_time,
        mainline: mainline_time,
    })
}

fn process_item(
    item: WorkItem,
    source_blake3: &str,
    source_bytes: u64,
) -> Result<GameSummary, String> {
    process_item_mode(item, source_blake3, source_bytes, direct_identity_enabled())
}

pub(in crate::inspector) fn process_fused_game(
    ordinal: u64,
    stream: tokenization::TokenStream,
    blob: Vec<u8>,
    source_blake3: &str,
    source_bytes: u64,
) -> Result<GameSummary, String> {
    process_item_mode(
        WorkItem {
            ordinal,
            stream,
            blob,
        },
        source_blake3,
        source_bytes,
        true,
    )
}

fn absorb(state: &mut CollectorState, game: GameSummary) -> Result<(), String> {
    state.games = state
        .games
        .checked_add(1)
        .ok_or("Conteggio giochi probe oltre u64")?;
    state.tokens = state
        .tokens
        .checked_add(game.tokens)
        .ok_or("Conteggio token probe oltre u64")?;
    state.nodes = state
        .nodes
        .checked_add(game.nodes)
        .ok_or("Conteggio nodi probe oltre u64")?;
    state.moves = state
        .moves
        .checked_add(game.moves)
        .ok_or("Conteggio mosse probe oltre u64")?;
    state.comparable += u64::from(game.comparable);
    state.results = state
        .results
        .checked_add(game.results)
        .ok_or("Conteggio result probe oltre u64")?;
    state.parse += game.parse;
    state.interpret += game.interpret;
    state.interpret_core += game.interpret_core;
    state.interpret_core_view += game.interpret_core_view;
    state.interpret_core_setup += game.interpret_core_setup;
    state.interpret_core_movetext += game.interpret_core_movetext;
    state.interpret_core_finalize += game.interpret_core_finalize;
    state.move_total += game.move_total;
    state.move_range_raw += game.move_range_raw;
    state.move_position_clone += game.move_position_clone;
    state.move_apply_san += game.move_apply_san;
    state.move_count_profiled = state
        .move_count_profiled
        .checked_add(game.move_count_profiled)
        .ok_or("Conteggio mosse profilate probe oltre u64")?;
    state.move_apply_attempts = state
        .move_apply_attempts
        .checked_add(game.move_apply_attempts)
        .ok_or("Conteggio apply SAN probe oltre u64")?;
    state.interpret_convert += game.interpret_convert;
    state.interpret_validate += game.interpret_validate;
    state.catalog += game.catalog;
    state.occurrence += game.occurrence;
    state.mainline += game.mainline;

    for hasher in [&mut state.digest, &mut state.mainline_digest] {
        hasher.update(&game.ordinal.to_le_bytes());
    }
    state.digest.update(&game.tokens.to_le_bytes());
    state.digest.update(&game.nodes.to_le_bytes());
    for hasher in [&mut state.digest, &mut state.mainline_digest] {
        hasher.update(&game.mainline_moves.to_le_bytes());
        hasher.update(&game.results.to_le_bytes());
        hasher.update(&[u8::from(game.comparable)]);
        if let Some(identity) = game.identity.as_deref() {
            hasher.update(identity.as_bytes());
        }
    }
    Ok(())
}

fn collect_results(
    rx: Receiver<Result<GameSummary, String>>,
    compact_path: Option<PathBuf>,
    source_blake3: &str,
    source_bytes: u64,
) -> Result<CollectorState, String> {
    let mut state = CollectorState::default();
    let mut compact = compact_path
        .as_deref()
        .map(|path| CompactWriter::create(path, source_blake3, source_bytes))
        .transpose()?;
    let mut compact80 = compact80_output_path()
        .as_deref()
        .map(|path| Compact80Writer::create(path, source_blake3, source_bytes))
        .transpose()?;
    let mut first_error = None;
    while let Ok(result) = rx.recv() {
        match result {
            Ok(game) if first_error.is_none() => {
                if state.pending.insert(game.ordinal, game).is_some() {
                    first_error = Some("Ordinal duplicato nel collector probe".to_owned());
                    continue;
                }
                while let Some(game) = state.pending.remove(&state.next_ordinal) {
                    if let Some(writer) = compact.as_mut() {
                        if let Err(error) = writer.write_game(&game) {
                            first_error = Some(error);
                            break;
                        }
                    }
                    if let Some(writer) = compact80.as_mut() {
                        if let Err(error) = writer.write_game(&game) {
                            first_error = Some(error);
                            break;
                        }
                    }
                    if let Err(error) = absorb(&mut state, game) {
                        first_error = Some(error);
                        break;
                    }
                    state.next_ordinal += 1;
                }
            }
            Ok(_) => {}
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    if !state.pending.is_empty() {
        return Err("Gap di ordinal nel collector probe".to_owned());
    }
    if let Some(writer) = compact {
        let _ = writer.finish()?;
    }
    if let Some(writer) = compact80 {
        let _ = writer.finish()?;
    }
    Ok(state)
}

fn worker_loop(
    rx: Receiver<WorkItem>,
    tx: SyncSender<Result<GameSummary, String>>,
    source_blake3: String,
    source_bytes: u64,
) {
    while let Ok(item) = rx.recv() {
        match process_item(item, &source_blake3, source_bytes) {
            Ok(summary) => {
                if tx.send(Ok(summary)).is_err() {
                    break;
                }
            }
            Err(error) => {
                let _ = tx.send(Err(error));
                break;
            }
        }
    }
}

fn run_parallel(
    root: &Path,
    segmentation_path: &Path,
    workers: usize,
    depth: usize,
) -> Result<SemanticProbeResult, String> {
    if std::env::var_os("ANTISKILL_PROBE_SHARDS").is_some()
        || std::env::var_os("ANTISKILL_PROBE_SHARD").is_some()
    {
        return Err("Sharding esterno e worker interni non sono combinabili".to_owned());
    }
    let total_started = Instant::now();
    let loaded = segmentation::load_streaming_segmentation(root, segmentation_path, None)?;
    let source_blake3 = loaded.manifest.source_blake3.clone();
    let source_bytes = loaded.manifest.source_bytes;
    let mut tokenize_time = Duration::ZERO;
    let mut candidate_ordinal = 0_u64;

    let state = std::thread::scope(|scope| -> Result<CollectorState, String> {
        let result_capacity = workers
            .checked_mul(depth)
            .and_then(|value| value.checked_mul(2))
            .ok_or("Capacità risultati probe oltre usize")?;
        let (result_tx, result_rx) =
            mpsc::sync_channel::<Result<GameSummary, String>>(result_capacity);
        let compact_path = compact_output_path();
        let collector_source_blake3 = source_blake3.clone();
        let collector = scope.spawn(move || {
            collect_results(
                result_rx,
                compact_path,
                &collector_source_blake3,
                source_bytes,
            )
        });
        let mut senders: Vec<SyncSender<WorkItem>> = Vec::with_capacity(workers);
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let (tx, rx) = mpsc::sync_channel::<WorkItem>(depth);
            senders.push(tx);
            let result_tx = result_tx.clone();
            let source_blake3 = source_blake3.clone();
            handles
                .push(scope.spawn(move || worker_loop(rx, result_tx, source_blake3, source_bytes)));
        }
        drop(result_tx);

        let mut producer_error = None;
        let produce = loaded.for_each_segment(|segment| {
            if segment.kind != "game_candidate" {
                return Ok(());
            }
            let ordinal = candidate_ordinal;
            candidate_ordinal = candidate_ordinal
                .checked_add(1)
                .ok_or("Ordinal candidato probe oltre u64")?;
            let started = Instant::now();
            let (stream, blob) =
                tokenization::tokenize_streaming_file_segment_with_bytes(&loaded, &segment)?;
            add(&mut tokenize_time, started);
            let index = ordinal as usize % workers;
            senders[index]
                .send(WorkItem {
                    ordinal,
                    stream,
                    blob,
                })
                .map_err(|_| "Worker probe terminato prematuramente".to_owned())
        });
        if let Err(error) = produce {
            producer_error = Some(error);
        }
        drop(senders);
        for handle in handles {
            if handle.join().is_err() && producer_error.is_none() {
                producer_error = Some("Worker probe in panic".to_owned());
            }
        }
        let collected = collector
            .join()
            .map_err(|_| "Collector probe in panic".to_owned())??;
        if let Some(error) = producer_error {
            return Err(error);
        }
        Ok(collected)
    })?;

    Ok(SemanticProbeResult {
        games: state.games,
        tokens: state.tokens,
        nodes: state.nodes,
        moves: state.moves,
        comparable: state.comparable,
        mainline_results: state.results,
        digest: state.digest.finalize().to_hex().to_string(),
        mainline_digest: state.mainline_digest.finalize().to_hex().to_string(),
        total: total_started.elapsed(),
        tokenize: tokenize_time,
        parse: state.parse,
        interpret: state.interpret,
        interpret_core: state.interpret_core,
        interpret_core_view: state.interpret_core_view,
        interpret_core_setup: state.interpret_core_setup,
        interpret_core_movetext: state.interpret_core_movetext,
        interpret_core_finalize: state.interpret_core_finalize,
        move_total: state.move_total,
        move_range_raw: state.move_range_raw,
        move_position_clone: state.move_position_clone,
        move_apply_san: state.move_apply_san,
        move_count_profiled: state.move_count_profiled,
        move_apply_attempts: state.move_apply_attempts,
        interpret_convert: state.interpret_convert,
        interpret_validate: state.interpret_validate,
        catalog: state.catalog,
        occurrence: state.occurrence,
        mainline: state.mainline,
    })
}

fn fused_source_manifest_path() -> Option<PathBuf> {
    std::env::var_os("ANTISKILL_PROBE_FUSED_SOURCE_MANIFEST").map(PathBuf::from)
}

fn result_from_state(
    state: CollectorState,
    total: Duration,
    tokenize: Duration,
) -> SemanticProbeResult {
    SemanticProbeResult {
        games: state.games,
        tokens: state.tokens,
        nodes: state.nodes,
        moves: state.moves,
        comparable: state.comparable,
        mainline_results: state.results,
        digest: state.digest.finalize().to_hex().to_string(),
        mainline_digest: state.mainline_digest.finalize().to_hex().to_string(),
        total,
        tokenize,
        parse: state.parse,
        interpret: state.interpret,
        interpret_core: state.interpret_core,
        interpret_core_view: state.interpret_core_view,
        interpret_core_setup: state.interpret_core_setup,
        interpret_core_movetext: state.interpret_core_movetext,
        interpret_core_finalize: state.interpret_core_finalize,
        move_total: state.move_total,
        move_range_raw: state.move_range_raw,
        move_position_clone: state.move_position_clone,
        move_apply_san: state.move_apply_san,
        move_count_profiled: state.move_count_profiled,
        move_apply_attempts: state.move_apply_attempts,
        interpret_convert: state.interpret_convert,
        interpret_validate: state.interpret_validate,
        catalog: state.catalog,
        occurrence: state.occurrence,
        mainline: state.mainline,
    }
}

fn run_fused_source(
    root: &Path,
    source_manifest_path: &Path,
    workers: usize,
    depth: usize,
) -> Result<SemanticProbeResult, String> {
    if std::env::var_os("ANTISKILL_PROBE_SHARDS").is_some()
        || std::env::var_os("ANTISKILL_PROBE_SHARD").is_some()
    {
        return Err("Sharding esterno e fused source probe non sono combinabili".to_owned());
    }
    let total_started = Instant::now();
    let loaded = source::load_source_manifest(root, source_manifest_path, None)?;
    let source_blake3 = loaded.manifest.blake3.clone();
    let source_bytes = loaded.manifest.bytes;
    let object_path = loaded.object_path;
    let mut tokenize_time = Duration::ZERO;
    let mut candidate_ordinal = 0_u64;

    let state = std::thread::scope(|scope| -> Result<CollectorState, String> {
        let result_capacity = workers
            .checked_mul(depth)
            .and_then(|value| value.checked_mul(2))
            .ok_or("Capacità risultati fused probe oltre usize")?;
        let (result_tx, result_rx) =
            mpsc::sync_channel::<Result<GameSummary, String>>(result_capacity);
        let compact_path = compact_output_path();
        let collector_source_blake3 = source_blake3.clone();
        let collector = scope.spawn(move || {
            collect_results(
                result_rx,
                compact_path,
                &collector_source_blake3,
                source_bytes,
            )
        });
        let mut senders: Vec<SyncSender<WorkItem>> = Vec::with_capacity(workers);
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let (tx, rx) = mpsc::sync_channel::<WorkItem>(depth);
            senders.push(tx);
            let result_tx = result_tx.clone();
            let worker_source_blake3 = source_blake3.clone();
            handles.push(
                scope.spawn(move || worker_loop(rx, result_tx, worker_source_blake3, source_bytes)),
            );
        }
        drop(result_tx);

        let mut producer_error = None;
        let scan = segmentation::for_each_game_candidate_blob(&object_path, |segment, blob| {
            let ordinal = candidate_ordinal;
            candidate_ordinal = candidate_ordinal
                .checked_add(1)
                .ok_or("Ordinal candidato fused probe oltre u64")?;
            let started = Instant::now();
            let stream = tokenization::tokenize_direct_segment_bytes(&segment, &blob)?;
            add(&mut tokenize_time, started);
            let index = ordinal as usize % workers;
            senders[index]
                .send(WorkItem {
                    ordinal,
                    stream,
                    blob,
                })
                .map_err(|_| "Worker fused probe terminato prematuramente".to_owned())
        });
        match scan {
            Ok((bytes, hash, games)) => {
                if bytes != source_bytes || hash != source_blake3 {
                    producer_error =
                        Some("SourceBlob fused probe non corrisponde al manifest".to_owned());
                } else if games != candidate_ordinal {
                    producer_error = Some("Conteggio fused game_candidate incoerente".to_owned());
                }
            }
            Err(error) => producer_error = Some(error),
        }
        drop(senders);
        for handle in handles {
            if handle.join().is_err() && producer_error.is_none() {
                producer_error = Some("Worker fused probe in panic".to_owned());
            }
        }
        let collected = collector
            .join()
            .map_err(|_| "Collector fused probe in panic".to_owned())??;
        if let Some(error) = producer_error {
            return Err(error);
        }
        Ok(collected)
    })?;

    Ok(result_from_state(
        state,
        total_started.elapsed(),
        tokenize_time,
    ))
}

pub(super) fn run(root: &Path, segmentation_path: &Path) -> Result<SemanticProbeResult, String> {
    let (workers, depth) = worker_config()?;
    if let Some(source_manifest) = fused_source_manifest_path() {
        return run_fused_source(root, &source_manifest, workers, depth);
    }
    if workers > 1 {
        return run_parallel(root, segmentation_path, workers, depth);
    }
    if compact_output_path().is_some() {
        return Err("Compact output probe richiede almeno 2 worker".to_owned());
    }

    let total_started = Instant::now();
    let loaded = segmentation::load_streaming_segmentation(root, segmentation_path, None)?;
    let source_blake3 = loaded.manifest.source_blake3.clone();
    let source_bytes = loaded.manifest.source_bytes;
    let (shard_count, shard_index) = shard_config()?;
    let mut candidate_ordinal = 0_u64;
    let mut state = CollectorState::default();
    let mut tokenize_time = Duration::ZERO;

    loaded.for_each_segment(|segment| {
        if segment.kind != "game_candidate" {
            return Ok(());
        }
        let ordinal = candidate_ordinal;
        candidate_ordinal = candidate_ordinal
            .checked_add(1)
            .ok_or("Ordinal candidato probe oltre u64")?;
        if ordinal % shard_count != shard_index {
            return Ok(());
        }
        let started = Instant::now();
        let (stream, blob) =
            tokenization::tokenize_streaming_file_segment_with_bytes(&loaded, &segment)?;
        add(&mut tokenize_time, started);
        let summary = process_item(
            WorkItem {
                ordinal,
                stream,
                blob,
            },
            &source_blake3,
            source_bytes,
        )?;
        absorb(&mut state, summary)?;
        state.next_ordinal = ordinal + 1;
        Ok(())
    })?;

    Ok(SemanticProbeResult {
        games: state.games,
        tokens: state.tokens,
        nodes: state.nodes,
        moves: state.moves,
        comparable: state.comparable,
        mainline_results: state.results,
        digest: state.digest.finalize().to_hex().to_string(),
        mainline_digest: state.mainline_digest.finalize().to_hex().to_string(),
        total: total_started.elapsed(),
        tokenize: tokenize_time,
        parse: state.parse,
        interpret: state.interpret,
        interpret_core: state.interpret_core,
        interpret_core_view: state.interpret_core_view,
        interpret_core_setup: state.interpret_core_setup,
        interpret_core_movetext: state.interpret_core_movetext,
        interpret_core_finalize: state.interpret_core_finalize,
        move_total: state.move_total,
        move_range_raw: state.move_range_raw,
        move_position_clone: state.move_position_clone,
        move_apply_san: state.move_apply_san,
        move_count_profiled: state.move_count_profiled,
        move_apply_attempts: state.move_apply_attempts,
        interpret_convert: state.interpret_convert,
        interpret_validate: state.interpret_validate,
        catalog: state.catalog,
        occurrence: state.occurrence,
        mainline: state.mainline,
    })
}
