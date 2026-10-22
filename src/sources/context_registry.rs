use super::{
    event_candidates::{EventCandidateRecord, EVENT_CANDIDATE_SCHEMA_VERSION},
    fsi_calendar,
    torneionline_tpol::{self, TpolContextGameV1},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;
const PROFILE: &str = "antiskill_official_context_registry_v1";
const SOURCE_ADAPTER: &str = "torneionline-tpol";
const MANIFEST_NAME: &str = "registry.json";
const CROSSWALK_SCHEMA_VERSION: u32 = 1;
const CROSSWALK_PROFILE: &str = "antiskill_context_registry_crosswalk_candidate_v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct RegistryEventV1 {
    event_code: String,
    event_name: String,
    event_start_date: String,
    event_end_date: String,
    context_index_blake3: String,
    context_index_bytes: u64,
    contexts: u64,
    joinable_contexts: u64,
    roster_source_blake3: String,
    results_source_blake3: String,
    crosstable_source_blake3: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct RegistryShardV1 {
    fide_id: String,
    relative_path: String,
    blake3: String,
    bytes: u64,
    records: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct RegistryManifestV1 {
    schema_version: u32,
    profile: String,
    source_adapter: String,
    events: Vec<RegistryEventV1>,
    shards: Vec<RegistryShardV1>,
    total_contexts: u64,
    joinable_contexts: u64,
    skipped_missing_fide: u64,
}

#[derive(Clone, Debug)]
struct BuildConfig {
    root: PathBuf,
    indexes: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
struct QueryConfig {
    root: PathBuf,
    fide_id: String,
}

#[derive(Clone, Debug)]
struct CrosswalkConfig {
    root: PathBuf,
    fsi_candidates: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct CrosswalkCandidateV1 {
    schema_version: u32,
    profile: String,
    evidence_class: String,
    methods: Vec<String>,
    cardinality: String,
    fsi_match_count: u64,
    tpol_match_count: u64,
    fsi_source_event_id: String,
    fsi_source_title: String,
    fsi_candidate_match_key: String,
    fsi_snapshot_ref: String,
    fsi_source_url: Option<String>,
    tpol_event_code: String,
    tpol_event_name: String,
    tpol_context_index_blake3: String,
    normalized_title: String,
    starts_on: String,
    ends_on: Option<String>,
}

#[derive(Clone, Debug)]
struct EventInput {
    path: PathBuf,
    event: RegistryEventV1,
}

pub(crate) fn print_build_help() {
    println!(
        "antiskill sources registry torneionline-tpol\n\nUSO:\n    antiskill sources registry torneionline-tpol --root DIR --index FILE --index FILE [...]\n"
    );
}

pub(crate) fn print_query_help() {
    println!(
        "antiskill sources query context-registry\n\nUSO:\n    antiskill sources query context-registry --root DIR --fide-id ID\n"
    );
}

pub(crate) fn print_crosswalk_help() {
    println!(
        "antiskill sources crosswalk context-registry\n\nUSO:\n    antiskill sources crosswalk context-registry --root DIR --fsi-candidates FILE.jsonl\n\nOutput: JSONL candidate-only; never authoritative player/event identity.\n"
    );
}

pub(crate) fn run_crosswalk<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_crosswalk_help();
        return Ok(());
    }
    let config = parse_crosswalk_args(args)?;
    let rows = crosswalk_candidates(&config)?;
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for row in rows {
        serde_json::to_writer(&mut out, &row)
            .map_err(|e| format!("Serializzazione crosswalk context registry: {e}"))?;
        out.write_all(b"\n")
            .map_err(|e| format!("Scrittura crosswalk context registry: {e}"))?;
    }
    out.flush()
        .map_err(|e| format!("Flush crosswalk context registry: {e}"))
}

pub(crate) fn run_build<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_build_help();
        return Ok(());
    }
    let config = parse_build_args(args)?;
    let manifest = build_registry(&config)?;
    println!("PROFILE\t{}", manifest.profile);
    println!("EVENTS\t{}", manifest.events.len());
    println!("FIDE_SHARDS\t{}", manifest.shards.len());
    println!("CONTEXTS\t{}", manifest.total_contexts);
    println!("JOINABLE\t{}", manifest.joinable_contexts);
    println!("SKIPPED_MISSING_FIDE\t{}", manifest.skipped_missing_fide);
    println!("ROOT\t{}", config.root.display());
    Ok(())
}

