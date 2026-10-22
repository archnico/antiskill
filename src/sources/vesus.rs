use crate::object_store::store_object;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE_KEY: &str = "vesus-linked-events";
const BUNDLE_SCHEMA: u32 = 1;
const INDEX_PROFILE: &str = "antiskill_vesus_official_context_v1";
const DEFAULT_API_URL: &str = "https://api.vesus.org/graphql";
const ORIGIN: &str = "https://vesus.org";
const RIGHTS_CLASS: &str = "organizer-public-operational-internal-use";
const TOURNAMENT_DOC: &str = "b21eddc8bbb8f9b350af17928aea8cbd";
const PAIRINGS_DOC: &str = "35896b7edbf567664dd3f5231d5d1e6c";
const SUMMARY_DOC: &str = "b5192f75548a8fcdc324aad610f53549";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PageEvidence {
    kind: String,
    operation_name: String,
    document_id: String,
    variable_short_key: String,
    url: String,
    http_status: u16,
    content_type: Option<String>,
    blake3: String,
    bytes: u64,
    object_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct BundleManifest {
    schema_version: u32,
    source_key: String,
    tournament_key: String,
    event_key: String,
    fetched_at_unix_ms: u128,
    rights_class: String,
    retrieved_by: String,
    pages: Vec<PageEvidence>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct BundleReference {
    schema_version: u32,
    source_key: String,
    tournament_key: String,
    manifest_path: String,
    manifest_blake3: String,
}

#[derive(Clone, Debug)]
struct VerifiedBundle {
    manifest: BundleManifest,
    pages: BTreeMap<String, PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct VesusContextGameV1 {
    pub profile: String,
    pub context_id: String,
    pub event_code: String,
    pub event_name: String,
    pub event_start_date: String,
    pub event_end_date: String,
    pub tournament_key: String,
    pub tournament_name: String,
    pub round: u32,
    pub board: u32,
    pub white_slot: u32,
    pub black_slot: u32,
    pub white_name: String,
    pub black_name: String,
    pub white_fide_id: Option<String>,
    pub black_fide_id: Option<String>,
    pub white_fsi_id: Option<String>,
    pub black_fsi_id: Option<String>,
    pub result: String,
    pub roster_source_blake3: String,
    pub results_source_blake3: String,
    pub crosstable_source_blake3: String,
    pub evidence_class: String,
}

#[derive(Clone, Debug, Serialize)]
struct IndexReport {
    profile: &'static str,
    event_key: String,
    event_name: String,
    tournament_key: String,
    tournament_name: String,
    event_start_date: String,
    event_end_date: String,
    players: usize,
    players_with_fide: usize,
    published_rounds: u32,
    completed_published_rounds: u32,
    published_pairings: usize,
    accepted_contexts: usize,
    excluded_after_completed_cutoff: usize,
    excluded_bye_or_absent: usize,
    excluded_missing_fide: usize,
    excluded_nonstandard_or_inconsistent: usize,
    tournament_source_blake3: String,
    pairings_source_blake3: String,
    federation_summary_source_blake3: String,
    complete: bool,
}

#[derive(Clone, Debug)]
struct SyncConfig {
    root: PathBuf,
    tournament_key: String,
    api_url: String,
}

#[derive(Clone, Debug)]
struct IndexConfig {
    root: PathBuf,
    tournament_key: String,
    output: PathBuf,
    report: PathBuf,
}

#[derive(Clone, Debug)]
struct QueryConfig {
    index: PathBuf,
    fide_id: Option<String>,
    round: Option<u32>,
}

#[derive(Clone, Debug)]
struct EventMeta {
    event_key: String,
    event_name: String,
    tournament_key: String,
    tournament_name: String,
    start_date: String,
    end_date: String,
    published_rounds: u32,
    completed_published_rounds: u32,
}

#[derive(Clone, Debug)]
struct PlayerRow {
    slot: u32,
    name: String,
    fide_id: Option<String>,
    fsi_id: Option<String>,
    matches: Vec<String>,
}

#[derive(Clone, Debug)]
struct PairingRow {
    round: u32,
    board: u32,
    white_slot: i64,
    black_slot: i64,
}

#[derive(Default)]
struct Excluded {
    after_cutoff: usize,
    bye_or_absent: usize,
    missing_fide: usize,
    nonstandard: usize,
}

pub(crate) fn print_sync_help() {
    println!("antiskill sources sync vesus-linked-events\n\nUSO:\n    antiskill sources sync vesus-linked-events --root DIR --tournament-key KEY [--api-url URL]\n");
}

pub(crate) fn print_index_help() {
    println!("antiskill sources index vesus-linked-events\n\nUSO:\n    antiskill sources index vesus-linked-events --root DIR --tournament-key KEY --output FILE --report FILE\n");
}

pub(crate) fn print_query_help() {
    println!("antiskill sources query vesus-linked-events\n\nUSO:\n    antiskill sources query vesus-linked-events --index FILE [--fide-id ID] [--round N]\n");
}

pub(crate) fn run_sync<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_sync_help();
        return Ok(());
    }
    let config = parse_sync_args(args)?;
    let user_agent = format!(
        "antiskill/{} (+https://github.com/archnico/antiskill)",
        env!("CARGO_PKG_VERSION")
    );
    let tournament = fetch_graphql(
        &config.root,
        "tournament",
        &config.api_url,
        "TournamentPage_Query",
        TOURNAMENT_DOC,
        &config.tournament_key,
        &user_agent,
    )?;
    let tournament_bytes = read_regular_bytes(&config.root.join(&tournament.object_path))?;
    let event_key = parse_event_key(&tournament_bytes, &config.tournament_key)?;
    let pairings = fetch_graphql(
        &config.root,
        "pairings",
        &config.api_url,
        "PairingsPage_Query",
        PAIRINGS_DOC,
        &config.tournament_key,
        &user_agent,
    )?;
    let summary = fetch_graphql(
        &config.root,
        "federation-summary",
        &config.api_url,
        "EventFederationSummaryPage_Query",
        SUMMARY_DOC,
        &event_key,
        &user_agent,
    )?;
    let fetched_at_unix_ms = now_millis()?;
    let manifest = BundleManifest {
        schema_version: BUNDLE_SCHEMA,
        source_key: SOURCE_KEY.into(),
        tournament_key: config.tournament_key.clone(),
        event_key,
        fetched_at_unix_ms,
        rights_class: RIGHTS_CLASS.into(),
        retrieved_by: user_agent,
        pages: vec![tournament, pairings, summary],
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| format!("Serializzazione manifest Vesus: {e}"))?;
    let manifest_hash = blake3::hash(&manifest_bytes).to_hex().to_string();
    let relative = PathBuf::from("raw/manifests")
        .join(SOURCE_KEY)
        .join(&config.tournament_key)
        .join(format!(
            "{}-{}-{}.json",
            fetched_at_unix_ms,
            std::process::id(),
            &manifest_hash[..12]
        ));
    let manifest_path = config.root.join(&relative);
    write_new_bytes(&manifest_path, &manifest_bytes)?;
    let reference = BundleReference {
        schema_version: BUNDLE_SCHEMA,
        source_key: SOURCE_KEY.into(),
        tournament_key: config.tournament_key.clone(),
        manifest_path: path_string(&relative),
        manifest_blake3: manifest_hash,
    };
    let reference_path = config
        .root
        .join("raw/refs")
        .join(format!("{SOURCE_KEY}-{}.json", config.tournament_key));
    write_json_replace(&reference_path, &reference)?;
    println!("SOURCE\t{SOURCE_KEY}");
    println!("TOURNAMENT\t{}", config.tournament_key);
    println!("EVENT\t{}", manifest.event_key);
    println!("MANIFEST\t{}", manifest_path.display());
    Ok(())
}

pub(crate) fn run_index<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_index_help();
        return Ok(());
    }
    let config = parse_index_args(args)?;
    let bundle = load_verified_bundle(&config.root, &config.tournament_key)?;
    let tournament_bytes = read_regular_bytes(page(&bundle, "tournament")?)?;
    let pairings_bytes = read_regular_bytes(page(&bundle, "pairings")?)?;
    let summary_bytes = read_regular_bytes(page(&bundle, "federation-summary")?)?;
    let hashes = page_hashes(&bundle)?;
    let meta = parse_metadata(&tournament_bytes, &pairings_bytes, &bundle.manifest)?;
    let (players, pairings) = parse_summary(
        &summary_bytes,
        page_content_type(&bundle, "federation-summary")?,
        &meta,
    )?;
    let (contexts, excluded) = reconcile(
        &meta,
        &players,
        &pairings,
        &hashes["federation-summary"],
        &hashes["pairings"],
        &hashes["tournament"],
    )?;
    write_jsonl_new(&config.output, &contexts)?;
    let report = IndexReport {
        profile: INDEX_PROFILE,
        event_key: meta.event_key.clone(),
        event_name: meta.event_name.clone(),
        tournament_key: meta.tournament_key.clone(),
        tournament_name: meta.tournament_name.clone(),
        event_start_date: meta.start_date.clone(),
        event_end_date: meta.end_date.clone(),
        players: players.len(),
        players_with_fide: players.values().filter(|p| p.fide_id.is_some()).count(),
        published_rounds: meta.published_rounds,
        completed_published_rounds: meta.completed_published_rounds,
        published_pairings: pairings.len(),
        accepted_contexts: contexts.len(),
        excluded_after_completed_cutoff: excluded.after_cutoff,
        excluded_bye_or_absent: excluded.bye_or_absent,
        excluded_missing_fide: excluded.missing_fide,
        excluded_nonstandard_or_inconsistent: excluded.nonstandard,
        tournament_source_blake3: hashes["tournament"].clone(),
        pairings_source_blake3: hashes["pairings"].clone(),
        federation_summary_source_blake3: hashes["federation-summary"].clone(),
        complete: true,
    };
    write_json_replace(&config.report, &report)?;
    println!("PROFILE\t{INDEX_PROFILE}");
    println!("EVENT\t{}", report.event_key);
    println!("TOURNAMENT\t{}", report.tournament_key);
    println!("PLAYERS\t{}", report.players);
    println!("PLAYERS_WITH_FIDE\t{}", report.players_with_fide);
    println!("PUBLISHED_ROUNDS\t{}", report.published_rounds);
    println!(
        "COMPLETED_PUBLISHED_ROUNDS\t{}",
        report.completed_published_rounds
    );
    println!("CONTEXT_GAMES\t{}", report.accepted_contexts);
    println!("OUTPUT\t{}", config.output.display());
    println!("REPORT\t{}", config.report.display());
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
    let mut rows = load_verified_context_index(&config.index)?;
    if let Some(fide) = config.fide_id {
        rows.retain(|r| {
            r.white_fide_id.as_deref() == Some(&fide) || r.black_fide_id.as_deref() == Some(&fide)
        });
    }
    if let Some(round) = config.round {
        rows.retain(|r| r.round == round);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?
    );
    Ok(())
}

