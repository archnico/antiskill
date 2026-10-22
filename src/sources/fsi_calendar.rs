use super::event_candidates::{
    parse_calendar_date, EventCandidateRecord, EVENT_CANDIDATE_SCHEMA_VERSION,
};
use super::snapshot::{store_snapshot, SnapshotMetadata};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const SOURCE_KEY: &str = "fsi-calendar";
const DEFAULT_URL: &str = "https://www.federscacchi.com/fsi/index.php/calendario/calendario?tipo_evento=&den=&reg=&pro=&dtiniric=&dtfinric=&ord=1&senso=Asc&ric=1";
const RIGHTS_CLASS: &str = "official-internal-use";

#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    root: PathBuf,
    url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IndexConfig {
    snapshot: PathBuf,
    snapshot_ref: String,
    output: PathBuf,
    summary: PathBuf,
    base_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidatesConfig {
    index: PathBuf,
    output: PathBuf,
    report: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputFormat {
    Terminal,
    Jsonl,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct QueryConfig {
    index: PathBuf,
    from: Option<CalendarDate>,
    to: Option<CalendarDate>,
    region: Option<String>,
    province: Option<String>,
    category: Option<String>,
    format: OutputFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiffConfig {
    old: PathBuf,
    new: PathBuf,
    format: OutputFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StressConfig {
    fixture: PathBuf,
    report: PathBuf,
    iterations: u64,
    seed: u64,
}

impl OutputFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "terminal" => Ok(Self::Terminal),
            "jsonl" => Ok(Self::Jsonl),
            _ => Err(format!(
                "Formato sconosciuto: {value}. Valori ammessi: terminal, jsonl"
            )),
        }
    }
}

impl QueryConfig {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut index = None;
        let mut from = None;
        let mut to = None;
        let mut region = None;
        let mut province = None;
        let mut category = None;
        let mut format = OutputFormat::Terminal;
        let mut args = args.into_iter();

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--index" => {
                    index = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --index")?,
                    ));
                }
                "--from" => {
                    let value = args.next().ok_or("Manca il valore di --from")?;
                    from =
                        Some(CalendarDate::parse(&value).ok_or_else(|| {
                            format!("--from non valido: {value}. Usa GG-MM-AAAA")
                        })?);
                }
                "--to" => {
                    let value = args.next().ok_or("Manca il valore di --to")?;
                    to = Some(
                        CalendarDate::parse(&value)
                            .ok_or_else(|| format!("--to non valido: {value}. Usa GG-MM-AAAA"))?,
                    );
                }
                "--region" => region = Some(args.next().ok_or("Manca il valore di --region")?),
                "--province" => {
                    province = Some(args.next().ok_or("Manca il valore di --province")?)
                }
                "--category" => {
                    category = Some(args.next().ok_or("Manca il valore di --category")?)
                }
                "--format" => {
                    format =
                        OutputFormat::parse(&args.next().ok_or("Manca il valore di --format")?)?;
                }
                other => return Err(format!("Opzione sconosciuta: {other}")),
            }
        }

        if matches!((&from, &to), (Some(from), Some(to)) if from > to) {
            return Err("--from non può essere successivo a --to".to_owned());
        }
        for (option, value) in [
            ("--region", region.as_deref()),
            ("--province", province.as_deref()),
            ("--category", category.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(format!("{option} non può essere vuoto"));
            }
        }

        Ok(Self {
            index: index.ok_or("Parametro obbligatorio: --index")?,
            from,
            to,
            region,
            province,
            category,
            format,
        })
    }
}

impl DiffConfig {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut old = None;
        let mut new = None;
        let mut format = OutputFormat::Terminal;
        let mut args = args.into_iter();

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--old" => {
                    old = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --old")?,
                    ));
                }
                "--new" => {
                    new = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --new")?,
                    ));
                }
                "--format" => {
                    format =
                        OutputFormat::parse(&args.next().ok_or("Manca il valore di --format")?)?;
                }
                other => return Err(format!("Opzione sconosciuta: {other}")),
            }
        }

        Ok(Self {
            old: old.ok_or("Parametro obbligatorio: --old")?,
            new: new.ok_or("Parametro obbligatorio: --new")?,
            format,
        })
    }
}

impl StressConfig {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut fixture = None;
        let mut report = None;
        let mut iterations = None;
        let mut seed = None;
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--fixture" => {
                    fixture = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --fixture")?,
                    ));
                }
                "--report" => {
                    report = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --report")?,
                    ));
                }
                "--iterations" => {
                    iterations = Some(
                        args.next()
                            .ok_or("Manca il valore di --iterations")?
                            .parse::<u64>()
                            .map_err(|error| format!("--iterations non valido: {error}"))?,
                    );
                }
                "--seed" => {
                    seed = Some(
                        args.next()
                            .ok_or("Manca il valore di --seed")?
                            .parse::<u64>()
                            .map_err(|error| format!("--seed non valido: {error}"))?,
                    );
                }
                other => return Err(format!("Opzione sconosciuta: {other}")),
            }
        }

        let iterations = iterations.ok_or("Parametro obbligatorio: --iterations")?;
        if iterations == 0 {
            return Err("--iterations deve essere maggiore di zero".to_owned());
        }
        Ok(Self {
            fixture: fixture.ok_or("Parametro obbligatorio: --fixture")?,
            report: report.ok_or("Parametro obbligatorio: --report")?,
            iterations,
            seed: seed.ok_or("Parametro obbligatorio: --seed")?,
        })
    }
}

impl CandidatesConfig {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut index = None;
        let mut output = None;
        let mut report = None;
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--index" => {
                    index = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --index")?,
                    ));
                }
                "--output" => {
                    output = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --output")?,
                    ));
                }
                "--report" => {
                    report = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --report")?,
                    ));
                }
                other => return Err(format!("Opzione sconosciuta: {other}")),
            }
        }

        Ok(Self {
            index: index.ok_or("Parametro obbligatorio: --index")?,
            output: output.ok_or("Parametro obbligatorio: --output")?,
            report: report.ok_or("Parametro obbligatorio: --report")?,
        })
    }
}

impl IndexConfig {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut snapshot = None;
        let mut snapshot_ref = None;
        let mut output = None;
        let mut summary = None;
        let mut base_url = None;
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--snapshot" => {
                    snapshot = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --snapshot")?,
                    ));
                }
                "--snapshot-ref" => {
                    snapshot_ref = Some(args.next().ok_or("Manca il valore di --snapshot-ref")?);
                }
                "--output" => {
                    output = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --output")?,
                    ));
                }
                "--summary" => {
                    summary = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --summary")?,
                    ));
                }
                "--base-url" => {
                    base_url = Some(args.next().ok_or("Manca il valore di --base-url")?);
                }
                other => return Err(format!("Opzione sconosciuta: {other}")),
            }
        }

        let snapshot_ref = snapshot_ref.ok_or("Parametro obbligatorio: --snapshot-ref")?;
        if snapshot_ref.trim().is_empty() {
            return Err("--snapshot-ref non può essere vuoto".to_owned());
        }
        if matches!(base_url.as_deref(), Some("")) {
            return Err("--base-url non può essere vuoto".to_owned());
        }

        Ok(Self {
            snapshot: snapshot.ok_or("Parametro obbligatorio: --snapshot")?,
            snapshot_ref,
            output: output.ok_or("Parametro obbligatorio: --output")?,
            summary: summary.ok_or("Parametro obbligatorio: --summary")?,
            base_url,
        })
    }
}