pub(crate) fn run_query<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_query_help();
        return Ok(());
    }
    let config = parse_query_args(args)?;
    let contexts = resolve_fide_contexts(&config.root, &config.fide_id)?;
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for context in contexts {
        serde_json::to_writer(&mut out, &context)
            .map_err(|e| format!("Serializzazione query context registry: {e}"))?;
        out.write_all(b"\n")
            .map_err(|e| format!("Scrittura query context registry: {e}"))?;
    }
    out.flush()
        .map_err(|e| format!("Flush query context registry: {e}"))
}

pub(crate) fn resolve_fide_contexts(
    root: &Path,
    fide_id: &str,
) -> Result<Vec<TpolContextGameV1>, String> {
    let fide_id = canonical_numeric_id(fide_id, "FIDE")?;
    validate_registry_root(root)?;
    let manifest_path = root.join(MANIFEST_NAME);
    let manifest: RegistryManifestV1 = read_json_regular(&manifest_path, "manifest registry")?;
    validate_manifest(&manifest)?;
    let shard = manifest
        .shards
        .binary_search_by(|row| row.fide_id.as_str().cmp(&fide_id))
        .ok()
        .map(|index| &manifest.shards[index])
        .ok_or("FIDE ID assente dal context registry")?;
    let path = safe_relative_join(root, &shard.relative_path)?;
    let (actual_hash, actual_bytes) = hash_regular_file(&path)?;
    if actual_hash != shard.blake3 || actual_bytes != shard.bytes {
        return Err("Shard context registry alterato".into());
    }
    let events = manifest
        .events
        .iter()
        .map(|event| (event.event_code.as_str(), event))
        .collect::<BTreeMap<_, _>>();
    let file = File::open(&path).map_err(|e| format!("Apertura shard context registry: {e}"))?;
    let mut contexts = Vec::new();
    let mut previous_key: Option<(String, u32, u32, String)> = None;
    let mut context_ids = BTreeSet::new();
    for (line_no, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("Lettura shard context registry: {e}"))?;
        if line.trim().is_empty() {
            return Err(format!(
                "Shard context registry con riga vuota: {}",
                line_no + 1
            ));
        }
        let record: TpolContextGameV1 = serde_json::from_str(&line)
            .map_err(|e| format!("Shard context registry JSONL riga {}: {e}", line_no + 1))?;
        torneionline_tpol::validate_context_record(&record)?;
        if record.white_fide_id.is_none() || record.black_fide_id.is_none() {
            return Err("Shard registry contiene contesto non joinabile".into());
        }
        if record.white_fide_id.as_deref() != Some(fide_id.as_str())
            && record.black_fide_id.as_deref() != Some(fide_id.as_str())
        {
            return Err("Shard registry contiene FIDE ID estraneo".into());
        }
        if !context_ids.insert(record.context_id.clone()) {
            return Err("Shard registry con context_id duplicato".into());
        }
        let event = events
            .get(record.event_code.as_str())
            .ok_or("Shard registry riferito a evento non manifestato")?;
        validate_record_against_event(&record, event)?;
        let key = (
            record.event_code.clone(),
            record.round,
            record.board,
            record.context_id.clone(),
        );
        if previous_key
            .as_ref()
            .is_some_and(|previous| previous >= &key)
        {
            return Err("Shard context registry non ordinato deterministicamente".into());
        }
        previous_key = Some(key);
        contexts.push(record);
    }
    if contexts.len() as u64 != shard.records {
        return Err("Cardinalità shard context registry incoerente".into());
    }
    Ok(contexts)
}

fn parse_build_args(args: Vec<String>) -> Result<BuildConfig, String> {
    let mut root = None;
    let mut indexes = Vec::new();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--root" => {
                let value = it.next().ok_or("Manca --root")?;
                if root.replace(PathBuf::from(value)).is_some() {
                    return Err("--root specificato più volte".into());
                }
            }
            "--index" => indexes.push(PathBuf::from(it.next().ok_or("Manca --index")?)),
            other => return Err(format!("Opzione registry sconosciuta: {other}")),
        }
    }
    if indexes.len() < 2 {
        return Err("Il registry multi-evento richiede almeno due --index".into());
    }
    Ok(BuildConfig {
        root: root.ok_or("Serve --root DIR")?,
        indexes,
    })
}