pub(crate) fn resolve_fide_contexts(
    index: &Path,
    fide_id: &str,
) -> Result<Vec<VesusContextGameV1>, String> {
    let fide_id = canonical_id(fide_id, "FIDE")?;
    let mut rows = load_verified_context_index(index)?;
    rows.retain(|r| {
        r.white_fide_id.as_deref() == Some(fide_id.as_str())
            || r.black_fide_id.as_deref() == Some(fide_id.as_str())
    });
    Ok(rows)
}

pub(crate) fn load_verified_context_index(index: &Path) -> Result<Vec<VesusContextGameV1>, String> {
    let metadata = fs::symlink_metadata(index)
        .map_err(|e| format!("Indice Vesus {}: {e}", index.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("Indice Vesus non regolare: {}", index.display()));
    }
    let file = File::open(index).map_err(|e| format!("Apertura indice Vesus: {e}"))?;
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    let mut coords = BTreeSet::new();
    let mut fingerprint: Option<(String, String, String, String, String, String)> = None;
    for (line_no, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("Lettura indice Vesus: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let record: VesusContextGameV1 = serde_json::from_str(&line)
            .map_err(|e| format!("Indice Vesus JSONL riga {}: {e}", line_no + 1))?;
        validate_context_record(&record)?;
        if !ids.insert(record.context_id.clone()) {
            return Err("Indice Vesus con context_id duplicato".into());
        }
        if !coords.insert((record.round, record.board)) {
            return Err("Indice Vesus con turno/scacchiera duplicati".into());
        }
        let current = (
            record.event_code.clone(),
            record.tournament_key.clone(),
            record.event_start_date.clone(),
            record.roster_source_blake3.clone(),
            record.results_source_blake3.clone(),
            record.crosstable_source_blake3.clone(),
        );
        if let Some(expected) = &fingerprint {
            if expected != &current {
                return Err("Indice Vesus mescola eventi o snapshot differenti".into());
            }
        } else {
            fingerprint = Some(current);
        }
        records.push(record);
    }
    if records.is_empty() {
        return Err("Indice contestuale Vesus vuoto".into());
    }
    records.sort_by_key(|r| (r.round, r.board));
    Ok(records)
}

fn parse_sync_args(args: Vec<String>) -> Result<SyncConfig, String> {
    let mut root = None;
    let mut tournament_key = None;
    let mut api_url = DEFAULT_API_URL.to_owned();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--root" => root = Some(PathBuf::from(it.next().ok_or("Manca --root")?)),
            "--tournament-key" => {
                tournament_key = Some(canonical_key(&it.next().ok_or("Manca --tournament-key")?)?)
            }
            "--api-url" => api_url = it.next().ok_or("Manca --api-url")?,
            other => return Err(format!("Opzione sync Vesus sconosciuta: {other}")),
        }
    }
    Ok(SyncConfig {
        root: root.ok_or("Serve --root")?,
        tournament_key: tournament_key.ok_or("Serve --tournament-key")?,
        api_url,
    })
}

