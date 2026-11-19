//! Provable Top 3 movements between two complete month-scoped FSI/FIDE reports.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Default)]
struct Opt {
    current: Option<PathBuf>,
    previous: Option<PathBuf>,
    cohort: Option<String>,
    sex: Option<String>,
    format: String,
    top: Option<usize>,
}

fn help() {
    println!(
        r#"antiskill stats movement --current FULL-NOV.json --previous FULL-OCT.json --cohort U16 [--sex female] [--top 3] [--format table|json|csv]

Compare positions in two MONTHLY COMPLETE regional rankings. FSI membership snapshots
and FIDE rating releases must both belong to the correct months. The top3-only
reports are insufficient and MUST fail closed. Missing prior identity != new entrant.
Output remains private and uncertified."#
    );
}

fn previous_month(month: &str) -> Result<String, String> {
    let (a, b) = month.split_once('-').ok_or("Mese errato in report")?;
    let y: i32 = a.parse().map_err(|_| "Anno errato")?;
    let m: u8 = b.parse().map_err(|_| "Mese errato")?;
    if a.len() != 4 || b.len() != 2 || !(1..=12).contains(&m) {
        return Err("Data non YYYY-MM".into());
    }
    Ok(if m == 1 {
        format!("{:04}-12", y - 1)
    } else {
        format!("{y:04}-{:02}", m - 1)
    })
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Opt>, String> {
    let mut o = Opt {
        format: "table".into(),
        top: Some(3),
        ..Opt::default()
    };
    let mut it = args.into_iter();
    let Some(first) = it.next() else {
        return Ok(None);
    };
    if ["-h", "--help", "help"].contains(&first.as_str()) {
        return Ok(None);
    }
    let mut queue = std::iter::once(first).chain(it);
    while let Some(flag) = queue.next() {
        let val = queue
            .next()
            .ok_or_else(|| format!("{flag} richiede un valore"))?;
        match flag.as_str() {
            "--current" | "--report" => o.current = Some(PathBuf::from(val)),
            "--previous" | "--previous-report" => o.previous = Some(PathBuf::from(val)),
            "--cohort" => o.cohort = Some(val.to_uppercase()),
            "--sex" => o.sex = Some(val.to_lowercase()),
            "--format" => o.format = val,
            "--top" => o.top = Some(val.parse().map_err(|_| "Numero top invalido")?),
            _ => return Err(format!("Opzione movement sconosciuta: {flag}")),
        }
    }
    if !["table", "json", "csv"].contains(&o.format.as_str()) {
        return Err("Formato non valido".into());
    }
    if !["U08", "U10", "U12", "U14", "U16", "U18"].contains(&o.cohort.as_deref().unwrap_or("")) {
        return Err("Specificare --cohort U08|U10|U12|U14|U16|U18".into());
    }
    if !["absolute", "female"].contains(&o.sex.as_deref().unwrap_or("absolute")) {
        return Err("Sesso non valido".into());
    }
    if o.top.is_none_or(|n| n == 0 || n > 100) {
        return Err("--top fuori intervallo".into());
    }
    if o.current.is_none() || o.previous.is_none() {
        return Err("Servono entrambi --current e --previous".into());
    }
    Ok(Some(o))
}

fn read(path: &PathBuf) -> Result<Value, String> {
    let buf = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if buf.len() > 40_000_000 {
        return Err("Report eccessivamente grande".into());
    }
    serde_json::from_slice(&buf).map_err(|e| format!("JSON report {}: {e}", path.display()))
}
fn strkey<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn region(j: &Value) -> Result<String, String> {
    let vals = strkey(j, "filters")
        .split(',')
        .filter_map(|pair| pair.trim().split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect::<BTreeMap<_, _>>();
    let period = strkey(j, "period");
    let year = period.split('-').next().unwrap_or("");
    if vals.get("membership-year") != Some(&year) {
        return Err("Anno tesseramento incompatibile con periodo".into());
    }
    vals.get("membership-region")
        .filter(|r| !r.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| "Regione tesseramento mancante".into())
}
fn official(j: &Value) -> Result<(), String> {
    if strkey(j, "schema") != "antiskill.rankings.v1" || strkey(j, "scope") != "top3-region" {
        return Err("Report non Top 3 regionale".into());
    }
    if !["fide-standard", "fide-rapid", "fide-blitz"].contains(&strkey(j, "rating")) {
        return Err("Report privo di rating internazionale FIDE".into());
    }
    if strkey(j, "membership_authority") != "fsi-official-csv"
        || strkey(j, "rating_authority") != "fide-official-xml"
    {
        return Err("Per movement servono fonti FSI/FIDE ufficiali".into());
    }
    let month = strkey(j, "period");
    previous_month(month)?;
    if strkey(j, "fide_month") != month || !strkey(j, "fsi_as_of").starts_with(month) {
        return Err(
            "Periodo sorgente FSI/FIDE disallineato: historical cohort non verificabile".into(),
        );
    }
    if !j["fsi_manifest_proof"]["verified"]
        .as_bool()
        .unwrap_or(false)
    {
        return Err("Manifest FSI non verificato".into());
    }
    for key in ["fsi_source", "fide_source", "identity_source"] {
        if strkey(&j[key], "blake3").len() != 64 {
            return Err(format!("Fonte {key} priva di impronta BLAKE3 valida"));
        }
    }
    region(j)?;
    Ok(())
}
fn group<'a>(j: &'a Value, cohort: &str, sex: &str) -> Result<&'a Value, String> {
    let groups = j["groups"].as_array().ok_or("Gruppi assenti")?;
    let matching = groups
        .iter()
        .filter(|g| strkey(g, "cohort") == cohort && strkey(g, "sex") == sex)
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err("Gruppo assente o ambiguo".into());
    }
    let g = matching[0];
    let people = g["players"].as_array().ok_or("Giocatori assenti")?;
    let count = g["population"]
        .as_u64()
        .ok_or("Popolazione non dichiarata")? as usize;
    if people.len() != count {
        return Err(format!(
            "Report troncato: {} {}/{}; serve --limit completo nel motore Rankings",
            cohort,
            people.len(),
            count
        ));
    }
    let mut ids = BTreeSet::new();
    let mut fide = BTreeSet::new();
    for (i, p) in people.iter().enumerate() {
        let id = strkey(p, "fsi_id");
        let fid = strkey(p, "fide_id");
        if id.is_empty()
            || fid.is_empty()
            || !ids.insert(id)
            || !fide.insert(fid)
            || p["rank"].as_u64() != Some((i + 1) as u64)
        {
            return Err("Ranghi e identificativi FSI/FIDE non univoci o disordinati".into());
        }
    }
    Ok(g)
}
fn output(o: &Opt) -> Result<(), String> {
    let current = o.current.as_ref().unwrap();
    let previous = o.previous.as_ref().unwrap();
    let now = read(current)?;
    let old = read(previous)?;
    official(&now)?;
    official(&old)?;
    let month = strkey(&now, "period");
    if previous_month(month)? != strkey(&old, "period") {
        return Err("Servono due mesi consecutivi".into());
    }
    if strkey(&now, "rating") != strkey(&old, "rating") || region(&now)? != region(&old)? {
        return Err("Rating o regione diversi tra i report".into());
    }
    let cohort = o.cohort.as_ref().unwrap();
    let sex = o.sex.as_deref().unwrap_or("absolute");
    let a = group(&now, cohort, sex)?["players"].as_array().unwrap();
    let b = group(&old, cohort, sex)?["players"].as_array().unwrap();
    let current_map = a
        .iter()
        .map(|p| (strkey(p, "fsi_id"), p))
        .collect::<BTreeMap<_, _>>();
    let previous_map = b
        .iter()
        .map(|p| (strkey(p, "fsi_id"), p))
        .collect::<BTreeMap<_, _>>();
    let previous_fide_map = b
        .iter()
        .map(|p| (strkey(p, "fide_id"), strkey(p, "fsi_id")))
        .collect::<BTreeMap<_, _>>();
    for (id, p) in &current_map {
        if let Some(q) = previous_map.get(id) {
            if strkey(p, "fide_id") != strkey(q, "fide_id") {
                return Err(format!("Conflitto FIDE ID fra mesi per FSI {id}"));
            }
        }
        if let Some(old_fsi) = previous_fide_map.get(strkey(p, "fide_id")) {
            if *old_fsi != *id {
                return Err(format!(
                    "Un FIDE ID risulta associato a un altro FSI tra mesi: {id}"
                ));
            }
        }
    }
    let cutoff = o.top.unwrap();
    let mut rows = Vec::<Value>::new();
    let mut emitted = BTreeSet::<String>::new();
    for p in a.iter().take(cutoff) {
        let id = strkey(p, "fsi_id");
        let prior = previous_map.get(id).copied();
        let pr = prior.and_then(|p| p["rank"].as_u64());
        let cur = p["rank"].as_u64().unwrap();
        let state = match pr {
            Some(n) if n <= cutoff as u64 => "stayed",
            Some(_) => "entered",
            None => "unknown-prior",
        };
        let prior_rating = prior.and_then(|p| p["rating"].as_i64());
        let rating = p["rating"].as_i64();
        rows.push(json!({"name":strkey(p,"name"),"fsi_id":id,"fide_id":strkey(p,"fide_id"),
            "previous_rank":pr,"current_rank":cur,"rank_change":pr.map(|n|n as i64-cur as i64),
            "previous_rating":prior_rating,"rating":rating,"rating_delta":rating.zip(prior_rating).map(|(a,b)|a-b),
            "movement":state}));
        emitted.insert(id.to_string());
    }
    for p in b.iter().take(cutoff) {
        let id = strkey(p, "fsi_id");
        if emitted.contains(id) {
            continue;
        }
        let curr = current_map.get(id).copied();
        let curr_rank = curr.and_then(|p| p["rank"].as_u64());
        let prev_rank = p["rank"].as_u64().unwrap();
        let state = match curr_rank {
            Some(n) if n > cutoff as u64 => "exited",
            None => "unknown-current",
            _ => "stayed",
        };
        rows.push(json!({"name":curr.map(|q|strkey(q,"name")).unwrap_or(strkey(p,"name")),"fsi_id":id,"fide_id":strkey(p,"fide_id"),
            "previous_rank":prev_rank,"current_rank":curr_rank,
            "rank_change":curr_rank.map(|n|prev_rank as i64-n as i64),
            "previous_rating":p["rating"],"rating":curr.and_then(|q|q["rating"].as_i64()),
            "rating_delta":curr.and_then(|q|q["rating"].as_i64()).zip(p["rating"].as_i64()).map(|(a,b)|a-b),"movement":state}));
    }
    let unresolved = rows
        .iter()
        .filter(|r| strkey(r, "movement").starts_with("unknown"))
        .count();
    let meta = json!({"schema":"antiskill.stats.movement.v1","current_period":month,
        "previous_period":strkey(&old,"period"),"region":region(&now)?,"cohort":cohort,
        "sex":sex,"rating":strkey(&now,"rating"),"top":cutoff,
        "current_source":current,"previous_source":previous,
        "full_population_checked":true,"unresolved_count":unresolved,"certified":false});
    if o.format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"meta":meta,"rows":rows}))
                .map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let fields = [
        "name",
        "fsi_id",
        "fide_id",
        "previous_rank",
        "current_rank",
        "rank_change",
        "previous_rating",
        "rating",
        "rating_delta",
        "movement",
    ];
    if o.format == "csv" {
        let mut w = csv::Writer::from_writer(std::io::stdout());
        w.write_record(fields).map_err(|e| e.to_string())?;
        for r in &rows {
            w.write_record(fields.map(|k| {
                if r[k].is_null() {
                    String::new()
                } else if let Some(s) = r[k].as_str() {
                    s.to_string()
                } else {
                    r[k].to_string()
                }
            }))
            .map_err(|e| e.to_string())?;
        }
        w.flush().map_err(|e| e.to_string())?;
    } else {
        println!("MOVEMENT {}", meta);
        println!("{}", fields.join("\t"));
        for r in &rows {
            let vals = fields
                .iter()
                .map(|k| {
                    if r[*k].is_null() {
                        "—".into()
                    } else if let Some(s) = r[*k].as_str() {
                        s.to_string()
                    } else {
                        r[*k].to_string()
                    }
                })
                .collect::<Vec<String>>();
            println!("{}", vals.join("\t"));
        }
    }
    Ok(())
}
pub fn run(args: impl IntoIterator<Item = String>) -> ExitCode {
    let result = (|| {
        let Some(o) = parse(args)? else {
            help();
            return Ok(());
        };
        output(&o)
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("antiskill stats movement: {e}");
            ExitCode::from(2)
        }
    }
}