fn parse_query_args(args: Vec<String>) -> Result<QueryConfig, String> {
    let mut root = None;
    let mut fide_id = None;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--root" => {
                if root
                    .replace(PathBuf::from(it.next().ok_or("Manca --root")?))
                    .is_some()
                {
                    return Err("--root specificato più volte".into());
                }
            }
            "--fide-id" => {
                if fide_id
                    .replace(it.next().ok_or("Manca --fide-id")?)
                    .is_some()
                {
                    return Err("--fide-id specificato più volte".into());
                }
            }
            other => return Err(format!("Opzione query registry sconosciuta: {other}")),
        }
    }
    Ok(QueryConfig {
        root: root.ok_or("Serve --root DIR")?,
        fide_id: fide_id.ok_or("Serve --fide-id ID")?,
    })
}

fn parse_crosswalk_args(args: Vec<String>) -> Result<CrosswalkConfig, String> {
    let mut root = None;
    let mut fsi_candidates = None;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--root" => {
                if root
                    .replace(PathBuf::from(it.next().ok_or("Manca --root")?))
                    .is_some()
                {
                    return Err("--root specificato più volte".into());
                }
            }
            "--fsi-candidates" => {
                if fsi_candidates
                    .replace(PathBuf::from(it.next().ok_or("Manca --fsi-candidates")?))
                    .is_some()
                {
                    return Err("--fsi-candidates specificato più volte".into());
                }
            }
            other => return Err(format!("Opzione crosswalk registry sconosciuta: {other}")),
        }
    }
    Ok(CrosswalkConfig {
        root: root.ok_or("Serve --root DIR")?,
        fsi_candidates: fsi_candidates.ok_or("Serve --fsi-candidates FILE.jsonl")?,
    })
}

fn crosswalk_candidates(config: &CrosswalkConfig) -> Result<Vec<CrosswalkCandidateV1>, String> {
    validate_registry_root(&config.root)?;
    let manifest: RegistryManifestV1 =
        read_json_regular(&config.root.join(MANIFEST_NAME), "manifest registry")?;
    validate_manifest(&manifest)?;
    let candidates = load_fsi_candidates(&config.fsi_candidates)?;
    let mut raw = Vec::<(EventCandidateRecord, RegistryEventV1, Vec<String>)>::new();
    for candidate in candidates {
        for event in &manifest.events {
            let mut methods = Vec::new();
            if candidate
                .source_url
                .as_deref()
                .is_some_and(|url| contains_ascii_token_case_insensitive(url, &event.event_code))
            {
                methods.push("source_url_event_code_v1".to_owned());
            }
            let tpol_title = fsi_calendar::normalize_match_text(&event.event_name);
            let start = tpol_date_to_calendar(&event.event_start_date)?;
            let end = tpol_date_to_calendar(&event.event_end_date)?;
            if candidate.normalized_title == tpol_title && candidate.starts_on == start {
                match candidate.ends_on.as_deref() {
                    Some(candidate_end) if candidate_end == end => {
                        methods.push("exact_normalized_title_date_range_v1".to_owned());
                    }
                    None => {
                        methods.push("exact_normalized_title_start_v1".to_owned());
                    }
                    Some(_) => {}
                }
            }
            methods.sort();
            methods.dedup();
            if !methods.is_empty() {
                raw.push((candidate.clone(), event.clone(), methods));
            }
        }
    }
    let mut fsi_counts = BTreeMap::<String, u64>::new();
    let mut tpol_counts = BTreeMap::<String, u64>::new();
    for (candidate, event, _) in &raw {
        *fsi_counts
            .entry(candidate.source_event_id.clone())
            .or_default() += 1;
        *tpol_counts.entry(event.event_code.clone()).or_default() += 1;
    }
    let mut rows = raw
        .into_iter()
        .map(|(candidate, event, methods)| {
            let fsi_match_count = fsi_counts[&candidate.source_event_id];
            let tpol_match_count = tpol_counts[&event.event_code];
            let normalized_title = fsi_calendar::normalize_match_text(&event.event_name);
            let cardinality = match (fsi_match_count > 1, tpol_match_count > 1) {
                (false, false) => "one_to_one",
                (false, true) => "many_to_one",
                (true, false) => "one_to_many",
                (true, true) => "many_to_many",
            };
            CrosswalkCandidateV1 {
                schema_version: CROSSWALK_SCHEMA_VERSION,
                profile: CROSSWALK_PROFILE.to_owned(),
                evidence_class: "candidate_only".to_owned(),
                methods,
                cardinality: cardinality.to_owned(),
                fsi_match_count,
                tpol_match_count,
                fsi_source_event_id: candidate.source_event_id,
                fsi_source_title: candidate.source_title,
                fsi_candidate_match_key: candidate.candidate_match_key,
                fsi_snapshot_ref: candidate.snapshot_ref,
                fsi_source_url: candidate.source_url,
                tpol_event_code: event.event_code,
                tpol_event_name: event.event_name,
                tpol_context_index_blake3: event.context_index_blake3,
                normalized_title,
                starts_on: tpol_date_to_calendar(&event.event_start_date)
                    .expect("validated registry date"),
                ends_on: Some(
                    tpol_date_to_calendar(&event.event_end_date).expect("validated registry date"),
                ),
            }
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.fsi_source_event_id
            .cmp(&right.fsi_source_event_id)
            .then_with(|| left.tpol_event_code.cmp(&right.tpol_event_code))
    });
    Ok(rows)
}

