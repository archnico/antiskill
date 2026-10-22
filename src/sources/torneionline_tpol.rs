use crate::object_store::store_object;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE_KEY: &str = "torneionline-tpol";
const BUNDLE_SCHEMA: u32 = 1;
const INDEX_PROFILE: &str = "antiskill_tpol_official_context_v1";
const DEFAULT_BASE_URL: &str = "https://www.torneionline.com/tornei_d.php";
const RIGHTS_CLASS: &str = "official-operational-internal-use";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PageEvidence {
    kind: String,
    url: String,
    http_status: u16,
    content_type: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
    blake3: String,
    bytes: u64,
    object_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct BundleManifest {
    schema_version: u32,
    source_key: String,
    event_code: String,
    fetched_at_unix_ms: u128,
    rights_class: String,
    retrieved_by: String,
    pages: Vec<PageEvidence>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct BundleReference {
    schema_version: u32,
    source_key: String,
    event_code: String,
    manifest_path: String,
    manifest_blake3: String,
}

#[derive(Clone, Debug)]
struct VerifiedBundle {
    manifest: BundleManifest,
    pages: BTreeMap<String, PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EventMetadata {
    name: String,
    start_date: String,
    end_date: String,
}

#[derive(Clone, Copy, Debug)]
struct ContextSourceHashes<'a> {
    roster: &'a str,
    results: &'a str,
    crosstable: &'a str,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct TpolParticipantV1 {
    pub profile: String,
    pub event_code: String,
    pub slot: u32,
    pub raw_name: String,
    pub federation: Option<String>,
    pub birth_year: Option<u16>,
    pub fide_id: Option<String>,
    pub fsi_id: Option<String>,
    pub roster_source_blake3: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct TpolContextGameV1 {
    pub profile: String,
    pub context_id: String,
    pub event_code: String,
    pub event_name: String,
    pub event_start_date: String,
    pub event_end_date: String,
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
    event_code: String,
    event_name: String,
    event_start_date: String,
    event_end_date: String,
    participants: usize,
    rounds: u32,
    complete_pairings: usize,
    excluded_nonstandard_or_bye: usize,
    roster_source_blake3: String,
    results_source_blake3: String,
    crosstable_source_blake3: String,
    complete: bool,
}

#[derive(Clone, Debug)]
struct SyncConfig {
    root: PathBuf,
    event_code: String,
    base_url: String,
}

#[derive(Clone, Debug)]
struct IndexConfig {
    root: PathBuf,
    event_code: String,
    output: PathBuf,
    report: PathBuf,
}

#[derive(Clone, Debug)]
enum QueryKey {
    Fide(String),
    Fsi(String),
    Slot(u32),
}

#[derive(Clone, Debug)]
struct QueryConfig {
    index: PathBuf,
    round: Option<u32>,
    key: Option<QueryKey>,
}

#[derive(Clone, Debug)]
struct RawPairing {
    round: u32,
    board: u32,
    white_slot: u32,
    black_slot: u32,
    white_name: String,
    black_name: String,
    result: String,
}

#[derive(Clone, Debug)]
struct CrossRound {
    opponent: u32,
    color: char,
    score: String,
}

#[derive(Clone, Debug)]
struct CrossPlayer {
    name: String,
    rounds: Vec<Option<CrossRound>>,
}

pub(crate) fn print_sync_help() {
    println!("antiskill sources sync torneionline-tpol\n\nUSO:\n    antiskill sources sync torneionline-tpol --root DIR --event-code CODE [--base-url URL]\n");
}

pub(crate) fn print_index_help() {
    println!("antiskill sources index torneionline-tpol\n\nUSO:\n    antiskill sources index torneionline-tpol --root DIR --event-code CODE --output FILE --report FILE\n");
}

pub(crate) fn print_query_help() {
    println!("antiskill sources query torneionline-tpol\n\nUSO:\n    antiskill sources query torneionline-tpol --index FILE [--round N] [--fide-id ID | --fsi-id ID | --slot N]\n");
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
    let fetched_at_unix_ms = unix_time_ms()?;
    let user_agent = format!(
        "antiskill/{} (+https://github.com/archnico/antiskill)",
        env!("CARGO_PKG_VERSION")
    );
    let mut pages = Vec::new();
    for (kind, query) in [
        ("participants", "tipo=p&ord=u&sen=a"),
        ("results", "tipo=a&ord=u&sen=a&tux=0"),
        ("crosstable", "tipo=t&ord=u&sen=a"),
    ] {
        let url = format!("{}?codice={}&{query}", config.base_url, config.event_code);
        pages.push(fetch_page(&config.root, kind, &url, &user_agent)?);
    }
    let manifest = BundleManifest {
        schema_version: BUNDLE_SCHEMA,
        source_key: SOURCE_KEY.into(),
        event_code: config.event_code.clone(),
        fetched_at_unix_ms,
        rights_class: RIGHTS_CLASS.into(),
        retrieved_by: user_agent,
        pages,
    };
    let manifest_bytes = canonical_json_bytes(&manifest)?;
    let manifest_blake3 = blake3::hash(&manifest_bytes).to_hex().to_string();
    let manifest_relative = PathBuf::from("raw/manifests")
        .join(SOURCE_KEY)
        .join(event_key(&config.event_code))
        .join(format!(
            "{fetched_at_unix_ms}-{}-{}.json",
            std::process::id(),
            &manifest_blake3[..12]
        ));
    let manifest_path = config.root.join(&manifest_relative);
    write_bytes_new(&manifest_path, &manifest_bytes)?;
    let reference = BundleReference {
        schema_version: BUNDLE_SCHEMA,
        source_key: SOURCE_KEY.into(),
        event_code: config.event_code.clone(),
        manifest_path: path_string(&manifest_relative),
        manifest_blake3,
    };
    let reference_path = config.root.join("raw/refs").join(format!(
        "{SOURCE_KEY}-{}.json",
        event_key(&config.event_code)
    ));
    write_json_replace(&reference_path, &reference)?;
    println!("SOURCE\t{SOURCE_KEY}");
    println!("EVENT\t{}", config.event_code);
    println!("PAGES\t3");
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
    let bundle = load_verified_bundle(&config.root, &config.event_code)?;
    let participants_page = page(&bundle, "participants")?;
    let results_page = page(&bundle, "results")?;
    let crosstable_page = page(&bundle, "crosstable")?;
    let participants_bytes =
        fs::read(participants_page).map_err(|e| format!("Lettura roster T'Pol: {e}"))?;
    let results_bytes =
        fs::read(results_page).map_err(|e| format!("Lettura risultati T'Pol: {e}"))?;
    let crosstable_bytes =
        fs::read(crosstable_page).map_err(|e| format!("Lettura tabellone T'Pol: {e}"))?;
    let hashes = page_hashes(&bundle)?;
    let event_metadata = parse_event_metadata(&participants_bytes, &config.event_code)?;
    let participants = parse_participants(
        &participants_bytes,
        &config.event_code,
        &hashes["participants"],
    )?;
    let (pairings, rounds, excluded) = parse_results(&results_bytes)?;
    let cross = parse_crosstable(&crosstable_bytes, rounds)?;
    let contexts = reconcile_contexts(
        &config.event_code,
        &event_metadata,
        &participants,
        &pairings,
        &cross,
        ContextSourceHashes {
            roster: &hashes["participants"],
            results: &hashes["results"],
            crosstable: &hashes["crosstable"],
        },
    )?;
    write_jsonl_new(&config.output, &contexts)?;
    let report = IndexReport {
        profile: INDEX_PROFILE,
        event_code: config.event_code,
        event_name: event_metadata.name,
        event_start_date: event_metadata.start_date,
        event_end_date: event_metadata.end_date,
        participants: participants.len(),
        rounds,
        complete_pairings: contexts.len(),
        excluded_nonstandard_or_bye: excluded,
        roster_source_blake3: hashes["participants"].clone(),
        results_source_blake3: hashes["results"].clone(),
        crosstable_source_blake3: hashes["crosstable"].clone(),
        complete: true,
    };
    write_json_replace(&config.report, &report)?;
    println!("PROFILE\t{INDEX_PROFILE}");
    println!("EVENT\t{}", report.event_code);
    println!("PARTICIPANTS\t{}", report.participants);
    println!("ROUNDS\t{}", report.rounds);
    println!("CONTEXT_GAMES\t{}", report.complete_pairings);
    println!("EXCLUDED\t{}", report.excluded_nonstandard_or_bye);
    println!("OUTPUT\t{}", config.output.display());
    println!("REPORT\t{}", config.report.display());
    Ok(())
}

pub(crate) fn load_verified_context_index(index: &Path) -> Result<Vec<TpolContextGameV1>, String> {
    let metadata = fs::symlink_metadata(index)
        .map_err(|e| format!("Indice T'Pol {}: {e}", index.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("Indice T'Pol non regolare: {}", index.display()));
    }
    let file = File::open(index).map_err(|e| format!("Apertura indice T'Pol: {e}"))?;
    let mut records = Vec::new();
    let mut context_ids = BTreeSet::new();
    let mut coordinates = BTreeSet::new();
    let mut fingerprint: Option<(String, String, String, String, String, String)> = None;
    for (line_no, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("Lettura indice T'Pol: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let record: TpolContextGameV1 = serde_json::from_str(&line)
            .map_err(|e| format!("Indice T'Pol JSONL riga {}: {e}", line_no + 1))?;
        validate_context_record(&record)?;
        if !context_ids.insert(record.context_id.clone()) {
            return Err("Indice T'Pol con context_id duplicato".into());
        }
        if !coordinates.insert((record.round, record.board)) {
            return Err("Indice T'Pol con turno/scacchiera duplicati".into());
        }
        let current = (
            record.event_code.clone(),
            record.event_name.clone(),
            record.event_start_date.clone(),
            record.event_end_date.clone(),
            record.roster_source_blake3.clone(),
            format!(
                "{}:{}",
                record.results_source_blake3, record.crosstable_source_blake3
            ),
        );
        if let Some(expected) = &fingerprint {
            if expected != &current {
                return Err("Indice T'Pol mescola eventi o snapshot differenti".into());
            }
        } else {
            fingerprint = Some(current);
        }
        if record.white_fide_id.is_some() && record.white_fide_id == record.black_fide_id {
            return Err("Contesto T'Pol assegna lo stesso FIDE ID a entrambi i lati".into());
        }
        records.push(record);
    }
    if records.is_empty() {
        return Err("Indice contestuale T'Pol vuoto".into());
    }
    records.sort_by_key(|record| (record.round, record.board));
    Ok(records)
}

pub(crate) fn resolve_fide_contexts(
    index: &Path,
    fide_id: &str,
) -> Result<Vec<TpolContextGameV1>, String> {
    let fide_id = canonical_id(fide_id, "FIDE")?;
    let mut selected = load_verified_context_index(index)?
        .into_iter()
        .filter(|record| {
            record.white_fide_id.as_deref() == Some(fide_id.as_str())
                || record.black_fide_id.as_deref() == Some(fide_id.as_str())
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("FIDE ID assente dall'indice contestuale T'Pol".into());
    }
    selected.sort_by_key(|record| (record.round, record.board));
    Ok(selected)
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
    let file = File::open(&config.index).map_err(|e| format!("Apertura indice T'Pol: {e}"))?;
    let mut matches = Vec::new();
    for (line_no, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("Lettura indice T'Pol: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let record: TpolContextGameV1 = serde_json::from_str(&line)
            .map_err(|e| format!("Indice T'Pol JSONL riga {}: {e}", line_no + 1))?;
        validate_context_record(&record)?;
        if config.round.is_some_and(|round| record.round != round) {
            continue;
        }
        if let Some(key) = &config.key {
            let selected = match key {
                QueryKey::Fide(id) => {
                    record.white_fide_id.as_deref() == Some(id)
                        || record.black_fide_id.as_deref() == Some(id)
                }
                QueryKey::Fsi(id) => {
                    record.white_fsi_id.as_deref() == Some(id)
                        || record.black_fsi_id.as_deref() == Some(id)
                }
                QueryKey::Slot(slot) => record.white_slot == *slot || record.black_slot == *slot,
            };
            if !selected {
                continue;
            }
        }
        matches.push(record);
    }
    matches.sort_by_key(|record| {
        (
            record.round,
            record.board,
            record.white_slot,
            record.black_slot,
        )
    });
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &matches).map_err(|e| e.to_string())?;
    stdout.write_all(b"\n").map_err(|e| e.to_string())
}

fn parse_sync_args(args: Vec<String>) -> Result<SyncConfig, String> {
    let mut root = None;
    let mut event_code = None;
    let mut base_url = DEFAULT_BASE_URL.to_owned();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--root" => root = Some(PathBuf::from(it.next().ok_or("Manca --root")?)),
            "--event-code" => {
                event_code = Some(canonical_event_code(
                    &it.next().ok_or("Manca --event-code")?,
                )?)
            }
            "--base-url" => base_url = it.next().ok_or("Manca --base-url")?,
            other => return Err(format!("Opzione sync T'Pol sconosciuta: {other}")),
        }
    }
    if base_url.trim().is_empty() {
        return Err("--base-url vuoto".into());
    }
    Ok(SyncConfig {
        root: root.ok_or("Serve --root")?,
        event_code: event_code.ok_or("Serve --event-code")?,
        base_url,
    })
}

fn parse_index_args(args: Vec<String>) -> Result<IndexConfig, String> {
    let mut root = None;
    let mut event_code = None;
    let mut output = None;
    let mut report = None;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--root" => root = Some(PathBuf::from(it.next().ok_or("Manca --root")?)),
            "--event-code" => {
                event_code = Some(canonical_event_code(
                    &it.next().ok_or("Manca --event-code")?,
                )?)
            }
            "--output" => output = Some(PathBuf::from(it.next().ok_or("Manca --output")?)),
            "--report" => report = Some(PathBuf::from(it.next().ok_or("Manca --report")?)),
            other => return Err(format!("Opzione index T'Pol sconosciuta: {other}")),
        }
    }
    Ok(IndexConfig {
        root: root.ok_or("Serve --root")?,
        event_code: event_code.ok_or("Serve --event-code")?,
        output: output.ok_or("Serve --output")?,
        report: report.ok_or("Serve --report")?,
    })
}

fn parse_query_args(args: Vec<String>) -> Result<QueryConfig, String> {
    let mut index = None;
    let mut round = None;
    let mut key = None;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--index" => index = Some(PathBuf::from(it.next().ok_or("Manca --index")?)),
            "--round" => {
                round = Some(parse_positive_u32(
                    &it.next().ok_or("Manca --round")?,
                    "round",
                )?)
            }
            "--fide-id" => set_query_key(
                &mut key,
                QueryKey::Fide(canonical_id(&it.next().ok_or("Manca --fide-id")?, "FIDE")?),
            )?,
            "--fsi-id" => set_query_key(
                &mut key,
                QueryKey::Fsi(canonical_id(&it.next().ok_or("Manca --fsi-id")?, "FSI")?),
            )?,
            "--slot" => set_query_key(
                &mut key,
                QueryKey::Slot(parse_positive_u32(
                    &it.next().ok_or("Manca --slot")?,
                    "slot",
                )?),
            )?,
            other => return Err(format!("Opzione query T'Pol sconosciuta: {other}")),
        }
    }
    Ok(QueryConfig {
        index: index.ok_or("Serve --index")?,
        round,
        key,
    })
}