fn parse_index_args(args: Vec<String>) -> Result<IndexConfig, String> {
    let mut root = None;
    let mut tournament_key = None;
    let mut output = None;
    let mut report = None;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--root" => root = Some(PathBuf::from(it.next().ok_or("Manca --root")?)),
            "--tournament-key" => {
                tournament_key = Some(canonical_key(&it.next().ok_or("Manca --tournament-key")?)?)
            }
            "--output" => output = Some(PathBuf::from(it.next().ok_or("Manca --output")?)),
            "--report" => report = Some(PathBuf::from(it.next().ok_or("Manca --report")?)),
            other => return Err(format!("Opzione index Vesus sconosciuta: {other}")),
        }
    }
    Ok(IndexConfig {
        root: root.ok_or("Serve --root")?,
        tournament_key: tournament_key.ok_or("Serve --tournament-key")?,
        output: output.ok_or("Serve --output")?,
        report: report.ok_or("Serve --report")?,
    })
}

fn parse_query_args(args: Vec<String>) -> Result<QueryConfig, String> {
    let mut index = None;
    let mut fide_id = None;
    let mut round = None;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--index" => index = Some(PathBuf::from(it.next().ok_or("Manca --index")?)),
            "--fide-id" => {
                fide_id = Some(canonical_id(&it.next().ok_or("Manca --fide-id")?, "FIDE")?)
            }
            "--round" => {
                round = Some(parse_positive_u32(
                    &it.next().ok_or("Manca --round")?,
                    "round",
                )?)
            }
            other => return Err(format!("Opzione query Vesus sconosciuta: {other}")),
        }
    }
    Ok(QueryConfig {
        index: index.ok_or("Serve --index")?,
        fide_id,
        round,
    })
}