fn load_fsi_candidates(path: &Path) -> Result<Vec<EventCandidateRecord>, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("Candidati FSI {}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("Candidati FSI non regolari: {}", path.display()));
    }
    let file = File::open(path).map_err(|e| format!("Apertura candidati FSI: {e}"))?;
    let mut rows = Vec::new();
    let mut ids = BTreeSet::new();
    for (line_no, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("Lettura candidati FSI: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let row: EventCandidateRecord = serde_json::from_str(&line)
            .map_err(|e| format!("Candidati FSI JSONL riga {}: {e}", line_no + 1))?;
        if row.schema_version != EVENT_CANDIDATE_SCHEMA_VERSION
            || row.key_kind != "candidate-only"
            || row.source_key != "fsi-calendar"
            || row.source_event_id.trim().is_empty()
            || row.normalized_title.trim().is_empty()
            || super::event_candidates::parse_calendar_date(&row.starts_on).is_none()
            || !valid_candidate_key(&row.candidate_match_key)
            || row.snapshot_ref.trim().is_empty()
        {
            return Err(format!(
                "Candidato FSI non valido alla riga {}",
                line_no + 1
            ));
        }
        if row
            .ends_on
            .as_deref()
            .is_some_and(|date| super::event_candidates::parse_calendar_date(date).is_none())
        {
            return Err(format!(
                "Data finale candidato FSI non valida alla riga {}",
                line_no + 1
            ));
        }
        if !ids.insert(row.source_event_id.clone()) {
            return Err("Candidati FSI con source_event_id duplicato".into());
        }
        rows.push(row);
    }
    rows.sort_by(|left, right| left.source_event_id.cmp(&right.source_event_id));
    Ok(rows)
}

fn valid_candidate_key(value: &str) -> bool {
    value
        .strip_prefix("candidate-v1:blake3:")
        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn contains_ascii_token_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() || !needle.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return false;
    }
    let haystack = haystack.to_ascii_lowercase();
    let needle = needle.to_ascii_lowercase();
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.len() > haystack.len() {
        return false;
    }
    (0..=haystack.len() - needle.len()).any(|start| {
        let end = start + needle.len();
        &haystack[start..end] == needle
            && (start == 0 || !haystack[start - 1].is_ascii_alphanumeric())
            && (end == haystack.len() || !haystack[end].is_ascii_alphanumeric())
    })
}

fn tpol_date_to_calendar(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'.'
        || bytes[7] != b'.'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| !matches!(index, 4 | 7) && !byte.is_ascii_digit())
    {
        return Err(format!("Data T'Pol registry non valida: {value}"));
    }
    let converted = format!("{}-{}-{}", &value[8..10], &value[5..7], &value[0..4]);
    super::event_candidates::parse_calendar_date(&converted)
        .ok_or_else(|| format!("Data T'Pol registry non valida: {value}"))?;
    Ok(converted)
}