fn set_query_key(target: &mut Option<QueryKey>, value: QueryKey) -> Result<(), String> {
    if target.replace(value).is_some() {
        Err("Specificare un solo identificatore nella query T'Pol".into())
    } else {
        Ok(())
    }
}

fn fetch_page(
    root: &Path,
    kind: &str,
    url: &str,
    user_agent: &str,
) -> Result<PageEvidence, String> {
    let mut response = ureq::get(url)
        .header("User-Agent", user_agent)
        .header("Accept", "text/html,*/*;q=0.1")
        .header("Accept-Encoding", "identity")
        .call()
        .map_err(|e| format!("Download T'Pol {kind} fallito: {e}"))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("T'Pol {kind}: HTTP {status}"));
    }
    let content_type = header_string(&response, "content-type");
    let etag = header_string(&response, "etag");
    let last_modified = header_string(&response, "last-modified");
    let stored = store_object(root, response.body_mut().as_reader())?;
    Ok(PageEvidence {
        kind: kind.into(),
        url: url.into(),
        http_status: status,
        content_type,
        etag,
        last_modified,
        blake3: stored.blake3,
        bytes: stored.bytes,
        object_path: path_string(&stored.object_relative),
    })
}

fn header_string(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn load_verified_bundle(root: &Path, event_code: &str) -> Result<VerifiedBundle, String> {
    let reference_path = root
        .join("raw/refs")
        .join(format!("{SOURCE_KEY}-{}.json", event_key(event_code)));
    let reference: BundleReference = read_json_regular(&reference_path, "riferimento T'Pol")?;
    if reference.schema_version != BUNDLE_SCHEMA
        || reference.source_key != SOURCE_KEY
        || reference.event_code != event_code
    {
        return Err("Riferimento bundle T'Pol incoerente".into());
    }
    let manifest_path = safe_join(root, &reference.manifest_path)?;
    let manifest_bytes = read_regular_bytes(&manifest_path)?;
    let actual_manifest_hash = blake3::hash(&manifest_bytes).to_hex().to_string();
    if actual_manifest_hash != reference.manifest_blake3 {
        return Err("Manifest bundle T'Pol alterato".into());
    }
    let manifest: BundleManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|e| format!("Manifest T'Pol JSON: {e}"))?;
    if manifest.schema_version != BUNDLE_SCHEMA
        || manifest.source_key != SOURCE_KEY
        || manifest.event_code != event_code
        || manifest.pages.len() != 3
    {
        return Err("Manifest bundle T'Pol incoerente".into());
    }
    let mut pages = BTreeMap::new();
    let mut kinds = BTreeSet::new();
    for page in &manifest.pages {
        if !kinds.insert(page.kind.clone())
            || !matches!(
                page.kind.as_str(),
                "participants" | "results" | "crosstable"
            )
        {
            return Err("Pagine bundle T'Pol duplicate/sconosciute".into());
        }
        let path = safe_join(root, &page.object_path)?;
        let bytes = read_regular_bytes(&path)?;
        if bytes.len() as u64 != page.bytes
            || blake3::hash(&bytes).to_hex().to_string() != page.blake3
        {
            return Err(format!("Oggetto T'Pol {} alterato", page.kind));
        }
        pages.insert(page.kind.clone(), path);
    }
    if kinds.len() != 3 {
        return Err("Bundle T'Pol incompleto".into());
    }
    Ok(VerifiedBundle { manifest, pages })
}

