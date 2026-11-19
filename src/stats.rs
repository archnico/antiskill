use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Default)]
struct Opt {
    cmd: String,
    root: Option<PathBuf>,
    report: Option<PathBuf>,
    region: Option<String>,
    club: Option<String>,
    year: Option<i32>,
    month: Option<String>,
    rating: Option<String>,
    under: Option<i32>,
    by: Option<Vec<String>>,
    cohort: Option<String>,
    sex: Option<String>,
    limit: Option<usize>,
    format: String,
}
fn help() {
    println!(
        r#"Antiskill Stats — interrogazioni statistiche (sola lettura)

 antiskill stats members --region TOS --year 2026 --by club
 antiskill stats members --region TOS --year 2026 --by club,type --under 18
 antiskill stats members --club 17029 --year 2026 --by type
 antiskill stats top3 --region TOS --month 2026-11 --rating standard
 antiskill stats top3 --region TOS --month 2026-11 --cohort U16 --sex female
 antiskill stats club --club 17029 --month 2026-11 --rating standard
 antiskill stats coverage --region TOS --month 2026-11 --by club-province
 antiskill stats reports

 --root DIR oppure ANTISKILL_STATS_ROOT (default XDG_DATA_HOME/antiskill)
 --report FILE     report esplicito per top3, club, coverage
 --region CODE     regione di affiliazione (non residenza)
 --club CODE       codice federale del circolo
 --year YYYY       anno di tesseramento (members)
 --month YYYY-MM   periodo report (top3/club/coverage)
 --rating standard|rapid|blitz|italia (default standard)
 --by club,club-name,club-province,residence-province,region,type,category,birth-year,country
 --under N         filtro per età (members o report club)
 --cohort U08|U10|U12|U14|U16|U18|ALL
 --sex female|absolute
 --limit N         massimo righe per gruppo (top3/club) o totali (members)
 --format table|csv|json (default table)

Le classifiche richiedono report Rankings già calcolati; non si inventano
dati mancanti. Nessuna pubblicazione automatica di dati personali o minori."#
    );
}
fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Opt>, String> {
    let mut it = args.into_iter();
    let Some(cmd) = it.next() else {
        return Ok(None);
    };
    if ["help", "-h", "--help"].contains(&cmd.as_str()) {
        return Ok(None);
    }
    if !["members", "top3", "club", "coverage", "reports"].contains(&cmd.as_str()) {
        return Err(format!("Sottocomando non valido: {cmd}"));
    }
    let mut o = Opt {
        cmd,
        format: "table".into(),
        ..Default::default()
    };
    while let Some(k) = it.next() {
        if ["help", "-h", "--help"].contains(&k.as_str()) {
            return Ok(None);
        }
        let v = it
            .next()
            .ok_or_else(|| format!("Valore mancante per {k}"))?;
        match k.as_str() {
            "--root" => o.root = Some(PathBuf::from(v)),
            "--report" => o.report = Some(PathBuf::from(v)),
            "--region" => o.region = Some(v.to_uppercase()),
            "--club" => o.club = Some(v),
            "--year" => o.year = Some(v.parse().map_err(|_| "Anno non valido")?),
            "--month" => o.month = Some(v),
            "--rating" => o.rating = Some(v.to_lowercase()),
            "--under" => o.under = Some(v.parse().map_err(|_| "Età non valida")?),
            "--by" => o.by = Some(v.split(',').map(str::to_owned).collect()),
            "--cohort" => o.cohort = Some(v.to_uppercase()),
            "--sex" => o.sex = Some(v.to_lowercase()),
            "--limit" => o.limit = Some(v.parse().map_err(|_| "Limite non valido")?),
            "--format" => o.format = v,
            _ => return Err(format!("Opzione sconosciuta: {k}")),
        }
    }
    if !["table", "csv", "json"].contains(&o.format.as_str()) {
        return Err("Formato non valido".into());
    }
    if o.limit == Some(0) || o.under.is_some_and(|n| !(1..=120).contains(&n)) {
        return Err("Limite/età non validi".into());
    }
    if o.year.is_some_and(|n| !(2000..=2100).contains(&n)) {
        return Err("Anno non valido".into());
    }
    if let Some(m) = &o.month {
        if m.len() != 7
            || !m
                .chars()
                .enumerate()
                .all(|(i, c)| if i == 4 { c == '-' } else { c.is_ascii_digit() })
            || m[5..]
                .parse::<u8>()
                .map_or(true, |n| !(1..=12).contains(&n))
        {
            return Err("Mese richiesto YYYY-MM".into());
        }
    }
    if let Some(c) = &o.club {
        if c.len() < 3 || !c.bytes().all(|b| b.is_ascii_digit()) {
            return Err("Codice circolo non valido".into());
        }
    }
    if let Some(r) = &o.rating {
        if !["standard", "rapid", "blitz", "italia"].contains(&r.as_str()) {
            return Err("Rating non valido".into());
        }
    }
    if let Some(v) = &o.by {
        if v.is_empty() || v.len() > 3 || v.iter().any(|s| s.is_empty()) {
            return Err("Dimensioni --by invalide".into());
        }
    }
    let invalid = match o.cmd.as_str() {
        "members" => {
            o.month.is_some()
                || o.rating.is_some()
                || o.report.is_some()
                || o.cohort.is_some()
                || o.sex.is_some()
        }
        "top3" => o.club.is_some() || o.year.is_some() || o.under.is_some() || o.by.is_some(),
        "club" => o.region.is_some() || o.year.is_some() || o.by.is_some(),
        "coverage" => {
            o.club.is_some()
                || o.year.is_some()
                || o.under.is_some()
                || o.cohort.is_some()
                || o.sex.is_some()
        }
        "reports" => {
            o.region.is_some()
                || o.club.is_some()
                || o.year.is_some()
                || o.month.is_some()
                || o.rating.is_some()
                || o.report.is_some()
                || o.under.is_some()
                || o.by.is_some()
                || o.cohort.is_some()
                || o.sex.is_some()
        }
        _ => false,
    };
    if invalid {
        return Err(format!(
            "Filtri incompatibili con stats {} (nessun filtro ignorato)",
            o.cmd
        ));
    }
    Ok(Some(o))
}
fn root(o: &Opt) -> Result<PathBuf, String> {
    let p = o
        .root
        .clone()
        .or_else(|| env::var_os("ANTISKILL_STATS_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| {
            let base = env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
                .unwrap_or_else(|| PathBuf::from("."));
            base.join("antiskill")
        });
    if !p.is_dir() {
        return Err(format!("Archivio assente: {}. Usare --root", p.display()));
    }
    Ok(p)
}
fn read_json(p: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?)
        .map_err(|e| format!("{}: {e}", p.display()))
}
fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn print_rows(keys: &[&str], rows: &[Vec<String>], meta: Value, fmt: &str) -> Result<(), String> {
    if fmt == "json" {
        let records: Vec<Value> = rows
            .iter()
            .map(|r| {
                let mut m = serde_json::Map::new();
                for (k, v) in keys.iter().zip(r) {
                    let value = if ["memberships", "rank", "rating", "delta"].contains(k) {
                        v.parse::<i64>().map_or_else(|_| json!(v), |n| json!(n))
                    } else {
                        json!(v)
                    };
                    m.insert((*k).into(), value);
                }
                Value::Object(m)
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"meta":meta,"rows":records}))
                .map_err(|e| e.to_string())?
        );
    } else if fmt == "csv" {
        let mut w = csv::Writer::from_writer(io::stdout());
        w.write_record(keys).map_err(|e| e.to_string())?;
        for r in rows {
            w.write_record(r).map_err(|e| e.to_string())?
        }
        w.flush().map_err(|e| e.to_string())?;
        eprintln!("METADATA {}", meta);
    } else {
        println!("METADATA {}", meta);
        println!("{}", keys.join("\t"));
        for row in rows {
            println!("{}", row.join("\t"))
        }
        if rows.is_empty() {
            println!("Nessun risultato per i filtri selezionati")
        }
    }
    Ok(())
}
fn fsi_dim(rec: &csv::StringRecord, key: &str) -> Result<String, String> {
    let idx = match key {
        "club" => 8,
        "club-name" => 9,
        "club-province" => 10,
        "residence-province" => 5,
        "region" => 11,
        "type" => 12,
        "category" => 4,
        "birth-year" => 3,
        "country" => 7,
        _ => return Err(format!("Dimensione FSI sconosciuta: {key}")),
    };
    Ok(rec.get(idx).unwrap_or("").trim().into())
}
fn source(root: &Path, key: &str) -> Result<(Vec<u8>, Value), String> {
    let dir = root.join("raw/manifests").join(key);
    let mut files = fs::read_dir(&dir)
        .map_err(|e| format!("Snapshot FSI {key} assente: {e}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|v| v.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    files.sort();
    let p = files.last().ok_or(format!("Manifest FSI {key} assente"))?;
    let j = read_json(p)?;
    if s(&j, "source_key") != key || j["http_status"] != 200 {
        return Err("Manifest FSI incoerente".into());
    }
    let rel = s(&j, "object_path");
    if !rel.starts_with("raw/objects/blake3/") || rel.contains("..") {
        return Err("Percorso oggetto non valido".into());
    }
    let bytes = fs::read(root.join(rel)).map_err(|e| format!("Oggetto sorgente mancante: {e}"))?;
    if j["bytes"].as_u64() != Some(bytes.len() as u64)
        || s(&j, "blake3") != blake3::hash(&bytes).to_hex().as_str()
    {
        return Err("BLAKE3/size fonte FSI non coincidono".into());
    }
    Ok((bytes, j))
}
fn members(o: &Opt, root: &Path) -> Result<(), String> {
    let year = o.year.ok_or("Specificare --year YYYY")?;
    let key = if let Some(r) = &o.region {
        format!("fsi-members-region-{}-{year}", r.to_lowercase())
    } else if let Some(c) = &o.club {
        format!("fsi-members-club-{c}-{year}")
    } else {
        return Err("Specificare --region o --club: nessuna scansione nazionale implicita".into());
    };
    let dims = o.by.clone().unwrap_or_else(|| vec!["club".into()]);
    for d in &dims {
        fsi_dim(&csv::StringRecord::new(), d)?;
    }
    let (bytes, manifest) = source(root, &key)?;
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b';')
        .has_headers(false)
        .from_reader(bytes.as_slice());
    let mut groups: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let (mut total, mut matched) = (0usize, 0usize);
    for result in rdr.records() {
        let row = result.map_err(|e| format!("CSV FSI: {e}"))?;
        if row.len() != 15
            || row.get(0).unwrap_or("").is_empty()
            || !row.get(0).unwrap_or("").bytes().all(|b| b.is_ascii_digit())
        {
            return Err("Riga FSI non valida".into());
        }
        total += 1;
        if o.region.as_deref() == Some("TOS") && row.get(11) != Some("TOSCANA") {
            return Err("Regione FSI non coerente".into());
        }
        if o.club
            .as_ref()
            .is_some_and(|c| row.get(8) != Some(c.as_str()))
        {
            continue;
        }
        if let Some(age) = o.under {
            let born = row
                .get(3)
                .unwrap_or("")
                .parse::<i32>()
                .map_err(|_| "Anno nascita FSI non valido")?;
            if year - born >= age {
                continue;
            }
        }
        matched += 1;
        ids.insert(row[0].to_owned());
        let keys = dims
            .iter()
            .map(|d| fsi_dim(&row, d))
            .collect::<Result<Vec<_>, _>>()?;
        *groups.entry(keys).or_default() += 1;
    }
    if total == 0 {
        return Err("Snapshot FSI vuoto".into());
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    if let Some(n) = o.limit {
        groups.truncate(n)
    }
    let mut heads = dims.iter().map(String::as_str).collect::<Vec<_>>();
    heads.push("memberships");
    let rows = groups
        .into_iter()
        .map(|(mut k, n)| {
            k.push(n.to_string());
            k
        })
        .collect::<Vec<_>>();
    let meta = json!({"schema":"antiskill.stats.members.v1","source":key,"blake3":manifest["blake3"],"snapshot_at_unix_ms":manifest["fetched_at_unix_ms"],"year":year,"region":o.region,"club":o.club,"under":o.under,"population_rows":total,"matched_rows":matched,"distinct_fsi_ids":ids.len(),"units":"membership-records","certified":false});
    print_rows(&heads, &rows, meta, &o.format)
}
fn reports(root: &Path) -> Result<Vec<(PathBuf, Value)>, String> {
    let mut out = Vec::new();
    for e in fs::read_dir(root.join("reports")).map_err(|e| format!("Report assenti: {e}"))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if !p.is_dir()
            || !p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .starts_with("rankings-")
        {
            continue;
        }
        for f in fs::read_dir(p).map_err(|e| e.to_string())? {
            let path = f.map_err(|e| e.to_string())?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let j = read_json(&path)?;
            if j["schema"] == "antiskill.rankings.v1" {
                out.push((path, j))
            }
        }
    }
    Ok(out)
}
fn filters(j: &Value, k: &str, v: &str) -> bool {
    s(j, "filters")
        .split(',')
        .filter_map(|p| p.trim().split_once('='))
        .any(|(a, b)| a.trim() == k && b.trim() == v)
}
fn matches_report(o: &Opt, j: &Value) -> bool {
    let scope = if o.cmd == "club" {
        "club"
    } else {
        "top3-region"
    };
    if s(j, "scope") != scope {
        return false;
    }
    if o.month.as_ref().is_some_and(|m| s(j, "period") != m) {
        return false;
    }
    let rating = match o.rating.as_deref().unwrap_or("standard") {
        "standard" => "fide-standard",
        "rapid" => "fide-rapid",
        "blitz" => "fide-blitz",
        _ => "italia",
    };
    if s(j, "rating") != rating {
        return false;
    }
    if scope == "club" {
        if !o.club.as_ref().is_some_and(|c| filters(j, "club", c)) {
            return false;
        }
        if !filters(j, "under", &o.under.map_or("all".into(), |n| n.to_string())) {
            return false;
        }
    } else if !o
        .region
        .as_ref()
        .is_some_and(|r| filters(j, "membership-region", r))
    {
        return false;
    }
    true
}
fn selected(o: &Opt, root: &Path) -> Result<(PathBuf, Value), String> {
    if let Some(p) = &o.report {
        let j = read_json(p)?;
        if j["schema"] != "antiskill.rankings.v1" || !matches_report(o, &j) {
            return Err("Report incompatibile con la selezione".into());
        }
        return Ok((p.clone(), j));
    }
    if o.month.is_none() {
        return Err("Specificare --month oppure --report".into());
    }
    let found = reports(root)?
        .into_iter()
        .filter(|(_, j)| matches_report(o, j))
        .collect::<Vec<_>>();
    match found.len(){
        0=>Err("Report assente: generare con antiskill rankings, oppure usare --report. Nessun dato inventato".into()),
        1=>Ok(found.into_iter().next().unwrap()),
        n=>Err(format!("{n} report compatibili: usare --report FILE per disambiguare")),
    }
}
fn from_report(o: &Opt, root: &Path) -> Result<(), String> {
    let (path, j) = selected(o, root)?;
    let meta = json!({"schema":"antiskill.stats.report.v1","path":path,"period":j["period"],"scope":j["scope"],"rating":j["rating"],"filters":j["filters"],"certified":j["certified"],"fsi_as_of":j["fsi_as_of"],"fide_month":j["fide_month"],"source":j["source"],"fsi_source":j["fsi_source"],"fide_source":j["fide_source"],"membership_authority":j["membership_authority"],"rating_authority":j["rating_authority"]});
    if o.cmd == "coverage" {
        let by =
            o.by.as_ref()
                .and_then(|v| v.first())
                .map(String::as_str)
                .unwrap_or("type");
        if o.by.as_ref().is_some_and(|b| b.len() != 1) {
            return Err("coverage supporta una sola dimensione".into());
        }
        let key = match by {
            "type" => "membership_types",
            "club-province" => "club_provinces",
            "residence-province" => "residence_provinces",
            _ => return Err("Dimensione coverage sconosciuta".into()),
        };
        let c = j["coverage"][key]
            .as_object()
            .ok_or("Report senza dati coverage FSI")?;
        let mut rows = c
            .iter()
            .map(|(k, v)| vec![k.clone(), v.as_u64().unwrap_or(0).to_string()])
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            b[1].parse::<usize>()
                .unwrap_or(0)
                .cmp(&a[1].parse::<usize>().unwrap_or(0))
                .then(a[0].cmp(&b[0]))
        });
        if let Some(n) = o.limit {
            rows.truncate(n)
        }
        return print_rows(
            &[by, "memberships"],
            &rows,
            json!({"report":meta,"coverage":j["coverage"],"units":"membership-records"}),
            &o.format,
        );
    }
    let mut rows = Vec::new();
    for g in j["groups"].as_array().ok_or("Report senza gruppi")? {
        if o.cohort.as_ref().is_some_and(|c| s(g, "cohort") != c) {
            continue;
        }
        if o.sex.as_ref().is_some_and(|c| s(g, "sex") != c) {
            continue;
        }
        for p in g["players"]
            .as_array()
            .ok_or("Gruppo senza giocatori")?
            .iter()
            .take(o.limit.unwrap_or(usize::MAX))
        {
            rows.push(vec![
                s(g, "cohort").into(),
                s(g, "sex").into(),
                p["rank"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                s(p, "name").into(),
                s(p, "fsi_id").into(),
                s(p, "fide_id").into(),
                s(p, "club").into(),
                p["rating"]
                    .as_i64()
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                p["rating_delta"]
                    .as_i64()
                    .map(|v| format!("{v:+}"))
                    .unwrap_or_else(|| "-".into()),
            ]);
        }
    }
    print_rows(
        &[
            "cohort", "sex", "rank", "player", "fsi_id", "fide_id", "club", "rating", "delta",
        ],
        &rows,
        meta,
        &o.format,
    )
}
pub fn run(args: impl IntoIterator<Item = String>) -> ExitCode {
    let result = (|| {
        let Some(o) = parse(args)? else {
            help();
            return Ok(());
        };
        let r = root(&o)?;
        match o.cmd.as_str() {
            "members" => members(&o, &r),
            "reports" => {
                let mut rows = reports(&r)?
                    .into_iter()
                    .map(|(p, j)| {
                        vec![
                            s(&j, "period").into(),
                            s(&j, "scope").into(),
                            s(&j, "rating").into(),
                            s(&j, "filters").into(),
                            p.display().to_string(),
                        ]
                    })
                    .collect::<Vec<_>>();
                rows.sort();
                if let Some(n) = o.limit {
                    rows.truncate(n)
                }
                print_rows(
                    &["month", "scope", "rating", "filters", "path"],
                    &rows,
                    json!({"schema":"antiskill.stats.reports.v1","count":rows.len()}),
                    &o.format,
                )
            }
            _ => from_report(&o, &r),
        }
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("antiskill stats: {e}");
            ExitCode::from(2)
        }
    }
}