fn build_registry(config: &BuildConfig) -> Result<RegistryManifestV1, String> {
    if fs::symlink_metadata(&config.root).is_ok() {
        return Err(format!(
            "Root registry già esistente: {}",
            config.root.display()
        ));
    }
    let mut inputs = Vec::new();
    let mut event_codes = BTreeSet::new();
    for path in &config.indexes {
        let records = torneionline_tpol::load_verified_context_index(path)?;
        let first = records.first().ok_or("Indice T'Pol vuoto")?;
        if !event_codes.insert(first.event_code.clone()) {
            return Err(format!(
                "Event code duplicato nel registry: {}",
                first.event_code
            ));
        }
        let (index_hash, index_bytes) = hash_regular_file(path)?;
        let joinable = records
            .iter()
            .filter(|row| row.white_fide_id.is_some() && row.black_fide_id.is_some())
            .count() as u64;
        inputs.push(EventInput {
            path: path.clone(),
            event: RegistryEventV1 {
                event_code: first.event_code.clone(),
                event_name: first.event_name.clone(),
                event_start_date: first.event_start_date.clone(),
                event_end_date: first.event_end_date.clone(),
                context_index_blake3: index_hash,
                context_index_bytes: index_bytes,
                contexts: records.len() as u64,
                joinable_contexts: joinable,
                roster_source_blake3: first.roster_source_blake3.clone(),
                results_source_blake3: first.results_source_blake3.clone(),
                crosstable_source_blake3: first.crosstable_source_blake3.clone(),
            },
        });
    }
    inputs.sort_by(|left, right| left.event.event_code.cmp(&right.event.event_code));
    let result = (|| {
        fs::create_dir(&config.root)
            .map_err(|e| format!("Creazione root context registry: {e}"))?;
        let fide_dir = config.root.join("fide");
        fs::create_dir(&fide_dir).map_err(|e| format!("Creazione shard registry: {e}"))?;
        let mut counts = BTreeMap::<String, u64>::new();
        let mut total_contexts = 0_u64;
        let mut joinable_contexts = 0_u64;
        for input in &inputs {
            let expected = (
                input.event.context_index_blake3.as_str(),
                input.event.context_index_bytes,
            );
            let before = hash_regular_file(&input.path)?;
            if before.0 != expected.0 || before.1 != expected.1 {
                return Err(format!(
                    "Indice T'Pol cambiato dopo il preflight: {}",
                    input.path.display()
                ));
            }
            let records = torneionline_tpol::load_verified_context_index(&input.path)?;
            let after = hash_regular_file(&input.path)?;
            if after.0 != expected.0 || after.1 != expected.1 {
                return Err(format!(
                    "Indice T'Pol cambiato durante la costruzione registry: {}",
                    input.path.display()
                ));
            }
            for record in records {
                total_contexts = total_contexts
                    .checked_add(1)
                    .ok_or("Troppi contesti registry")?;
                let (Some(white), Some(black)) = (
                    record.white_fide_id.as_deref(),
                    record.black_fide_id.as_deref(),
                ) else {
                    continue;
                };
                joinable_contexts = joinable_contexts
                    .checked_add(1)
                    .ok_or("Troppi contesti joinable registry")?;
                let line = serde_json::to_vec(&record)
                    .map_err(|e| format!("Serializzazione shard registry: {e}"))?;
                for fide_id in [white, black] {
                    append_shard(&fide_dir, fide_id, &line)?;
                    *counts.entry(fide_id.to_owned()).or_default() += 1;
                }
            }
        }
        let mut shards = Vec::new();
        for (fide_id, records) in counts {
            let relative_path = format!("fide/{fide_id}.jsonl");
            let path = config.root.join(&relative_path);
            let (hash, bytes) = hash_regular_file(&path)?;
            shards.push(RegistryShardV1 {
                fide_id,
                relative_path,
                blake3: hash,
                bytes,
                records,
            });
        }
        let manifest = RegistryManifestV1 {
            schema_version: SCHEMA_VERSION,
            profile: PROFILE.into(),
            source_adapter: SOURCE_ADAPTER.into(),
            events: inputs.iter().map(|input| input.event.clone()).collect(),
            shards,
            total_contexts,
            joinable_contexts,
            skipped_missing_fide: total_contexts - joinable_contexts,
        };
        validate_manifest(&manifest)?;
        write_manifest_new(&config.root.join(MANIFEST_NAME), &manifest)?;
        Ok(manifest)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&config.root);
    }
    result
}

fn append_shard(directory: &Path, fide_id: &str, line: &[u8]) -> Result<(), String> {
    let fide_id = canonical_numeric_id(fide_id, "FIDE")?;
    let path = directory.join(format!("{fide_id}.jsonl"));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Apertura shard {}: {e}", path.display()))?;
    file.write_all(line)
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| format!("Scrittura shard {}: {e}", path.display()))
}