fn page<'a>(bundle: &'a VerifiedBundle, kind: &str) -> Result<&'a Path, String> {
    bundle
        .pages
        .get(kind)
        .map(PathBuf::as_path)
        .ok_or_else(|| format!("Pagina T'Pol mancante: {kind}"))
}

fn page_hashes(bundle: &VerifiedBundle) -> Result<BTreeMap<String, String>, String> {
    let mut hashes = BTreeMap::new();
    for page in &bundle.manifest.pages {
        hashes.insert(page.kind.clone(), page.blake3.clone());
    }
    if hashes.len() == 3 {
        Ok(hashes)
    } else {
        Err("Hash pagine T'Pol incompleti".into())
    }
}

fn parse_event_metadata(input: &[u8], event_code: &str) -> Result<EventMetadata, String> {
    let html = decode_mixed(input);
    let lower = html.to_ascii_lowercase();
    let marker = "dati generali del torneo";
    let section_start = lower
        .find(marker)
        .ok_or("T'Pol: sezione dati generali evento non trovata")?;
    let heading_end_rel = lower[section_start..]
        .find("</table>")
        .ok_or("T'Pol: fine intestazione dati generali non trovata")?;
    let heading_end = section_start + heading_end_rel + "</table>".len();
    let data_start_rel = lower[heading_end..]
        .find("<table")
        .ok_or("T'Pol: tabella dati generali non trovata")?;
    let data_start = heading_end + data_start_rel;
    let data_end_rel = lower[data_start..]
        .find("</table>")
        .ok_or("T'Pol: fine tabella dati generali non trovata")?;
    let data_end = data_start + data_end_rel;
    let section = &html[data_start..data_end];
    let cells = fragment_cells(section);
    for window in cells.windows(10) {
        if window[0] == event_code {
            let name = window[1].trim().to_owned();
            let start_date = canonical_tpol_date(&window[5])?;
            let end_date = canonical_tpol_date(&window[6])?;
            if name.is_empty() {
                return Err("T'Pol: denominazione evento vuota".into());
            }
            return Ok(EventMetadata {
                name,
                start_date,
                end_date,
            });
        }
    }
    Err("T'Pol: riga dati generali evento non trovata".into())
}