fn fetch_graphql(
    root: &Path,
    kind: &str,
    url: &str,
    operation_name: &str,
    document_id: &str,
    short_key: &str,
    user_agent: &str,
) -> Result<PageEvidence, String> {
    let body = json!({"operationName": operation_name, "variables": {"shortKey": short_key}, "docId": document_id});
    let body_bytes =
        serde_json::to_vec(&body).map_err(|e| format!("Serializzazione richiesta Vesus: {e}"))?;
    let mut response = ureq::post(url)
        .header("Origin", ORIGIN)
        .header("Referer", "https://vesus.org/")
        .header("User-Agent", user_agent)
        .header("Accept", "application/json,multipart/mixed")
        .header("Accept-Encoding", "identity")
        .header("Content-Type", "application/json")
        .send(&body_bytes)
        .map_err(|e| format!("GraphQL Vesus {kind} fallito: {e}"))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("Vesus {kind}: HTTP {status}"));
    }
    let content_type = header_string(&response, "content-type");
    let stored = store_object(root, response.body_mut().as_reader())?;
    Ok(PageEvidence {
        kind: kind.into(),
        operation_name: operation_name.into(),
        document_id: document_id.into(),
        variable_short_key: short_key.into(),
        url: url.into(),
        http_status: status,
        content_type,
        blake3: stored.blake3,
        bytes: stored.bytes,
        object_path: path_string(&stored.object_relative),
    })
}

fn parse_event_key(bytes: &[u8], tournament_key: &str) -> Result<String, String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| format!("TournamentPage Vesus JSON: {e}"))?;
    let tournament = value
        .pointer("/data/tournament")
        .ok_or("Vesus TournamentPage senza tournament")?;
    if tournament.get("shortKey").and_then(Value::as_str) != Some(tournament_key) {
        return Err("Vesus TournamentPage shortKey inatteso".into());
    }
    canonical_key(
        tournament
            .pointer("/event/shortKey")
            .and_then(Value::as_str)
            .ok_or("Vesus TournamentPage senza event.shortKey")?,
    )
}

fn parse_metadata(
    tournament_bytes: &[u8],
    pairings_bytes: &[u8],
    manifest: &BundleManifest,
) -> Result<EventMeta, String> {
    let t: Value = serde_json::from_slice(tournament_bytes)
        .map_err(|e| format!("TournamentPage JSON: {e}"))?;
    let p: Value =
        serde_json::from_slice(pairings_bytes).map_err(|e| format!("PairingsPage JSON: {e}"))?;
    let tv = t
        .pointer("/data/tournament")
        .ok_or("TournamentPage senza tournament")?;
    let pv = p
        .pointer("/data/tournament")
        .ok_or("PairingsPage senza tournament")?;
    let tournament_key = canonical_key(
        tv.get("shortKey")
            .and_then(Value::as_str)
            .ok_or("TournamentPage senza shortKey")?,
    )?;
    if tournament_key != manifest.tournament_key
        || pv.get("shortKey").and_then(Value::as_str) != Some(tournament_key.as_str())
    {
        return Err("Bundle Vesus mescola tornei".into());
    }
    let event_key = canonical_key(
        tv.pointer("/event/shortKey")
            .and_then(Value::as_str)
            .ok_or("TournamentPage senza event key")?,
    )?;
    if event_key != manifest.event_key
        || pv.pointer("/event/shortKey").and_then(Value::as_str) != Some(event_key.as_str())
    {
        return Err("Bundle Vesus mescola eventi".into());
    }
    let published_rounds = u32_value(tv, "publishedRounds")?;
    let completed_published_rounds = u32_value(tv, "completedPublishedRounds")?;
    if u32_value(pv, "publishedRounds")? != published_rounds
        || u32_value(pv, "completedPublishedRounds")? != completed_published_rounds
    {
        return Err("Snapshot Vesus non coerente sullo stato dei turni".into());
    }
    if completed_published_rounds > published_rounds {
        return Err("Vesus completedPublishedRounds > publishedRounds".into());
    }
    let start = tv
        .get("start")
        .and_then(Value::as_str)
        .ok_or("TournamentPage senza start")?;
    let end = tv
        .get("end")
        .and_then(Value::as_str)
        .ok_or("TournamentPage senza end")?;
    Ok(EventMeta {
        event_key,
        event_name: tv
            .pointer("/event/name")
            .and_then(Value::as_str)
            .ok_or("TournamentPage senza event.name")?
            .trim()
            .to_owned(),
        tournament_key,
        tournament_name: tv
            .get("name")
            .and_then(Value::as_str)
            .ok_or("TournamentPage senza tournament.name")?
            .trim()
            .to_owned(),
        start_date: canonical_iso_date(start)?,
        end_date: canonical_iso_date(end)?,
        published_rounds,
        completed_published_rounds,
    })
}