impl Config {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut root = None;
        let mut url = DEFAULT_URL.to_owned();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--root" => {
                    root = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --root")?,
                    ));
                }
                "--url" => url = args.next().ok_or("Manca il valore di --url")?,
                other => return Err(format!("Opzione sconosciuta: {other}")),
            }
        }

        let root = root.ok_or("Parametro obbligatorio: --root")?;
        if url.trim().is_empty() {
            return Err("--url non può essere vuoto".to_owned());
        }

        Ok(Self { root, url })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CalendarEvent {
    pub source_id: String,
    pub name: String,
    pub starts_on: String,
    pub ends_on: Option<String>,
    pub region: String,
    pub event_type: String,
    pub province: String,
    pub location: String,
    pub notes: String,
    pub email: String,
    pub website_url: String,
    pub announcement_url: String,
    pub arbiters: String,
    pub time_control: String,
    pub sections: String,
    pub rounds: String,
    pub double_round_days: String,
    pub submitted_on: String,
    pub organizer: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct CalendarIndexRecord {
    schema_version: u32,
    source_key: String,
    source_id: String,
    title: String,
    starts_on: String,
    ends_on: Option<String>,
    location: Option<String>,
    province: Option<String>,
    region: Option<String>,
    organizer: Option<String>,
    event_type: Option<String>,
    event_url: Option<String>,
    attachment_urls: Vec<String>,
    #[serde(default)]
    classifications: Vec<EventClassification>,
    snapshot_ref: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct EventClassification {
    category: String,
    detail: Option<String>,
    evidence: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CalendarDate {
    year: u32,
    month: u32,
    day: u32,
}

impl CalendarDate {
    fn parse(value: &str) -> Option<Self> {
        let (year, month, day) = parse_calendar_date(value)?;
        Some(Self { year, month, day })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ChangedField {
    field: &'static str,
    old: serde_json::Value,
    new: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CalendarChange {
    Added,
    Removed,
    Modified,
    Unchanged,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct CalendarDiffRecord {
    change: CalendarChange,
    source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    record: Option<CalendarIndexRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    old: Option<CalendarIndexRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new: Option<CalendarIndexRecord>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fields: Vec<ChangedField>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ClassificationExample {
    source_event_id: String,
    title: String,
    evidence: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
struct ClassificationStats {
    events: u64,
    examples: Vec<ClassificationExample>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
struct ClassificationReport {
    events_analyzed: u64,
    classified_events: u64,
    unclassified_events: u64,
    by_category: BTreeMap<String, ClassificationStats>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct CollisionExample {
    candidate_match_key: String,
    source_event_ids: Vec<String>,
    source_titles: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
struct MatchCandidateReport {
    events_analyzed: u64,
    candidate_keys: u64,
    unique_keys: u64,
    collision_keys: u64,
    events_in_collisions: u64,
    events_without_sufficient_data: u64,
    collision_examples: Vec<CollisionExample>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
struct CalendarIndexSummary {
    events_read: u64,
    events_exported: u64,
    duplicate_identifiers: u64,
    events_without_identifier: u64,
    events_with_missing_essential_fields: u64,
    parsing_errors: u64,
    classifications: ClassificationReport,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CalendarAnalysis {
    events: Vec<CalendarEvent>,
    summary: CalendarIndexSummary,
}

const STRESS_EXAMPLE_LIMIT: usize = 5;
const STRESS_PROGRESS_INTERVAL: u64 = 10_000;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum MutationKind {
    WhitespaceAndLines,
    HtmlEntitiesAndUnicode,
    ReorderedAttributes,
    MissingOptionalTags,
    TruncatedFields,
    DuplicatedBlocks,
    RelativeAndAbsoluteLinks,
    PartiallyMalformedMarkup,
}

impl MutationKind {
    const ALL: [Self; 8] = [
        Self::WhitespaceAndLines,
        Self::HtmlEntitiesAndUnicode,
        Self::ReorderedAttributes,
        Self::MissingOptionalTags,
        Self::TruncatedFields,
        Self::DuplicatedBlocks,
        Self::RelativeAndAbsoluteLinks,
        Self::PartiallyMalformedMarkup,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct StressExample {
    iteration: u64,
    mutation: MutationKind,
    outcome: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct StressReport {
    report_version: u32,
    fixture_blake3: String,
    fixture_bytes: u64,
    seed: u64,
    iterations_requested: u64,
    iterations_completed: u64,
    successful_parses: u64,
    parser_errors: u64,
    panics: u64,
    mutation_counts: BTreeMap<String, u64>,
    outcome_counts: BTreeMap<String, u64>,
    parsed_event_count_min: Option<u64>,
    parsed_event_count_max: Option<u64>,
    examples: Vec<StressExample>,
}

impl StressReport {
    fn new(fixture: &[u8], seed: u64, iterations: u64) -> Self {
        Self {
            report_version: 1,
            fixture_blake3: blake3::hash(fixture).to_hex().to_string(),
            fixture_bytes: fixture.len() as u64,
            seed,
            iterations_requested: iterations,
            iterations_completed: 0,
            successful_parses: 0,
            parser_errors: 0,
            panics: 0,
            mutation_counts: BTreeMap::new(),
            outcome_counts: BTreeMap::new(),
            parsed_event_count_min: None,
            parsed_event_count_max: None,
            examples: Vec::new(),
        }
    }

    fn count_outcome(&mut self, key: &str) {
        *self.outcome_counts.entry(key.to_owned()).or_default() += 1;
    }

    fn example(&mut self, iteration: u64, mutation: MutationKind, outcome: &str) {
        let existing = self
            .examples
            .iter()
            .filter(|example| example.outcome == outcome)
            .count();
        if existing < STRESS_EXAMPLE_LIMIT {
            self.examples.push(StressExample {
                iteration,
                mutation,
                outcome: outcome.to_owned(),
            });
        }
    }
}

pub(crate) fn print_help() {
    println!(
        "\
antiskill sources sync fsi-calendar

Scarica il calendario ufficiale FSI e conserva l'HTML byte-per-byte in uno
snapshot content-addressed. Il comando non interpreta né filtra gli eventi.

USO:
    antiskill sources sync fsi-calendar --root DIR [--url URL]

OUTPUT:
    raw/objects/blake3/       oggetti immutabili deduplicati
    raw/manifests/            un manifest per ogni acquisizione
    raw/refs/                 riferimento aggiornabile all'ultima acquisizione
"
    );
}

fn print_index_help() {
    println!(
        "\
antiskill sources index fsi-calendar

Legge uno snapshot HTML locale del calendario FSI e produce un indice JSONL
deterministico, senza accesso di rete.

USO:
    antiskill sources index fsi-calendar \\
        --snapshot FILE --snapshot-ref REF \\
        --output FILE --summary FILE [--base-url URL]

--base-url viene usato soltanto per risolvere URL relativi presenti nell'HTML.
I file di output devono essere nuovi.
"
    );
}

fn print_candidates_help() {
    println!(
        "\
antiskill sources candidates fsi-calendar

Legge un indice JSONL FSI locale e produce chiavi deterministiche utili
soltanto per generare candidati di collegamento.

USO:
    antiskill sources candidates fsi-calendar \\
        --index FILE --output FILE --report FILE

I file di output devono essere nuovi. Nessuna chiave prodotta prova l'identità
di due eventi.
"
    );
}

fn print_query_help() {
    println!(
        "\
antiskill sources query fsi-calendar

Interroga offline un indice JSONL del calendario FSI. I filtri sono combinabili;
gli estremi temporali sono inclusivi e gli eventi su più giorni sono selezionati
quando intersecano l'intervallo.

USO:
    antiskill sources query fsi-calendar --index FILE \\
        [--from GG-MM-AAAA] [--to GG-MM-AAAA] \\
        [--region TESTO] [--province TESTO] [--category TESTO] \\
        [--format terminal|jsonl]

Il formato predefinito è terminal. L'output JSONL conserva esattamente i campi
dell'indice, compresi i valori null. Regione, provincia e categoria richiedono
una corrispondenza completa, senza distinzione tra maiuscole e minuscole ASCII.
"
    );
}

fn print_diff_help() {
    println!(
        "\
antiskill sources diff fsi-calendar

Confronta offline due indici JSONL del calendario FSI usando source_id.
Gli eventi sono classificati come aggiunti, rimossi, modificati o invariati.
Il confronto dei contenuti esclude snapshot_ref; i riferimenti originali
restano comunque nell'output JSONL.

USO:
    antiskill sources diff fsi-calendar --old FILE --new FILE \\
        [--format terminal|jsonl]

Il formato predefinito è terminal. Un source_id duplicato rende ambiguo il
confronto: il comando segnala tutte le righe duplicate e non scarta record.
"
    );
}

fn print_stress_help() {
    println!(
        "\
antiskill sources stress fsi-calendar

Esegue mutazioni deterministiche in-process di una fixture HTML locale e
produce un rapporto JSON riproducibile. Non effettua accessi di rete.

USO:
    antiskill sources stress fsi-calendar \\
        --fixture FILE --iterations N --seed N --report FILE

Il rapporto deve essere un file nuovo.
"
    );
}

pub(crate) fn run_query<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_query_help();
        return Ok(());
    }

    let config = QueryConfig::from_args(args)?;
    let index = fs::read(&config.index)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.index.display()))?;
    let output = query_calendar_index(&index, &config)?;
    write_stdout(&output, "risultati query")
}

pub(crate) fn run_diff<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_diff_help();
        return Ok(());
    }

    let config = DiffConfig::from_args(args)?;
    let old = fs::read(&config.old)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.old.display()))?;
    let new = fs::read(&config.new)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.new.display()))?;
    let output = diff_calendar_indexes(&old, &new, config.format)?;
    write_stdout(&output, "confronto indici")
}

pub(crate) fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_help();
        return Ok(());
    }

    let config = Config::from_args(args)?;
    let fetched_at_unix_ms = unix_time_ms()?;
    let user_agent = format!(
        "antiskill/{} (+https://github.com/archnico/antiskill)",
        env!("CARGO_PKG_VERSION")
    );
    let mut response = fetch_calendar(&config.url, &user_agent)?;

    let metadata = SnapshotMetadata {
        source_key: SOURCE_KEY.to_owned(),
        source_url: config.url,
        fetched_at_unix_ms,
        http_status: response.status().as_u16(),
        content_type: response_header(&response, "content-type"),
        content_length_header: response_header(&response, "content-length")
            .and_then(|value| value.parse::<u64>().ok()),
        etag: response_header(&response, "etag"),
        last_modified: response_header(&response, "last-modified"),
        content_disposition: response_header(&response, "content-disposition"),
        rights_class: RIGHTS_CLASS.to_owned(),
        retrieved_by: user_agent,
    };

    let stored = store_snapshot(&config.root, metadata, response.body_mut().as_reader())?;

    println!("SOURCE\t{}", stored.manifest.source_key);
    println!(
        "STATUS\t{}",
        if stored.manifest.new_object {
            "nuovo"
        } else {
            "invariato"
        }
    );
    println!("BLAKE3\t{}", stored.manifest.blake3);
    println!("BYTES\t{}", stored.manifest.bytes);
    println!("OBJECT\t{}", stored.object_path.display());
    println!("MANIFEST\t{}", stored.manifest_path.display());

    let html = std::fs::read(&stored.object_path).map_err(|error| {
        format!(
            "Snapshot salvato ma non rileggibile da {}: {error}",
            stored.object_path.display()
        )
    })?;
    let events = parse_calendar(&html).map_err(|error| {
        format!(
            "Snapshot conservato in {}, parsing interrotto: {error}",
            stored.object_path.display()
        )
    })?;
    println!("EVENTS\t{}", events.len());

    Ok(())
}

pub(crate) fn run_index<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_index_help();
        return Ok(());
    }

    let config = IndexConfig::from_args(args)?;
    if config.output == config.summary {
        return Err("--output e --summary devono indicare file distinti".to_owned());
    }
    ensure_new_output(&config.output)?;
    ensure_new_output(&config.summary)?;

    let html = fs::read(&config.snapshot)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.snapshot.display()))?;
    let (jsonl, summary) = build_index(&html, &config.snapshot_ref, config.base_url.as_deref())?;
    write_new(&config.output, &jsonl)?;
    let mut summary_json = serde_json::to_vec_pretty(&summary)
        .map_err(|error| format!("Errore serializzando il riepilogo: {error}"))?;
    summary_json.push(b'\n');
    write_new(&config.summary, &summary_json)?;

    println!("EVENTS_READ\t{}", summary.events_read);
    println!("EVENTS_EXPORTED\t{}", summary.events_exported);
    println!("DUPLICATE_IDENTIFIERS\t{}", summary.duplicate_identifiers);
    println!(
        "EVENTS_WITHOUT_IDENTIFIER\t{}",
        summary.events_without_identifier
    );
    println!(
        "EVENTS_MISSING_ESSENTIAL\t{}",
        summary.events_with_missing_essential_fields
    );
    println!("PARSING_ERRORS\t{}", summary.parsing_errors);
    println!("OUTPUT\t{}", config.output.display());
    println!("SUMMARY\t{}", config.summary.display());

    Ok(())
}

pub(crate) fn run_candidates<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_candidates_help();
        return Ok(());
    }

    let config = CandidatesConfig::from_args(args)?;
    if config.output == config.report {
        return Err("--output e --report devono indicare file distinti".to_owned());
    }
    ensure_new_output(&config.output)?;
    ensure_new_output(&config.report)?;

    let index = fs::read(&config.index)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.index.display()))?;
    let (jsonl, report) = build_match_candidates(&index)?;
    write_new(&config.output, &jsonl)?;
    let mut report_json = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("Errore serializzando il rapporto candidati: {error}"))?;
    report_json.push(b'\n');
    write_new(&config.report, &report_json)?;

    println!("EVENTS_ANALYZED\t{}", report.events_analyzed);
    println!("CANDIDATE_KEYS\t{}", report.candidate_keys);
    println!("UNIQUE_KEYS\t{}", report.unique_keys);
    println!("COLLISION_KEYS\t{}", report.collision_keys);
    println!("EVENTS_IN_COLLISIONS\t{}", report.events_in_collisions);
    println!(
        "EVENTS_WITHOUT_SUFFICIENT_DATA\t{}",
        report.events_without_sufficient_data
    );
    println!("OUTPUT\t{}", config.output.display());
    println!("REPORT\t{}", config.report.display());

    Ok(())
}

pub(crate) fn run_stress<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")) {
        print_stress_help();
        return Ok(());
    }

    let config = StressConfig::from_args(args)?;
    ensure_new_output(&config.report)?;
    let fixture = fs::read(&config.fixture)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.fixture.display()))?;
    let original_hash = blake3::hash(&fixture);

    println!("FIXTURE\t{}", config.fixture.display());
    println!("FIXTURE_BLAKE3\t{original_hash}");
    println!("ITERATIONS\t{}", config.iterations);
    println!("SEED\t{}", config.seed);
    println!("PROCESS_MODEL\tin-process");
    let started = Instant::now();
    let report = stress_calendar(&fixture, config.iterations, config.seed, |completed| {
        let elapsed = started.elapsed().as_secs_f64();
        let rate = if elapsed > 0.0 {
            completed as f64 / elapsed
        } else {
            0.0
        };
        println!("PROGRESS\t{completed}\t{elapsed:.3}s\t{rate:.2}/s");
    });

    if blake3::hash(&fixture) != original_hash {
        return Err("La fixture in memoria è stata modificata".to_owned());
    }
    let mut json = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("Errore serializzando il rapporto stress: {error}"))?;
    json.push(b'\n');
    write_new(&config.report, &json)?;

    let elapsed = started.elapsed().as_secs_f64();
    println!("COMPLETED\t{}", report.iterations_completed);
    println!("SUCCESSFUL_PARSES\t{}", report.successful_parses);
    println!("PARSER_ERRORS\t{}", report.parser_errors);
    println!("PANICS\t{}", report.panics);
    println!("ELAPSED_SECONDS\t{elapsed:.3}");
    println!(
        "RATE_PER_SECOND\t{:.2}",
        report.iterations_completed as f64 / elapsed
    );
    println!("REPORT\t{}", config.report.display());
    Ok(())
}

fn stress_calendar<F>(fixture: &[u8], iterations: u64, seed: u64, mut progress: F) -> StressReport
where
    F: FnMut(u64),
{
    let mut report = StressReport::new(fixture, seed, iterations);
    let mut rng = SplitMix64(seed);

    for iteration in 0..iterations {
        let kind = MutationKind::ALL[iteration as usize % MutationKind::ALL.len()];
        let mutated = mutate_fixture(fixture, kind, iteration, &mut rng);
        let mutation_key = serde_json::to_value(kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| format!("{kind:?}"));
        *report.mutation_counts.entry(mutation_key).or_default() += 1;

        match catch_unwind(AssertUnwindSafe(|| parse_calendar(&mutated))) {
            Ok(Ok(events)) => {
                let count = events.len() as u64;
                report.successful_parses += 1;
                report.count_outcome("success");
                report.parsed_event_count_min = Some(
                    report
                        .parsed_event_count_min
                        .map_or(count, |old| old.min(count)),
                );
                report.parsed_event_count_max = Some(
                    report
                        .parsed_event_count_max
                        .map_or(count, |old| old.max(count)),
                );
            }
            Ok(Err(error)) => {
                let outcome = classify_stress_error(&error);
                report.parser_errors += 1;
                report.count_outcome(outcome);
                report.example(iteration, kind, outcome);
            }
            Err(_) => {
                report.panics += 1;
                report.count_outcome("panic");
                report.example(iteration, kind, "panic");
            }
        }

        report.iterations_completed += 1;
        if report
            .iterations_completed
            .is_multiple_of(STRESS_PROGRESS_INTERVAL)
        {
            progress(report.iterations_completed);
        }
    }
    report.examples.sort_by(|left, right| {
        left.outcome
            .cmp(&right.outcome)
            .then_with(|| left.iteration.cmp(&right.iteration))
    });
    report
}