fn validate_manifest(manifest: &RegistryManifestV1) -> Result<(), String> {
    if manifest.schema_version != SCHEMA_VERSION
        || manifest.profile != PROFILE
        || manifest.source_adapter != SOURCE_ADAPTER
        || manifest.events.len() < 2
    {
        return Err("Manifest context registry con profilo/schema non validi".into());
    }
    let mut previous_event = None::<&str>;
    let mut total = 0_u64;
    let mut joinable = 0_u64;
    for event in &manifest.events {
        if event.event_code.trim().is_empty()
            || event.event_name.trim().is_empty()
            || event.contexts == 0
            || event.joinable_contexts > event.contexts
            || !valid_blake3(&event.context_index_blake3)
            || !valid_blake3(&event.roster_source_blake3)
            || !valid_blake3(&event.results_source_blake3)
            || !valid_blake3(&event.crosstable_source_blake3)
        {
            return Err("Manifest context registry con evento non valido".into());
        }
        if previous_event.is_some_and(|previous| previous >= event.event_code.as_str()) {
            return Err("Eventi registry non ordinati/unici".into());
        }
        previous_event = Some(&event.event_code);
        total = total
            .checked_add(event.contexts)
            .ok_or("Totale registry oltre u64")?;
        joinable = joinable
            .checked_add(event.joinable_contexts)
            .ok_or("Totale joinable registry oltre u64")?;
    }
    if total != manifest.total_contexts
        || joinable != manifest.joinable_contexts
        || total - joinable != manifest.skipped_missing_fide
    {
        return Err("Totali manifest context registry incoerenti".into());
    }
    let mut previous_fide = None::<&str>;
    let mut shard_records = 0_u64;
    for shard in &manifest.shards {
        canonical_numeric_id(&shard.fide_id, "FIDE")?;
        if shard.records == 0 || shard.bytes == 0 || !valid_blake3(&shard.blake3) {
            return Err("Manifest context registry con shard non valido".into());
        }
        if previous_fide.is_some_and(|previous| previous >= shard.fide_id.as_str()) {
            return Err("Shard registry non ordinati/unici".into());
        }
        previous_fide = Some(&shard.fide_id);
        let expected = format!("fide/{}.jsonl", shard.fide_id);
        if shard.relative_path != expected {
            return Err("Percorso shard registry non canonico".into());
        }
        shard_records = shard_records
            .checked_add(shard.records)
            .ok_or("Totale record shard registry oltre u64")?;
    }
    let expected_shard_records = manifest
        .joinable_contexts
        .checked_mul(2)
        .ok_or("Totale atteso record shard registry oltre u64")?;
    if shard_records != expected_shard_records {
        return Err("Cardinalità globale shard context registry incoerente".into());
    }
    Ok(())
}

fn validate_record_against_event(
    record: &TpolContextGameV1,
    event: &RegistryEventV1,
) -> Result<(), String> {
    if record.event_name != event.event_name
        || record.event_start_date != event.event_start_date
        || record.event_end_date != event.event_end_date
        || record.roster_source_blake3 != event.roster_source_blake3
        || record.results_source_blake3 != event.results_source_blake3
        || record.crosstable_source_blake3 != event.crosstable_source_blake3
    {
        return Err("Contesto shard incoerente col manifest evento".into());
    }
    Ok(())
}

fn validate_registry_root(root: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|e| format!("Root context registry {}: {e}", root.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Root context registry non regolare".into());
    }
    Ok(())
}

fn write_manifest_new(path: &Path, manifest: &RegistryManifestV1) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|e| format!("Serializzazione manifest context registry: {e}"))?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("Creazione manifest context registry: {e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("Scrittura manifest context registry: {e}"))
}

fn read_json_regular<T>(path: &Path, label: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("{label} {}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{label} non regolare: {}", path.display()));
    }
    let bytes = fs::read(path).map_err(|e| format!("Lettura {label}: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Parsing {label}: {e}"))
}

fn safe_relative_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("Percorso shard registry non sicuro".into());
    }
    Ok(root.join(path))
}

fn hash_regular_file(path: &Path) -> Result<(String, u64), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("File registry {}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("File registry non regolare: {}", path.display()));
    }
    let mut file = File::open(path).map_err(|e| format!("Apertura file registry: {e}"))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|e| format!("Lettura file registry: {e}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        bytes = bytes
            .checked_add(count as u64)
            .ok_or("Dimensione registry oltre u64")?;
    }
    Ok((hasher.finalize().to_hex().to_string(), bytes))
}

fn canonical_numeric_id(raw: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{label} ID non valido: {raw}"));
    }
    let canonical = value.trim_start_matches('0');
    if canonical.is_empty() {
        return Err(format!("{label} ID non valido: {raw}"));
    }
    Ok(canonical.to_owned())
}