fn parse_summary(
    bytes: &[u8],
    content_type: &str,
    meta: &EventMeta,
) -> Result<(BTreeMap<u32, PlayerRow>, Vec<PairingRow>), String> {
    let parts = parse_graphql_parts(bytes, content_type)?;
    let base = parts.first().ok_or("Summary Vesus senza parte base")?;
    let event = base
        .pointer("/data/event")
        .ok_or("Summary Vesus senza event")?;
    if event.get("shortKey").and_then(Value::as_str) != Some(meta.event_key.as_str())
        || event.get("name").and_then(Value::as_str) != Some(meta.event_name.as_str())
    {
        return Err("Summary Vesus riferita a evento diverso".into());
    }
    let mut target = None;
    for part in &parts[1..] {
        let Some(incremental) = part.get("incremental").and_then(Value::as_array) else {
            continue;
        };
        for item in incremental {
            let Some(data) = item.get("data") else {
                continue;
            };
            if data.get("shortKey").and_then(Value::as_str) == Some(meta.tournament_key.as_str()) {
                target = Some(data.clone());
            }
        }
    }
    let target = target.ok_or("Summary Vesus senza defer del torneo target")?;
    if u32_value(&target, "publishedRounds")? != meta.published_rounds
        || u32_value(&target, "completedPublishedRounds")? != meta.completed_published_rounds
    {
        return Err("Summary Vesus non coerente sullo stato dei turni".into());
    }
    let raw_players = target
        .get("uniquePlayers")
        .and_then(Value::as_array)
        .ok_or("Summary Vesus senza uniquePlayers")?;
    let mut players = BTreeMap::new();
    let mut fide_seen = BTreeMap::<String, u32>::new();
    for raw in raw_players {
        if raw.get("__typename").and_then(Value::as_str) != Some("player") {
            continue;
        }
        let slot = u32_value(raw, "rankedId")?;
        if slot == 0 {
            return Err("Vesus player con rankedId zero".into());
        }
        let name = raw
            .get("name")
            .and_then(Value::as_str)
            .ok_or("Vesus player senza name")?
            .trim()
            .to_owned();
        let fide_id = optional_id(raw.get("fideId"), "FIDE")?;
        let fsi_id = optional_id(raw.get("nationalId"), "FSI")?;
        if let Some(id) = &fide_id {
            if let Some(other) = fide_seen.insert(id.clone(), slot) {
                if other != slot {
                    return Err("Vesus duplica FIDE ID fra slot differenti".into());
                }
            }
        }
        let matches = raw
            .get("matches")
            .and_then(Value::as_array)
            .ok_or("Vesus player senza matches")?
            .iter()
            .map(|v| {
                v.as_str()
                    .ok_or("Vesus match non stringa")
                    .map(str::to_owned)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if players
            .insert(
                slot,
                PlayerRow {
                    slot,
                    name,
                    fide_id,
                    fsi_id,
                    matches,
                },
            )
            .is_some()
        {
            return Err("Vesus rankedId duplicato".into());
        }
    }
    if players.is_empty() {
        return Err("Summary Vesus senza player classificati".into());
    }
    let raw_pairings = target
        .get("pairings")
        .and_then(Value::as_array)
        .ok_or("Summary Vesus senza pairings")?;
    let mut pairings = Vec::new();
    let mut coords = BTreeSet::new();
    for raw in raw_pairings {
        let round = u32_value(raw, "round")?;
        let board = raw
            .get("board")
            .and_then(Value::as_str)
            .ok_or("Vesus pairing senza board")?
            .parse::<u32>()
            .map_err(|_| "Vesus board non numerica")?;
        if round == 0 || board == 0 || !coords.insert((round, board)) {
            return Err("Vesus pairing con coordinate nulle/duplicate".into());
        }
        pairings.push(PairingRow {
            round,
            board,
            white_slot: i64_value(raw, "whiteId")?,
            black_slot: i64_value(raw, "blackId")?,
        });
    }
    pairings.sort_by_key(|p| (p.round, p.board));
    Ok((players, pairings))
}

fn reconcile(
    meta: &EventMeta,
    players: &BTreeMap<u32, PlayerRow>,
    pairings: &[PairingRow],
    summary_hash: &str,
    pairings_hash: &str,
    tournament_hash: &str,
) -> Result<(Vec<VesusContextGameV1>, Excluded), String> {
    let mut contexts = Vec::new();
    let mut excluded = Excluded::default();
    for pairing in pairings {
        if pairing.round > meta.completed_published_rounds {
            excluded.after_cutoff += 1;
            continue;
        }
        if pairing.white_slot <= 0 || pairing.black_slot <= 0 {
            excluded.bye_or_absent += 1;
            continue;
        }
        let white = players
            .get(&(pairing.white_slot as u32))
            .ok_or("Vesus pairing riferito a whiteId sconosciuto")?;
        let black = players
            .get(&(pairing.black_slot as u32))
            .ok_or("Vesus pairing riferito a blackId sconosciuto")?;
        let (Some(white_fide), Some(black_fide)) = (&white.fide_id, &black.fide_id) else {
            excluded.missing_fide += 1;
            continue;
        };
        if white_fide == black_fide {
            return Err("Vesus assegna lo stesso FIDE ID ai due lati".into());
        }
        let Some(white_token) = white.matches.get((pairing.round - 1) as usize) else {
            excluded.nonstandard += 1;
            continue;
        };
        let Some(black_token) = black.matches.get((pairing.round - 1) as usize) else {
            excluded.nonstandard += 1;
            continue;
        };
        let Some((white_score, white_color, white_opp)) = parse_standard_match(white_token) else {
            excluded.nonstandard += 1;
            continue;
        };
        let Some((black_score, black_color, black_opp)) = parse_standard_match(black_token) else {
            excluded.nonstandard += 1;
            continue;
        };
        if white_color != 'w'
            || black_color != 'b'
            || white_opp != black.slot
            || black_opp != white.slot
        {
            excluded.nonstandard += 1;
            continue;
        }
        let result = reciprocal_result(&white_score, &black_score)
            .ok_or("Vesus match reciproci con punteggi incompatibili")?;
        let context_id = context_id(meta, pairing, &result);
        contexts.push(VesusContextGameV1 {
            profile: INDEX_PROFILE.into(),
            context_id,
            event_code: meta.event_key.clone(),
            event_name: meta.event_name.clone(),
            event_start_date: meta.start_date.clone(),
            event_end_date: meta.end_date.clone(),
            tournament_key: meta.tournament_key.clone(),
            tournament_name: meta.tournament_name.clone(),
            round: pairing.round,
            board: pairing.board,
            white_slot: white.slot,
            black_slot: black.slot,
            white_name: white.name.clone(),
            black_name: black.name.clone(),
            white_fide_id: Some(white_fide.clone()),
            black_fide_id: Some(black_fide.clone()),
            white_fsi_id: white.fsi_id.clone(),
            black_fsi_id: black.fsi_id.clone(),
            result,
            roster_source_blake3: summary_hash.into(),
            results_source_blake3: pairings_hash.into(),
            crosstable_source_blake3: tournament_hash.into(),
            evidence_class: "official_context_join".into(),
        });
    }
    contexts.sort_by_key(|r| (r.round, r.board));
    Ok((contexts, excluded))
}

fn parse_standard_match(raw: &str) -> Option<(String, char, u32)> {
    if raw.contains('F') || raw.contains('-') {
        return None;
    }
    let (score, rest) = if let Some(rest) = raw.strip_prefix('½') {
        ("1/2".to_owned(), rest)
    } else if let Some(rest) = raw.strip_prefix('1') {
        ("1".to_owned(), rest)
    } else {
        let rest = raw.strip_prefix('0')?;
        ("0".to_owned(), rest)
    };
    let mut chars = rest.chars();
    let color = chars.next()?;
    if !matches!(color, 'w' | 'b') {
        return None;
    }
    let opponent = chars.as_str().parse::<u32>().ok()?;
    if opponent == 0 {
        return None;
    }
    Some((score, color, opponent))
}

fn reciprocal_result(white: &str, black: &str) -> Option<String> {
    match (white, black) {
        ("1", "0") => Some("1-0".into()),
        ("0", "1") => Some("0-1".into()),
        ("1/2", "1/2") => Some("1/2-1/2".into()),
        _ => None,
    }
}

fn context_id(meta: &EventMeta, pairing: &PairingRow, result: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    for part in [
        INDEX_PROFILE,
        meta.event_key.as_str(),
        meta.tournament_key.as_str(),
        &pairing.round.to_string(),
        &pairing.board.to_string(),
        &pairing.white_slot.to_string(),
        &pairing.black_slot.to_string(),
        result,
    ] {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("vc1-{}", hasher.finalize().to_hex())
}

pub(crate) fn validate_context_record(record: &VesusContextGameV1) -> Result<(), String> {
    if record.profile != INDEX_PROFILE
        || record.evidence_class != "official_context_join"
        || !record.context_id.starts_with("vc1-")
    {
        return Err("Record context Vesus con profilo/identità non valida".into());
    }
    canonical_key(&record.event_code)?;
    canonical_key(&record.tournament_key)?;
    if record.event_name.trim().is_empty()
        || record.tournament_name.trim().is_empty()
        || record.round == 0
        || record.board == 0
        || record.white_slot == 0
        || record.black_slot == 0
        || record.white_slot == record.black_slot
    {
        return Err("Record context Vesus con metadati/coordinate non validi".into());
    }
    let meta = EventMeta {
        event_key: record.event_code.clone(),
        event_name: record.event_name.clone(),
        tournament_key: record.tournament_key.clone(),
        tournament_name: record.tournament_name.clone(),
        start_date: record.event_start_date.clone(),
        end_date: record.event_end_date.clone(),
        published_rounds: record.round,
        completed_published_rounds: record.round,
    };
    let pairing = PairingRow {
        round: record.round,
        board: record.board,
        white_slot: i64::from(record.white_slot),
        black_slot: i64::from(record.black_slot),
    };
    if record.context_id != context_id(&meta, &pairing, &record.result) {
        return Err("Record context Vesus con context_id non ricalcolabile".into());
    }
    if !is_pgn_date(&record.event_start_date)
        || !is_pgn_date(&record.event_end_date)
        || record.event_start_date > record.event_end_date
    {
        return Err("Record Vesus con intervallo date non valido".into());
    }
    reciprocal_result(
        match record.result.as_str() {
            "1-0" => "1",
            "0-1" => "0",
            "1/2-1/2" => "1/2",
            _ => return Err("Risultato Vesus non supportato".into()),
        },
        match record.result.as_str() {
            "1-0" => "0",
            "0-1" => "1",
            "1/2-1/2" => "1/2",
            _ => unreachable!(),
        },
    )
    .ok_or("Risultato Vesus non valido")?;
    for hash in [
        &record.roster_source_blake3,
        &record.results_source_blake3,
        &record.crosstable_source_blake3,
    ] {
        validate_blake3(hash)?;
    }
    let wf = record
        .white_fide_id
        .as_deref()
        .ok_or("Contesto Vesus senza FIDE bianco")?;
    let bf = record
        .black_fide_id
        .as_deref()
        .ok_or("Contesto Vesus senza FIDE nero")?;
    if canonical_id(wf, "FIDE")? != wf || canonical_id(bf, "FIDE")? != bf || wf == bf {
        return Err("Contesto Vesus con FIDE non canonici/duplicati".into());
    }
    if let Some(id) = &record.white_fsi_id {
        canonical_id(id, "FSI")?;
    }
    if let Some(id) = &record.black_fsi_id {
        canonical_id(id, "FSI")?;
    }
    Ok(())
}

fn parse_graphql_parts(bytes: &[u8], content_type: &str) -> Result<Vec<Value>, String> {
    if content_type
        .to_ascii_lowercase()
        .starts_with("application/json")
    {
        return Ok(vec![
            serde_json::from_slice(bytes).map_err(|e| format!("Vesus JSON: {e}"))?
        ]);
    }
    let boundary = multipart_boundary(content_type)?;
    let text = std::str::from_utf8(bytes).map_err(|e| format!("Multipart Vesus non UTF-8: {e}"))?;
    let delimiter = format!("--{boundary}");
    let mut values = Vec::new();
    for segment in text.split(&delimiter) {
        let segment = segment.trim_matches(['\r', '\n']);
        if segment.is_empty() || segment == "--" {
            continue;
        }
        let (_, body) = segment
            .split_once("\r\n\r\n")
            .or_else(|| segment.split_once("\n\n"))
            .ok_or("Parte multipart Vesus senza separatore header/body")?;
        let body = body.trim_matches(['\r', '\n', '-']);
        if body.is_empty() {
            continue;
        }
        values.push(
            serde_json::from_str(body).map_err(|e| format!("Parte multipart Vesus JSON: {e}"))?,
        );
    }
    if values.is_empty() {
        return Err("Multipart Vesus senza parti JSON".into());
    }
    Ok(values)
}

fn multipart_boundary(content_type: &str) -> Result<String, String> {
    for part in content_type.split(';').skip(1) {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("boundary=") {
            let value = value.trim_matches('"');
            if !value.is_empty() {
                return Ok(value.to_owned());
            }
        }
    }
    Err("Content-Type multipart Vesus senza boundary".into())
}

fn load_verified_bundle(root: &Path, tournament_key: &str) -> Result<VerifiedBundle, String> {
    let reference_path = root
        .join("raw/refs")
        .join(format!("{SOURCE_KEY}-{tournament_key}.json"));
    let reference: BundleReference = read_json_regular(&reference_path, "riferimento Vesus")?;
    if reference.schema_version != BUNDLE_SCHEMA
        || reference.source_key != SOURCE_KEY
        || reference.tournament_key != tournament_key
    {
        return Err("Riferimento bundle Vesus incoerente".into());
    }
    let manifest_path = safe_join(root, &reference.manifest_path)?;
    let manifest_bytes = read_regular_bytes(&manifest_path)?;
    if blake3::hash(&manifest_bytes).to_hex().to_string() != reference.manifest_blake3 {
        return Err("Manifest Vesus alterato".into());
    }
    let manifest: BundleManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|e| format!("Manifest Vesus JSON: {e}"))?;
    if manifest.schema_version != BUNDLE_SCHEMA
        || manifest.source_key != SOURCE_KEY
        || manifest.tournament_key != tournament_key
        || manifest.pages.len() != 3
    {
        return Err("Manifest bundle Vesus incoerente".into());
    }
    let mut pages = BTreeMap::new();
    let mut kinds = BTreeSet::new();
    for page in &manifest.pages {
        if !kinds.insert(page.kind.clone())
            || !matches!(
                page.kind.as_str(),
                "tournament" | "pairings" | "federation-summary"
            )
        {
            return Err("Pagine bundle Vesus duplicate/sconosciute".into());
        }
        let path = safe_join(root, &page.object_path)?;
        let bytes = read_regular_bytes(&path)?;
        if bytes.len() as u64 != page.bytes
            || blake3::hash(&bytes).to_hex().to_string() != page.blake3
        {
            return Err(format!("Oggetto Vesus {} alterato", page.kind));
        }
        pages.insert(page.kind.clone(), path);
    }
    Ok(VerifiedBundle { manifest, pages })
}

fn page<'a>(bundle: &'a VerifiedBundle, kind: &str) -> Result<&'a Path, String> {
    bundle
        .pages
        .get(kind)
        .map(PathBuf::as_path)
        .ok_or_else(|| format!("Pagina Vesus mancante: {kind}"))
}
fn page_content_type<'a>(bundle: &'a VerifiedBundle, kind: &str) -> Result<&'a str, String> {
    bundle
        .manifest
        .pages
        .iter()
        .find(|p| p.kind == kind)
        .and_then(|p| p.content_type.as_deref())
        .ok_or_else(|| format!("Content-Type Vesus mancante: {kind}"))
}
fn page_hashes(bundle: &VerifiedBundle) -> Result<BTreeMap<String, String>, String> {
    let h = bundle
        .manifest
        .pages
        .iter()
        .map(|p| (p.kind.clone(), p.blake3.clone()))
        .collect::<BTreeMap<_, _>>();
    if h.len() == 3 {
        Ok(h)
    } else {
        Err("Hash pagine Vesus incompleti".into())
    }
}