fn classify_stress_error(error: &str) -> &'static str {
    if error.starts_with("HTML FSI non valido come UTF-8") {
        "invalid_utf8"
    } else if error.starts_with("Nessun evento FSI riconoscibile") {
        "no_recognizable_events"
    } else if error.ends_with("eventi FSI non analizzabili") {
        "unparseable_events"
    } else {
        "other_parser_error"
    }
}

#[derive(Clone, Copy)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn index(&mut self, len: usize) -> usize {
        if len == 0 {
            0
        } else {
            self.next() as usize % len
        }
    }
}

fn mutate_fixture(
    fixture: &[u8],
    kind: MutationKind,
    iteration: u64,
    rng: &mut SplitMix64,
) -> Vec<u8> {
    let input = String::from_utf8_lossy(fixture);
    match kind {
        MutationKind::WhitespaceAndLines => mutate_selected(&input, "><", rng, |_| {
            format!(">\n {}<", " ".repeat((iteration as usize % 7) + 1))
        }),
        MutationKind::HtmlEntitiesAndUnicode => {
            let alternatives = [
                ("&nbsp;", "&#160;"),
                ("&amp;", "&#38;"),
                ("à", "a\u{0300}"),
                ("è", "e\u{0300}"),
            ];
            let (from, to) = alternatives[rng.index(alternatives.len())];
            mutate_selected(&input, from, rng, |_| to.to_owned())
        }
        MutationKind::ReorderedAttributes => reorder_selected_attributes(&input, rng),
        MutationKind::MissingOptionalTags => {
            let tags = ["</div>", "</a>", "</span>"];
            let tag = tags[rng.index(tags.len())];
            mutate_selected(&input, tag, rng, |_| String::new())
        }
        MutationKind::TruncatedFields => truncate_selected_field(&input, rng),
        MutationKind::DuplicatedBlocks => duplicate_selected_event_block(&input, rng),
        MutationKind::RelativeAndAbsoluteLinks => mutate_selected_link(&input, iteration, rng),
        MutationKind::PartiallyMalformedMarkup => {
            let tokens = [">", "\"", "</div>"];
            let token = tokens[rng.index(tokens.len())];
            mutate_selected(&input, token, rng, |_| String::new())
        }
    }
    .into_bytes()
}

fn match_positions(input: &str, needle: &str) -> Vec<usize> {
    input
        .match_indices(needle)
        .map(|(index, _)| index)
        .collect()
}

fn mutate_selected<F>(input: &str, needle: &str, rng: &mut SplitMix64, replacement: F) -> String
where
    F: FnOnce(&str) -> String,
{
    let positions = match_positions(input, needle);
    let Some(start) = positions.get(rng.index(positions.len())).copied() else {
        return input.to_owned();
    };
    let mut output = String::with_capacity(input.len() + 16);
    output.push_str(&input[..start]);
    output.push_str(&replacement(needle));
    output.push_str(&input[start + needle.len()..]);
    output
}

fn reorder_selected_attributes(input: &str, rng: &mut SplitMix64) -> String {
    let tags: Vec<(usize, usize)> = input
        .match_indices("<a ")
        .filter_map(|(start, _)| {
            let end = input[start..].find('>').map(|offset| start + offset + 1)?;
            (attribute_spans(&input[start..end]).len() >= 2).then_some((start, end))
        })
        .collect();
    let Some((start, end)) = tags.get(rng.index(tags.len())).copied() else {
        return input.to_owned();
    };
    let tag = &input[start..end];
    let spans = attribute_spans(tag);
    let (first_start, first_end) = spans[0];
    let (second_start, second_end) = spans[1];
    let mut reordered = String::with_capacity(tag.len());
    reordered.push_str(&tag[..first_start]);
    reordered.push_str(&tag[second_start..second_end]);
    reordered.push(' ');
    reordered.push_str(&tag[first_start..first_end]);
    reordered.push_str(&tag[second_end..]);

    let mut output = String::with_capacity(input.len());
    output.push_str(&input[..start]);
    output.push_str(&reordered);
    output.push_str(&input[end..]);
    output
}

fn attribute_spans(tag: &str) -> Vec<(usize, usize)> {
    let bytes = tag.as_bytes();
    let mut spans = Vec::new();
    let mut index = tag.find(char::is_whitespace).unwrap_or(tag.len());
    while index < bytes.len() {
        while matches!(bytes.get(index), Some(byte) if byte.is_ascii_whitespace()) {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] == b'>' {
            break;
        }
        let start = index;
        let mut quote = None;
        while index < bytes.len() {
            match (quote, bytes[index]) {
                (None, b'"' | b'\'') => quote = Some(bytes[index]),
                (Some(delimiter), byte) if byte == delimiter => quote = None,
                (None, byte) if byte.is_ascii_whitespace() || byte == b'>' => break,
                _ => {}
            }
            index += 1;
        }
        spans.push((start, index));
    }
    spans
}

fn truncate_selected_field(input: &str, rng: &mut SplitMix64) -> String {
    let labels = [
        "Immissione:",
        "Provincia:",
        "Luogo:",
        "Organizzatore:",
        "Bando:",
    ];
    let label = labels[rng.index(labels.len())];
    let positions = match_positions(input, label);
    let Some(start) = positions.get(rng.index(positions.len())).copied() else {
        return input.to_owned();
    };
    let value_start = start + label.len();
    let end = input[value_start..]
        .find('<')
        .map(|offset| value_start + offset)
        .unwrap_or(input.len());
    let keep = (end - value_start) / 2;
    let mut output = String::with_capacity(input.len());
    output.push_str(&input[..value_start + keep]);
    output.push_str(&input[end..]);
    output
}

fn duplicate_selected_event_block(input: &str, rng: &mut SplitMix64) -> String {
    let starts: Vec<usize> = input.match_indices("<h1").map(|(index, _)| index).collect();
    let Some(position) = starts.get(rng.index(starts.len())).copied() else {
        return input.to_owned();
    };
    let end = starts
        .iter()
        .copied()
        .find(|candidate| *candidate > position)
        .unwrap_or(input.len());
    let block = &input[position..end];
    let mut output = String::with_capacity(input.len() + block.len());
    output.push_str(&input[..end]);
    output.push_str(block);
    output.push_str(&input[end..]);
    output
}

fn mutate_selected_link(input: &str, iteration: u64, rng: &mut SplitMix64) -> String {
    let positions = match_positions(input, "href=");
    let Some(start) = positions.get(rng.index(positions.len())).copied() else {
        return input.to_owned();
    };
    let value_start = start + "href=".len();
    let quote = input.as_bytes().get(value_start).copied();
    let (content_start, content_end) = match quote {
        Some(b'"' | b'\'') => {
            let delimiter = quote.unwrap() as char;
            let content_start = value_start + 1;
            let content_end = input[content_start..]
                .find(delimiter)
                .map(|offset| content_start + offset)
                .unwrap_or(content_start);
            (content_start, content_end)
        }
        _ => {
            let content_end = input[value_start..]
                .find(|character: char| character.is_whitespace() || character == '>')
                .map(|offset| value_start + offset)
                .unwrap_or(input.len());
            (value_start, content_end)
        }
    };
    let replacement = if iteration.is_multiple_of(2) {
        format!("/fsi/stress/{iteration}")
    } else {
        format!("https://example.invalid/fsi/stress/{iteration}")
    };
    let mut output = String::with_capacity(input.len() + replacement.len());
    output.push_str(&input[..content_start]);
    output.push_str(&replacement);
    output.push_str(&input[content_end..]);
    output
}

fn ensure_new_output(path: &Path) -> Result<(), String> {
    if path.exists() {
        Err(format!("Il file di output esiste già: {}", path.display()))
    } else {
        Ok(())
    }
}

fn write_new(path: &Path, contents: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("Impossibile creare {}: {error}", path.display()))?;
    file.write_all(contents)
        .map_err(|error| format!("Impossibile scrivere {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("Impossibile sincronizzare {}: {error}", path.display()))
}

fn fetch_calendar(url: &str, user_agent: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    ureq::get(url)
        .header("User-Agent", user_agent)
        .header("Accept", "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1")
        .header("Accept-Encoding", "identity")
        .call()
        .map_err(|error| format!("Download di {SOURCE_KEY} fallito: {error}"))
}

fn response_header(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

pub(crate) fn parse_calendar(html: &[u8]) -> Result<Vec<CalendarEvent>, String> {
    let analysis = analyze_calendar(html)?;
    if analysis.events.is_empty() {
        return Err("Nessun evento FSI riconoscibile nello snapshot".to_owned());
    }
    if analysis.summary.parsing_errors != 0 {
        return Err(format!(
            "{} eventi FSI non analizzabili",
            analysis.summary.parsing_errors
        ));
    }
    Ok(analysis.events)
}

fn analyze_calendar(html: &[u8]) -> Result<CalendarAnalysis, String> {
    let html = std::str::from_utf8(html)
        .map_err(|error| format!("HTML FSI non valido come UTF-8: {error}"))?;
    let mut analysis = CalendarAnalysis::default();

    for block in split_ascii_case_insensitive(html, "<h1") {
        let text = html_to_lines(block);
        let dates = parse_dates(&text);
        let submission_text = field(&text, "Immissione:");
        let is_event = submission_text.is_some()
            || (dates.is_some()
                && (field(&text, "Provincia:").is_some()
                    || field(&text, "Luogo:").is_some()
                    || field(&text, "Immissione:").is_some()));
        if !is_event {
            continue;
        }
        analysis.summary.events_read += 1;

        let submission = submission_text
            .as_deref()
            .and_then(|value| parse_submission(value).ok());
        if submission.is_none() {
            analysis.summary.events_without_identifier += 1;
        }
        let name = text.first().cloned().unwrap_or_default();
        let Some((submitted_on, source_id)) = submission else {
            analysis.summary.events_with_missing_essential_fields += 1;
            analysis.summary.parsing_errors += 1;
            continue;
        };
        let Some((starts_on, ends_on, date_index)) = dates else {
            analysis.summary.events_with_missing_essential_fields += 1;
            analysis.summary.parsing_errors += 1;
            continue;
        };
        if name.is_empty() {
            analysis.summary.events_with_missing_essential_fields += 1;
            analysis.summary.parsing_errors += 1;
            continue;
        }
        let (region, event_type) =
            region_and_type(&text, date_index).unwrap_or_else(|| (String::new(), String::new()));

        analysis.events.push(CalendarEvent {
            source_id,
            name,
            starts_on,
            ends_on,
            region,
            event_type,
            province: field(&text, "Provincia:").unwrap_or_default(),
            location: field(&text, "Luogo:").unwrap_or_default(),
            notes: field(&text, "Note:").unwrap_or_default(),
            email: link_after_label(block, "E-mail:")
                .or_else(|| link_after_label(block, "Email:"))
                .and_then(|link| link.strip_prefix("mailto:").map(str::to_owned))
                .unwrap_or_default(),
            website_url: link_after_label(block, "Sito:").unwrap_or_default(),
            announcement_url: link_after_label(block, "Bando:").unwrap_or_default(),
            arbiters: field(&text, "Arbitri:").unwrap_or_default(),
            time_control: field(&text, "Tempo riflessione:").unwrap_or_default(),
            sections: field(&text, "Tornei:").unwrap_or_default(),
            rounds: field(&text, "Turni:").unwrap_or_default(),
            double_round_days: field(&text, "Giorni con doppi turni:").unwrap_or_default(),
            submitted_on,
            organizer: field(&text, "Organizzatore:")
                .or_else(|| field(&text, "Organizzazione:"))
                .unwrap_or_default(),
        });
    }

    let mut identifiers = HashMap::<&str, u64>::new();
    for event in &analysis.events {
        *identifiers.entry(&event.source_id).or_default() += 1;
    }
    analysis.summary.duplicate_identifiers =
        identifiers.values().filter(|count| **count > 1).count() as u64;
    analysis.summary.events_exported = analysis.events.len() as u64;

    Ok(analysis)
}

fn build_index(
    html: &[u8],
    snapshot_ref: &str,
    base_url: Option<&str>,
) -> Result<(Vec<u8>, CalendarIndexSummary), String> {
    let analysis = analyze_calendar(html)?;
    let mut records: Vec<CalendarIndexRecord> = analysis
        .events
        .iter()
        .map(|event| CalendarIndexRecord {
            schema_version: 1,
            source_key: SOURCE_KEY.to_owned(),
            source_id: event.source_id.clone(),
            title: event.name.clone(),
            starts_on: event.starts_on.clone(),
            ends_on: event.ends_on.clone(),
            location: nonempty(&event.location),
            province: nonempty(&event.province),
            region: nonempty(&event.region),
            organizer: nonempty(&event.organizer),
            event_type: nonempty(&event.event_type),
            event_url: nonempty(&event.website_url).map(|url| normalize_url(&url, base_url)),
            attachment_urls: nonempty(&event.announcement_url)
                .map(|url| vec![normalize_url(&url, base_url)])
                .unwrap_or_default(),
            classifications: classify_event_title(&event.name),
            snapshot_ref: snapshot_ref.to_owned(),
        })
        .collect();
    records.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then_with(|| left.starts_on.cmp(&right.starts_on))
            .then_with(|| left.title.cmp(&right.title))
    });

    let mut summary = analysis.summary;
    summary.classifications = build_classification_report(&records);
    let mut jsonl = Vec::new();
    for record in records {
        serde_json::to_writer(&mut jsonl, &record)
            .map_err(|error| format!("Errore serializzando evento FSI: {error}"))?;
        jsonl.push(b'\n');
    }

    Ok((jsonl, summary))
}