fn fragment_cells(fragment: &str) -> Vec<String> {
    let lower = fragment.to_ascii_lowercase();
    let mut cells = Vec::new();
    let mut cursor = 0usize;
    while let Some(td_rel) = lower[cursor..].find("<td") {
        let td_start = cursor + td_rel;
        let Some(tag_end_rel) = lower[td_start..].find('>') else {
            break;
        };
        let body_start = td_start + tag_end_rel + 1;
        let Some(td_end_rel) = lower[body_start..].find("</td>") else {
            break;
        };
        let td_end = body_start + td_end_rel;
        cells.push(cell_text(&fragment[body_start..td_end]));
        cursor = td_end + 5;
    }
    cells
}

fn canonical_tpol_date(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if !is_tpol_date(value) {
        return Err(format!("Data T'Pol non valida: {raw}"));
    }
    let day = &value[0..2];
    let month = &value[3..5];
    let year = &value[6..10];
    Ok(format!("{year}.{month}.{day}"))
}

fn is_tpol_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() == 10 && b[2] == b'-' && b[5] == b'-' {
        return b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 2 || i == 5 || c.is_ascii_digit());
    }
    if b.len() == 10 && b[4] == b'.' && b[7] == b'.' {
        return b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    }
    false
}

fn parse_participants(
    input: &[u8],
    event_code: &str,
    source_hash: &str,
) -> Result<BTreeMap<u32, TpolParticipantV1>, String> {
    let rows = table_rows(input);
    let mut participants = BTreeMap::new();
    let mut fide_seen = BTreeMap::<String, u32>::new();
    let mut fsi_seen = BTreeMap::<String, u32>::new();
    for cells in rows {
        if cells.len() < 16 || parse_positive_u32_opt(&cells[1]).is_none() || cells[4].is_empty() {
            continue;
        }
        let slot = parse_positive_u32(&cells[1], "slot partecipante")?;
        let fide_id = optional_id(&cells[14], "FIDE")?;
        let fsi_id = optional_id(&cells[15], "FSI")?;
        let participant = TpolParticipantV1 {
            profile: INDEX_PROFILE.into(),
            event_code: event_code.into(),
            slot,
            raw_name: cells[4].clone(),
            federation: nonempty(&cells[5]),
            birth_year: optional_u16(&cells[12], "anno nascita")?,
            fide_id: fide_id.clone(),
            fsi_id: fsi_id.clone(),
            roster_source_blake3: source_hash.into(),
        };
        if participants.insert(slot, participant).is_some() {
            return Err(format!("Slot T'Pol duplicato: {slot}"));
        }
        if let Some(id) = fide_id {
            if let Some(previous) = fide_seen.insert(id.clone(), slot) {
                if previous != slot {
                    return Err(format!("FIDE ID {id} su più slot T'Pol"));
                }
            }
        }
        if let Some(id) = fsi_id {
            if let Some(previous) = fsi_seen.insert(id.clone(), slot) {
                if previous != slot {
                    return Err(format!("FSI ID {id} su più slot T'Pol"));
                }
            }
        }
    }
    if participants.is_empty() {
        return Err("Nessun partecipante T'Pol riconosciuto".into());
    }
    Ok(participants)
}

