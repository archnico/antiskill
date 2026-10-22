use super::common::{
    path_to_manifest_string, prepare_directory_chain, publish_compressed_json,
    publish_compressed_json_stream,
};
use super::{
    for_each_verified_logical_game_row, ProductLogicalGameRowV1, VerifiedLogicalGameScanV1,
};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const VIEW_PROFILE: &str = "antiskill_unique_game_view_v2";
const GROUPS_PROFILE: &str = "antiskill_unique_game_groups_v2";
const LOGICAL_ID_PROFILE: &str = "antiskill_logical_game_id_v1";
const CANONICAL_MAINLINE_PROFILE: &str = "antiskill_canonical_mainline_v1";
const RECORDS_PROFILE: &str = "antiskill_unique_game_records_v2";
const RECORD_ORDERING_POLICY: &str =
    "comparable_canonical_then_occurrence__noncomparable_occurrence_v1";
const REPRESENTATIVE_POLICY: &str =
    "core_metadata_completeness_v1__lowest_public_occurrence_id_tiebreak";
const RECORD_BYTES: usize = 65;
const RUN_TARGET_BYTES: usize = 16 * 1024 * 1024;
const RUN_RECORDS: usize = RUN_TARGET_BYTES / RECORD_BYTES;
const MERGE_FANIN: usize = 64;