fn classify_event_title(title: &str) -> Vec<EventClassification> {
    let mut classifications = Vec::new();

    let mut youth_evidence = Vec::new();
    if let Some(evidence) = word_evidence(title, "CIG") {
        youth_evidence.push(evidence);
    }
    let youth = word_evidence(title, "GIOVANILE").or_else(|| word_evidence(title, "GIOVANILI"));
    let under_18 = word_evidence(title, "U18").or_else(|| phrase_evidence(title, "UNDER 18", true));
    if let (Some(youth), Some(under_18)) = (youth, under_18) {
        youth_evidence.extend([youth, under_18]);
    }
    push_classification(&mut classifications, "cig_u18", None, youth_evidence);

    if let Some(cis) = word_evidence(title, "CIS") {
        let series = [
            ("SERIE MASTER", "master"),
            ("SERIE A1", "a1"),
            ("SERIE A2", "a2"),
            ("SERIE A", "a"),
            ("SERIE B", "b"),
            ("SERIE C", "c"),
            ("SERIE PROMOZIONE", "promozione"),
        ]
        .into_iter()
        .find_map(|(phrase, detail)| {
            phrase_evidence(title, phrase, true).map(|evidence| (detail, evidence))
        });
        let mut evidence = vec![cis];
        let detail = series.map(|(detail, series_evidence)| {
            evidence.push(series_evidence);
            detail.to_owned()
        });
        push_classification(&mut classifications, "cis", detail, evidence);
    }

    let cia = word_evidence(title, "CIA")
        .or_else(|| phrase_evidence(title, "CAMPIONATO ITALIANO ASSOLUTO", true));
    if let Some(cia) = cia {
        if let Some(stage) = word_evidence(title, "SEMIFINALE") {
            push_classification(
                &mut classifications,
                "cia_semifinal",
                None,
                vec![cia.clone(), stage],
            );
        }
        if let Some(stage) = word_evidence(title, "FINALE") {
            push_classification(&mut classifications, "cia_final", None, vec![cia, stage]);
        }
    }

    for (category, term) in [("rapid", "RAPID"), ("blitz", "BLITZ")] {
        if let Some(evidence) = word_evidence(title, term) {
            push_classification(&mut classifications, category, None, vec![evidence]);
        }
    }

    let school_evidence = word_evidence(title, "SCOLASTICO")
        .or_else(|| word_evidence(title, "SCOLASTICA"))
        .or_else(|| phrase_evidence(title, "SCACCHI SCUOLA", true));
    if let Some(evidence) = school_evidence {
        push_classification(&mut classifications, "school", None, vec![evidence]);
    }

    let international =
        word_evidence(title, "INTERNAZIONALE").or_else(|| word_evidence(title, "INTERNATIONAL"));
    if let Some(evidence) = international {
        push_classification(&mut classifications, "international", None, vec![evidence]);
    }

    let championship =
        word_evidence(title, "CAMPIONATO").or_else(|| word_evidence(title, "CAMPIONATI"));
    if let Some(championship) = championship {
        if let Some(scope) =
            word_evidence(title, "REGIONALE").or_else(|| word_evidence(title, "REGIONALI"))
        {
            push_classification(
                &mut classifications,
                "regional_championship",
                None,
                vec![championship.clone(), scope],
            );
        }
        if let Some(scope) =
            word_evidence(title, "PROVINCIALE").or_else(|| word_evidence(title, "PROVINCIALI"))
        {
            push_classification(
                &mut classifications,
                "provincial_championship",
                None,
                vec![championship, scope],
            );
        }
    }

    if classifications.is_empty() {
        classifications.push(EventClassification {
            category: "unclassified".to_owned(),
            detail: None,
            evidence: Vec::new(),
        });
    }
    classifications
}

fn push_classification(
    classifications: &mut Vec<EventClassification>,
    category: &str,
    detail: Option<String>,
    mut evidence: Vec<String>,
) {
    if evidence.is_empty() {
        return;
    }
    evidence.sort();
    evidence.dedup();
    classifications.push(EventClassification {
        category: category.to_owned(),
        detail,
        evidence,
    });
}

fn word_evidence(title: &str, word: &str) -> Option<String> {
    phrase_evidence(title, word, true)
}

fn phrase_evidence(title: &str, phrase: &str, require_boundaries: bool) -> Option<String> {
    let lower = title.to_ascii_lowercase();
    let phrase_lower = phrase.to_ascii_lowercase();
    lower
        .match_indices(&phrase_lower)
        .find_map(|(start, matched)| {
            let end = start + matched.len();
            let before = title[..start].chars().next_back();
            let after = title[end..].chars().next();
            let boundaries_match = !require_boundaries
                || (before.is_none_or(|character| !character.is_alphanumeric())
                    && after.is_none_or(|character| !character.is_alphanumeric()));
            boundaries_match.then(|| title[start..end].to_owned())
        })
}

fn build_classification_report(records: &[CalendarIndexRecord]) -> ClassificationReport {
    let mut report = ClassificationReport {
        events_analyzed: records.len() as u64,
        ..ClassificationReport::default()
    };
    for record in records {
        let classified = record
            .classifications
            .iter()
            .any(|classification| classification.category != "unclassified");
        if classified {
            report.classified_events += 1;
        } else {
            report.unclassified_events += 1;
        }
        for classification in &record.classifications {
            let stats = report
                .by_category
                .entry(classification.category.clone())
                .or_default();
            stats.events += 1;
            stats.examples.push(ClassificationExample {
                source_event_id: record.source_id.clone(),
                title: record.title.clone(),
                evidence: classification.evidence.clone(),
            });
        }
    }
    for stats in report.by_category.values_mut() {
        stats.examples.sort_by(|left, right| {
            left.source_event_id
                .cmp(&right.source_event_id)
                .then_with(|| left.title.cmp(&right.title))
        });
        stats.examples.truncate(5);
    }
    report
}

