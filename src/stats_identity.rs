//! Read-only, fail-closed FSI <-> FIDE lookup from the exported private registry.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const SCHEMA: &str = "antiskill.stats.identity.v1";
const CSV_SCHEMA: &str = "antiskill.identity-crosswalk-export.v1";

#[derive(Debug)]
struct Opt {
    kind: String,
    id: String,
    format: String,
    root: PathBuf,
    crosswalk: Option<PathBuf>,
}

fn help() {
    println!(
        r#"Antiskill Stats · Identita FSI ↔ FIDE (registro privato)

 antiskill stats identity --fsi 192691
 antiskill stats identity --fide 28551931
 antiskill stats identity --fsi 192691 --format json
 antiskill stats identity --fide 28551931 --format csv
 antiskill stats identity --fsi 192691 --crosswalk FILE.csv

 --fsi ID / --fide ID   obbligatorio esattamente uno, numerico
 --format table|json|csv (default table)
 --root DIR             oppure ANTISKILL_STATS_ROOT
 --crosswalk FILE.csv   selezione esplicita di snapshot privato e sua provenienza

Output VERIFIED solo per collegamenti univoci con prova di fonte.
REVIEW / SOURCE_ASSERTED sono candidati, mai identita' certe.
File integri SHA256 e metadati di provenienza obbligatori; nessuna rete.
Exit 0 VERIFIED, 3 REVIEW/ambiguous, 4 not_found, 2 errore."#
    );
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Opt>, String> {
    let mut args = args.into_iter().peekable();
    if args.peek().is_none() {
        return Ok(None);
    }
    let (mut kind, mut id, mut root, mut crosswalk) = (None, None, None, None);
    let mut format = "table".to_string();
    while let Some(flag) = args.next() {
        if ["--help", "-h", "help"].contains(&flag.as_str()) {
            return Ok(None);
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Manca valore per {flag}"))?;
        match flag.as_str() {
            "--fsi" | "--fsi-id" => {
                if kind.is_some() {
                    return Err("Specificare esattamente uno tra --fsi e --fide".into());
                }
                kind = Some("fsi".to_string());
                id = Some(value);
            }
            "--fide" | "--fide-id" => {
                if kind.is_some() {
                    return Err("Specificare esattamente uno tra --fsi e --fide".into());
                }
                kind = Some("fide".to_string());
                id = Some(value);
            }
            "--root" => {
                if root.replace(PathBuf::from(value)).is_some() {
                    return Err("--root duplicato".into());
                }
            }
            "--crosswalk" => {
                if crosswalk.replace(PathBuf::from(value)).is_some() {
                    return Err("--crosswalk duplicato".into());
                }
            }
            "--format" => format = value,
            _ => return Err(format!("Opzione sconosciuta: {flag}")),
        }
    }
    let kind = kind.ok_or("Specificare --fsi ID oppure --fide ID")?;
    let id = id.unwrap();
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("ID numerico non valido".into());
    }
    let id = id.parse::<u64>().map_err(|_| "ID fuori intervallo")?;
    if id == 0 {
        return Err("ID deve essere positivo".into());
    }
    if !["table", "json", "csv"].contains(&format.as_str()) {
        return Err("Formato non valido: table|json|csv".into());
    }
    let root = root
        .or_else(|| env::var_os("ANTISKILL_STATS_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| {
            env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(|v| PathBuf::from(v).join(".local/share")))
                .unwrap_or_else(|| PathBuf::from("."))
                .join("antiskill")
        });
    Ok(Some(Opt {
        kind,
        id: id.to_string(),
        format,
        root,
        crosswalk,
    }))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("JSON {}: {e}", path.display()))
}
fn field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn is_sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn sha256(path: &Path) -> Result<String, String> {
    let out = Command::new("sha256sum")
        .arg("--")
        .arg(path)
        .output()
        .map_err(|e| format!("sha256sum non disponibile: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "SHA256 {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let value = text
        .split_whitespace()
        .next()
        .ok_or("sha256sum non ha prodotto un'impronta")?;
    if !is_sha(value) {
        return Err("Impronta SHA256 non valida".into());
    }
    Ok(value.to_ascii_lowercase())
}

fn provenance_path(path: &Path) -> PathBuf {
    let modern = PathBuf::from(format!("{}.provenance.json", path.display()));
    if modern.is_file() {
        return modern;
    }
    let legacy = path.with_extension("provenance.json");
    if legacy.is_file() {
        return legacy;
    }
    modern
}

fn select_crosswalk(opt: &Opt) -> Result<PathBuf, String> {
    if let Some(path) = &opt.crosswalk {
        return Ok(path.clone());
    }
    let dir = opt.root.join("durable/identity");
    let canonical_dir = dir
        .canonicalize()
        .map_err(|e| format!("Registro privato {}: {e}", dir.display()))?;
    let mut candidates = Vec::new();
    for file in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let path = file.map_err(|e| e.to_string())?.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.starts_with("verified-fsi-fide-") || !name.ends_with(".csv") || !path.is_file() {
            continue;
        }
        let actual = path.canonicalize().map_err(|e| e.to_string())?;
        if !actual.starts_with(&canonical_dir) {
            return Err("Crosswalk collegato fuori registro privato".into());
        }
        let proof = read_json(&provenance_path(&path))?;
        if field(&proof, "schema") != CSV_SCHEMA {
            return Err("Snapshot identita' con provenienza mancante".into());
        }
        let at = field(&proof, "exported_at");
        if at.is_empty() {
            return Err("Manca timestamp esportazione identita'".into());
        }
        candidates.push((at.to_string(), path));
    }
    if candidates.is_empty() {
        return Err("Nessun export identita' verificato: eseguire refresh su n2".into());
    }
    candidates.sort();
    let last = candidates.last().unwrap();
    if candidates.len() > 1 && candidates[candidates.len() - 2].0 == last.0 {
        return Err("Due export identita' con stessa data: indicare --crosswalk".into());
    }
    Ok(last.1.clone())
}

#[derive(Clone)]
struct Entry {
    fsi: String,
    fide: String,
    status: String,
}

struct Crosswalk {
    proof: Value,
    path: PathBuf,
    rows: Vec<Entry>,
    totals: BTreeMap<String, u64>,
}

fn load(path: PathBuf) -> Result<Crosswalk, String> {
    let meta = read_json(&provenance_path(&path))?;
    if field(&meta, "schema") != CSV_SCHEMA
        || field(&meta, "source_table") != "identity_link"
        || field(&meta, "rights_class") != "internal"
        || meta["publication_allowed"] != false
    {
        return Err("Provenienza del crosswalk insufficiente o permessi non compatibili".into());
    }
    let expected = field(&meta, "crosswalk_sha256");
    if !is_sha(expected) || !is_sha(field(&meta, "source_db_sha256")) {
        return Err("SHA256 di provenienza mancante".into());
    }
    let actual = sha256(&path)?;
    if actual != expected.to_ascii_lowercase() {
        return Err("SHA256 del crosswalk divergente: rifiuto identificazione".into());
    }
    let mut csv = csv::ReaderBuilder::new()
        .from_path(&path)
        .map_err(|e| e.to_string())?;
    let hdr = csv.headers().map_err(|e| e.to_string())?;
    if hdr.iter().collect::<Vec<_>>() != ["fsi_id", "fide_id", "status"] {
        return Err("Schema CSV crosswalk non valido".into());
    }
    let mut rows = Vec::new();
    let mut fsi_ids = BTreeSet::new();
    let mut verified_fide = BTreeSet::new();
    let mut totals = BTreeMap::<String, u64>::new();
    for line in csv.records() {
        let line = line.map_err(|e| e.to_string())?;
        if line.len() != 3 {
            return Err("Riga crosswalk incompleta".into());
        }
        let normalized = |v: &str| -> Result<String, String> {
            if v.is_empty() || !v.bytes().all(|x| x.is_ascii_digit()) {
                return Err("ID crosswalk non numerico".into());
            }
            let id = v
                .parse::<u64>()
                .map_err(|_| "ID crosswalk fuori intervallo")?;
            if id == 0 || id.to_string() != v {
                return Err("ID crosswalk non canonico".into());
            }
            Ok(v.into())
        };
        let fsi = normalized(&line[0])?;
        let fide = normalized(&line[1])?;
        let status = line[2].to_string();
        if !["VERIFIED", "REVIEW", "SOURCE_ASSERTED"].contains(&status.as_str()) {
            return Err("Stato crosswalk inatteso".into());
        }
        if !fsi_ids.insert(fsi.clone()) {
            return Err("FSI duplicato nel crosswalk".into());
        }
        if status == "VERIFIED" && !verified_fide.insert(fide.clone()) {
            return Err("FIDE VERIFIED associato a più FSI".into());
        }
        *totals.entry(status.clone()).or_default() += 1;
        rows.push(Entry { fsi, fide, status });
    }
    if rows.len() as u64
        != meta["records"]
            .as_u64()
            .or_else(|| meta["total_rows"].as_u64())
            .ok_or("Contatore righe mancante")?
    {
        return Err("Conteggio righe diverso dalla provenienza".into());
    }
    for k in ["VERIFIED", "REVIEW", "SOURCE_ASSERTED"] {
        if *totals.get(k).unwrap_or(&0)
            != meta["status_counts"][k]
                .as_u64()
                .ok_or("Contatore stati mancante")?
        {
            return Err("Conteggio stati diverso dalla provenienza".into());
        }
    }
    Ok(Crosswalk {
        proof: meta,
        path,
        rows,
        totals,
    })
}

fn render(opt: &Opt, cw: &Crosswalk) -> Result<i32, String> {
    let found = cw
        .rows
        .iter()
        .filter(|r| {
            if opt.kind == "fsi" {
                r.fsi == opt.id
            } else {
                r.fide == opt.id
            }
        })
        .collect::<Vec<_>>();
    let state = if found.is_empty() {
        "not_found"
    } else if found.len() != 1 {
        "ambiguous"
    } else {
        match found[0].status.as_str() {
            "VERIFIED" => "verified",
            "REVIEW" => "review",
            "SOURCE_ASSERTED" => "source_asserted",
            _ => return Err("Stato imprevisto".into()),
        }
    };
    let official = state == "verified";
    let fsi = found.first().filter(|_| official).map(|r| r.fsi.as_str());
    let fide = found.first().filter(|_| official).map(|r| r.fide.as_str());
    let candidates = if official {
        Vec::<Value>::new()
    } else {
        found
            .iter()
            .map(|r| {
                json!({"fsi_id":r.fsi,"fide_id":r.fide,
                                   "status":r.status})
            })
            .collect::<Vec<_>>()
    };
    let provenance = json!({
        "exported_at":cw.proof["exported_at"],
        "source_table":cw.proof["source_table"],
        "source_db_sha256":cw.proof["source_db_sha256"],
        "crosswalk_sha256":cw.proof["crosswalk_sha256"],
        "source_path":cw.path,
        "status_counts":cw.totals,
        "rights_class":"internal",
        "publication_allowed":false
    });
    let value = json!({
        "schema":SCHEMA,"queried_by":opt.kind,"queried_id":opt.id,"status":state,
        "fsi_id":fsi,"fide_id":fide,"candidates":candidates,
        "certified":false,"provenance":provenance,
        "note":"VERIFIED refers to the identity link in the private registry, not publication certification."
    });
    match opt.format.as_str() {
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
        ),
        "csv" => {
            let mut w = csv::Writer::from_writer(std::io::stdout());
            w.write_record([
                "query_type",
                "queried_id",
                "fsi_id",
                "fide_id",
                "status",
                "crosswalk_sha256",
            ])
            .map_err(|e| e.to_string())?;
            w.write_record([
                &opt.kind,
                &opt.id,
                fsi.unwrap_or(""),
                fide.unwrap_or(""),
                state,
                field(&cw.proof, "crosswalk_sha256"),
            ])
            .map_err(|e| e.to_string())?;
            w.flush().map_err(|e| e.to_string())?;
        }
        _ => {
            println!(
                "FSI ↔ FIDE   {} {}   {}",
                opt.kind.to_uppercase(),
                opt.id,
                state.to_uppercase()
            );
            if official {
                println!(
                    "FSI ID: {}\nFIDE ID: {}\nStato: VERIFIED",
                    fsi.unwrap(),
                    fide.unwrap()
                );
            } else if state == "not_found" {
                println!("Nessuna associazione nel registro disponibile.");
            } else {
                println!(
                    "Nessuna associazione CERTA. Candidati da revisionare: {}",
                    found.len()
                );
                for r in &found {
                    println!("candidato FSI={} FIDE={} stato={}", r.fsi, r.fide, r.status);
                }
            }
            println!(
                "Fonte: registro privato identity_link · export {}",
                field(&cw.proof, "exported_at")
            );
            println!("SHA256 registro: {}", field(&cw.proof, "source_db_sha256"));
            println!("Pubblicazione: non autorizzata");
        }
    }
    Ok(match state {
        "verified" => 0,
        "not_found" => 4,
        _ => 3,
    })
}

pub fn run(args: impl IntoIterator<Item = String>) -> ExitCode {
    let result = (|| {
        let Some(opt) = parse(args)? else {
            help();
            return Ok(0);
        };
        let path = select_crosswalk(&opt)?;
        let source = load(path)?;
        render(&opt, &source)
    })();
    match result {
        Ok(n) => ExitCode::from(n as u8),
        Err(e) => {
            eprintln!("antiskill stats identity: {e}");
            ExitCode::from(2)
        }
    }
}