fn valid_blake3(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "antiskill-context-registry-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn tc1(event: &str, round: u32, white_slot: u32, black_slot: u32, result: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"antiskill_tpol_official_context_v1");
        hasher.update(b"\0");
        hasher.update(event.as_bytes());
        hasher.update(b"\0");
        hasher.update(round.to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(white_slot.to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(black_slot.to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(result.as_bytes());
        format!("tc1-{}", hasher.finalize().to_hex())
    }

    struct RowSpec<'a> {
        event: &'a str,
        event_name: &'a str,
        round: u32,
        board: u32,
        white_slot: u32,
        black_slot: u32,
        white_fide: Option<&'a str>,
        black_fide: Option<&'a str>,
        hash_char: char,
    }

    fn row(spec: RowSpec<'_>) -> TpolContextGameV1 {
        let hash = spec.hash_char.to_string().repeat(64);
        TpolContextGameV1 {
            profile: "antiskill_tpol_official_context_v1".into(),
            context_id: tc1(
                spec.event,
                spec.round,
                spec.white_slot,
                spec.black_slot,
                "1-0",
            ),
            event_code: spec.event.into(),
            event_name: spec.event_name.into(),
            event_start_date: "2025.01.01".into(),
            event_end_date: "2025.01.02".into(),
            round: spec.round,
            board: spec.board,
            white_slot: spec.white_slot,
            black_slot: spec.black_slot,
            white_name: format!("White {}", spec.white_slot),
            black_name: format!("Black {}", spec.black_slot),
            white_fide_id: spec.white_fide.map(str::to_owned),
            black_fide_id: spec.black_fide.map(str::to_owned),
            white_fsi_id: None,
            black_fsi_id: None,
            result: "1-0".into(),
            roster_source_blake3: hash.clone(),
            results_source_blake3: hash.clone(),
            crosstable_source_blake3: hash,
            evidence_class: "official_context_join".into(),
        }
    }

    fn write_index(path: &Path, rows: &[TpolContextGameV1]) {
        let mut bytes = Vec::new();
        for row in rows {
            serde_json::to_writer(&mut bytes, row).unwrap();
            bytes.push(b'\n');
        }
        fs::write(path, bytes).unwrap();
    }

    fn fixtures(base: &Path) -> (PathBuf, PathBuf) {
        let a = base.join("event-a.jsonl");
        let b = base.join("event-b.jsonl");
        write_index(
            &a,
            &[row(RowSpec {
                event: "A",
                event_name: "Event Alpha",
                round: 1,
                board: 1,
                white_slot: 1,
                black_slot: 2,
                white_fide: Some("111"),
                black_fide: Some("222"),
                hash_char: 'a',
            })],
        );
        write_index(
            &b,
            &[
                row(RowSpec {
                    event: "B",
                    event_name: "Event Beta",
                    round: 1,
                    board: 1,
                    white_slot: 3,
                    black_slot: 4,
                    white_fide: Some("111"),
                    black_fide: Some("333"),
                    hash_char: 'b',
                }),
                row(RowSpec {
                    event: "B",
                    event_name: "Event Beta",
                    round: 1,
                    board: 2,
                    white_slot: 5,
                    black_slot: 6,
                    white_fide: Some("111"),
                    black_fide: None,
                    hash_char: 'b',
                }),
            ],
        );
        (a, b)
    }

    #[test]
    fn registry_is_deterministic_sharded_and_skips_unjoinable_contexts() {
        let temp = TestDir::new();
        let (a, b) = fixtures(&temp.0);
        let root_one = temp.0.join("registry-one");
        let root_two = temp.0.join("registry-two");
        let one = build_registry(&BuildConfig {
            root: root_one.clone(),
            indexes: vec![b.clone(), a.clone()],
        })
        .unwrap();
        let two = build_registry(&BuildConfig {
            root: root_two.clone(),
            indexes: vec![a, b],
        })
        .unwrap();
        assert_eq!(one, two);
        assert_eq!(one.total_contexts, 3);
        assert_eq!(one.joinable_contexts, 2);
        assert_eq!(one.skipped_missing_fide, 1);
        assert_eq!(
            fs::read(root_one.join(MANIFEST_NAME)).unwrap(),
            fs::read(root_two.join(MANIFEST_NAME)).unwrap()
        );
        for fide in ["111", "222", "333"] {
            assert_eq!(
                fs::read(root_one.join(format!("fide/{fide}.jsonl"))).unwrap(),
                fs::read(root_two.join(format!("fide/{fide}.jsonl"))).unwrap()
            );
        }
        let contexts = resolve_fide_contexts(&root_one, "000111").unwrap();
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0].event_code, "A");
        assert_eq!(contexts[1].event_code, "B");
        assert!(resolve_fide_contexts(&root_one, "999").is_err());

        let manifest_path = root_two.join(MANIFEST_NAME);
        let mut manifest: RegistryManifestV1 =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.shards[0].records -= 1;
        let mut bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        bytes.push(b'\n');
        fs::write(&manifest_path, bytes).unwrap();
        assert!(resolve_fide_contexts(&root_two, "111").is_err());
    }

    fn fsi_candidate(
        id: &str,
        title: &str,
        start: &str,
        end: Option<&str>,
        source_url: Option<&str>,
    ) -> EventCandidateRecord {
        let normalized_title = fsi_calendar::normalize_match_text(title);
        let canonical = format!("title={normalized_title} | start={start}");
        EventCandidateRecord {
            schema_version: EVENT_CANDIDATE_SCHEMA_VERSION,
            key_kind: "candidate-only".into(),
            source_key: "fsi-calendar".into(),
            source_event_id: id.into(),
            source_title: title.into(),
            normalized_title,
            starts_on: start.into(),
            ends_on: end.map(str::to_owned),
            location: None,
            province: None,
            region: None,
            source_url: source_url.map(str::to_owned),
            event_categories: vec!["unclassified".into()],
            classification_evidence: BTreeMap::from([("unclassified".into(), Vec::new())]),
            canonical_match_string: canonical.clone(),
            candidate_match_key: format!(
                "candidate-v1:blake3:{}",
                blake3::hash(canonical.as_bytes()).to_hex()
            ),
            snapshot_ref: "synthetic-fsi-snapshot".into(),
        }
    }

    fn write_fsi_candidates(path: &Path, rows: &[EventCandidateRecord]) {
        let mut bytes = Vec::new();
        for row in rows {
            serde_json::to_writer(&mut bytes, row).unwrap();
            bytes.push(b'\n');
        }
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn crosswalk_is_candidate_only_exact_and_non_fuzzy() {
        let temp = TestDir::new();
        let (a, b) = fixtures(&temp.0);
        let root = temp.0.join("registry");
        build_registry(&BuildConfig {
            root: root.clone(),
            indexes: vec![a, b],
        })
        .unwrap();
        let candidates = temp.0.join("fsi-candidates.jsonl");
        write_fsi_candidates(
            &candidates,
            &[
                fsi_candidate("10", "Event Alpha", "01-01-2025", Some("02-01-2025"), None),
                fsi_candidate(
                    "20",
                    "Completely Different",
                    "03-01-2025",
                    None,
                    Some("https://example.invalid/tpol/event-code/B"),
                ),
                fsi_candidate(
                    "30",
                    "Event Alpha Open",
                    "01-01-2025",
                    Some("02-01-2025"),
                    None,
                ),
            ],
        );
        let rows = crosswalk_candidates(&CrosswalkConfig {
            root,
            fsi_candidates: candidates,
        })
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].fsi_source_event_id, "10");
        assert_eq!(rows[0].tpol_event_code, "A");
        assert_eq!(rows[0].evidence_class, "candidate_only");
        assert_eq!(rows[0].cardinality, "one_to_one");
        assert_eq!(rows[0].methods, ["exact_normalized_title_date_range_v1"]);
        assert_eq!(rows[1].fsi_source_event_id, "20");
        assert_eq!(rows[1].tpol_event_code, "B");
        assert_eq!(rows[1].methods, ["source_url_event_code_v1"]);
        assert!(rows.iter().all(|row| row.fsi_source_event_id != "30"));
    }

    #[test]
    fn duplicate_event_and_tampered_shard_fail_closed() {
        let temp = TestDir::new();
        let (a, b) = fixtures(&temp.0);
        let duplicate = temp.0.join("event-a-copy.jsonl");
        fs::copy(&a, &duplicate).unwrap();
        let duplicate_root = temp.0.join("duplicate");
        assert!(build_registry(&BuildConfig {
            root: duplicate_root.clone(),
            indexes: vec![a.clone(), duplicate],
        })
        .is_err());
        assert!(!duplicate_root.exists());

        let manifest_root = temp.0.join("manifest-tamper");
        build_registry(&BuildConfig {
            root: manifest_root.clone(),
            indexes: vec![a.clone(), b.clone()],
        })
        .unwrap();
        let manifest_path = manifest_root.join(MANIFEST_NAME);
        let mut manifest: RegistryManifestV1 =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.shards.pop();
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        assert!(resolve_fide_contexts(&manifest_root, "111").is_err());

        let root = temp.0.join("registry");
        build_registry(&BuildConfig {
            root: root.clone(),
            indexes: vec![a, b],
        })
        .unwrap();
        let shard = root.join("fide/111.jsonl");
        fs::OpenOptions::new()
            .append(true)
            .open(&shard)
            .unwrap()
            .write_all(b"tamper\n")
            .unwrap();
        assert!(resolve_fide_contexts(&root, "111").is_err());
    }
}