fn query_calendar_index(index_jsonl: &[u8], config: &QueryConfig) -> Result<Vec<u8>, String> {
    let mut records = read_calendar_index(index_jsonl)?;
    records.retain(|record| record_matches_query(record, config));
    sort_calendar_records(&mut records);

    match config.format {
        OutputFormat::Jsonl => serialize_jsonl(&records, "risultato query FSI"),
        OutputFormat::Terminal => {
            let mut output =
                b"SOURCE_ID\tDATE\tEND_DATE\tTITLE\tREGION\tPROVINCE\tCATEGORIES\n".to_vec();
            for record in records {
                let categories = record
                    .classifications
                    .iter()
                    .map(|classification| classification.category.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                let row = [
                    terminal_cell(&record.source_id),
                    terminal_cell(&record.starts_on),
                    terminal_cell(record.ends_on.as_deref().unwrap_or_default()),
                    terminal_cell(&record.title),
                    terminal_cell(record.region.as_deref().unwrap_or_default()),
                    terminal_cell(record.province.as_deref().unwrap_or_default()),
                    terminal_cell(&categories),
                ]
                .join("\t");
                output.extend_from_slice(row.as_bytes());
                output.push(b'\n');
            }
            Ok(output)
        }
    }
}

fn record_matches_query(record: &CalendarIndexRecord, config: &QueryConfig) -> bool {
    let Some(start) = CalendarDate::parse(&record.starts_on) else {
        return false;
    };
    let end = record
        .ends_on
        .as_deref()
        .and_then(CalendarDate::parse)
        .unwrap_or(start);
    let intersects_from = config.from.is_none_or(|from| end >= from);
    let intersects_to = config.to.is_none_or(|to| start <= to);
    let region_matches = option_matches(record.region.as_deref(), config.region.as_deref());
    let province_matches = option_matches(record.province.as_deref(), config.province.as_deref());
    let category_matches = config.category.as_deref().is_none_or(|wanted| {
        record
            .classifications
            .iter()
            .any(|classification| text_matches(&classification.category, wanted))
    });

    intersects_from && intersects_to && region_matches && province_matches && category_matches
}

fn option_matches(actual: Option<&str>, wanted: Option<&str>) -> bool {
    wanted.is_none_or(|wanted| actual.is_some_and(|actual| text_matches(actual, wanted)))
}

fn text_matches(actual: &str, wanted: &str) -> bool {
    actual.trim().eq_ignore_ascii_case(wanted.trim())
}

fn diff_calendar_indexes(
    old_jsonl: &[u8],
    new_jsonl: &[u8],
    format: OutputFormat,
) -> Result<Vec<u8>, String> {
    let old_records = read_calendar_index_with_lines(old_jsonl)?;
    let new_records = read_calendar_index_with_lines(new_jsonl)?;
    let duplicate_errors = [
        ("vecchio", duplicate_source_ids(&old_records)),
        ("nuovo", duplicate_source_ids(&new_records)),
    ]
    .into_iter()
    .filter(|(_, duplicates)| !duplicates.is_empty())
    .map(|(label, duplicates)| format!("Indice {label}: {}", duplicates.join("; ")))
    .collect::<Vec<_>>();
    if !duplicate_errors.is_empty() {
        return Err(format!(
            "source_id duplicati: {}",
            duplicate_errors.join(" | ")
        ));
    }
    let old_records = unique_calendar_index(old_records);
    let new_records = unique_calendar_index(new_records);
    let mut ids = old_records
        .keys()
        .chain(new_records.keys())
        .cloned()
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();

    let mut differences = Vec::with_capacity(ids.len());
    for source_id in ids {
        let old = old_records.get(&source_id);
        let new = new_records.get(&source_id);
        let difference = match (old, new) {
            (None, Some(record)) => CalendarDiffRecord {
                change: CalendarChange::Added,
                source_id,
                record: None,
                old: None,
                new: Some(record.clone()),
                fields: Vec::new(),
            },
            (Some(record), None) => CalendarDiffRecord {
                change: CalendarChange::Removed,
                source_id,
                record: None,
                old: Some(record.clone()),
                new: None,
                fields: Vec::new(),
            },
            (Some(old), Some(new)) => {
                let fields = changed_calendar_fields(old, new)?;
                if fields.is_empty() {
                    CalendarDiffRecord {
                        change: CalendarChange::Unchanged,
                        source_id,
                        record: Some(new.clone()),
                        old: None,
                        new: None,
                        fields,
                    }
                } else {
                    CalendarDiffRecord {
                        change: CalendarChange::Modified,
                        source_id,
                        record: None,
                        old: Some(old.clone()),
                        new: Some(new.clone()),
                        fields,
                    }
                }
            }
            (None, None) => unreachable!("identificatore proveniente dall'unione delle chiavi"),
        };
        differences.push(difference);
    }

    match format {
        OutputFormat::Jsonl => serialize_jsonl(&differences, "differenza calendario FSI"),
        OutputFormat::Terminal => format_terminal_diff(&differences),
    }
}

fn duplicate_source_ids(records: &[(usize, CalendarIndexRecord)]) -> Vec<String> {
    let mut lines_by_id = BTreeMap::<String, Vec<usize>>::new();
    for (line, record) in records {
        lines_by_id
            .entry(record.source_id.clone())
            .or_default()
            .push(*line);
    }
    lines_by_id
        .iter()
        .filter(|(_, lines)| lines.len() > 1)
        .map(|(source_id, lines)| {
            format!(
                "{source_id} (righe {})",
                lines
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect()
}

fn unique_calendar_index(
    records: Vec<(usize, CalendarIndexRecord)>,
) -> BTreeMap<String, CalendarIndexRecord> {
    records
        .into_iter()
        .map(|(_, record)| (record.source_id.clone(), record))
        .collect()
}

fn read_calendar_index(input: &[u8]) -> Result<Vec<CalendarIndexRecord>, String> {
    Ok(read_calendar_index_with_lines(input)?
        .into_iter()
        .map(|(_, record)| record)
        .collect())
}

fn read_calendar_index_with_lines(
    input: &[u8],
) -> Result<Vec<(usize, CalendarIndexRecord)>, String> {
    let mut records = Vec::new();
    for (line_index, line) in input.split(|byte| *byte == b'\n').enumerate() {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let record: CalendarIndexRecord = serde_json::from_slice(line).map_err(|error| {
            format!(
                "Indice JSONL non valido alla riga {}: {error}",
                line_index + 1
            )
        })?;
        if record.schema_version != 1 {
            return Err(format!(
                "Versione indice FSI non supportata alla riga {}: {}",
                line_index + 1,
                record.schema_version
            ));
        }
        if record.source_key != SOURCE_KEY {
            return Err(format!(
                "Sorgente incompatibile alla riga {}: {}",
                line_index + 1,
                record.source_key
            ));
        }
        if record.source_id.trim().is_empty() {
            return Err(format!(
                "source_id mancante o vuoto alla riga {}",
                line_index + 1
            ));
        }
        let start = CalendarDate::parse(&record.starts_on).ok_or_else(|| {
            format!(
                "Data iniziale non valida alla riga {}: {}",
                line_index + 1,
                record.starts_on
            )
        })?;
        if let Some(value) = &record.ends_on {
            let end = CalendarDate::parse(value).ok_or_else(|| {
                format!(
                    "Data finale non valida alla riga {}: {value}",
                    line_index + 1
                )
            })?;
            if end < start {
                return Err(format!(
                    "Intervallo temporale incoerente alla riga {}: {} - {value}",
                    line_index + 1,
                    record.starts_on
                ));
            }
        }
        records.push((line_index + 1, record));
    }
    Ok(records)
}

fn changed_calendar_fields(
    old: &CalendarIndexRecord,
    new: &CalendarIndexRecord,
) -> Result<Vec<ChangedField>, String> {
    const COMPARED_FIELDS: &[&str] = &[
        "title",
        "starts_on",
        "ends_on",
        "location",
        "province",
        "region",
        "organizer",
        "event_type",
        "event_url",
        "attachment_urls",
        "classifications",
    ];
    let old = serde_json::to_value(old)
        .map_err(|error| format!("Errore serializzando il vecchio evento: {error}"))?;
    let new = serde_json::to_value(new)
        .map_err(|error| format!("Errore serializzando il nuovo evento: {error}"))?;

    Ok(COMPARED_FIELDS
        .iter()
        .filter_map(|field| {
            let old_value = old.get(field)?;
            let new_value = new.get(field)?;
            (old_value != new_value).then(|| ChangedField {
                field,
                old: old_value.clone(),
                new: new_value.clone(),
            })
        })
        .collect())
}

fn format_terminal_diff(records: &[CalendarDiffRecord]) -> Result<Vec<u8>, String> {
    let mut output = b"CHANGE\tSOURCE_ID\tTITLE\tDETAILS\n".to_vec();
    for record in records {
        let change = match record.change {
            CalendarChange::Added => "added",
            CalendarChange::Removed => "removed",
            CalendarChange::Modified => "modified",
            CalendarChange::Unchanged => "unchanged",
        };
        let details = if record.fields.is_empty() {
            String::new()
        } else {
            record
                .fields
                .iter()
                .map(|field| {
                    let old = serde_json::to_string(&field.old).map_err(|error| {
                        format!("Errore serializzando valore precedente: {error}")
                    })?;
                    let new = serde_json::to_string(&field.new)
                        .map_err(|error| format!("Errore serializzando valore nuovo: {error}"))?;
                    Ok(format!("{}: {old} -> {new}", field.field))
                })
                .collect::<Result<Vec<_>, String>>()?
                .join("; ")
        };
        let title = record
            .record
            .as_ref()
            .or(record.new.as_ref())
            .or(record.old.as_ref())
            .map(|event| event.title.as_str())
            .unwrap_or_default();
        output.extend_from_slice(
            format!(
                "{change}\t{}\t{}\t{}\n",
                terminal_cell(&record.source_id),
                terminal_cell(title),
                terminal_cell(&details)
            )
            .as_bytes(),
        );
    }
    Ok(output)
}

fn serialize_jsonl<T: Serialize>(records: &[T], label: &str) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    for record in records {
        serde_json::to_writer(&mut output, record)
            .map_err(|error| format!("Errore serializzando {label}: {error}"))?;
        output.push(b'\n');
    }
    Ok(output)
}

fn sort_calendar_records(records: &mut [CalendarIndexRecord]) {
    records.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then_with(|| left.starts_on.cmp(&right.starts_on))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.snapshot_ref.cmp(&right.snapshot_ref))
            .then_with(|| left.cmp(right))
    });
}

fn terminal_cell(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\t' | '\n' | '\r' => ' ',
            other => other,
        })
        .collect()
}

fn write_stdout(output: &[u8], label: &str) -> Result<(), String> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    writer
        .write_all(output)
        .map_err(|error| format!("Impossibile scrivere {label}: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("Impossibile completare {label}: {error}"))
}

fn build_match_candidates(index_jsonl: &[u8]) -> Result<(Vec<u8>, MatchCandidateReport), String> {
    let mut report = MatchCandidateReport::default();
    let mut candidates = Vec::new();

    for (line_index, line) in index_jsonl.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        report.events_analyzed += 1;
        let record: CalendarIndexRecord = serde_json::from_slice(line).map_err(|error| {
            format!(
                "Indice JSONL non valido alla riga {}: {error}",
                line_index + 1
            )
        })?;
        if record.source_key != SOURCE_KEY {
            return Err(format!(
                "Sorgente incompatibile alla riga {}: {}",
                line_index + 1,
                record.source_key
            ));
        }

        let normalized_title = normalize_match_text(&record.title);
        if normalized_title.is_empty() || record.starts_on.is_empty() {
            report.events_without_sufficient_data += 1;
            continue;
        }
        if record.classifications.is_empty() {
            return Err(format!(
                "Indice FSI senza classificazione alla riga {}",
                line_index + 1
            ));
        }
        let canonical_match_string = canonical_match_string(&record, &normalized_title);
        let candidate_match_key = format!(
            "candidate-v1:blake3:{}",
            blake3::hash(canonical_match_string.as_bytes()).to_hex()
        );
        let (event_categories, classification_evidence) =
            candidate_classification(&record.classifications);

        candidates.push(EventCandidateRecord {
            schema_version: EVENT_CANDIDATE_SCHEMA_VERSION,
            key_kind: "candidate-only".to_owned(),
            source_key: record.source_key,
            source_event_id: record.source_id,
            source_title: record.title,
            normalized_title,
            starts_on: record.starts_on,
            ends_on: record.ends_on,
            location: record.location,
            province: record.province,
            region: record.region,
            source_url: record.event_url,
            event_categories,
            classification_evidence,
            canonical_match_string,
            candidate_match_key,
            snapshot_ref: record.snapshot_ref,
        });
    }

    candidates.sort_by(|left, right| {
        left.source_event_id
            .cmp(&right.source_event_id)
            .then_with(|| left.starts_on.cmp(&right.starts_on))
            .then_with(|| left.source_title.cmp(&right.source_title))
    });

    let mut groups = BTreeMap::<String, Vec<&EventCandidateRecord>>::new();
    for candidate in &candidates {
        groups
            .entry(candidate.candidate_match_key.clone())
            .or_default()
            .push(candidate);
    }
    report.candidate_keys = groups.len() as u64;
    report.unique_keys = groups.values().filter(|events| events.len() == 1).count() as u64;
    report.collision_keys = groups.values().filter(|events| events.len() > 1).count() as u64;
    report.events_in_collisions = groups
        .values()
        .filter(|events| events.len() > 1)
        .map(|events| events.len() as u64)
        .sum();
    report.collision_examples = groups
        .into_iter()
        .filter(|(_, events)| events.len() > 1)
        .take(10)
        .map(|(candidate_match_key, events)| CollisionExample {
            candidate_match_key,
            source_event_ids: events
                .iter()
                .map(|event| event.source_event_id.clone())
                .collect(),
            source_titles: events
                .iter()
                .map(|event| event.source_title.clone())
                .collect(),
        })
        .collect();

    let mut jsonl = Vec::new();
    for candidate in candidates {
        serde_json::to_writer(&mut jsonl, &candidate)
            .map_err(|error| format!("Errore serializzando candidato FSI: {error}"))?;
        jsonl.push(b'\n');
    }

    Ok((jsonl, report))
}

fn candidate_classification(
    classifications: &[EventClassification],
) -> (Vec<String>, BTreeMap<String, Vec<String>>) {
    let mut evidence = BTreeMap::<String, Vec<String>>::new();
    for classification in classifications {
        evidence
            .entry(classification.category.clone())
            .or_default()
            .extend(classification.evidence.iter().cloned());
    }
    for values in evidence.values_mut() {
        values.sort();
        values.dedup();
    }
    let categories = evidence.keys().cloned().collect();
    (categories, evidence)
}

fn canonical_match_string(record: &CalendarIndexRecord, normalized_title: &str) -> String {
    let mut parts = vec![
        format!("title={normalized_title}"),
        format!("start={}", record.starts_on),
    ];
    if let Some(end) = &record.ends_on {
        parts.push(format!("end={end}"));
    }
    for (label, value) in [
        ("location", record.location.as_deref()),
        ("province", record.province.as_deref()),
        ("region", record.region.as_deref()),
    ] {
        if let Some(value) = value {
            parts.push(format!("{label}={}", normalize_match_text(value)));
        }
    }
    parts.join(" | ")
}

pub(crate) fn normalize_match_text(value: &str) -> String {
    let unicode_equivalent = compose_common_latin(value);
    let mut normalized = String::with_capacity(unicode_equivalent.len());
    let mut needs_space = false;

    for character in unicode_equivalent.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() || matches!(character, '°' | '+' | '&') {
            if needs_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(character);
            needs_space = false;
        } else {
            needs_space = true;
        }
    }

    normalized
}

fn compose_common_latin(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        let Some(combining) = characters.peek().copied() else {
            output.push(character);
            continue;
        };
        let composed = match (character, combining) {
            ('a', '\u{300}') => Some('à'),
            ('a', '\u{301}') => Some('á'),
            ('e', '\u{300}') => Some('è'),
            ('e', '\u{301}') => Some('é'),
            ('i', '\u{300}') => Some('ì'),
            ('i', '\u{301}') => Some('í'),
            ('o', '\u{300}') => Some('ò'),
            ('o', '\u{301}') => Some('ó'),
            ('u', '\u{300}') => Some('ù'),
            ('u', '\u{301}') => Some('ú'),
            ('A', '\u{300}') => Some('À'),
            ('A', '\u{301}') => Some('Á'),
            ('E', '\u{300}') => Some('È'),
            ('E', '\u{301}') => Some('É'),
            ('I', '\u{300}') => Some('Ì'),
            ('I', '\u{301}') => Some('Í'),
            ('O', '\u{300}') => Some('Ò'),
            ('O', '\u{301}') => Some('Ó'),
            ('U', '\u{300}') => Some('Ù'),
            ('U', '\u{301}') => Some('Ú'),
            _ => None,
        };
        if let Some(composed) = composed {
            output.push(composed);
            characters.next();
        } else {
            output.push(character);
        }
    }
    output
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn normalize_url(value: &str, base_url: Option<&str>) -> String {
    if value.starts_with("http://") || value.starts_with("https://") || base_url.is_none() {
        return value.to_owned();
    }

    let base = base_url.expect("base verificata").trim_end_matches('/');
    if value.starts_with("//") {
        let scheme = base
            .split_once("://")
            .map(|(scheme, _)| scheme)
            .unwrap_or("https");
        format!("{scheme}:{value}")
    } else if value.starts_with('/') {
        let origin = base
            .split_once("://")
            .map(|(scheme, rest)| {
                let authority = rest.split('/').next().unwrap_or(rest);
                format!("{scheme}://{authority}")
            })
            .unwrap_or_else(|| base.to_owned());
        format!("{origin}{value}")
    } else {
        format!("{base}/{value}")
    }
}