fn header_string(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}
fn now_millis() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .map_err(|e| format!("Orologio precedente a UNIX_EPOCH: {e}"))
}
fn canonical_key(raw: &str) -> Result<String, String> {
    let v = raw.trim();
    if v.is_empty()
        || v.len() > 64
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        Err(format!("Short key Vesus non valida: {raw}"))
    } else {
        Ok(v.to_owned())
    }
}
fn canonical_id(raw: &str, label: &str) -> Result<String, String> {
    let v = raw.trim();
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("{label} ID non valido: {raw}"));
    }
    let n = v
        .parse::<u64>()
        .map_err(|_| format!("{label} ID fuori range"))?;
    if n == 0 {
        Err(format!("{label} ID zero"))
    } else {
        Ok(n.to_string())
    }
}
fn optional_id(value: Option<&Value>, label: &str) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => canonical_id(s, label).map(Some),
        Some(Value::Number(n)) => canonical_id(&n.to_string(), label).map(Some),
        _ => Err(format!("{label} ID Vesus non scalare")),
    }
}
fn parse_positive_u32(raw: &str, label: &str) -> Result<u32, String> {
    let n = raw
        .parse::<u32>()
        .map_err(|_| format!("{label} non numerico"))?;
    if n == 0 {
        Err(format!("{label} deve essere positivo"))
    } else {
        Ok(n)
    }
}
fn u32_value(v: &Value, key: &str) -> Result<u32, String> {
    let raw = v
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Vesus senza {key} numerico"))?;
    u32::try_from(raw).map_err(|_| format!("Vesus {key} fuori range"))
}
fn i64_value(v: &Value, key: &str) -> Result<i64, String> {
    v.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("Vesus senza {key} intero"))
}
fn canonical_iso_date(raw: &str) -> Result<String, String> {
    let b = raw.as_bytes();
    if b.len() < 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b[..10]
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err(format!("Data ISO Vesus non valida: {raw}"));
    }
    Ok(format!("{}.{}.{}", &raw[0..4], &raw[5..7], &raw[8..10]))
}
fn is_pgn_date(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() == 10
        && b[4] == b'.'
        && b[7] == b'.'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}