fn parse_results(input: &[u8]) -> Result<(Vec<RawPairing>, u32, usize), String> {
    let rows = table_rows(input);
    let mut round = None;
    let mut max_round = 0_u32;
    let mut pairings = Vec::new();
    let mut excluded = 0_usize;
    let mut round_slots = BTreeSet::<(u32, u32)>::new();
    for cells in rows {
        if let Some(found) = parse_round_heading(&cells) {
            round = Some(found);
            max_round = max_round.max(found);
            continue;
        }
        if cells.len() < 20
            || parse_positive_u32_opt(&cells[1]).is_none()
            || parse_positive_u32_opt(&cells[2]).is_none()
        {
            continue;
        }
        let Some(current_round) = round else {
            continue;
        };
        let Some(black_slot) = parse_positive_u32_opt(&cells[9]) else {
            excluded += 1;
            continue;
        };
        let white_slot = parse_positive_u32(&cells[2], "slot bianco")?;
        if !round_slots.insert((current_round, white_slot))
            || !round_slots.insert((current_round, black_slot))
        {
            return Err(format!("Giocatore duplicato al turno {current_round}"));
        }
        let Some(result) = canonical_result(&cells[16], &cells[18]) else {
            excluded += 1;
            continue;
        };
        pairings.push(RawPairing {
            round: current_round,
            board: parse_positive_u32(&cells[1], "scacchiera")?,
            white_slot,
            black_slot,
            white_name: cells[5].clone(),
            black_name: cells[12].clone(),
            result,
        });
    }
    if pairings.is_empty() || max_round == 0 {
        return Err("Nessun pairing T'Pol riconosciuto".into());
    }
    Ok((pairings, max_round, excluded))
}

fn parse_crosstable(input: &[u8], rounds: u32) -> Result<BTreeMap<u32, CrossPlayer>, String> {
    let rows = table_rows(input);
    let mut players = BTreeMap::new();
    let expected_min = 9 + rounds as usize * 5;
    for cells in rows {
        if cells.len() < expected_min
            || parse_positive_u32_opt(&cells[2]).is_none()
            || cells[5].is_empty()
        {
            continue;
        }
        let slot = parse_positive_u32(&cells[2], "slot tabellone")?;
        let mut round_data = Vec::new();
        for turn in 0..rounds as usize {
            let base = 9 + turn * 5;
            let opponent =
                parse_positive_u32_opt(cells.get(base).map(String::as_str).unwrap_or(""));
            let color = cells.get(base + 1).and_then(|value| value.chars().next());
            let score = cells
                .get(base + 2)
                .map(|value| canonical_score(value))
                .transpose()?;
            let item = match (opponent, color, score) {
                (Some(opponent), Some(color @ ('B' | 'N')), Some(score)) => Some(CrossRound {
                    opponent,
                    color,
                    score,
                }),
                (None, _, _) => None,
                _ => {
                    return Err(format!(
                        "Tabellone T'Pol incompleto per slot {slot}, turno {}",
                        turn + 1
                    ))
                }
            };
            round_data.push(item);
        }
        if players
            .insert(
                slot,
                CrossPlayer {
                    name: cells[5].clone(),
                    rounds: round_data,
                },
            )
            .is_some()
        {
            return Err(format!("Slot tabellone duplicato: {slot}"));
        }
    }
    if players.is_empty() {
        return Err("Nessun giocatore nel tabellone T'Pol".into());
    }
    Ok(players)
}