fn split_ascii_case_insensitive<'a>(input: &'a str, marker: &str) -> Vec<&'a str> {
    let lower = input.to_ascii_lowercase();
    let marker = marker.to_ascii_lowercase();
    let starts: Vec<usize> = lower
        .match_indices(&marker)
        .map(|(index, _)| index)
        .collect();

    starts
        .iter()
        .enumerate()
        .map(|(position, start)| {
            let end = starts.get(position + 1).copied().unwrap_or(input.len());
            &input[*start..end]
        })
        .collect()
}

fn html_to_lines(html: &str) -> Vec<String> {
    let mut text = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'<' && looks_like_tag(bytes, index) {
            let end = bytes[index..]
                .iter()
                .position(|byte| *byte == b'>')
                .map(|offset| index + offset)
                .unwrap_or(bytes.len() - 1);
            let tag = html[index + 1..end].trim().to_ascii_lowercase();
            if tag.starts_with("br")
                || tag.starts_with("/h1")
                || tag.starts_with("/h2")
                || tag.starts_with("/h4")
                || tag.starts_with("/div")
            {
                text.push('\n');
            }
            index = end + 1;
        } else {
            let character = html[index..].chars().next().expect("indice UTF-8 valido");
            text.push(character);
            index += character.len_utf8();
        }
    }

    decode_entities(&text)
        .lines()
        .map(normalize_space)
        .filter(|line| !line.is_empty())
        .collect()
}

fn looks_like_tag(bytes: &[u8], index: usize) -> bool {
    matches!(
        bytes.get(index + 1),
        Some(b'a'..=b'z' | b'A'..=b'Z' | b'/' | b'!' | b'?')
    )
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
}

fn normalize_space(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn field(lines: &[String], label: &str) -> Option<String> {
    lines.iter().find_map(|line| {
        line.get(..label.len())
            .filter(|prefix| prefix.eq_ignore_ascii_case(label))
            .map(|_| line[label.len()..].trim().to_owned())
    })
}

fn parse_dates(lines: &[String]) -> Option<(String, Option<String>, usize)> {
    lines.iter().enumerate().find_map(|(index, line)| {
        let dates: Vec<&str> = line
            .split(" - ")
            .map(str::trim)
            .filter(|value| is_date(value))
            .collect();
        match dates.as_slice() {
            [start] => Some(((*start).to_owned(), None, index)),
            [start, end] => Some(((*start).to_owned(), Some((*end).to_owned()), index)),
            _ => None,
        }
    })
}

fn is_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[2] == b'-'
        && bytes[5] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 2 | 5) || byte.is_ascii_digit())
}

fn region_and_type(lines: &[String], date_index: usize) -> Option<(String, String)> {
    let values: Vec<String> = lines
        .iter()
        .skip(date_index + 1)
        .filter(|line| !line.eq_ignore_ascii_case("Image"))
        .take_while(|line| !line.to_ascii_lowercase().starts_with("provincia:"))
        .take(2)
        .cloned()
        .collect();

    match values.as_slice() {
        [region, event_type] => Some((region.clone(), event_type.clone())),
        _ => None,
    }
}

fn parse_submission(value: &str) -> Result<(String, String), String> {
    let open = value
        .rfind('(')
        .ok_or_else(|| format!("Immissione FSI senza identificatore: {value}"))?;
    let close = value[open..]
        .find(')')
        .map(|offset| open + offset)
        .ok_or_else(|| format!("Immissione FSI senza parentesi chiusa: {value}"))?;
    let source_id = value[open + 1..close].trim();
    if source_id.is_empty() || !source_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("Identificatore evento FSI non valido: {source_id}"));
    }

    Ok((value[..open].trim().to_owned(), source_id.to_owned()))
}

fn link_after_label(html: &str, label: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let label_index = lower.find(&label.to_ascii_lowercase())?;
    let remainder = &html[label_index + label.len()..];
    let lower_remainder = &lower[label_index + label.len()..];
    let href_index = lower_remainder.find("href")?;
    let before_href = &lower_remainder[..href_index];
    if before_href.contains("</div") {
        return None;
    }

    let after_href = &remainder[href_index + "href".len()..];
    let value = after_href.trim_start().strip_prefix('=')?.trim_start();
    let (link, _) = match value.as_bytes().first()? {
        b'"' => value[1..].split_once('"')?,
        b'\'' => value[1..].split_once('\'')?,
        _ => {
            let end = value
                .find(|character: char| character.is_whitespace() || character == '>')
                .unwrap_or(value.len());
            (&value[..end], &value[end..])
        }
    };

    Some(decode_entities(link))
}