fn validate_blake3(v: &str) -> Result<(), String> {
    if v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("BLAKE3 Vesus non valido".into())
    }
}
fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let p = Path::new(relative);
    if p.is_absolute() {
        return Err("Percorso Vesus assoluto non ammesso".into());
    }
    for c in p.components() {
        if !matches!(c, Component::Normal(_)) {
            return Err("Percorso Vesus non canonico".into());
        }
    }
    Ok(root.join(p))
}
fn read_regular_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let m = fs::symlink_metadata(path).map_err(|e| format!("Metadati {}: {e}", path.display()))?;
    if m.file_type().is_symlink() || !m.is_file() {
        return Err(format!("File Vesus non regolare: {}", path.display()));
    }
    fs::read(path).map_err(|e| format!("Lettura {}: {e}", path.display()))
}
fn read_json_regular<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T, String> {
    serde_json::from_slice(&read_regular_bytes(path)?).map_err(|e| format!("{label} JSON: {e}"))
}
fn write_new_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Creazione {}: {e}", parent.display()))?;
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("Creazione {}: {e}", path.display()))?;
    f.write_all(bytes)
        .map_err(|e| format!("Scrittura {}: {e}", path.display()))?;
    f.sync_all()
        .map_err(|e| format!("Sync {}: {e}", path.display()))
}
fn write_json_replace<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Creazione {}: {e}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&tmp, &bytes).map_err(|e| format!("Scrittura {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("Pubblicazione {}: {e}", path.display()))
}
fn write_jsonl_new<T: Serialize>(path: &Path, values: &[T]) -> Result<(), String> {
    if values.is_empty() {
        return Err("Indice Vesus vuoto non pubblicabile".into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Creazione {}: {e}", parent.display()))?;
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("Creazione {}: {e}", path.display()))?;
    for v in values {
        serde_json::to_writer(&mut f, v).map_err(|e| e.to_string())?;
        f.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    f.sync_all().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_match_tokens_are_strict() {
        assert_eq!(parse_standard_match("1w31"), Some(("1".into(), 'w', 31)));
        assert_eq!(parse_standard_match("½b4"), Some(("1/2".into(), 'b', 4)));
        assert!(parse_standard_match("1Fw31").is_none());
        assert!(parse_standard_match("-w31").is_none());
        assert!(parse_standard_match("1w0").is_none());
    }

    fn sample_context_record() -> VesusContextGameV1 {
        let meta = EventMeta {
            event_key: "SYNV0001".into(),
            event_name: "4° Rapid Città Sintetica".into(),
            tournament_key: "SYNT0001".into(),
            tournament_name: "Open Fide".into(),
            start_date: "2026.09.13".into(),
            end_date: "2026.09.13".into(),
            published_rounds: 1,
            completed_published_rounds: 1,
        };
        let pairing = PairingRow {
            round: 1,
            board: 1,
            white_slot: 1,
            black_slot: 31,
        };
        VesusContextGameV1 {
            profile: INDEX_PROFILE.into(),
            context_id: context_id(&meta, &pairing, "1-0"),
            event_code: meta.event_key,
            event_name: meta.event_name,
            event_start_date: meta.start_date,
            event_end_date: meta.end_date,
            tournament_key: meta.tournament_key,
            tournament_name: meta.tournament_name,
            round: 1,
            board: 1,
            white_slot: 1,
            black_slot: 31,
            white_name: "Alfa, Aurora".into(),
            black_name: "Beta, Bruno".into(),
            white_fide_id: Some("920000001".into()),
            black_fide_id: Some("920000002".into()),
            white_fsi_id: Some("920001".into()),
            black_fsi_id: Some("920002".into()),
            result: "1-0".into(),
            roster_source_blake3: "a".repeat(64),
            results_source_blake3: "b".repeat(64),
            crosstable_source_blake3: "c".repeat(64),
            evidence_class: "official_context_join".into(),
        }
    }

    #[test]
    fn tampered_context_id_fails_closed() {
        let mut record = sample_context_record();
        assert!(validate_context_record(&record).is_ok());
        record.context_id = format!("vc1-{}", "0".repeat(64));
        assert!(validate_context_record(&record).is_err());
    }

    #[test]
    fn multipart_parser_accepts_relay_defer_shape() {
        let body=b"\r\n---\r\nContent-Type: application/json; charset=utf-8\r\n\r\n{\"data\":{\"event\":{}},\"hasNext\":true}\r\n---\r\nContent-Type: application/json; charset=utf-8\r\n\r\n{\"incremental\":[],\"hasNext\":false}\r\n-----\r\n";
        let values = parse_graphql_parts(body, "multipart/mixed; boundary=\"-\"").unwrap();
        assert_eq!(values.len(), 2);
    }
}