fn reconcile_contexts(
    event_code: &str,
    event_metadata: &EventMetadata,
    participants: &BTreeMap<u32, TpolParticipantV1>,
    pairings: &[RawPairing],
    cross: &BTreeMap<u32, CrossPlayer>,
    hashes: ContextSourceHashes<'_>,
) -> Result<Vec<TpolContextGameV1>, String> {
    let mut contexts = Vec::new();
    let mut keys = BTreeSet::new();
    for pairing in pairings {
        let white = participants
            .get(&pairing.white_slot)
            .ok_or_else(|| format!("Slot bianco {} assente dal roster", pairing.white_slot))?;
        let black = participants
            .get(&pairing.black_slot)
            .ok_or_else(|| format!("Slot nero {} assente dal roster", pairing.black_slot))?;
        if normalize_name(&white.raw_name) != normalize_name(&pairing.white_name)
            || normalize_name(&black.raw_name) != normalize_name(&pairing.black_name)
        {
            return Err(format!(
                "Nome risultati/roster incoerente al turno {}, scacchiera {}",
                pairing.round, pairing.board
            ));
        }
        let white_cross = cross
            .get(&pairing.white_slot)
            .ok_or_else(|| format!("Slot {} assente dal tabellone", pairing.white_slot))?;
        let black_cross = cross
            .get(&pairing.black_slot)
            .ok_or_else(|| format!("Slot {} assente dal tabellone", pairing.black_slot))?;
        if normalize_name(&white_cross.name) != normalize_name(&white.raw_name)
            || normalize_name(&black_cross.name) != normalize_name(&black.raw_name)
        {
            return Err("Nome tabellone/roster incoerente".into());
        }
        let turn_index = (pairing.round - 1) as usize;
        let wc = white_cross
            .rounds
            .get(turn_index)
            .and_then(Option::as_ref)
            .ok_or("Round bianco assente dal tabellone")?;
        let bc = black_cross
            .rounds
            .get(turn_index)
            .and_then(Option::as_ref)
            .ok_or("Round nero assente dal tabellone")?;
        let (white_score, black_score) = result_scores(&pairing.result)?;
        if wc.opponent != pairing.black_slot
            || wc.color != 'B'
            || wc.score != white_score
            || bc.opponent != pairing.white_slot
            || bc.color != 'N'
            || bc.score != black_score
        {
            return Err(format!(
                "Pairing non reciproco al turno {}, scacchiera {}",
                pairing.round, pairing.board
            ));
        }
        let key = (pairing.round, pairing.white_slot, pairing.black_slot);
        if !keys.insert(key) {
            return Err("Context game T'Pol duplicato".into());
        }
        let context_id = context_id(event_code, pairing);
        contexts.push(TpolContextGameV1 {
            profile: INDEX_PROFILE.into(),
            context_id,
            event_code: event_code.into(),
            event_name: event_metadata.name.clone(),
            event_start_date: event_metadata.start_date.clone(),
            event_end_date: event_metadata.end_date.clone(),
            round: pairing.round,
            board: pairing.board,
            white_slot: pairing.white_slot,
            black_slot: pairing.black_slot,
            white_name: white.raw_name.clone(),
            black_name: black.raw_name.clone(),
            white_fide_id: white.fide_id.clone(),
            black_fide_id: black.fide_id.clone(),
            white_fsi_id: white.fsi_id.clone(),
            black_fsi_id: black.fsi_id.clone(),
            result: pairing.result.clone(),
            roster_source_blake3: hashes.roster.into(),
            results_source_blake3: hashes.results.into(),
            crosstable_source_blake3: hashes.crosstable.into(),
            evidence_class: "official_context_join".into(),
        });
    }
    contexts.sort_by_key(|record| {
        (
            record.round,
            record.board,
            record.white_slot,
            record.black_slot,
        )
    });
    Ok(contexts)
}

fn table_rows(input: &[u8]) -> Vec<Vec<String>> {
    let html = decode_mixed(input);
    let lower = html.to_ascii_lowercase();
    let mut rows = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = lower[cursor..].find("<tr") {
        let start_tag = cursor + offset;
        let Some(tag_end_rel) = lower[start_tag..].find('>') else {
            break;
        };
        let body_start = start_tag + tag_end_rel + 1;
        let Some(row_end_rel) = lower[body_start..].find("</tr>") else {
            break;
        };
        let row_end = body_start + row_end_rel;
        let row = &html[body_start..row_end];
        let row_lower = &lower[body_start..row_end];
        let mut cells = Vec::new();
        let mut cell_cursor = 0;
        while let Some(td_rel) = row_lower[cell_cursor..].find("<td") {
            let td_start = cell_cursor + td_rel;
            let Some(td_tag_end_rel) = row_lower[td_start..].find('>') else {
                break;
            };
            let td_body = td_start + td_tag_end_rel + 1;
            let Some(td_end_rel) = row_lower[td_body..].find("</td>") else {
                break;
            };
            let td_end = td_body + td_end_rel;
            cells.push(cell_text(&row[td_body..td_end]));
            cell_cursor = td_end + 5;
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
        cursor = row_end + 5;
    }
    rows
}

fn cell_text(fragment: &str) -> String {
    let mut text = String::with_capacity(fragment.len());
    let bytes = fragment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'<' {
            if let Some(end_rel) = bytes[index..].iter().position(|byte| *byte == b'>') {
                text.push(' ');
                index += end_rel + 1;
                continue;
            }
        }
        let character = fragment[index..]
            .chars()
            .next()
            .expect("indice UTF-8 valido");
        text.push(character);
        index += character.len_utf8();
    }
    normalize_space(&decode_entities(&text))
}

fn decode_mixed(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index].is_ascii() {
            out.push(char::from(input[index]));
            index += 1;
            continue;
        }
        let mut consumed = false;
        for width in 2..=4 {
            if index + width <= input.len() {
                if let Ok(value) = std::str::from_utf8(&input[index..index + width]) {
                    if value.chars().count() == 1 {
                        out.push_str(value);
                        index += width;
                        consumed = true;
                        break;
                    }
                }
            }
        }
        if !consumed {
            out.push(char::from(input[index]));
            index += 1;
        }
    }
    out
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("Â½", "½")
}

fn parse_round_heading(cells: &[String]) -> Option<u32> {
    cells.iter().find_map(|cell| {
        let lower = cell.to_ascii_lowercase();
        let marker = "abbinamenti e risultati del turno";
        lower.find(marker).and_then(|index| {
            lower[index + marker.len()..]
                .split_whitespace()
                .find_map(parse_positive_u32_opt)
        })
    })
}

fn canonical_result(left: &str, right: &str) -> Option<String> {
    let left = canonical_score(left).ok()?;
    let right = canonical_score(right).ok()?;
    match (left.as_str(), right.as_str()) {
        ("1", "0") => Some("1-0".into()),
        ("0", "1") => Some("0-1".into()),
        ("1/2", "1/2") => Some("1/2-1/2".into()),
        _ => None,
    }
}