fn unix_time_ms() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("Orologio di sistema precedente a UNIX_EPOCH: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::event_candidates::validate_event_jsonl;

    const SYNTHETIC_FIXTURE: &[u8] =
        include_bytes!("../../tests/fixtures/fsi-calendar/calendar-synthetic.html");
    const QUERY_FIXTURE: &[u8] =
        include_bytes!("../../tests/fixtures/fsi-calendar/query-index.jsonl");
    const DIFF_OLD_FIXTURE: &[u8] =
        include_bytes!("../../tests/fixtures/fsi-calendar/diff-old.jsonl");
    const DIFF_NEW_FIXTURE: &[u8] =
        include_bytes!("../../tests/fixtures/fsi-calendar/diff-new.jsonl");

    fn query_config() -> QueryConfig {
        QueryConfig {
            index: PathBuf::from("fixture"),
            from: None,
            to: None,
            region: None,
            province: None,
            category: None,
            format: OutputFormat::Jsonl,
        }
    }

    fn jsonl_values(input: &[u8]) -> Vec<serde_json::Value> {
        input
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("record JSONL"))
            .collect()
    }

    fn calendar_jsonl(records: &[CalendarIndexRecord]) -> Vec<u8> {
        serialize_jsonl(records, "fixture calendario").expect("serializzazione fixture")
    }

    #[test]
    fn parses_required_root_and_optional_url() {
        let config = Config::from_args([
            "--root".to_owned(),
            "/tmp/archive".to_owned(),
            "--url".to_owned(),
            "https://example.invalid/calendar".to_owned(),
        ])
        .expect("configurazione valida");

        assert_eq!(config.root, PathBuf::from("/tmp/archive"));
        assert_eq!(config.url, "https://example.invalid/calendar");
    }

    #[test]
    fn parses_synthetic_calendar_without_geographic_filtering() {
        let events = parse_calendar(SYNTHETIC_FIXTURE).expect("parsing fixture FSI sintetica");

        assert_eq!(events.len(), 6);
        assert!(events.iter().any(|event| event.region == "TOSCANA"));
        assert!(events.iter().any(|event| event.region == "LAZIO"));
        assert!(events.iter().any(|event| event.region == "SICILIA"));

        let event = events
            .iter()
            .find(|event| event.source_id == "900001")
            .expect("evento stabile nella fixture");
        assert_eq!(event.name, "Coppa Aurora & Giovani Rapid");
        assert_eq!(event.starts_on, "04-09-2026");
        assert_eq!(event.ends_on.as_deref(), Some("06-09-2026"));
        assert_eq!(event.province, "Firenze");
        assert_eq!(event.location, "Padiglione Arcobaleno");
        assert_eq!(
            event.website_url,
            "https://calendar.example.invalid/events/900001"
        );
        assert_eq!(
            event.announcement_url,
            "https://calendar.example.invalid/attachments/900001.pdf"
        );
        assert_eq!(event.notes, "Fascia Elo < 1600 & laboratorio finale");

        let event_without_geography = events
            .iter()
            .find(|event| event.source_id == "900002")
            .expect("evento privo di geografia opzionale");
        assert_eq!(event_without_geography.starts_on, "12-09-2026");
        assert_eq!(event_without_geography.ends_on, None);
        assert!(event_without_geography.province.is_empty());
        assert!(event_without_geography.location.is_empty());
    }

    #[test]
    fn synthetic_fixture_index_is_offline_and_deterministic() {
        let snapshot_ref = "fixture:fsi-calendar-synthetic-v1";
        let first = build_index(SYNTHETIC_FIXTURE, snapshot_ref, None).expect("primo indice");
        let second = build_index(SYNTHETIC_FIXTURE, snapshot_ref, None).expect("secondo indice");

        assert_eq!(first, second);
        assert_eq!(first.1.events_read, 6);
        assert_eq!(first.1.events_exported, 6);
        assert_eq!(first.1.events_without_identifier, 0);
        assert_eq!(first.1.events_with_missing_essential_fields, 0);
        assert_eq!(first.1.parsing_errors, 0);
        assert_eq!(first.1.classifications.events_analyzed, 6);
        assert_eq!(first.1.classifications.classified_events, 4);
        assert_eq!(first.1.classifications.unclassified_events, 2);
        assert_eq!(first.0.iter().filter(|byte| **byte == b'\n').count(), 6);
        let records: Vec<serde_json::Value> = first
            .0
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("record indice JSON"))
            .collect();
        assert!(records.iter().all(|record| {
            !record["classifications"]
                .as_array()
                .expect("classificazioni array")
                .is_empty()
        }));
        assert_eq!(records[0]["source_id"], "900001");
        assert_eq!(records[0]["snapshot_ref"], snapshot_ref);
        assert_eq!(records[5]["source_id"], "900006");
    }

    #[test]
    fn synthetic_fixture_candidates_are_offline_and_deterministic() {
        let snapshot_ref = "fixture:fsi-calendar-synthetic-v1";
        let (index, _) =
            build_index(SYNTHETIC_FIXTURE, snapshot_ref, None).expect("indice fixture");
        let first = build_match_candidates(&index).expect("primi candidati");
        let second = build_match_candidates(&index).expect("secondi candidati");

        assert_eq!(first, second);
        assert_eq!(first.1.events_analyzed, 6);
        assert_eq!(first.1.events_without_sufficient_data, 0);
        assert_eq!(first.0.iter().filter(|byte| **byte == b'\n').count(), 6);
        assert_eq!(first.1.candidate_keys, 6);
        assert_eq!(first.1.unique_keys, 6);
        assert_eq!(first.1.collision_keys, 0);

        let validation: serde_json::Value = serde_json::from_slice(
            &validate_event_jsonl(&first.0).expect("validazione indice FSI"),
        )
        .expect("rapporto validazione JSON");
        assert_eq!(validation["total_records"], 6);
        assert_eq!(validation["deterministically_ordered"], true);
        assert_eq!(validation["invalid_source_urls"], 1);
        assert_eq!(
            validation["validation_errors"],
            validation["invalid_source_urls"]
        );
        let first_candidate: serde_json::Value = serde_json::from_slice(
            first
                .0
                .split(|byte| *byte == b'\n')
                .find(|line| !line.is_empty())
                .expect("primo candidato"),
        )
        .expect("candidato JSON");
        assert_eq!(first_candidate["schema_version"], 2);
        assert_eq!(first_candidate["source_event_id"], "900001");
        assert!(first_candidate["candidate_match_key"]
            .as_str()
            .expect("chiave candidata")
            .starts_with("candidate-v1:blake3:"));
        assert!(first_candidate["event_categories"].is_array());
        assert!(first_candidate["classification_evidence"].is_object());
    }

    #[test]
    fn conservative_title_normalization_preserves_discriminating_tokens() {
        assert_eq!(
            normalize_match_text("  Il  12° CIS—U18: Serie A  "),
            "il 12° cis u18 serie a"
        );
        assert_eq!(
            normalize_match_text("Citt\u{61}\u{300} d’Italia"),
            normalize_match_text("Città d'Italia")
        );
    }

    #[test]
    fn title_classification_is_conservative_and_keeps_evidence() {
        let categories = |title: &str| {
            classify_event_title(title)
                .into_iter()
                .map(|classification| classification.category)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            categories("Finale CIA Rapid Internazionale"),
            ["cia_final", "rapid", "international"]
        );
        assert_eq!(
            categories("Semifinale CIA - Campionato scolastico"),
            ["cia_semifinal", "school"]
        );
        let cis = classify_event_title("CIS Serie A2");
        assert_eq!(cis[0].category, "cis");
        assert_eq!(cis[0].detail.as_deref(), Some("a2"));
        assert_eq!(cis[0].evidence, ["CIS", "Serie A2"]);
        assert_eq!(
            categories("Campionato Regionale CIG U18 Blitz"),
            ["cig_u18", "blitz", "regional_championship"]
        );
        assert_eq!(
            categories("Campionato Provinciale Giovanile U18"),
            ["cig_u18", "provincial_championship"]
        );
        assert_eq!(categories("CISF Open"), ["unclassified"]);
        assert_eq!(categories("Open regionale lampo"), ["unclassified"]);
    }

    #[test]
    fn collision_report_groups_equal_candidate_strings() {
        let records = [
            CalendarIndexRecord {
                schema_version: 1,
                source_key: SOURCE_KEY.to_owned(),
                source_id: "10".to_owned(),
                title: "Open—Roma".to_owned(),
                starts_on: "01-09-2026".to_owned(),
                ends_on: None,
                location: Some("Roma".to_owned()),
                province: None,
                region: None,
                organizer: None,
                event_type: None,
                event_url: Some("https://example.invalid/10".to_owned()),
                attachment_urls: Vec::new(),
                classifications: vec![EventClassification {
                    category: "unclassified".to_owned(),
                    detail: None,
                    evidence: Vec::new(),
                }],
                snapshot_ref: "fixture".to_owned(),
            },
            CalendarIndexRecord {
                schema_version: 1,
                source_key: SOURCE_KEY.to_owned(),
                source_id: "11".to_owned(),
                title: "open roma".to_owned(),
                starts_on: "01-09-2026".to_owned(),
                ends_on: None,
                location: Some("ROMA".to_owned()),
                province: None,
                region: None,
                organizer: None,
                event_type: None,
                event_url: None,
                attachment_urls: Vec::new(),
                classifications: vec![EventClassification {
                    category: "unclassified".to_owned(),
                    detail: None,
                    evidence: Vec::new(),
                }],
                snapshot_ref: "fixture".to_owned(),
            },
        ];
        let mut jsonl = Vec::new();
        for record in records {
            serde_json::to_writer(&mut jsonl, &record).expect("record indice");
            jsonl.push(b'\n');
        }

        let (output, report) = build_match_candidates(&jsonl).expect("candidati");

        assert_eq!(report.events_analyzed, 2);
        assert_eq!(report.candidate_keys, 1);
        assert_eq!(report.unique_keys, 0);
        assert_eq!(report.collision_keys, 1);
        assert_eq!(report.events_in_collisions, 2);
        assert_eq!(report.collision_examples[0].source_event_ids, ["10", "11"]);
        let candidates: Vec<serde_json::Value> = output
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("candidato JSON"))
            .collect();
        assert_eq!(candidates[0]["key_kind"], "candidate-only");
        assert_eq!(candidates[0]["source_url"], "https://example.invalid/10");
        assert!(candidates[1]["source_url"].is_null());
    }

    #[test]
    fn detects_duplicate_and_missing_identifiers() {
        let html = br#"
            <h1>Primo evento</h1>
            <h2>01-08-2026</h2><h4>LAZIO</h4><h4>Rapid</h4>
            <div>Provincia: Roma</div><div>Immissione: 01-07-2026 (42)</div>
            <h1>Secondo evento</h1>
            <h2>02-08-2026</h2><h4>LAZIO</h4><h4>Blitz</h4>
            <div>Provincia: Roma</div><div>Immissione: 02-07-2026 (42)</div>
            <h1>Evento senza identificatore</h1>
            <h2>03-08-2026</h2><h4>LAZIO</h4><h4>Standard</h4>
            <div>Provincia: Roma</div>
            <h1>Evento senza data</h1>
            <h4>LAZIO</h4><h4>Standard</h4>
            <div>Provincia: Roma</div><div>Immissione: 04-07-2026 (43)</div>
        "#;

        let (_, summary) = build_index(html, "test-snapshot", None).expect("indice sintetico");

        assert_eq!(summary.events_read, 4);
        assert_eq!(summary.events_exported, 2);
        assert_eq!(summary.duplicate_identifiers, 1);
        assert_eq!(summary.events_without_identifier, 1);
        assert_eq!(summary.events_with_missing_essential_fields, 2);
        assert_eq!(summary.parsing_errors, 2);
    }

    #[test]
    fn synthetic_document_preserves_order_regions_entities_nesting_and_links() {
        let html = br#"
            <H1><span>Open &amp; Coppa</span></H1>
            <h2><span>01-08-2026</span></h2>
            <h4>LAZIO</h4><h4>Tipo non classificato</h4>
            <div>Provincia: Roma</div>
            <div>Luogo: <span>Castel&nbsp;Gandolfo</span></div>
            <div><strong>Sito:</strong> <a class="event" href="/eventi/101">scheda</a></div>
            <div><strong>Bando:</strong> <a href="https://docs.example/101.pdf">PDF</a></div>
            <div>Immissione: 01-07-2026 (101)</div>

            <h1><em>Secondo evento</em></h1>
            <h2>02-08-2026</h2>
            <h4>SICILIA</h4><h4>Altro Evento</h4>
            <div>Immissione: 02-07-2026 (102)</div>
        "#;

        let first = parse_calendar(html).expect("documento sintetico");
        let second = parse_calendar(html).expect("secondo parsing");
        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|event| event.source_id.as_str())
                .collect::<Vec<_>>(),
            ["101", "102"]
        );
        assert_eq!(first[0].name, "Open & Coppa");
        assert_eq!(first[0].region, "LAZIO");
        assert_eq!(first[0].location, "Castel Gandolfo");
        assert_eq!(first[0].website_url, "/eventi/101");
        assert_eq!(first[0].announcement_url, "https://docs.example/101.pdf");
        assert_eq!(first[1].region, "SICILIA");
        assert!(first[1].province.is_empty());
        assert!(first[1].location.is_empty());
        assert!(first[1].website_url.is_empty());

        let (jsonl, summary) = build_index(html, "synthetic", Some("https://fsi.example/calendar"))
            .expect("indice sintetico");
        let records: Vec<CalendarIndexRecord> = jsonl
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("record JSONL"))
            .collect();
        assert_eq!(summary.events_exported, 2);
        assert_eq!(
            records[0].event_url.as_deref(),
            Some("https://fsi.example/eventi/101")
        );
        assert_eq!(records[0].attachment_urls, ["https://docs.example/101.pdf"]);
        assert_eq!(records[1].location, None);
        assert_eq!(records[1].province, None);
        assert_eq!(records[1].event_url, None);
        assert_eq!(records[1].classifications[0].category, "unclassified");
        assert!(records[1].classifications[0].evidence.is_empty());
    }

    #[test]
    fn exact_duplicate_blocks_are_preserved_and_reported() {
        let block = br#"
            <h1>Evento duplicato</h1>
            <h2>03-08-2026</h2><h4>PIEMONTE</h4><h4>Standard</h4>
            <div>Provincia: Torino</div>
            <div>Immissione: 03-07-2026 (103)</div>
        "#;
        let mut html = block.to_vec();
        html.extend_from_slice(block);

        let analysis = analyze_calendar(&html).expect("documento con duplicato");
        assert_eq!(analysis.events.len(), 2);
        assert_eq!(analysis.events[0], analysis.events[1]);
        assert_eq!(analysis.summary.duplicate_identifiers, 1);
    }

    #[test]
    fn unrecognized_dates_and_malformed_blocks_fail_without_panicking() {
        let inputs: &[&[u8]] = &[
            br#"
                <h1>Data testuale</h1><h2>prossimamente</h2>
                <div>Provincia: Milano</div>
                <div>Immissione: 01-07-2026 (104)</div>
            "#,
            br#"
                <h1>Blocco troncato
                <div>Immissione: 01-07-2026 (105)
            "#,
            br#"
                <h1>Identificatore malformato</h1><h2>04-08-2026</h2>
                <div>Immissione: 01-07-2026 (abc)</div>
            "#,
        ];

        for input in inputs {
            let result = catch_unwind(AssertUnwindSafe(|| parse_calendar(input)));
            assert!(result.is_ok(), "panic con input {input:?}");
            assert!(result.expect("assenza panic").is_err());
        }
    }

    #[test]
    fn normalizes_relative_urls_only_with_explicit_base() {
        assert_eq!(normalize_url("/evento/42", None), "/evento/42");
        assert_eq!(
            normalize_url(
                "/evento/42",
                Some("https://www.federscacchi.com/fsi/calendario")
            ),
            "https://www.federscacchi.com/evento/42"
        );
        assert_eq!(
            normalize_url("https://example.org/evento", Some("https://base.invalid")),
            "https://example.org/evento"
        );
    }

    #[test]
    fn absent_fields_are_exported_as_null_not_invented_values() {
        let (jsonl, _) =
            build_index(SYNTHETIC_FIXTURE, "fixture-snapshot", None).expect("indice fixture");
        let records: Vec<serde_json::Value> = jsonl
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("record JSONL"))
            .collect();

        assert_eq!(records.len(), 6);
        assert!(records.iter().all(|record| record["organizer"].is_null()));
        let absent = records
            .iter()
            .find(|record| record["source_id"] == "900002")
            .expect("record con campi opzionali assenti");
        assert!(absent["location"].is_null());
        assert!(absent["province"].is_null());
        assert!(absent["organizer"].is_null());
        assert_eq!(absent["attachment_urls"], serde_json::json!([]));
    }

    #[test]
    fn rejects_input_without_events() {
        let error = parse_calendar(b"<html><h1>Calendario</h1></html>")
            .expect_err("HTML senza eventi da rifiutare");
        assert!(error.contains("Nessun evento"));
    }

    #[test]
    fn arbitrary_truncated_inputs_return_without_panicking() {
        let inputs: &[&[u8]] = &[
            b"",
            b"<",
            b"<h1",
            b"<h1>Evento",
            b"<h1>Evento</h1><h2>01-08-2026",
            b"<h1>Evento</h1><h2>01-08-2026</h2><div>Immissione: (",
            b"<h1><a href=\"unterminated",
            b"\xff\xfe<h1>Evento</h1>",
            "<h1>Èvënto ♞</h1><div>Immissione: 01-01-2026 (1)</div>".as_bytes(),
        ];

        for input in inputs {
            let result = catch_unwind(AssertUnwindSafe(|| parse_calendar(input)));
            assert!(result.is_ok(), "panic con input {input:?}");
        }
    }

    #[test]
    fn stress_mutations_are_deterministic_and_cover_all_families() {
        let first = stress_calendar(SYNTHETIC_FIXTURE, 16, 0x4653_4920_5354_5245, |_| {});
        let second = stress_calendar(SYNTHETIC_FIXTURE, 16, 0x4653_4920_5354_5245, |_| {});

        assert_eq!(first, second);
        assert_eq!(first.iterations_completed, 16);
        assert_eq!(first.mutation_counts.len(), MutationKind::ALL.len());
        assert_eq!(first.panics, 0);
        assert_eq!(
            first.successful_parses + first.parser_errors + first.panics,
            16
        );
    }

    #[test]
    fn attribute_reordering_preserves_tag_attributes() {
        let input = r#"<a class="button primary" href="/event/42">Evento</a>"#;
        let output = reorder_selected_attributes(input, &mut SplitMix64(7));

        assert_eq!(
            output,
            r#"<a href="/event/42" class="button primary">Evento</a>"#
        );
    }

    #[test]
    fn query_without_filters_returns_every_event_in_deterministic_order() {
        let config = query_config();
        let first = query_calendar_index(QUERY_FIXTURE, &config).expect("prima query");
        let second = query_calendar_index(QUERY_FIXTURE, &config).expect("seconda query");
        let records = jsonl_values(&first);

        assert_eq!(first, second);
        assert_eq!(records.len(), 4);
        assert_eq!(
            records
                .iter()
                .map(|record| record["source_id"].as_str().expect("source_id"))
                .collect::<Vec<_>>(),
            ["101", "102", "103", "104"]
        );
    }

    #[test]
    fn query_date_boundaries_are_inclusive() {
        let mut config = query_config();
        config.from = CalendarDate::parse("10-08-2026");
        config.to = CalendarDate::parse("10-08-2026");

        let output = query_calendar_index(QUERY_FIXTURE, &config).expect("query estremi");
        let ids = jsonl_values(&output)
            .into_iter()
            .map(|record| record["source_id"].as_str().expect("source_id").to_owned())
            .collect::<Vec<_>>();

        assert_eq!(ids, ["101", "103"]);
    }

    #[test]
    fn query_supports_independent_from_and_to_boundaries() {
        let mut from_config = query_config();
        from_config.from = CalendarDate::parse("16-08-2026");
        let from_ids = jsonl_values(
            &query_calendar_index(QUERY_FIXTURE, &from_config).expect("query solo from"),
        )
        .into_iter()
        .map(|record| record["source_id"].as_str().expect("source_id").to_owned())
        .collect::<Vec<_>>();

        let mut to_config = query_config();
        to_config.to = CalendarDate::parse("09-08-2026");
        let to_ids =
            jsonl_values(&query_calendar_index(QUERY_FIXTURE, &to_config).expect("query solo to"))
                .into_iter()
                .map(|record| record["source_id"].as_str().expect("source_id").to_owned())
                .collect::<Vec<_>>();

        assert_eq!(from_ids, ["102", "103"]);
        assert_eq!(to_ids, ["103"]);
    }

    #[test]
    fn query_includes_event_spanning_the_whole_interval() {
        let mut config = query_config();
        config.from = CalendarDate::parse("12-08-2026");
        config.to = CalendarDate::parse("13-08-2026");

        let output = query_calendar_index(QUERY_FIXTURE, &config).expect("query intersezione");
        let records = jsonl_values(&output);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["source_id"], "103");
    }

    #[test]
    fn query_combines_date_region_province_and_category_filters() {
        let mut config = query_config();
        config.from = CalendarDate::parse("11-08-2026");
        config.to = CalendarDate::parse("12-08-2026");
        config.region = Some("toscana".to_owned());
        config.province = Some("FIRENZE".to_owned());
        config.category = Some("Rapid".to_owned());

        let output = query_calendar_index(QUERY_FIXTURE, &config).expect("query combinata");
        let records = jsonl_values(&output);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["source_id"], "103");
    }

    #[test]
    fn query_preserves_absent_optional_fields_as_null() {
        let mut config = query_config();
        config.province = Some("Firenze".to_owned());
        config.from = CalendarDate::parse("11-08-2026");

        let output = query_calendar_index(QUERY_FIXTURE, &config).expect("query campi assenti");
        let records = jsonl_values(&output);

        assert_eq!(records.len(), 1);
        assert!(records[0]["location"].is_null());
        assert!(records[0]["organizer"].is_null());
        assert!(records[0]["event_url"].is_null());
    }

    #[test]
    fn query_accepts_omitted_optional_fields_without_inventing_values() {
        let input = br#"{"schema_version":1,"source_key":"fsi-calendar","source_id":"minimal","title":"Evento minimo","starts_on":"29-07-2026","attachment_urls":[],"snapshot_ref":"fixture:minimal"}
"#;

        let output = query_calendar_index(input, &query_config()).expect("query record minimo");
        let records = jsonl_values(&output);

        assert_eq!(records.len(), 1);
        for field in [
            "ends_on",
            "location",
            "province",
            "region",
            "organizer",
            "event_type",
            "event_url",
        ] {
            assert!(records[0][field].is_null(), "{field} non è null");
        }
        assert_eq!(records[0]["classifications"], serde_json::json!([]));
    }

    #[test]
    fn query_accepts_an_empty_index_in_both_formats() {
        let mut config = query_config();
        assert_eq!(
            query_calendar_index(b"", &config).expect("JSONL vuoto"),
            b""
        );

        config.format = OutputFormat::Terminal;
        assert_eq!(
            query_calendar_index(b"", &config).expect("terminale vuoto"),
            b"SOURCE_ID\tDATE\tEND_DATE\tTITLE\tREGION\tPROVINCE\tCATEGORIES\n"
        );
    }

    #[test]
    fn query_config_rejects_invalid_dates_unknown_options_and_missing_index() {
        for invalid in [
            "31-02-2026",
            "1-08-2026",
            "01-8-2026",
            "+1-08-2026",
            "01-01-0000",
        ] {
            let error = QueryConfig::from_args([
                "--index".to_owned(),
                "fixture".to_owned(),
                "--from".to_owned(),
                invalid.to_owned(),
            ])
            .expect_err("data non valida da rifiutare");
            assert!(error.contains("--from non valido"), "{error}");
        }
        assert!(QueryConfig::from_args(["--unknown".to_owned()])
            .expect_err("opzione sconosciuta")
            .contains("Opzione sconosciuta"));
        assert!(QueryConfig::from_args(Vec::<String>::new())
            .expect_err("indice obbligatorio")
            .contains("--index"));
    }

    #[test]
    fn query_order_is_independent_of_input_order_when_primary_keys_tie() {
        let mut records = read_calendar_index(QUERY_FIXTURE).expect("fixture query");
        records.truncate(2);
        for record in &mut records {
            record.source_id = "same".to_owned();
            record.starts_on = "10-08-2026".to_owned();
            record.title = "Stesso titolo".to_owned();
            record.snapshot_ref = "same-snapshot".to_owned();
        }
        let forward = calendar_jsonl(&records);
        records.reverse();
        let reversed = calendar_jsonl(&records);

        let first = query_calendar_index(&forward, &query_config()).expect("ordine diretto");
        let second = query_calendar_index(&reversed, &query_config()).expect("ordine inverso");

        assert_eq!(first, second);
    }

    #[test]
    fn index_reader_rejects_malformed_schema_identity_and_date_range() {
        let malformed =
            query_calendar_index(b"{not-json}\n", &query_config()).expect_err("JSON malformato");
        assert!(malformed.contains("riga 1"));

        let mut record = read_calendar_index(QUERY_FIXTURE)
            .expect("fixture query")
            .remove(0);
        record.schema_version = 2;
        let schema = query_calendar_index(&calendar_jsonl(&[record.clone()]), &query_config())
            .expect_err("schema errato");
        assert!(schema.contains("Versione indice FSI non supportata"));

        record.schema_version = 1;
        record.source_id.clear();
        let identity = query_calendar_index(&calendar_jsonl(&[record.clone()]), &query_config())
            .expect_err("identificatore vuoto");
        assert!(identity.contains("source_id mancante o vuoto"));

        record.source_id = "range".to_owned();
        record.ends_on = Some("04-08-2026".to_owned());
        let range = query_calendar_index(&calendar_jsonl(&[record]), &query_config())
            .expect_err("intervallo incoerente");
        assert!(range.contains("Intervallo temporale incoerente"));
    }

    #[test]
    fn diff_reports_added_removed_modified_and_unchanged_records() {
        let output = diff_calendar_indexes(DIFF_OLD_FIXTURE, DIFF_NEW_FIXTURE, OutputFormat::Jsonl)
            .expect("diff fixture");
        let records = jsonl_values(&output);

        assert_eq!(
            records
                .iter()
                .map(|record| record["change"].as_str().expect("change"))
                .collect::<Vec<_>>(),
            ["unchanged", "removed", "modified", "added"]
        );
        assert_eq!(records[0]["source_id"], "1");
        assert_eq!(records[1]["source_id"], "2");
        assert_eq!(records[2]["source_id"], "3");
        assert_eq!(records[3]["source_id"], "4");
        assert_eq!(records[2]["fields"][0]["field"], "title");
        assert_eq!(records[2]["fields"][0]["old"], "Titolo precedente");
        assert_eq!(records[2]["fields"][0]["new"], "Titolo nuovo");
        assert_eq!(records[2]["fields"][1]["field"], "province");
        assert!(records[2]["fields"][1]["old"].is_null());
        assert_eq!(records[2]["fields"][1]["new"], "Firenze");
    }

    #[test]
    fn diff_rejects_duplicate_source_ids_without_overwriting_records() {
        let mut duplicated = DIFF_OLD_FIXTURE.to_vec();
        duplicated.extend_from_slice(
            DIFF_OLD_FIXTURE
                .split(|byte| *byte == b'\n')
                .next()
                .expect("prima riga"),
        );
        duplicated.push(b'\n');

        let error = diff_calendar_indexes(&duplicated, DIFF_NEW_FIXTURE, OutputFormat::Jsonl)
            .expect_err("duplicato da rifiutare");

        assert!(error.contains("Indice vecchio"));
        assert!(error.contains("source_id duplicati"));
        assert!(error.contains("1 (righe 1, 4)"));
    }

    #[test]
    fn diff_reports_duplicates_from_old_and_new_together() {
        let old_record = read_calendar_index(DIFF_OLD_FIXTURE)
            .expect("fixture old")
            .remove(0);
        let new_record = read_calendar_index(DIFF_NEW_FIXTURE)
            .expect("fixture new")
            .remove(0);
        let old = calendar_jsonl(&[old_record.clone(), old_record]);
        let new = calendar_jsonl(&[new_record.clone(), new_record]);

        let error = diff_calendar_indexes(&old, &new, OutputFormat::Jsonl)
            .expect_err("duplicati su entrambi i lati");

        assert!(error.contains("Indice vecchio"), "{error}");
        assert!(error.contains("1 (righe 1, 2)"), "{error}");
        assert!(error.contains("Indice nuovo"), "{error}");
        assert!(error.contains("4 (righe 1, 2)"), "{error}");
    }

    #[test]
    fn diff_accepts_empty_indexes_in_both_formats() {
        assert_eq!(
            diff_calendar_indexes(b"", b"", OutputFormat::Jsonl).expect("diff JSONL vuoto"),
            b""
        );
        assert_eq!(
            diff_calendar_indexes(b"", b"", OutputFormat::Terminal).expect("diff terminale vuoto"),
            b"CHANGE\tSOURCE_ID\tTITLE\tDETAILS\n"
        );
    }

    #[test]
    fn diff_preserves_null_vectors_and_classifications_in_changes() {
        let old = read_calendar_index(DIFF_OLD_FIXTURE)
            .expect("fixture old")
            .remove(0);
        let mut new = old.clone();
        new.location = Some("Firenze".to_owned());
        new.attachment_urls = vec!["https://example.invalid/bando.pdf".to_owned()];
        new.classifications = vec![EventClassification {
            category: "rapid".to_owned(),
            detail: None,
            evidence: vec!["Rapid".to_owned()],
        }];

        let output = diff_calendar_indexes(
            &calendar_jsonl(&[old]),
            &calendar_jsonl(&[new]),
            OutputFormat::Jsonl,
        )
        .expect("diff strutture");
        let records = jsonl_values(&output);
        let fields = records[0]["fields"].as_array().expect("campi modificati");

        assert!(fields
            .iter()
            .any(|field| field["field"] == "location" && field["old"].is_null()));
        assert!(fields.iter().any(|field| {
            field["field"] == "attachment_urls"
                && field["new"] == serde_json::json!(["https://example.invalid/bando.pdf"])
        }));
        assert!(fields.iter().any(|field| {
            field["field"] == "classifications"
                && field["new"][0]["category"] == serde_json::json!("rapid")
                && field["new"][0]["detail"].is_null()
        }));
    }

    #[test]
    fn diff_config_rejects_missing_unknown_and_invalid_format_options() {
        assert!(
            DiffConfig::from_args(["--old".to_owned(), "old".to_owned()])
                .expect_err("new obbligatorio")
                .contains("--new")
        );
        assert!(DiffConfig::from_args(["--unknown".to_owned()])
            .expect_err("opzione sconosciuta")
            .contains("Opzione sconosciuta"));
        assert!(DiffConfig::from_args([
            "--old".to_owned(),
            "old".to_owned(),
            "--new".to_owned(),
            "new".to_owned(),
            "--format".to_owned(),
            "xml".to_owned(),
        ])
        .expect_err("formato sconosciuto")
        .contains("Formato sconosciuto"));
    }

    #[test]
    fn diff_output_is_deterministic_for_both_formats() {
        for format in [OutputFormat::Jsonl, OutputFormat::Terminal] {
            let first = diff_calendar_indexes(DIFF_OLD_FIXTURE, DIFF_NEW_FIXTURE, format)
                .expect("primo diff");
            let second = diff_calendar_indexes(DIFF_OLD_FIXTURE, DIFF_NEW_FIXTURE, format)
                .expect("secondo diff");
            assert_eq!(first, second);
        }
    }
}
