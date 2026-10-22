use super::snapshot::{load_verified_latest_snapshot, store_snapshot, SnapshotMetadata};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE_KEY: &str = "torneionline-allin";
const DEFAULT_URL: &str = "https://www.torneionline.com/dwn/allin.csv";
const RIGHTS_CLASS: &str = "official-operational-internal-use";
const BRIDGE_PROFILE: &str = "antiskill_torneionline_allin_identity_bridge_v1";
const EXPECTED_COLUMNS: usize = 33;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    root: PathBuf,
    url: String,
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
                "--url" => {
                    url = args.next().ok_or("Manca il valore di --url")?;
                }
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

pub(crate) fn print_help() {
    println!(
        "\
antiskill sources sync torneionline-allin

Scarica allin.csv e ne conserva uno snapshot content-addressed.
Non interpreta ancora le righe del CSV.

USO:
    antiskill sources sync torneionline-allin --root DIR [--url URL]

OUTPUT:
    raw/objects/blake3/       oggetti immutabili deduplicati
    raw/manifests/            un manifest per ogni acquisizione
    raw/refs/                 riferimento aggiornabile all'ultima acquisizione
"
    );
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

    let mut response = ureq::get(config.url.as_str())
        .header("User-Agent", user_agent.as_str())
        .header("Accept", "text/csv,text/plain;q=0.9,*/*;q=0.1")
        .header("Accept-Encoding", "identity")
        .call()
        .map_err(|error| format!("Download di {SOURCE_KEY} fallito: {error}"))?;

    let http_status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let content_length_header = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let last_modified = response
        .headers()
        .get("last-modified")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let content_disposition = response
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let metadata = SnapshotMetadata {
        source_key: SOURCE_KEY.to_owned(),
        source_url: config.url,
        fetched_at_unix_ms,
        http_status,
        content_type,
        content_length_header,
        etag,
        last_modified,
        content_disposition,
        rights_class: RIGHTS_CLASS.to_owned(),
        retrieved_by: user_agent,
    };

    let reader = response.body_mut().as_reader();
    let stored = store_snapshot(&config.root, metadata, reader)?;

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

    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IndexConfig {
    root: PathBuf,
    output: PathBuf,
    report: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum QueryKey {
    Fide(String),
    Fsi(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct QueryConfig {
    index: PathBuf,
    key: QueryKey,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AllinBridgeRecordV1 {
    pub profile: String,
    pub source_blake3: String,
    pub source_bytes: u64,
    pub source_line: u64,
    pub raw_name: String,
    pub name_signature: String,
    pub signature_unique: bool,
    pub fsi_id: String,
    pub fide_id: String,
    pub birth_year: Option<u16>,
    pub birth_date: Option<String>,
    pub federation: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AllinBridgeResolutionV1 {
    pub profile: String,
    pub fide_id: String,
    pub fsi_id: String,
    pub raw_names: Vec<String>,
    pub name_signatures: Vec<String>,
    pub all_signatures_unique: bool,
    pub observations: Vec<AllinBridgeRecordV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct MappingConflict {
    namespace: &'static str,
    id: String,
    counterparts: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct IndexReport {
    profile: &'static str,
    source_blake3: String,
    source_bytes: u64,
    source_manifest: String,
    rows: u64,
    bridge_rows: u64,
    unique_fsi_ids: usize,
    unique_fide_ids: usize,
    ambiguous_name_signatures: usize,
    mapping_conflicts: Vec<MappingConflict>,
    complete: bool,
}

impl IndexConfig {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut root = None;
        let mut output = None;
        let mut report = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--root" => root = Some(PathBuf::from(args.next().ok_or("Manca --root")?)),
                "--output" => output = Some(PathBuf::from(args.next().ok_or("Manca --output")?)),
                "--report" => report = Some(PathBuf::from(args.next().ok_or("Manca --report")?)),
                other => return Err(format!("Opzione index ALLIN sconosciuta: {other}")),
            }
        }
        Ok(Self {
            root: root.ok_or("Parametro obbligatorio: --root")?,
            output: output.ok_or("Parametro obbligatorio: --output")?,
            report: report.ok_or("Parametro obbligatorio: --report")?,
        })
    }
}

impl QueryConfig {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut index = None;
        let mut key = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--index" => index = Some(PathBuf::from(args.next().ok_or("Manca --index")?)),
                "--fide-id" => {
                    let value = canonical_id(&args.next().ok_or("Manca --fide-id")?, "FIDE")?;
                    if key.replace(QueryKey::Fide(value)).is_some() {
                        return Err("Specificare un solo identificatore di query".into());
                    }
                }
                "--fsi-id" => {
                    let value = canonical_id(&args.next().ok_or("Manca --fsi-id")?, "FSI")?;
                    if key.replace(QueryKey::Fsi(value)).is_some() {
                        return Err("Specificare un solo identificatore di query".into());
                    }
                }
                other => return Err(format!("Opzione query ALLIN sconosciuta: {other}")),
            }
        }
        Ok(Self {
            index: index.ok_or("Parametro obbligatorio: --index")?,
            key: key.ok_or("Serve --fide-id oppure --fsi-id")?,
        })
    }
}

pub(crate) fn print_index_help() {
    println!(
        "antiskill sources index torneionline-allin\n\nUSO:\n    antiskill sources index torneionline-allin --root DIR --output FILE --report FILE\n"
    );
}

pub(crate) fn print_query_help() {
    println!(
        "antiskill sources query torneionline-allin\n\nUSO:\n    antiskill sources query torneionline-allin --index FILE (--fide-id ID | --fsi-id ID)\n"
    );
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
    let config = IndexConfig::from_args(args)?;
    let snapshot = load_verified_latest_snapshot(&config.root, SOURCE_KEY)?;
    let file = File::open(&snapshot.object_path)
        .map_err(|error| format!("Apertura snapshot ALLIN: {error}"))?;
    let (records, mut report) = parse_bridge_index(
        BufReader::new(file),
        &snapshot.blake3,
        snapshot.bytes,
        &snapshot.manifest_path.display().to_string(),
    )?;
    write_json_replace(&config.report, &report)?;
    if !report.mapping_conflicts.is_empty() {
        return Err("Bridge ALLIN ambiguo: conflitti FSI↔FIDE nel report".into());
    }
    write_jsonl_new(&config.output, &records)?;
    report.complete = true;
    write_json_replace(&config.report, &report)?;
    println!("PROFILE\t{BRIDGE_PROFILE}");
    println!("SOURCE_BLAKE3\t{}", snapshot.blake3);
    println!("ROWS\t{}", report.rows);
    println!("BRIDGE_ROWS\t{}", report.bridge_rows);
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
    let config = QueryConfig::from_args(args)?;
    let resolution = resolve_bridge(&config.index, &config.key)?;
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &resolution)
        .map_err(|error| format!("Serializzazione query ALLIN: {error}"))?;
    stdout.write_all(b"\n").map_err(|error| error.to_string())
}

pub(crate) fn resolve_fide_bridge(
    index: &Path,
    fide_id: &str,
) -> Result<AllinBridgeResolutionV1, String> {
    resolve_bridge(index, &QueryKey::Fide(canonical_id(fide_id, "FIDE")?))
}

fn resolve_bridge(index: &Path, key: &QueryKey) -> Result<AllinBridgeResolutionV1, String> {
    let metadata = fs::symlink_metadata(index)
        .map_err(|error| format!("Indice ALLIN {}: {error}", index.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("Indice ALLIN non regolare: {}", index.display()));
    }
    let file = File::open(index).map_err(|error| format!("Indice ALLIN: {error}"))?;
    let mut matches = Vec::new();
    let mut fsi_to_fide = BTreeMap::<String, BTreeSet<String>>::new();
    let mut fide_to_fsi = BTreeMap::<String, BTreeSet<String>>::new();
    let mut signature_to_pairs = BTreeMap::<String, BTreeSet<(String, String)>>::new();
    let mut uniqueness_claims = Vec::<(usize, String, bool)>::new();
    let mut source_identity: Option<(String, u64)> = None;
    let mut previous_source_line = 1_u64;

    for (zero_line, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|error| format!("Lettura indice ALLIN: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let record: AllinBridgeRecordV1 = serde_json::from_str(&line)
            .map_err(|error| format!("Indice ALLIN JSONL riga {}: {error}", zero_line + 1))?;
        validate_bridge_record(&record)?;
        if record.source_line <= previous_source_line {
            return Err(format!(
                "Indice ALLIN fuori ordine o con source_line duplicata: {}",
                record.source_line
            ));
        }
        previous_source_line = record.source_line;
        match &source_identity {
            None => source_identity = Some((record.source_blake3.clone(), record.source_bytes)),
            Some((hash, bytes))
                if hash == &record.source_blake3 && *bytes == record.source_bytes => {}
            Some(_) => return Err("Indice ALLIN mescola snapshot sorgente differenti".into()),
        }

        fsi_to_fide
            .entry(record.fsi_id.clone())
            .or_default()
            .insert(record.fide_id.clone());
        fide_to_fsi
            .entry(record.fide_id.clone())
            .or_default()
            .insert(record.fsi_id.clone());
        signature_to_pairs
            .entry(record.name_signature.clone())
            .or_default()
            .insert((record.fsi_id.clone(), record.fide_id.clone()));
        uniqueness_claims.push((
            zero_line + 1,
            record.name_signature.clone(),
            record.signature_unique,
        ));

        let selected = match key {
            QueryKey::Fide(id) => &record.fide_id == id,
            QueryKey::Fsi(id) => &record.fsi_id == id,
        };
        if selected {
            matches.push(record);
        }
    }
    if source_identity.is_none() {
        return Err("Indice ALLIN vuoto".into());
    }
    let mut mapping_conflicts = Vec::new();
    append_mapping_conflicts(&mut mapping_conflicts, "fsi", &fsi_to_fide);
    append_mapping_conflicts(&mut mapping_conflicts, "fide", &fide_to_fsi);
    if !mapping_conflicts.is_empty() {
        return Err("Indice ALLIN contiene mapping FSI↔FIDE conflittuali".into());
    }
    for (line, signature, claimed_unique) in uniqueness_claims {
        let actual_unique = signature_to_pairs
            .get(&signature)
            .is_some_and(|pairs| pairs.len() == 1);
        if claimed_unique != actual_unique {
            return Err(format!(
                "Indice ALLIN riga {line}: signature_unique non verificabile"
            ));
        }
    }
    if matches.is_empty() {
        return Err("Identificatore assente dall'indice ALLIN".into());
    }
    matches.sort_by_key(|record| record.source_line);
    let fide_ids = matches
        .iter()
        .map(|record| record.fide_id.clone())
        .collect::<BTreeSet<_>>();
    let fsi_ids = matches
        .iter()
        .map(|record| record.fsi_id.clone())
        .collect::<BTreeSet<_>>();
    if fide_ids.len() != 1 || fsi_ids.len() != 1 {
        return Err("Bridge ALLIN conflittuale per l'identificatore richiesto".into());
    }
    Ok(AllinBridgeResolutionV1 {
        profile: BRIDGE_PROFILE.into(),
        fide_id: fide_ids.into_iter().next().expect("set non vuoto"),
        fsi_id: fsi_ids.into_iter().next().expect("set non vuoto"),
        raw_names: matches
            .iter()
            .map(|record| record.raw_name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        name_signatures: matches
            .iter()
            .map(|record| record.name_signature.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        all_signatures_unique: matches.iter().all(|record| record.signature_unique),
        observations: matches,
    })
}

fn parse_bridge_index<R: BufRead>(
    mut reader: R,
    source_blake3: &str,
    source_bytes: u64,
    source_manifest: &str,
) -> Result<(Vec<AllinBridgeRecordV1>, IndexReport), String> {
    let mut header = Vec::new();
    if reader
        .read_until(b'\n', &mut header)
        .map_err(|error| error.to_string())?
        == 0
    {
        return Err("Snapshot ALLIN vuoto".into());
    }
    trim_line_end(&mut header);
    let columns = split_fields(&header);
    validate_header(&columns)?;

    let mut records = Vec::new();
    let mut rows = 0_u64;
    let mut fsi_to_fide = BTreeMap::<String, BTreeSet<String>>::new();
    let mut fide_to_fsi = BTreeMap::<String, BTreeSet<String>>::new();
    let mut signature_to_pairs = BTreeMap::<String, BTreeSet<(String, String)>>::new();
    let mut line = Vec::new();
    let mut source_line = 1_u64;
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        source_line += 1;
        trim_line_end(&mut line);
        if line.is_empty() {
            continue;
        }
        rows += 1;
        let fields = split_fields(&line);
        if fields.len() != EXPECTED_COLUMNS {
            return Err(format!(
                "ALLIN riga {source_line}: attese {EXPECTED_COLUMNS} colonne, trovate {}",
                fields.len()
            ));
        }
        let fsi_raw = decode_field(fields[6]);
        let fide_raw = decode_field(fields[8]);
        let fsi = optional_id(&fsi_raw, "FSI", source_line)?;
        let fide = optional_id(&fide_raw, "FIDE", source_line)?;
        let (Some(fsi_id), Some(fide_id)) = (fsi, fide) else {
            continue;
        };
        let raw_name = decode_field(fields[0]);
        let signature = crate::identity::player_name_signature(&raw_name)
            .ok_or_else(|| format!("ALLIN riga {source_line}: nome non identificabile"))?;
        fsi_to_fide
            .entry(fsi_id.clone())
            .or_default()
            .insert(fide_id.clone());
        fide_to_fsi
            .entry(fide_id.clone())
            .or_default()
            .insert(fsi_id.clone());
        signature_to_pairs
            .entry(signature.clone())
            .or_default()
            .insert((fsi_id.clone(), fide_id.clone()));
        records.push(AllinBridgeRecordV1 {
            profile: BRIDGE_PROFILE.into(),
            source_blake3: source_blake3.into(),
            source_bytes,
            source_line,
            raw_name,
            name_signature: signature,
            signature_unique: false,
            fsi_id,
            fide_id,
            birth_year: optional_u16(&decode_field(fields[5]), "anno nascita", source_line)?,
            birth_date: optional_text(&decode_field(fields[19])),
            federation: optional_text(&decode_field(fields[4])),
        });
    }
    for record in &mut records {
        record.signature_unique = signature_to_pairs
            .get(&record.name_signature)
            .is_some_and(|pairs| pairs.len() == 1);
    }
    let mut mapping_conflicts = Vec::new();
    append_mapping_conflicts(&mut mapping_conflicts, "fsi", &fsi_to_fide);
    append_mapping_conflicts(&mut mapping_conflicts, "fide", &fide_to_fsi);
    let report = IndexReport {
        profile: BRIDGE_PROFILE,
        source_blake3: source_blake3.into(),
        source_bytes,
        source_manifest: source_manifest.into(),
        rows,
        bridge_rows: records.len() as u64,
        unique_fsi_ids: fsi_to_fide.len(),
        unique_fide_ids: fide_to_fsi.len(),
        ambiguous_name_signatures: signature_to_pairs
            .values()
            .filter(|pairs| pairs.len() > 1)
            .count(),
        mapping_conflicts,
        complete: false,
    };
    Ok((records, report))
}

fn append_mapping_conflicts(
    conflicts: &mut Vec<MappingConflict>,
    namespace: &'static str,
    mappings: &BTreeMap<String, BTreeSet<String>>,
) {
    for (id, counterparts) in mappings {
        if counterparts.len() > 1 {
            conflicts.push(MappingConflict {
                namespace,
                id: id.clone(),
                counterparts: counterparts.iter().cloned().collect(),
            });
        }
    }
}

fn validate_header(fields: &[&[u8]]) -> Result<(), String> {
    if fields.len() != EXPECTED_COLUMNS {
        return Err(format!("Header ALLIN: attese {EXPECTED_COLUMNS} colonne"));
    }
    for (index, expected) in [
        (0, "Nominativo"),
        (6, "ID FSI"),
        (8, "FIN"),
        (19, "Data Nascita"),
    ] {
        if decode_field(fields[index]) != expected {
            return Err(format!("Header ALLIN inatteso alla colonna {}", index + 1));
        }
    }
    Ok(())
}

fn split_fields(line: &[u8]) -> Vec<&[u8]> {
    line.split(|byte| *byte == b';').collect()
}

fn trim_line_end(line: &mut Vec<u8>) {
    while matches!(line.last(), Some(b'\n' | b'\r')) {
        line.pop();
    }
}

fn decode_field(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| char::from(*byte))
        .collect::<String>()
        .trim()
        .to_owned()
}

fn canonical_id(raw: &str, namespace: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
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

fn optional_id(raw: &str, namespace: &str, line: u64) -> Result<Option<String>, String> {
    let value = raw.trim();
    if value.is_empty() || value == "0" {
        return Ok(None);
    }
    canonical_id(value, namespace)
        .map(Some)
        .map_err(|error| format!("ALLIN riga {line}: {error}"))
}

fn optional_u16(raw: &str, label: &str, line: u64) -> Result<Option<u16>, String> {
    let value = raw.trim();
    if value.is_empty() || value == "0" {
        return Ok(None);
    }
    value
        .parse::<u16>()
        .map(Some)
        .map_err(|_| format!("ALLIN riga {line}: {label} non valido: {raw}"))
}

fn optional_text(raw: &str) -> Option<String> {
    let value = raw.trim();
    (!value.is_empty() && value != "0").then(|| value.to_owned())
}

fn validate_bridge_record(record: &AllinBridgeRecordV1) -> Result<(), String> {
    if record.profile != BRIDGE_PROFILE {
        return Err("Profilo indice ALLIN non supportato".into());
    }
    if canonical_id(&record.fsi_id, "FSI")? != record.fsi_id
        || canonical_id(&record.fide_id, "FIDE")? != record.fide_id
    {
        return Err("Record bridge ALLIN con identificatore non canonico".into());
    }
    if record.source_line < 2 || record.source_bytes == 0 {
        return Err("Record bridge ALLIN con provenance sorgente non valida".into());
    }
    if record.source_blake3.len() != 64
        || !record
            .source_blake3
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("Record bridge ALLIN con BLAKE3 sorgente non valido".into());
    }
    let calculated = crate::identity::player_name_signature(&record.raw_name)
        .ok_or("Record bridge ALLIN senza nome identificabile")?;
    if calculated != record.name_signature {
        return Err("Record bridge ALLIN con firma nome incoerente".into());
    }
    Ok(())
}

fn write_jsonl_new(path: &Path, records: &[AllinBridgeRecordV1]) -> Result<(), String> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("Creazione indice ALLIN {}: {error}", path.display()))?;
    let mut writer = BufWriter::new(file);
    for record in records {
        serde_json::to_writer(&mut writer, record)
            .map_err(|error| format!("Serializzazione indice ALLIN: {error}"))?;
        writer.write_all(b"\n").map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn write_json_replace<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|error| error.to_string())?;
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("Creazione report ALLIN: {error}"))?;
    serde_json::to_writer_pretty(&mut file, value)
        .map_err(|error| format!("Serializzazione report ALLIN: {error}"))?;
    file.write_all(b"\n").map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| format!("Pubblicazione report ALLIN: {error}"))
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

    #[test]
    fn parses_required_root_and_optional_url() {
        let config = Config::from_args([
            "--root".to_owned(),
            "/tmp/archive".to_owned(),
            "--url".to_owned(),
            "https://example.invalid/allin.csv".to_owned(),
        ])
        .expect("configurazione valida");

        assert_eq!(config.root, PathBuf::from("/tmp/archive"));
        assert_eq!(config.url, "https://example.invalid/allin.csv");
    }

    #[test]
    fn uses_the_official_default_url() {
        let config = Config::from_args(["--root".to_owned(), "/tmp/archive".to_owned()])
            .expect("configurazione valida");

        assert_eq!(config.url, DEFAULT_URL);
    }

    #[test]
    fn rejects_missing_root() {
        let error = Config::from_args(Vec::<String>::new()).expect_err("--root è obbligatorio");
        assert!(error.contains("--root"));
    }

    fn allin_fixture(rows: &[(&str, &str, &str, &str, &str)]) -> Vec<u8> {
        let header = "Nominativo;Elo;Categoria;K Italia;Federazione;Anno Nascita;ID FSI;Sex;FIN;Elo FIDE Std;K Elo FIDE Std;Games Elo FIDE Std;Elo FIDE Rapid;K Elo FIDE Rapid;Games Elo FIDE Rapid;Elo FIDE Blitz;K Elo FIDE Blitz;Games Elo FIDE Blitz;Attivo;Data Nascita;Provincia Residenza;Regione Residenza;Provincia Tesseramento;Regione Tesseramento;Cittadinanza;Data Ultimo Torneo;Circolo Attuale;Circolo Anno Precedente;Circolo Due Anni Precedenti;Anno Ultimo Tesseramento;Tipo Tessera;Numero Tessera;\r\n";
        let mut bytes = header.as_bytes().to_vec();
        for (name, fsi, fide, birth_year, birth_date) in rows {
            let fields = vec![
                (*name).to_owned(),
                "0".into(),
                "".into(),
                "0".into(),
                "ITA".into(),
                (*birth_year).to_owned(),
                (*fsi).to_owned(),
                "M".into(),
                (*fide).to_owned(),
                "0".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "Y".into(),
                (*birth_date).to_owned(),
                "FI".into(),
                "TOS".into(),
                "FI".into(),
                "TOS".into(),
                "ITA".into(),
                "".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "2026".into(),
                "TA".into(),
                "1".into(),
                "".into(),
            ];
            bytes.extend_from_slice(fields.join(";").as_bytes());
            bytes.extend_from_slice(b"\r\n");
        }
        bytes
    }

    #[test]
    fn bridge_index_preserves_ids_provenance_and_flags_homonyms() {
        let input = allin_fixture(&[
            (
                "ESEMPIO AURORA '",
                "900001",
                "900000001",
                "1990",
                "01-01-1990",
            ),
            ("ESEMPIO, AURORA", "200000", "999000", "1990", "01-01-1990"),
            ("UNIQUE PLAYER", "200001", "910000003", "1980", "02-02-1980"),
        ]);
        let (records, report) = parse_bridge_index(
            std::io::Cursor::new(input),
            "abc",
            123,
            "raw/manifests/test.json",
        )
        .expect("indice bridge");
        assert_eq!(records.len(), 3);
        assert_eq!(report.bridge_rows, 3);
        assert!(report.mapping_conflicts.is_empty());
        assert_eq!(report.ambiguous_name_signatures, 1);
        let target = &records[0];
        assert_eq!(target.fsi_id, "900001");
        assert_eq!(target.fide_id, "900000001");
        assert_eq!(target.source_line, 2);
        assert_eq!(target.source_blake3, "abc");
        assert!(!target.signature_unique);
        assert_eq!(target.birth_year, Some(1990));
        assert_eq!(target.birth_date.as_deref(), Some("01-01-1990"));
    }

    #[test]
    fn mapping_conflicts_are_explicit_and_invalid_ids_fail_closed() {
        let conflicting = allin_fixture(&[
            ("PLAYER ONE", "100", "200", "1990", "01-01-1990"),
            ("PLAYER ONE", "100", "201", "1990", "01-01-1990"),
        ]);
        let (_, report) =
            parse_bridge_index(std::io::Cursor::new(conflicting), "abc", 1, "manifest")
                .expect("parse conflitto");
        assert_eq!(report.mapping_conflicts.len(), 1);
        assert_eq!(report.mapping_conflicts[0].namespace, "fsi");
        assert_eq!(report.mapping_conflicts[0].counterparts, vec!["200", "201"]);

        let malformed = allin_fixture(&[("PLAYER TWO", "10A", "300", "1990", "01-01-1990")]);
        let error = parse_bridge_index(std::io::Cursor::new(malformed), "abc", 1, "manifest")
            .expect_err("ID invalido deve fallire");
        assert!(error.contains("ID FSI non valido"));
    }

    #[test]
    fn resolve_bridge_recomputes_uniqueness_and_rejects_tampering() {
        let input = allin_fixture(&[
            ("SAME NAME", "100", "200", "1990", "01-01-1990"),
            ("SAME, NAME", "101", "201", "1991", "01-01-1991"),
        ]);
        let (mut records, _) = parse_bridge_index(
            std::io::Cursor::new(input),
            &"b".repeat(64),
            321,
            "manifest",
        )
        .expect("parse");
        assert!(records.iter().all(|record| !record.signature_unique));
        records[0].signature_unique = true;
        let path = std::env::temp_dir().join(format!(
            "antiskill-allin-tampered-{}-{}.jsonl",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_jsonl_new(&path, &records).expect("write index");
        let error = resolve_fide_bridge(&path, "200").expect_err("tampering deve fallire");
        assert!(error.contains("signature_unique non verificabile"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn resolve_bridge_rejects_mixed_source_snapshots() {
        let input = allin_fixture(&[
            ("PLAYER ONE", "100", "200", "1990", "01-01-1990"),
            ("PLAYER TWO", "101", "201", "1991", "01-01-1991"),
        ]);
        let (mut records, _) = parse_bridge_index(
            std::io::Cursor::new(input),
            &"c".repeat(64),
            321,
            "manifest",
        )
        .expect("parse");
        records[1].source_blake3 = "d".repeat(64);
        let path = std::env::temp_dir().join(format!(
            "antiskill-allin-mixed-{}-{}.jsonl",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_jsonl_new(&path, &records).expect("write index");
        let error = resolve_fide_bridge(&path, "200").expect_err("mix deve fallire");
        assert!(error.contains("snapshot sorgente differenti"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn resolve_bridge_is_namespace_safe_and_deterministic() {
        let input = allin_fixture(&[(
            "ESEMPIO AURORA '",
            "900001",
            "900000001",
            "1990",
            "01-01-1990",
        )]);
        let (records, _) =
            parse_bridge_index(std::io::Cursor::new(input), &"a".repeat(64), 1, "manifest")
                .expect("parse");
        let path = std::env::temp_dir().join(format!(
            "antiskill-allin-bridge-{}-{}.jsonl",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_jsonl_new(&path, &records).expect("write index");
        let resolution = resolve_fide_bridge(&path, "00900000001").expect("lookup FIDE");
        assert_eq!(resolution.fide_id, "900000001");
        assert_eq!(resolution.fsi_id, "900001");
        assert_eq!(resolution.raw_names, vec!["ESEMPIO AURORA '"]);
        assert!(resolution.all_signatures_unique);
        fs::remove_file(path).unwrap();
    }
}