fn canonical_score(raw: &str) -> Result<String, String> {
    let value = normalize_space(raw).replace("Â½", "½");
    match value.as_str() {
        "1" => Ok("1".into()),
        "0" => Ok("0".into()),
        "½" | "1/2" => Ok("1/2".into()),
        "" => Err("Punteggio T'Pol vuoto".into()),
        other => Err(format!("Punteggio T'Pol non supportato: {other}")),
    }
}

fn result_scores(result: &str) -> Result<(String, String), String> {
    match result {
        "1-0" => Ok(("1".into(), "0".into())),
        "0-1" => Ok(("0".into(), "1".into())),
        "1/2-1/2" => Ok(("1/2".into(), "1/2".into())),
        _ => Err(format!("Risultato T'Pol non supportato: {result}")),
    }
}

fn context_id(event_code: &str, pairing: &RawPairing) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(INDEX_PROFILE.as_bytes());
    hasher.update(b"\0");
    hasher.update(event_code.as_bytes());
    hasher.update(b"\0");
    hasher.update(pairing.round.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(pairing.white_slot.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(pairing.black_slot.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(pairing.result.as_bytes());
    format!("tc1-{}", hasher.finalize().to_hex())
}

pub(crate) fn validate_context_record(record: &TpolContextGameV1) -> Result<(), String> {
    if record.profile != INDEX_PROFILE
        || record.evidence_class != "official_context_join"
        || !record.context_id.starts_with("tc1-")
    {
        return Err("Record context T'Pol con profilo/identità non valida".into());
    }
    canonical_event_code(&record.event_code)?;
    let expected_context_id = context_id(
        &record.event_code,
        &RawPairing {
            round: record.round,
            board: record.board,
            white_slot: record.white_slot,
            black_slot: record.black_slot,
            white_name: record.white_name.clone(),
            black_name: record.black_name.clone(),
            result: record.result.clone(),
        },
    );
    if record.context_id != expected_context_id {
        return Err("Record context T'Pol con context_id non ricalcolabile".into());
    }
    if record.event_name.trim().is_empty()
        || !is_tpol_date(&record.event_start_date)
        || !is_tpol_date(&record.event_end_date)
        || record.event_start_date > record.event_end_date
    {
        return Err("Record context T'Pol con metadati evento non validi".into());
    }
    if record.round == 0
        || record.board == 0
        || record.white_slot == 0
        || record.black_slot == 0
        || record.white_slot == record.black_slot
    {
        return Err("Record context T'Pol con coordinate non valide".into());
    }
    result_scores(&record.result)?;
    for hash in [
        &record.roster_source_blake3,
        &record.results_source_blake3,
        &record.crosstable_source_blake3,
    ] {
        validate_blake3(hash)?;
    }
    if let Some(id) = &record.white_fide_id {
        canonical_id(id, "FIDE")?;
    }
    if let Some(id) = &record.black_fide_id {
        canonical_id(id, "FIDE")?;
    }
    if let Some(id) = &record.white_fsi_id {
        canonical_id(id, "FSI")?;
    }
    if let Some(id) = &record.black_fsi_id {
        canonical_id(id, "FSI")?;
    }
    Ok(())
}

fn canonical_event_code(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(format!("Codice torneo T'Pol non valido: {raw}"));
    }
    Ok(value.to_owned())
}

fn canonical_id(raw: &str, namespace: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("ID {namespace} non valido: {raw}"));
    }
    let numeric = value
        .parse::<u64>()
        .map_err(|_| format!("ID {namespace} non valido: {raw}"))?;
    if numeric == 0 {
        return Err(format!("ID {namespace} non valido: {raw}"));
    }
    Ok(numeric.to_string())
}

fn optional_id(raw: &str, namespace: &str) -> Result<Option<String>, String> {
    if raw.trim().is_empty() || raw.trim() == "0" {
        Ok(None)
    } else {
        canonical_id(raw, namespace).map(Some)
    }
}

fn optional_u16(raw: &str, label: &str) -> Result<Option<u16>, String> {
    if raw.trim().is_empty() || raw.trim() == "0" {
        return Ok(None);
    }
    raw.trim()
        .parse::<u16>()
        .map(Some)
        .map_err(|_| format!("{label} non valido: {raw}"))
}

fn parse_positive_u32(raw: &str, label: &str) -> Result<u32, String> {
    parse_positive_u32_opt(raw).ok_or_else(|| format!("{label} non valido: {raw}"))
}

fn parse_positive_u32_opt(raw: &str) -> Option<u32> {
    let value = raw.trim().parse::<u32>().ok()?;
    (value > 0).then_some(value)
}

fn nonempty(raw: &str) -> Option<String> {
    let value = raw.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn normalize_name(raw: &str) -> String {
    normalize_space(raw).to_lowercase()
}
fn normalize_space(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn event_key(code: &str) -> String {
    code.to_ascii_lowercase()
}
fn path_string(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("Percorso T'Pol non sicuro: {relative}"));
    }
    Ok(root.join(path))
}

fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_bytes_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Percorso senza directory padre")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("Creazione {}: {e}", path.display()))?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

fn write_json_replace<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path.parent().ok_or("Percorso JSON senza directory padre")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(
        ".{}-{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("tpol.json"),
        std::process::id()
    ));
    if temp.exists() {
        fs::remove_file(&temp).map_err(|e| e.to_string())?;
    }
    let bytes = canonical_json_bytes(value)?;
    write_bytes_new(&temp, &bytes)?;
    fs::rename(&temp, path).map_err(|e| e.to_string())
}