type SortRecord = [u8; RECORD_BYTES];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ArtifactRefV1 {
    path: String,
    blake3: String,
    logical_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct UniqueGameViewManifestV1 {
    schema_version: u32,
    profile: String,
    logical_game_id_profile: String,
    representative_policy: String,
    noncomparable_policy: String,
    record_listing_profile: String,
    record_ordering_policy: String,
    collection: ArtifactRefV1,
    duplicate_groups: ArtifactRefV1,
    source_occurrences: u64,
    comparable_occurrences: u64,
    noncomparable_singletons: u64,
    comparable_logical_games: u64,
    logical_games: u64,
    collapsed_occurrences: u64,
    duplicate_logical_games: u64,
    duplicate_group_occurrences: u64,
    max_occurrences_per_logical_game: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UniqueGameViewSummaryV1 {
    pub(crate) view_path: PathBuf,
    pub(crate) view_blake3: String,
    pub(crate) view_bytes: u64,
    pub(crate) groups_path: PathBuf,
    pub(crate) groups_blake3: String,
    pub(crate) groups_bytes: u64,
    pub(crate) source_occurrences: u64,
    pub(crate) comparable_occurrences: u64,
    pub(crate) noncomparable_singletons: u64,
    pub(crate) comparable_logical_games: u64,
    pub(crate) logical_games: u64,
    pub(crate) collapsed_occurrences: u64,
    pub(crate) duplicate_logical_games: u64,
    pub(crate) duplicate_group_occurrences: u64,
    pub(crate) max_occurrences_per_logical_game: u64,
}

#[derive(Default)]
struct GroupStats {
    seen_comparable_occurrences: u64,
    comparable_logical_games: u64,
    collapsed_occurrences: u64,
    duplicate_logical_games: u64,
    duplicate_group_occurrences: u64,
    max_occurrences_per_logical_game: u64,
}

struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new(root: &Path) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "Orologio di sistema precedente a UNIX_EPOCH")?
            .as_nanos();
        let base = root.join("raw/tmp");
        fs::create_dir_all(&base)
            .map_err(|error| format!("Creazione area temporanea unique-games: {error}"))?;
        let path = base.join(format!(
            "antiskill-unique-games-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path)
            .map_err(|error| format!("Creazione temporanei unique-games: {error}"))?;
        Ok(Self { path })
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn decode_hex_hash(value: &str, label: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 {
        return Err(format!("{label} non è un hash BLAKE3 canonico"));
    }
    let mut out = [0_u8; 32];
    let bytes = value.as_bytes();
    for index in 0..32 {
        let hi = hex_nibble(bytes[index * 2])
            .ok_or_else(|| format!("{label} non è un hash BLAKE3 canonico"))?;
        let lo = hex_nibble(bytes[index * 2 + 1])
            .ok_or_else(|| format!("{label} non è un hash BLAKE3 canonico"))?;
        out[index] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        48..=57 => Some(byte - 48),
        97..=102 => Some(byte - 97 + 10),
        _ => None,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn comparable_logical_id(canonical_mainline_id: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(LOGICAL_ID_PROFILE.as_bytes());
    hasher.update(b"\0canonical\0");
    hasher.update(CANONICAL_MAINLINE_PROFILE.as_bytes());
    hasher.update(b"\0");
    hasher.update(canonical_mainline_id.as_bytes());
    hasher.update(b"\n");
    format!("u1-{}", hasher.finalize().to_hex())
}

fn noncomparable_logical_id(occurrence_id: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(LOGICAL_ID_PROFILE.as_bytes());
    hasher.update(b"\0occurrence\0antiskill_game_occurrence_v1\0");
    hasher.update(occurrence_id.as_bytes());
    hasher.update(b"\n");
    format!("u1-{}", hasher.finalize().to_hex())
}

fn sort_record(row: &ProductLogicalGameRowV1) -> Result<Option<SortRecord>, String> {
    if row.comparable != row.canonical_mainline_id.is_some() {
        return Err("Confrontabilità logical-game row incoerente".into());
    }
    if !row.comparable {
        decode_hex_hash(&row.occurrence_id, "occurrence_id")?;
        return Ok(None);
    }
    let canonical = decode_hex_hash(
        row.canonical_mainline_id
            .as_deref()
            .ok_or("Occurrence confrontabile senza canonical_mainline_id")?,
        "canonical_mainline_id",
    )?;
    let occurrence = decode_hex_hash(&row.occurrence_id, "occurrence_id")?;
    let mut record = [0_u8; RECORD_BYTES];
    record[..32].copy_from_slice(&canonical);
    record[32..64].copy_from_slice(&occurrence);
    record[64] = row.representative_quality;
    Ok(Some(record))
}

fn write_record(writer: &mut impl Write, record: &SortRecord) -> Result<(), String> {
    writer
        .write_all(record)
        .map_err(|error| format!("Scrittura sort unique-games: {error}"))
}

fn read_record(reader: &mut impl Read) -> Result<Option<SortRecord>, String> {
    let mut record = [0_u8; RECORD_BYTES];
    let first = reader
        .read(&mut record[..1])
        .map_err(|error| format!("Lettura sort unique-games: {error}"))?;
    if first == 0 {
        return Ok(None);
    }
    reader
        .read_exact(&mut record[1..])
        .map_err(|_| "Record sort unique-games troncato".to_owned())?;
    Ok(Some(record))
}

fn flush_run(
    records: &mut Vec<SortRecord>,
    directory: &Path,
    prefix: &str,
    index: usize,
) -> Result<PathBuf, String> {
    records.sort_unstable();
    let path = directory.join(format!("{prefix}-{index:06}.bin"));
    let mut writer = BufWriter::new(
        File::create(&path).map_err(|error| format!("Creazione run unique-games: {error}"))?,
    );
    for record in records.iter() {
        write_record(&mut writer, record)?;
    }
    writer
        .flush()
        .map_err(|error| format!("Flush run unique-games: {error}"))?;
    records.clear();
    Ok(path)
}

fn create_runs(raw: &Path, directory: &Path, prefix: &str) -> Result<Vec<PathBuf>, String> {
    let mut reader = BufReader::new(
        File::open(raw).map_err(|error| format!("Apertura spool unique-games: {error}"))?,
    );
    let mut records = Vec::with_capacity(RUN_RECORDS);
    let mut runs = Vec::new();
    while let Some(record) = read_record(&mut reader)? {
        records.push(record);
        if records.len() >= RUN_RECORDS {
            let index = runs.len();
            runs.push(flush_run(&mut records, directory, prefix, index)?);
        }
    }
    if !records.is_empty() {
        let index = runs.len();
        runs.push(flush_run(&mut records, directory, prefix, index)?);
    }
    compact_runs(runs, directory, prefix)
}

fn merge_records<F>(paths: &[PathBuf], mut on_record: F) -> Result<(), String>
where
    F: FnMut(SortRecord) -> Result<(), String>,
{
    let mut readers = paths
        .iter()
        .map(|path| {
            File::open(path)
                .map(BufReader::new)
                .map_err(|error| format!("Apertura run unique-games: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut heap = BinaryHeap::<Reverse<(SortRecord, usize)>>::new();
    for (index, reader) in readers.iter_mut().enumerate() {
        if let Some(record) = read_record(reader)? {
            heap.push(Reverse((record, index)));
        }
    }
    while let Some(Reverse((record, index))) = heap.pop() {
        on_record(record)?;
        if let Some(next) = read_record(&mut readers[index])? {
            heap.push(Reverse((next, index)));
        }
    }
    Ok(())
}

fn compact_runs(
    mut runs: Vec<PathBuf>,
    directory: &Path,
    prefix: &str,
) -> Result<Vec<PathBuf>, String> {
    let mut round = 0_usize;
    while runs.len() > MERGE_FANIN {
        let mut next = Vec::new();
        for (batch_index, batch) in runs.chunks(MERGE_FANIN).enumerate() {
            let path = directory.join(format!("{prefix}-merge-{round:03}-{batch_index:06}.bin"));
            let mut writer = BufWriter::new(
                File::create(&path)
                    .map_err(|error| format!("Creazione merge unique-games: {error}"))?,
            );
            merge_records(batch, |record| write_record(&mut writer, &record))?;
            writer
                .flush()
                .map_err(|error| format!("Flush merge unique-games: {error}"))?;
            next.push(path);
        }
        for path in &runs {
            fs::remove_file(path).map_err(|error| format!("Pulizia run unique-games: {error}"))?;
        }
        runs = next;
        round = round.checked_add(1).ok_or("Round merge oltre usize")?;
    }
    Ok(runs)
}

struct CurrentGroup {
    canonical: [u8; 32],
    first_occurrence: [u8; 32],
    last_occurrence: [u8; 32],
    representative_occurrence: [u8; 32],
    representative_quality: u8,
    count: u64,
    object_open: bool,
}

fn open_group_object(
    writer: &mut dyn Write,
    group: &CurrentGroup,
    second_occurrence: &[u8; 32],
    first_output: &mut bool,
) -> Result<(), String> {
    if !*first_output {
        writer
            .write_all(b",")
            .map_err(|error| format!("Scrittura gruppi unique-games: {error}"))?;
    }
    *first_output = false;
    let canonical = encode_hex(&group.canonical);
    let first = format!("g1-{}", encode_hex(&group.first_occurrence));
    let second = format!("g1-{}", encode_hex(second_occurrence));
    let logical = comparable_logical_id(&canonical);
    write!(
        writer,
        "{{\"logical_game_id\":\"{logical}\",\"canonical_mainline_id\":\"{canonical}\",\"occurrence_ids\":[\"{first}\",\"{second}\""
    )
    .map_err(|error| format!("Scrittura gruppi unique-games: {error}"))
}

fn finish_group(
    writer: &mut dyn Write,
    group: &mut Option<CurrentGroup>,
    stats: &mut GroupStats,
) -> Result<(), String> {
    let Some(group) = group.take() else {
        return Ok(());
    };
    stats.comparable_logical_games = stats
        .comparable_logical_games
        .checked_add(1)
        .ok_or("Numero logical games oltre u64")?;
    stats.max_occurrences_per_logical_game =
        stats.max_occurrences_per_logical_game.max(group.count);
    if group.count > 1 {
        if !group.object_open {
            return Err("Gruppo duplicato unique-games senza oggetto aperto".into());
        }
        let representative = format!("g1-{}", encode_hex(&group.representative_occurrence));
        write!(
            writer,
            "],\"representative_occurrence_id\":\"{representative}\",\"occurrence_count\":{}}}",
            group.count
        )
        .map_err(|error| format!("Scrittura gruppi unique-games: {error}"))?;
        stats.duplicate_logical_games = stats
            .duplicate_logical_games
            .checked_add(1)
            .ok_or("Numero gruppi unique-games oltre u64")?;
        stats.duplicate_group_occurrences = stats
            .duplicate_group_occurrences
            .checked_add(group.count)
            .ok_or("Numero occurrence duplicate oltre u64")?;
        stats.collapsed_occurrences = stats
            .collapsed_occurrences
            .checked_add(group.count - 1)
            .ok_or("Numero occurrence collassate oltre u64")?;
    }
    Ok(())
}

fn write_groups_document(
    writer: &mut dyn Write,
    runs: &[PathBuf],
    collection_path: &str,
    collection_blake3: &str,
    collection_bytes: u64,
    stats: &mut GroupStats,
) -> Result<(), String> {
    let collection_path_json =
        serde_json::to_string(collection_path).map_err(|error| error.to_string())?;
    write!(
        writer,
        "{{\"schema_version\":1,\"profile\":\"{GROUPS_PROFILE}\",\"logical_game_id_profile\":\"{LOGICAL_ID_PROFILE}\",\"representative_policy\":\"{REPRESENTATIVE_POLICY}\",\"collection\":{{\"path\":{collection_path_json},\"blake3\":\"{collection_blake3}\",\"bytes\":{collection_bytes}}},\"groups\":["
    )
    .map_err(|error| format!("Scrittura gruppi unique-games: {error}"))?;
    let mut current: Option<CurrentGroup> = None;
    let mut first_output = true;
    merge_records(runs, |record| {
        stats.seen_comparable_occurrences = stats
            .seen_comparable_occurrences
            .checked_add(1)
            .ok_or("Numero occurrence confrontabili oltre u64")?;
        let canonical: [u8; 32] = record[..32].try_into().expect("32 byte canonical");
        let occurrence: [u8; 32] = record[32..64].try_into().expect("32 byte occurrence");
        let representative_quality = record[64];
        if current
            .as_ref()
            .is_some_and(|group| group.canonical != canonical)
        {
            finish_group(writer, &mut current, stats)?;
        }
        if current.is_none() {
            current = Some(CurrentGroup {
                canonical,
                first_occurrence: occurrence,
                last_occurrence: occurrence,
                representative_occurrence: occurrence,
                representative_quality,
                count: 1,
                object_open: false,
            });
            return Ok(());
        }
        let group = current.as_mut().expect("gruppo appena verificato");
        if occurrence <= group.last_occurrence {
            return Err("Occurrence duplicata o non ordinata nel logical-game group".into());
        }
        group.count = group
            .count
            .checked_add(1)
            .ok_or("Dimensione logical-game group oltre u64")?;
        if representative_quality > group.representative_quality {
            group.representative_occurrence = occurrence;
            group.representative_quality = representative_quality;
        }
        if group.count == 2 {
            open_group_object(writer, group, &occurrence, &mut first_output)?;
            group.object_open = true;
        } else {
            let occurrence = format!("g1-{}", encode_hex(&occurrence));
            write!(writer, ",\"{occurrence}\"")
                .map_err(|error| format!("Scrittura gruppi unique-games: {error}"))?;
        }
        group.last_occurrence = occurrence;
        Ok(())
    })?;
    finish_group(writer, &mut current, stats)?;
    writer
        .write_all(b"]}\n")
        .map_err(|error| format!("Scrittura gruppi unique-games: {error}"))
}

fn serialized_manifest(manifest: &UniqueGameViewManifestV1) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("Serializzazione Unique Game View: {error}"))?;
    bytes.push(10);
    Ok(bytes)
}

fn relative_internal_path(
    canonical_root: &Path,
    path: &Path,
    label: &str,
) -> Result<String, String> {
    let relative = path
        .strip_prefix(canonical_root)
        .map_err(|_| format!("{label} fuori workspace"))?;
    Ok(path_to_manifest_string(relative))
}

pub(crate) fn create(root: &Path, collection: &Path) -> Result<UniqueGameViewSummaryV1, String> {
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let temporary = TempWorkspace::new(root)?;
    let raw_path = temporary.path.join("comparable.bin");
    let file = File::create(&raw_path)
        .map_err(|error| format!("Creazione spool unique-games: {error}"))?;
    let mut raw_writer = BufWriter::new(file);
    let mut callback_rows = 0_u64;
    let mut comparable_occurrences = 0_u64;
    let mut noncomparable_singletons = 0_u64;
    let scan: VerifiedLogicalGameScanV1 =
        for_each_verified_logical_game_row(root, collection, |row| {
            callback_rows = callback_rows
                .checked_add(1)
                .ok_or("Numero occurrence oltre u64")?;
            match sort_record(&row)? {
                Some(record) => {
                    comparable_occurrences = comparable_occurrences
                        .checked_add(1)
                        .ok_or("Numero occurrence confrontabili oltre u64")?;
                    write_record(&mut raw_writer, &record)
                }
                None => {
                    noncomparable_singletons = noncomparable_singletons
                        .checked_add(1)
                        .ok_or("Numero occurrence non confrontabili oltre u64")?;
                    Ok(())
                }
            }
        })?;
    raw_writer
        .flush()
        .map_err(|error| format!("Flush spool unique-games: {error}"))?;
    if scan.rows != callback_rows || scan.rows != comparable_occurrences + noncomparable_singletons
    {
        return Err("Cardinalità scansione unique-games incoerente".into());
    }

    let runs = create_runs(&raw_path, &temporary.path, "canonical")?;
    if comparable_occurrences > 0 && runs.is_empty() {
        return Err("Sort unique-games vuoto con occurrence confrontabili".into());
    }

    let collection_path = path_to_manifest_string(collection);
    let groups_directory = prepare_directory_chain(
        root,
        &canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("unique_game_groups", "directory Unique Game Groups"),
        ],
    )?;
    let mut stats = GroupStats::default();
    let (groups_target, _new_groups, groups_blake3, groups_bytes) = publish_compressed_json_stream(
        &canonical_root,
        &groups_directory,
        "Unique Game Groups",
        |writer| {
            write_groups_document(
                writer,
                &runs,
                &collection_path,
                &scan.collection_blake3,
                scan.collection_bytes,
                &mut stats,
            )
        },
    )?;
    if stats.seen_comparable_occurrences != comparable_occurrences {
        return Err("Sort unique-games ha perso occurrence confrontabili".into());
    }
    let logical_games = stats
        .comparable_logical_games
        .checked_add(noncomparable_singletons)
        .ok_or("Numero logical games oltre u64")?;
    let expected_collapsed = scan
        .rows
        .checked_sub(logical_games)
        .ok_or("Logical games oltre occurrence sorgente")?;
    if expected_collapsed != stats.collapsed_occurrences {
        return Err("Conteggio occurrence collassate incoerente".into());
    }
    let max_occurrences = stats
        .max_occurrences_per_logical_game
        .max(if noncomparable_singletons > 0 { 1 } else { 0 });
    let groups_path =
        relative_internal_path(&canonical_root, &groups_target, "Unique Game Groups")?;
    let manifest = UniqueGameViewManifestV1 {
        schema_version: 1,
        profile: VIEW_PROFILE.into(),
        logical_game_id_profile: LOGICAL_ID_PROFILE.into(),
        representative_policy: REPRESENTATIVE_POLICY.into(),
        noncomparable_policy: "singleton_occurrence_v1".into(),
        record_listing_profile: RECORDS_PROFILE.into(),
        record_ordering_policy: RECORD_ORDERING_POLICY.into(),
        collection: ArtifactRefV1 {
            path: collection_path,
            blake3: scan.collection_blake3,
            logical_bytes: scan.collection_bytes,
        },
        duplicate_groups: ArtifactRefV1 {
            path: groups_path.clone(),
            blake3: groups_blake3.clone(),
            logical_bytes: groups_bytes,
        },
        source_occurrences: scan.rows,
        comparable_occurrences,
        noncomparable_singletons,
        comparable_logical_games: stats.comparable_logical_games,
        logical_games,
        collapsed_occurrences: stats.collapsed_occurrences,
        duplicate_logical_games: stats.duplicate_logical_games,
        duplicate_group_occurrences: stats.duplicate_group_occurrences,
        max_occurrences_per_logical_game: max_occurrences,
    };
    let manifest_bytes = serialized_manifest(&manifest)?;
    let view_blake3 = blake3::hash(&manifest_bytes).to_hex().to_string();
    let view_bytes = u64::try_from(manifest_bytes.len()).map_err(|_| "Manifest oltre u64")?;
    let views_directory = prepare_directory_chain(
        root,
        &canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("unique_game_views", "directory Unique Game View"),
        ],
    )?;
    let (view_target, _new_view) = publish_compressed_json(
        &canonical_root,
        &views_directory,
        &view_blake3,
        &manifest_bytes,
        "Unique Game View",
    )?;
    Ok(UniqueGameViewSummaryV1 {
        view_path: PathBuf::from(relative_internal_path(
            &canonical_root,
            &view_target,
            "Unique Game View",
        )?),
        view_blake3,
        view_bytes,
        groups_path: PathBuf::from(groups_path),
        groups_blake3,
        groups_bytes,
        source_occurrences: manifest.source_occurrences,
        comparable_occurrences: manifest.comparable_occurrences,
        noncomparable_singletons: manifest.noncomparable_singletons,
        comparable_logical_games: manifest.comparable_logical_games,
        logical_games: manifest.logical_games,
        collapsed_occurrences: manifest.collapsed_occurrences,
        duplicate_logical_games: manifest.duplicate_logical_games,
        duplicate_group_occurrences: manifest.duplicate_group_occurrences,
        max_occurrences_per_logical_game: manifest.max_occurrences_per_logical_game,
    })
}

struct ListingGroup {
    canonical: [u8; 32],
    last_occurrence: [u8; 32],
    representative_occurrence: [u8; 32],
    representative_quality: u8,
    count: u64,
}

fn start_listing_group(
    writer: &mut dyn Write,
    canonical: [u8; 32],
    occurrence: [u8; 32],
    representative_quality: u8,
) -> Result<ListingGroup, String> {
    let canonical_id = encode_hex(&canonical);
    let occurrence_id = format!("g1-{}", encode_hex(&occurrence));
    let logical_id = comparable_logical_id(&canonical_id);
    write!(
        writer,
        "{{\"schema\":\"{RECORDS_PROFILE}\",\"record\":\"logical_game\",\"logical_game_id\":\"{logical_id}\",\"canonical_mainline_id\":\"{canonical_id}\",\"occurrence_ids\":[\"{occurrence_id}\""
    )
    .map_err(|error| format!("Scrittura listing unique-games: {error}"))?;
    Ok(ListingGroup {
        canonical,
        last_occurrence: occurrence,
        representative_occurrence: occurrence,
        representative_quality,
        count: 1,
    })
}
fn finish_listing_group(
    writer: &mut dyn Write,
    current: &mut Option<ListingGroup>,
    logical_games: &mut u64,
) -> Result<(), String> {
    let Some(group) = current.take() else {
        return Ok(());
    };
    let representative = format!("g1-{}", encode_hex(&group.representative_occurrence));
    writeln!(
        writer,
        "],\"representative_occurrence_id\":\"{representative}\",\"occurrence_count\":{}}}",
        group.count
    )
    .map_err(|error| format!("Scrittura listing unique-games: {error}"))?;
    *logical_games = logical_games
        .checked_add(1)
        .ok_or("Numero logical games listing oltre u64")?;
    Ok(())
}

fn write_comparable_listing(
    writer: &mut dyn Write,
    runs: &[PathBuf],
) -> Result<(u64, u64), String> {
    let mut current: Option<ListingGroup> = None;
    let mut logical_games = 0_u64;
    let mut occurrences = 0_u64;
    merge_records(runs, |record| {
        occurrences = occurrences
            .checked_add(1)
            .ok_or("Numero occurrence listing oltre u64")?;
        let canonical: [u8; 32] = record[..32].try_into().expect("32 byte canonical");
        let occurrence: [u8; 32] = record[32..64].try_into().expect("32 byte occurrence");
        let representative_quality = record[64];
        if current
            .as_ref()
            .is_some_and(|group| group.canonical != canonical)
        {
            finish_listing_group(writer, &mut current, &mut logical_games)?;
        }
        if current.is_none() {
            current = Some(start_listing_group(
                writer,
                canonical,
                occurrence,
                representative_quality,
            )?);
            return Ok(());
        }
        let group = current.as_mut().expect("gruppo listing appena verificato");
        if occurrence <= group.last_occurrence {
            return Err("Occurrence duplicata o non ordinata nel listing logical-game".into());
        }
        let occurrence_id = format!("g1-{}", encode_hex(&occurrence));
        write!(writer, ",\"{occurrence_id}\"")
            .map_err(|error| format!("Scrittura listing unique-games: {error}"))?;
        group.last_occurrence = occurrence;
        group.count = group
            .count
            .checked_add(1)
            .ok_or("Dimensione logical-game listing oltre u64")?;
        if representative_quality > group.representative_quality {
            group.representative_occurrence = occurrence;
            group.representative_quality = representative_quality;
        }
        Ok(())
    })?;
    finish_listing_group(writer, &mut current, &mut logical_games)?;
    Ok((logical_games, occurrences))
}

fn write_noncomparable_listing(writer: &mut dyn Write, runs: &[PathBuf]) -> Result<u64, String> {
    let mut count = 0_u64;
    let mut last_occurrence: Option<[u8; 32]> = None;
    merge_records(runs, |record| {
        if record[..32] != record[32..64] {
            return Err("Record non-comparable listing incoerente".into());
        }
        let occurrence: [u8; 32] = record[..32].try_into().expect("32 byte occurrence");
        if last_occurrence.is_some_and(|last| occurrence <= last) {
            return Err("Occurrence non-comparable duplicata o non ordinata".into());
        }
        let occurrence_id = format!("g1-{}", encode_hex(&occurrence));
        let logical_id = noncomparable_logical_id(&occurrence_id);
        writeln!(
            writer,
            "{{\"schema\":\"{RECORDS_PROFILE}\",\"record\":\"logical_game\",\"logical_game_id\":\"{logical_id}\",\"canonical_mainline_id\":null,\"representative_occurrence_id\":\"{occurrence_id}\",\"occurrence_ids\":[\"{occurrence_id}\"],\"occurrence_count\":1}}"
        )
        .map_err(|error| format!("Scrittura listing unique-games: {error}"))?;
        last_occurrence = Some(occurrence);
        count = count
            .checked_add(1)
            .ok_or("Numero singleton listing oltre u64")?;
        Ok(())
    })?;
    Ok(count)
}

pub(crate) fn write_records(
    root: &Path,
    collection: &Path,
    writer: &mut dyn Write,
) -> Result<(), String> {
    let temporary = TempWorkspace::new(root)?;
    let comparable_raw = temporary.path.join("listing-comparable.bin");
    let noncomparable_raw = temporary.path.join("listing-noncomparable.bin");
    let mut comparable_writer = BufWriter::new(
        File::create(&comparable_raw)
            .map_err(|error| format!("Creazione spool listing comparabili: {error}"))?,
    );
    let mut noncomparable_writer = BufWriter::new(
        File::create(&noncomparable_raw)
            .map_err(|error| format!("Creazione spool listing non-comparabili: {error}"))?,
    );
    let mut callback_rows = 0_u64;
    let mut comparable_occurrences = 0_u64;
    let mut noncomparable_singletons = 0_u64;
    let scan = for_each_verified_logical_game_row(root, collection, |row| {
        callback_rows = callback_rows
            .checked_add(1)
            .ok_or("Numero occurrence listing oltre u64")?;
        if row.comparable {
            let record =
                sort_record(&row)?.ok_or("Occurrence comparabile esclusa dal sort listing")?;
            write_record(&mut comparable_writer, &record)?;
            comparable_occurrences = comparable_occurrences
                .checked_add(1)
                .ok_or("Numero comparabili listing oltre u64")?;
            return Ok(());
        }
        let occurrence = decode_hex_hash(&row.occurrence_id, "occurrence_id")?;
        let mut record = [0_u8; RECORD_BYTES];
        record[..32].copy_from_slice(&occurrence);
        record[32..64].copy_from_slice(&occurrence);
        write_record(&mut noncomparable_writer, &record)?;
        noncomparable_singletons = noncomparable_singletons
            .checked_add(1)
            .ok_or("Numero non-comparabili listing oltre u64")?;
        Ok(())
    })?;
    comparable_writer
        .flush()
        .map_err(|error| format!("Flush spool listing comparabili: {error}"))?;
    noncomparable_writer
        .flush()
        .map_err(|error| format!("Flush spool listing non-comparabili: {error}"))?;
    if scan.rows != callback_rows || scan.rows != comparable_occurrences + noncomparable_singletons
    {
        return Err("Cardinalità listing unique-games incoerente".into());
    }

    let comparable_runs = create_runs(&comparable_raw, &temporary.path, "listing-canonical")?;
    let noncomparable_runs = create_runs(&noncomparable_raw, &temporary.path, "listing-singleton")?;
    let collection_path = path_to_manifest_string(collection);
    let mut stats = GroupStats::default();
    let mut sink = std::io::sink();
    write_groups_document(
        &mut sink,
        &comparable_runs,
        &collection_path,
        &scan.collection_blake3,
        scan.collection_bytes,
        &mut stats,
    )?;
    if stats.seen_comparable_occurrences != comparable_occurrences {
        return Err("Listing ha perso occurrence confrontabili".into());
    }
    let logical_games = stats
        .comparable_logical_games
        .checked_add(noncomparable_singletons)
        .ok_or("Numero logical games listing oltre u64")?;
    let escaped_collection_path =
        serde_json::to_string(&collection_path).map_err(|error| error.to_string())?;
    writeln!(
        writer,
        "{{\"schema\":\"{RECORDS_PROFILE}\",\"record\":\"header\",\"collection\":{{\"path\":{escaped_collection_path},\"blake3\":\"{}\",\"logical_bytes\":{}}},\"source_occurrences\":{},\"logical_games\":{},\"comparable_logical_games\":{},\"noncomparable_singletons\":{},\"ordering_policy\":\"{RECORD_ORDERING_POLICY}\",\"representative_policy\":\"{REPRESENTATIVE_POLICY}\",\"provenance_policy\":\"public_occurrence_ids_v1\",\"complete\":false}}",
        scan.collection_blake3,
        scan.collection_bytes,
        scan.rows,        logical_games,
        stats.comparable_logical_games,
        noncomparable_singletons,
    )
    .map_err(|error| format!("Scrittura header listing unique-games: {error}"))?;

    let (listed_comparable_games, listed_comparable_occurrences) =
        write_comparable_listing(writer, &comparable_runs)?;
    let listed_noncomparable = write_noncomparable_listing(writer, &noncomparable_runs)?;
    if listed_comparable_games != stats.comparable_logical_games
        || listed_comparable_occurrences != comparable_occurrences
        || listed_noncomparable != noncomparable_singletons
    {
        return Err("Accounting finale listing unique-games incoerente".into());
    }
    let listed_logical_games = listed_comparable_games
        .checked_add(listed_noncomparable)
        .ok_or("Numero logical games listing oltre u64")?;
    if listed_logical_games != logical_games {
        return Err("Numero logical games listing diverso dal conteggio verificato".into());
    }
    writeln!(
        writer,
        "{{\"schema\":\"{RECORDS_PROFILE}\",\"record\":\"end\",\"source_occurrences\":{},\"logical_games\":{},\"complete\":true}}",
        scan.rows,
        logical_games,
    )
    .map_err(|error| format!("Scrittura footer listing unique-games: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_ids_are_domain_separated_and_deterministic() {
        let canonical = "ab".repeat(32);
        let occurrence = "cd".repeat(32);
        let first = comparable_logical_id(&canonical);
        assert_eq!(first, comparable_logical_id(&canonical));
        assert!(first.starts_with("u1-"));
        assert_ne!(first, noncomparable_logical_id(&occurrence));
    }

    #[test]
    fn sort_record_is_fixed_width_and_rejects_incoherence() {
        let comparable = ProductLogicalGameRowV1 {
            occurrence_id: "b".repeat(64),
            canonical_mainline_id: Some("a".repeat(64)),
            comparable: true,
            representative_quality: 17,
        };
        let record = sort_record(&comparable).unwrap().unwrap();
        assert_eq!(encode_hex(&record[..32]), "a".repeat(64));
        assert_eq!(encode_hex(&record[32..64]), "b".repeat(64));
        assert_eq!(record[64], 17);
        let bad = ProductLogicalGameRowV1 {
            comparable: false,
            ..comparable
        };
        assert!(sort_record(&bad).is_err());
    }

    #[test]
    fn temporary_workspace_is_local_and_cleans_up() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "antiskill-unique-games-temp-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("raw/tmp")).unwrap();
        let path;
        {
            let temporary = TempWorkspace::new(&root).unwrap();
            path = temporary.path.clone();
            assert!(path.starts_with(root.join("raw/tmp")));
        }
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