fn write_jsonl_new<T: Serialize>(path: &Path, values: &[T]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("Creazione {}: {e}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    for value in values {
        serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
        writer.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    writer.flush().map_err(|e| e.to_string())
}

fn read_json_regular<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T, String> {
    let bytes = read_regular_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{label} JSON non valido: {e}"))
}

fn read_regular_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let meta =
        fs::symlink_metadata(path).map_err(|e| format!("Metadati {}: {e}", path.display()))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(format!("File T'Pol non regolare: {}", path.display()));
    }
    fs::read(path).map_err(|e| format!("Lettura {}: {e}", path.display()))
}

fn validate_blake3(value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err("BLAKE3 non valido".into())
    }
}

fn unix_time_ms() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(body: &str) -> Vec<u8> {
        format!("<html><body>{body}</body></html>").into_bytes()
    }

    #[test]
    fn mixed_decoder_and_cells_preserve_half_and_latin1() {
        let input =
            b"<tr><td>Giocatore Caf\xe8</td><td>\xc2\xbd</td><td>&nbsp; A &amp; B </td></tr>";
        let rows = table_rows(input);
        assert_eq!(rows[0], vec!["Giocatore Cafè", "½", "A & B"]);
    }

    #[test]
    fn synthetic_three_view_context_is_reciprocal_and_deterministic() {
        let participants = wrap(
            r#"
<table><tr><td></td><td>1</td><td></td><td>CM</td><td>Alpha One</td><td>ITA</td><td>TOS</td><td>FI</td><td>2000</td><td></td><td>1900</td><td>2000</td><td>1990</td><td>M</td><td>111</td><td>211</td></tr>
<tr><td></td><td>2</td><td></td><td>1N</td><td>Beta Two</td><td>ITA</td><td>TOS</td><td>FI</td><td>1800</td><td></td><td>1800</td><td>1800</td><td>1991</td><td>M</td><td>112</td><td>212</td></tr></table>"#,
        );
        let results = wrap(
            r#"
<table><tr><td>Abbinamenti e risultati del turno 1</td></tr>
<tr><td></td><td>1</td><td>1</td><td></td><td>CM</td><td>Alpha One</td><td></td><td>ITA</td><td>2000</td><td>2</td><td></td><td>1N</td><td>Beta Two</td><td></td><td>ITA</td><td>1800</td><td>1</td><td>-</td><td>0</td><td></td></tr></table>"#,
        );
        let cross = wrap(
            r#"
<table><tr><td></td><td>1</td><td>1</td><td></td><td>CM</td><td>Alpha One</td><td>1</td><td>ITA</td><td></td><td>2</td><td>B</td><td>1</td><td></td><td></td><td>1</td></tr>
<tr><td></td><td>2</td><td>2</td><td></td><td>1N</td><td>Beta Two</td><td>0</td><td>ITA</td><td></td><td>1</td><td>N</td><td>0</td><td></td><td></td><td>-1</td></tr></table>"#,
        );
        let hash = "a".repeat(64);
        let roster = parse_participants(&participants, "TEST1", &hash).unwrap();
        let (pairings, rounds, excluded) = parse_results(&results).unwrap();
        assert_eq!(rounds, 1);
        assert_eq!(excluded, 0);
        let board = parse_crosstable(&cross, rounds).unwrap();
        let event = EventMetadata {
            name: "Synthetic Event".into(),
            start_date: "2025.01.01".into(),
            end_date: "2025.01.01".into(),
        };
        let source_hashes = ContextSourceHashes {
            roster: &hash,
            results: &hash,
            crosstable: &hash,
        };
        let first =
            reconcile_contexts("TEST1", &event, &roster, &pairings, &board, source_hashes).unwrap();
        let second =
            reconcile_contexts("TEST1", &event, &roster, &pairings, &board, source_hashes).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].white_fide_id.as_deref(), Some("111"));
        assert_eq!(first[0].black_fsi_id.as_deref(), Some("212"));
        assert_eq!(first[0].evidence_class, "official_context_join");
    }

    #[test]
    fn reciprocal_mismatch_fails_closed() {
        let hash = "b".repeat(64);
        let participants = BTreeMap::from([
            (
                1,
                TpolParticipantV1 {
                    profile: INDEX_PROFILE.into(),
                    event_code: "T".into(),
                    slot: 1,
                    raw_name: "A A".into(),
                    federation: None,
                    birth_year: None,
                    fide_id: Some("11".into()),
                    fsi_id: None,
                    roster_source_blake3: hash.clone(),
                },
            ),
            (
                2,
                TpolParticipantV1 {
                    profile: INDEX_PROFILE.into(),
                    event_code: "T".into(),
                    slot: 2,
                    raw_name: "B B".into(),
                    federation: None,
                    birth_year: None,
                    fide_id: Some("12".into()),
                    fsi_id: None,
                    roster_source_blake3: hash.clone(),
                },
            ),
        ]);
        let pairings = vec![RawPairing {
            round: 1,
            board: 1,
            white_slot: 1,
            black_slot: 2,
            white_name: "A A".into(),
            black_name: "B B".into(),
            result: "1-0".into(),
        }];
        let cross = BTreeMap::from([
            (
                1,
                CrossPlayer {
                    name: "A A".into(),
                    rounds: vec![Some(CrossRound {
                        opponent: 2,
                        color: 'B',
                        score: "1".into(),
                    })],
                },
            ),
            (
                2,
                CrossPlayer {
                    name: "B B".into(),
                    rounds: vec![Some(CrossRound {
                        opponent: 1,
                        color: 'B',
                        score: "0".into(),
                    })],
                },
            ),
        ]);
        let event = EventMetadata {
            name: "Synthetic Event".into(),
            start_date: "2025.01.01".into(),
            end_date: "2025.01.01".into(),
        };
        let source_hashes = ContextSourceHashes {
            roster: &hash,
            results: &hash,
            crosstable: &hash,
        };
        assert!(
            reconcile_contexts("T", &event, &participants, &pairings, &cross, source_hashes,)
                .is_err()
        );
    }
}
