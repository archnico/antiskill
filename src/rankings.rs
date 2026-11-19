//! Reproducible local rankings from immutable Allin CSV snapshots.
//! This vertical consumes sources; it does not assume that Allin is an official FIDE rating list.
#[path = "rankings/official.rs"]
mod official;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Top3,
    Club,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Rating {
    Italia,
    Standard,
    Rapid,
    Blitz,
}

impl Rating {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "italia" => Ok(Self::Italia),
            "standard" | "fide-standard" => Ok(Self::Standard),
            "rapid" | "fide-rapid" => Ok(Self::Rapid),
            "blitz" | "fide-blitz" => Ok(Self::Blitz),
            _ => Err(format!(
                "Rating sconosciuto: {s}; usare italia, standard, rapid o blitz"
            )),
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Italia => "italia",
            Self::Standard => "fide-standard",
            Self::Rapid => "fide-rapid",
            Self::Blitz => "fide-blitz",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Month {
    year: i32,
    month: u8,
}
impl Month {
    fn parse(s: &str) -> Result<Self, String> {
        let (a, b) = s
            .split_once('-')
            .ok_or("Mese richiesto nel formato YYYY-MM")?;
        if a.len() != 4 || b.len() != 2 {
            return Err("Mese richiesto nel formato YYYY-MM".into());
        }
        let year = a.parse::<i32>().map_err(|_| "Anno non valido")?;
        let month = b.parse::<u8>().map_err(|_| "Mese non valido")?;
        if !(1900..=2100).contains(&year) || !(1..=12).contains(&month) {
            return Err("Mese fuori intervallo".into());
        }
        Ok(Self { year, month })
    }
    fn label(self) -> String {
        format!("{:04}-{:02}", self.year, self.month)
    }
    fn previous(self) -> Self {
        if self.month == 1 {
            Self {
                year: self.year - 1,
                month: 12,
            }
        } else {
            Self {
                year: self.year,
                month: self.month - 1,
            }
        }
    }
}

#[derive(Debug)]
struct Config {
    mode: Mode,
    month: Month,
    previous_month: Option<Month>,
    snapshot: PathBuf,
    previous: Option<PathBuf>,
    region: String,
    club: Option<String>,
    under: Option<i32>,
    rating: Rating,
    limit: Option<usize>,
    format: String,
    output: Option<PathBuf>,
    fsi_csv: Option<PathBuf>,
    fsi_as_of: Option<String>,
    fide_xml: Option<PathBuf>,
    fide_month: Option<Month>,
    fsi_manifest: Option<PathBuf>,
}

fn help() {
    println!(
        r#"antiskill rankings — classifiche riproducibili da snapshot Allin

USO:
  antiskill rankings top3 --snapshot allin.csv --month YYYY-MM [OPZIONI]
  antiskill rankings club --snapshot allin.csv --month YYYY-MM --club 17029 [OPZIONI]

OPZIONI:
  --previous FILE       Snapshot Allin precedente (mai un report derivato)
  --previous-month YYYY-MM  Obbligatorio con --previous; deve precedere --month
  --region TOS          Regione tesseramento (default TOS; solo top3)
  --club 17029          Codice società federale (solo club)
  --under 18            Limita la vista club ai giovani di età inferiore a 18 anni
  --rating italia|standard|rapid|blitz  Default: italia per top3; standard per club
  --limit N             Numero massimo per gruppo (top3: 3; club: tutti)
  --format json|csv     Default: json, versione antiskill.rankings.v1
  --output FILE         Crea un nuovo file; rifiuta sovrascritture
  --fsi-csv FILE        CSV ufficiale FSI, senza intestazione (15 colonne)
  --fsi-as-of YYYY-MM-DD  Data di acquisizione FSI, nello stesso mese del report
  --fsi-manifest FILE    Manifest di acquisizione FSI con hash e scope verificabili
  --fide-xml FILE       Lista ufficiale FIDE XML estratta localmente
  --fide-month YYYY-MM  Mese nominale della lista FIDE (uguale a --month)
  --require-official    Richiede FSI e FIDE e rating internazionale; fallisce chiuso

NOTE:
  --month identifica il periodo attribuito dall'operatore allo snapshot: non
  verifica automaticamente la data di pubblicazione del file.
  La colonna Elo FIDE di Allin è una copia operativa, NON la lista FIDE ufficiale.
  Le variazioni sono tra snapshot, non variazioni provvisorie di torneo.
  Senza snapshot precedente i delta rimangono null (mai falsi nuovi ingressi).
  I dati grezzi conservano i rispettivi diritti: nessuna pubblicazione automatica.
  FIDE XML deve essere un file estratto da fonte autorizzata: la CLI non
  effettua download indiscriminati né distribuisce il corpus FIDE.
  Gli overlay ufficiali non ammettono confronti mensili fino all'arrivo
  di snapshot ufficiali precedenti; i report restano in verifica editoriale.
"#
    );
}

impl Config {
    fn from_args(args: Vec<String>) -> Result<Option<Self>, String> {
        let mut it = args.into_iter();
        let Some(cmd) = it.next() else {
            return Ok(None);
        };
        if matches!(cmd.as_str(), "help" | "--help" | "-h") {
            return Ok(None);
        }
        let mode = match cmd.as_str() {
            "top3" => Mode::Top3,
            "club" => Mode::Club,
            _ => return Err(format!("Comando rankings sconosciuto: {cmd}")),
        };
        let mut snapshot = None;
        let mut previous = None;
        let mut month = None;
        let mut previous_month = None;
        let mut region = "TOS".to_string();
        let mut club = None;
        let mut under = None;
        let mut rating = if mode == Mode::Top3 {
            Rating::Italia
        } else {
            Rating::Standard
        };
        let mut limit = if mode == Mode::Top3 { Some(3) } else { None };
        let mut format = "json".to_string();
        let mut output = None;
        let mut fsi_csv = None;
        let mut fsi_as_of = None;
        let mut fsi_manifest = None;
        let mut fide_xml = None;
        let mut fide_month = None;
        let mut require_official = false;
        while let Some(arg) = it.next() {
            if arg == "--require-official" {
                require_official = true;
                continue;
            }
            let val = match arg.as_str() {
                "--snapshot" | "--allin" | "--previous" | "--previous-allin" | "--month"
                | "--previous-month" | "--region" | "--club" | "--under" | "--rating"
                | "--limit" | "--format" | "--output" | "--fsi-csv" | "--fsi-as-of"
                | "--fide-xml" | "--fide-month" | "--fsi-manifest" => it
                    .next()
                    .ok_or_else(|| format!("Manca un valore per {arg}"))?,
                "--help" | "-h" => return Ok(None),
                _ => return Err(format!("Opzione rankings sconosciuta: {arg}")),
            };
            match arg.as_str() {
                "--snapshot" | "--allin" => snapshot = Some(PathBuf::from(val)),
                "--previous" | "--previous-allin" => previous = Some(PathBuf::from(val)),
                "--month" => month = Some(Month::parse(&val)?),
                "--previous-month" => previous_month = Some(Month::parse(&val)?),
                "--region" => region = val.to_ascii_uppercase(),
                "--club" => club = Some(val),
                "--under" => under = Some(val.parse::<i32>().map_err(|_| "Età non valida")?),
                "--rating" => rating = Rating::parse(&val)?,
                "--limit" => limit = Some(val.parse::<usize>().map_err(|_| "Limite non valido")?),
                "--format" => format = val,
                "--output" => output = Some(PathBuf::from(val)),
                "--fsi-csv" => fsi_csv = Some(PathBuf::from(val)),
                "--fsi-as-of" => fsi_as_of = Some(val),
                "--fide-xml" => fide_xml = Some(PathBuf::from(val)),
                "--fide-month" => fide_month = Some(Month::parse(&val)?),
                "--fsi-manifest" => fsi_manifest = Some(PathBuf::from(val)),
                _ => unreachable!(),
            }
        }
        let month = month.ok_or("Parametro obbligatorio: --month YYYY-MM")?;
        let snapshot = snapshot.ok_or("Parametro obbligatorio: --snapshot FILE")?;
        if previous.is_some() != previous_month.is_some() {
            return Err("--previous e --previous-month vanno forniti insieme".into());
        }
        if let Some(p) = previous_month {
            if (p.year, p.month) >= (month.year, month.month) {
                return Err("--previous-month deve precedere --month".into());
            }
            if p.label() != month.previous().label() {
                return Err("La prima verticale confronta solo mesi consecutivi".into());
            }
        }
        if !matches!(format.as_str(), "json" | "csv") {
            return Err("Formato: json o csv".into());
        }
        if region.len() != 3 || !region.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err("Regione richiesta come codice di tre lettere, es. TOS".into());
        }
        if mode == Mode::Top3 && (club.is_some() || under.is_some()) {
            return Err("--club e --under sono riservati alla vista club".into());
        }
        if mode == Mode::Club && club.as_deref().unwrap_or("").is_empty() {
            return Err("La vista club richiede --club CODICE".into());
        }
        if mode == Mode::Club && region != "TOS" {
            return Err("--region si usa soltanto con top3".into());
        }
        if let Some(n) = under {
            if !(1..=100).contains(&n) {
                return Err("Età fuori intervallo".into());
            }
        }
        if let Some(n) = limit {
            if n == 0 || n > 100000 {
                return Err("--limit deve essere positivo".into());
            }
        }
        if fsi_csv.is_some() != fsi_as_of.is_some() {
            return Err("Indicare insieme --fsi-csv e --fsi-as-of".into());
        }
        if let Some(ref date) = fsi_as_of {
            let (ym, day) = date
                .rsplit_once('-')
                .ok_or("Data FSI richiesta YYYY-MM-DD")?;
            let period = Month::parse(ym)?;
            if date.len() != 10
                || day.len() != 2
                || !day.bytes().all(|b| b.is_ascii_digit())
                || !(1..=31).contains(&day.parse::<u8>().map_err(|_| "Data FSI errata")?)
            {
                return Err("Data FSI richiesta YYYY-MM-DD".into());
            }
            if period.label() != month.label() {
                return Err("FSI as-of non coincide con il mese del report".into());
            }
        }
        if fsi_manifest.is_some() && fsi_csv.is_none() {
            return Err("--fsi-manifest richiede --fsi-csv".into());
        }
        if fide_xml.is_some() != fide_month.is_some() {
            return Err("Indicare insieme --fide-xml e --fide-month".into());
        }
        if fide_month.is_some_and(|f| f.label() != month.label()) {
            return Err("Mese della lista ufficiale FIDE diverso da --month".into());
        }
        if (fsi_csv.is_some() || fide_xml.is_some()) && previous.is_some() {
            return Err("Confronto storico con fonti ufficiali richiede entrambe le liste precedenti: non supportato in v2".into());
        }
        if require_official
            && (fsi_csv.is_none()
                || fide_xml.is_none()
                || rating == Rating::Italia
                || fsi_manifest.is_none())
        {
            return Err(
                "--require-official richiede --fsi-csv, --fsi-manifest, --fide-xml e --rating standard|rapid|blitz"
                    .into(),
            );
        }
        Ok(Some(Self {
            mode,
            month,
            previous_month,
            snapshot,
            previous,
            region,
            club,
            under,
            rating,
            limit,
            format,
            output,
            fsi_csv,
            fsi_as_of,
            fide_xml,
            fide_month,
            fsi_manifest,
        }))
    }
}

#[derive(Debug, Clone)]
struct Player {
    name: String,
    fsi_id: Option<String>,
    fide_id: Option<String>,
    year: Option<i32>,
    sex: String,
    region: String,
    club: String,
    membership_year: Option<i32>,
    category: String,
    italian: i32,
    standard: i32,
    rapid: i32,
    blitz: i32,
}
impl Player {
    fn rating(&self, rating: Rating) -> i32 {
        match rating {
            Rating::Italia => self.italian,
            Rating::Standard => self.standard,
            Rating::Rapid => self.rapid,
            Rating::Blitz => self.blitz,
        }
    }
}
fn rated(n: i32) -> bool {
    n > 0 && n != 1399
}
fn id(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() || s == "0" {
        None
    } else {
        Some(s.to_owned())
    }
}
fn num(s: &str, line: usize, label: &str) -> Result<i32, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }
    s.parse::<i32>()
        .map_err(|_| format!("Riga {line}: {label} non numerico: {s}"))
}
fn name_key(p: &Player) -> Option<String> {
    let y = p.year?;
    if p.name.is_empty() || p.sex.is_empty() {
        return None;
    }
    Some(format!(
        "{}|{}|{}",
        p.name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_uppercase(),
        y,
        p.sex
    ))
}
#[derive(Debug, Serialize)]
struct Source {
    source_key: &'static str,
    filename: String,
    blake3: String,
    records: usize,
    authority: &'static str,
}
struct Snapshot {
    players: Vec<Player>,
    source: Source,
    index: IdentityIndex,
    fsi_source: Option<Source>,
    fide_source: Option<Source>,
    coverage: Option<Coverage>,
    fsi_manifest_proof: Option<FsiManifestProof>,
}
fn parse_snapshot(path: &Path) -> Result<Snapshot, String> {
    let mut file = File::open(path).map_err(|e| format!("Apertura {}: {e}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Lettura {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b';')
        .from_path(path)
        .map_err(|e| format!("CSV {}: {e}", path.display()))?;
    let heads = rdr
        .headers()
        .map_err(|e| format!("Header CSV: {e}"))?
        .clone();
    let required = [
        "Nominativo",
        "ID FSI",
        "FIN",
        "Anno Nascita",
        "Sex",
        "Regione Tesseramento",
        "Circolo Attuale",
        "Anno Ultimo Tesseramento",
        "Categoria",
        "Elo",
        "Elo FIDE Std",
        "Elo FIDE Rapid",
        "Elo FIDE Blitz",
    ];
    let indexes: Vec<usize> = required
        .iter()
        .map(|k| {
            heads
                .iter()
                .position(|s| s.trim() == *k)
                .ok_or_else(|| format!("Colonna Allin assente: {k}"))
        })
        .collect::<Result<_, _>>()?;
    let mut players = Vec::new();
    let mut records = 0usize;
    for result in rdr.records() {
        let row = result.map_err(|e| format!("Record CSV {}: {e}", path.display()))?;
        records += 1;
        let get = |i: usize| row.get(indexes[i]).unwrap_or("").trim();
        let line = records + 1;
        let year = num(get(3), line, "Anno Nascita")?;
        let membership = num(get(7), line, "Anno Ultimo Tesseramento")?;
        let player = Player {
            name: get(0).to_string(),
            fsi_id: id(get(1)),
            fide_id: id(get(2)),
            year: if year > 0 { Some(year) } else { None },
            sex: get(4).to_ascii_uppercase(),
            region: get(5).to_ascii_uppercase(),
            club: get(6).to_owned(),
            membership_year: if membership > 0 {
                Some(membership)
            } else {
                None
            },
            category: get(8).to_owned(),
            italian: num(get(9), line, "Elo")?,
            standard: num(get(10), line, "Elo FIDE Std")?,
            rapid: num(get(11), line, "Elo FIDE Rapid")?,
            blitz: num(get(12), line, "Elo FIDE Blitz")?,
        };
        players.push(player);
    }
    if records == 0 {
        return Err("Snapshot Allin vuoto: nessun risultato attendibile".into());
    }
    let index = IdentityIndex::build(&players);
    Ok(Snapshot {
        players,
        index,
        fsi_source: None,
        fide_source: None,
        coverage: None,
        fsi_manifest_proof: None,
        source: Source {
            source_key: "torneionline-allin",
            filename: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            blake3: hasher.finalize().to_hex().to_string(),
            records,
            authority: "operational-not-official-fide-or-fsi",
        },
    })
}
#[derive(Default)]
struct IdentityIndex {
    fsi: HashMap<String, Option<usize>>,
    fide: HashMap<String, Option<usize>>,
    fallback: HashMap<String, Option<usize>>,
}
fn insert_unique(map: &mut HashMap<String, Option<usize>>, key: String, i: usize) {
    use std::collections::hash_map::Entry;
    match map.entry(key) {
        Entry::Vacant(e) => {
            e.insert(Some(i));
        }
        Entry::Occupied(mut e) => {
            e.insert(None);
        }
    }
}
impl IdentityIndex {
    fn build(players: &[Player]) -> Self {
        let mut x = Self::default();
        for (i, p) in players.iter().enumerate() {
            if let Some(id) = &p.fsi_id {
                insert_unique(&mut x.fsi, id.clone(), i)
            }
            if let Some(id) = &p.fide_id {
                insert_unique(&mut x.fide, id.clone(), i)
            }
            if let Some(k) = name_key(p) {
                insert_unique(&mut x.fallback, k, i)
            }
        }
        x
    }
    fn lookup(&self, p: &Player, prior: &[Player]) -> Option<usize> {
        // Never fall back to a plausible name when a known identifier is ambiguous.
        if p.fsi_id
            .as_ref()
            .is_some_and(|id| self.fsi.get(id) == Some(&None))
            || p.fide_id
                .as_ref()
                .is_some_and(|id| self.fide.get(id) == Some(&None))
        {
            return None;
        }
        let fsi = p
            .fsi_id
            .as_ref()
            .and_then(|v| self.fsi.get(v))
            .copied()
            .flatten();
        let fide = p
            .fide_id
            .as_ref()
            .and_then(|v| self.fide.get(v))
            .copied()
            .flatten();
        match (fsi, fide) {
            (Some(a), Some(b)) if a != b => return None,
            (Some(a), _) | (_, Some(a)) => return Some(a),
            _ => {}
        }
        // Fallback requires a unique name + birth year + sex AND no conflicting IDs.
        let j = name_key(p)
            .and_then(|k| self.fallback.get(&k))
            .copied()
            .flatten()?;
        let old = prior.get(j)?;
        if p.fsi_id
            .as_ref()
            .zip(old.fsi_id.as_ref())
            .is_some_and(|(a, b)| a != b)
            || p.fide_id
                .as_ref()
                .zip(old.fide_id.as_ref())
                .is_some_and(|(a, b)| a != b)
        {
            return None;
        }
        Some(j)
    }
}

#[derive(Serialize)]
struct Entry {
    rank: usize,
    name: String,
    fsi_id: Option<String>,
    fide_id: Option<String>,
    birth_year: Option<i32>,
    sex: String,
    club: String,
    category: String,
    rating: i32,
    rating_delta: Option<i32>,
    previous_rank: Option<usize>,
    rank_change: Option<i32>,
    entered_top: Option<bool>,
}
#[derive(Serialize)]
struct Group {
    cohort: String,
    sex: String,
    population: usize,
    players: Vec<Entry>,
}
#[derive(Serialize)]
struct Report {
    schema: &'static str,
    period: String,
    previous_period: Option<String>,
    source: Source,
    previous_source: Option<Source>,
    fsi_as_of: Option<String>,
    fide_month: Option<String>,
    fsi_source: Option<Source>,
    fide_source: Option<Source>,
    coverage: Option<Coverage>,
    fsi_manifest_proof: Option<FsiManifestProof>,
    membership_authority: &'static str,
    rating_authority: &'static str,
    certified: bool,
    scope: String,
    rating: &'static str,
    filters: String,
    note: &'static str,
    groups: Vec<Group>,
}
fn cohort(year: i32, birth: Option<i32>) -> Option<&'static str> {
    let b = birth?;
    if b > year {
        return None;
    }
    for (n, label) in [
        (8, "U08"),
        (10, "U10"),
        (12, "U12"),
        (14, "U14"),
        (16, "U16"),
        (18, "U18"),
    ] {
        if b >= year - n {
            return Some(label);
        }
    }
    None
}
fn in_group(p: &Player, cfg: &Config, m: Month, label: &str, sex: &str) -> bool {
    if p.name.trim().is_empty() || p.membership_year != Some(m.year) || !rated(p.rating(cfg.rating))
    {
        return false;
    }
    if sex == "female" && p.sex != "F" {
        return false;
    }
    match cfg.mode {
        Mode::Top3 => p.region == cfg.region && cohort(m.year, p.year) == Some(label),
        Mode::Club => {
            p.club == cfg.club.as_deref().unwrap_or("")
                && p.year.is_none_or(|y| y <= m.year)
                && cfg
                    .under
                    .is_none_or(|u| p.year.is_some_and(|y| y >= m.year - u))
        }
    }
}
fn ordered_indices(s: &Snapshot, cfg: &Config, m: Month, label: &str, sex: &str) -> Vec<usize> {
    let mut result: Vec<usize> = s
        .players
        .iter()
        .enumerate()
        .filter_map(|(i, p)| in_group(p, cfg, m, label, sex).then_some(i))
        .collect();
    result.sort_by(|&a, &b| {
        let pa = &s.players[a];
        let pb = &s.players[b];
        pb.rating(cfg.rating)
            .cmp(&pa.rating(cfg.rating))
            .then_with(|| pa.name.to_uppercase().cmp(&pb.name.to_uppercase()))
            .then_with(|| pa.fsi_id.cmp(&pb.fsi_id))
            .then_with(|| pa.fide_id.cmp(&pb.fide_id))
    });
    result
}
fn make_group(
    cfg: &Config,
    current: &Snapshot,
    previous: Option<&Snapshot>,
    label: &str,
    sex: &str,
) -> Group {
    let current_order = ordered_indices(current, cfg, cfg.month, label, sex);
    let mut previous_ranks = HashMap::<usize, usize>::new();
    if let (Some(p), Some(m)) = (previous, cfg.previous_month) {
        for (pos, i) in ordered_indices(p, cfg, m, label, sex)
            .into_iter()
            .enumerate()
        {
            previous_ranks.insert(i, pos + 1);
        }
    }
    let mut entries = Vec::new();
    for (pos, &i) in current_order.iter().enumerate() {
        if cfg.limit.is_some_and(|n| pos >= n) {
            break;
        }
        let p = &current.players[i];
        let match_id = previous.and_then(|prev| prev.index.lookup(p, &prev.players));
        let prev_player = previous.and_then(|prev| match_id.map(|j| &prev.players[j]));
        let prior_rank = match_id.and_then(|j| previous_ranks.get(&j).copied());
        let delta = prev_player
            .map(|p| p.rating(cfg.rating))
            .filter(|&r| rated(r))
            .map(|r| p.rating(cfg.rating) - r);
        let old_top = cfg.limit.unwrap_or(usize::MAX);
        entries.push(Entry {
            rank: pos + 1,
            name: p.name.clone(),
            fsi_id: p.fsi_id.clone(),
            fide_id: p.fide_id.clone(),
            birth_year: p.year,
            sex: p.sex.clone(),
            club: p.club.clone(),
            category: p.category.clone(),
            rating: p.rating(cfg.rating),
            rating_delta: delta,
            previous_rank: prior_rank,
            rank_change: prior_rank.map(|r| r as i32 - (pos + 1) as i32),
            entered_top: previous.map(|_| prior_rank.is_none_or(|r| r > old_top)),
        });
    }
    Group {
        cohort: label.to_string(),
        sex: sex.to_string(),
        population: current_order.len(),
        players: entries,
    }
}
fn report(cfg: &Config, current: Snapshot, previous: Option<Snapshot>) -> Report {
    let groups = match cfg.mode {
        Mode::Top3 => ["U08", "U10", "U12", "U14", "U16", "U18"]
            .iter()
            .flat_map(|c| {
                ["absolute", "female"]
                    .iter()
                    .map(|s| make_group(cfg, &current, previous.as_ref(), c, s))
            })
            .collect(),
        Mode::Club => {
            let label = cfg
                .under
                .map(|u| format!("U{u:02}"))
                .unwrap_or_else(|| "ALL".to_owned());
            vec![make_group(
                cfg,
                &current,
                previous.as_ref(),
                &label,
                "absolute",
            )]
        }
    };
    Report {
        schema:"antiskill.rankings.v1",period:cfg.month.label(),
        previous_period:cfg.previous_month.map(Month::label),
        source:current.source,previous_source:previous.map(|p|p.source),
        fsi_as_of:cfg.fsi_as_of.clone(),
        fide_month:cfg.fide_month.map(Month::label),
        membership_authority:if current.fsi_source.is_some(){"fsi-official-csv"}else{"allin-operational"},
        rating_authority:if cfg.rating==Rating::Italia {"allin-operational"}
            else if current.fide_source.is_some(){"fide-official-xml"}else{"allin-operational"},
        certified:false, // Needs reviewed source and licensing/permission proof before publishing.
        fsi_source:current.fsi_source,fide_source:current.fide_source,coverage:current.coverage,
        fsi_manifest_proof:current.fsi_manifest_proof,
        scope:match cfg.mode {Mode::Top3=>"top3-region".into(),Mode::Club=>"club".into()},
        rating:cfg.rating.label(),
        filters:match cfg.mode {Mode::Top3=>format!("membership-region={}, membership-year={}",cfg.region,cfg.month.year),
            Mode::Club=>format!("club={}, membership-year={}, under={}",cfg.club.as_deref().unwrap_or(""),cfg.month.year,
                cfg.under.map(|u|u.to_string()).unwrap_or_else(||"all".into()))},
        note:"Local ranking draft; FIDE/FSI CSV/XML inputs, source periods, rights and publication of minors require independent approval.",
        groups,
    }
}
fn render_csv(r: &Report) -> Result<Vec<u8>, String> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record([
            "period",
            "scope",
            "rating_type",
            "cohort",
            "sex_filter",
            "rank",
            "name",
            "fsi_id",
            "fide_id",
            "birth_year",
            "sex",
            "club",
            "rating",
            "rating_delta",
            "previous_rank",
            "rank_change",
            "entered_top",
            "source_blake3",
            "previous_source_blake3",
            "fsi_source_blake3",
            "fide_source_blake3",
            "membership_authority",
            "rating_authority",
            "fsi_as_of",
            "fide_month",
            "certified",
        ])
        .map_err(|e| format!("CSV header: {e}"))?;
    for g in &r.groups {
        for p in &g.players {
            writer
                .write_record([
                    r.period.clone(),
                    r.scope.clone(),
                    r.rating.into(),
                    g.cohort.clone(),
                    g.sex.clone(),
                    p.rank.to_string(),
                    p.name.clone(),
                    p.fsi_id.clone().unwrap_or_default(),
                    p.fide_id.clone().unwrap_or_default(),
                    p.birth_year.map(|x| x.to_string()).unwrap_or_default(),
                    p.sex.clone(),
                    p.club.clone(),
                    p.rating.to_string(),
                    p.rating_delta.map(|x| x.to_string()).unwrap_or_default(),
                    p.previous_rank.map(|x| x.to_string()).unwrap_or_default(),
                    p.rank_change.map(|x| x.to_string()).unwrap_or_default(),
                    p.entered_top.map(|x| x.to_string()).unwrap_or_default(),
                    r.source.blake3.clone(),
                    r.previous_source
                        .as_ref()
                        .map(|x| x.blake3.clone())
                        .unwrap_or_default(),
                    r.fsi_source
                        .as_ref()
                        .map(|x| x.blake3.clone())
                        .unwrap_or_default(),
                    r.fide_source
                        .as_ref()
                        .map(|x| x.blake3.clone())
                        .unwrap_or_default(),
                    r.membership_authority.to_string(),
                    r.rating_authority.to_string(),
                    r.fsi_as_of.clone().unwrap_or_default(),
                    r.fide_month.clone().unwrap_or_default(),
                    r.certified.to_string(),
                ])
                .map_err(|e| format!("CSV record: {e}"))?;
        }
    }
    writer
        .into_inner()
        .map_err(|e| format!("CSV finalizzazione: {e}"))
}
fn emit(path: Option<&Path>, data: &[u8]) -> Result<(), String> {
    match path {
        None => std::io::stdout()
            .write_all(data)
            .map_err(|e| format!("Output: {e}")),
        Some(path) => {
            if path.exists() {
                return Err(format!(
                    "File già esistente (nessuna sovrascrittura): {}",
                    path.display()
                ));
            }
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            fs::create_dir_all(parent).map_err(|e| format!("Directory output: {e}"))?;
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| format!("Creazione {}: {e}", path.display()))?;
            if let Err(e) = f.write_all(data).and_then(|_| f.sync_all()) {
                let _ = fs::remove_file(path);
                return Err(format!("Scrittura {}: {e}", path.display()));
            }
            Ok(())
        }
    }
}

#[derive(Debug, Serialize)]
struct Coverage {
    membership_records: usize,
    members_linked_to_allin: usize,
    members_with_fide_id: usize,
    fide_records_matched: usize,
    standard_rated: usize,
    rapid_rated: usize,
    blitz_rated: usize,
    membership_types: std::collections::BTreeMap<String, usize>,
    club_provinces: std::collections::BTreeMap<String, usize>,
    residence_provinces: std::collections::BTreeMap<String, usize>,
    distinct_clubs: usize,
}
fn local_source(
    path: &Path,
    key: &'static str,
    records: usize,
    authority: &'static str,
) -> Result<Source, String> {
    let mut file = File::open(path)
        .map_err(|e| format!("Apertura fonte ufficiale {}: {e}", path.display()))?;
    let mut hash = blake3::Hasher::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| format!("Lettura fonte {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(Source {
        source_key: key,
        filename: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        blake3: hash.finalize().to_hex().to_string(),
        records,
        authority,
    })
}
#[derive(Debug, Serialize)]
struct FsiManifestProof {
    source_key: String,
    fetched_at_unix_ms: u64,
    manifest_blake3: String,
    verified: bool,
}
fn days_from_civil(year: i32, month: i32, day: i32) -> i64 {
    // Gregorian calendar day count relative to 1970-01-01.
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i64::from(era * 146097 + doe - 719468)
}
fn verify_fsi_manifest(
    path: &Path,
    source: &Source,
    csv_path: &Path,
    expected_key: &str,
    as_of: &str,
    report_month: Month,
) -> Result<FsiManifestProof, String> {
    let data =
        fs::read(path).map_err(|e| format!("Lettura manifest FSI {}: {e}", path.display()))?;
    let v: serde_json::Value =
        serde_json::from_slice(&data).map_err(|e| format!("Manifest FSI non JSON: {e}"))?;
    let source_key = v
        .get("source_key")
        .and_then(|v| v.as_str())
        .ok_or("Manifest FSI senza source_key")?;
    if source_key != expected_key {
        return Err(format!(
            "Manifest FSI scope errato: {source_key}; previsto {expected_key}"
        ));
    }
    if v.get("schema_version").and_then(|v| v.as_u64()) != Some(1) {
        return Err("Schema manifest FSI non supportato".into());
    }
    if v.get("source_url").and_then(|v| v.as_str())
        != Some("https://www.federscacchi.com/fsi/FSI_Programmi/tesserati_csv.php")
    {
        return Err("Manifest FSI non proviene dall'endpoint ufficiale previsto".into());
    }
    if v.get("http_status").and_then(|v| v.as_u64()) != Some(200) {
        return Err("Manifest FSI senza HTTP 200".into());
    }
    if v.get("blake3").and_then(|v| v.as_str()) != Some(source.blake3.as_str()) {
        return Err(
            "BLAKE3 del CSV FSI diverso dal manifest: possibile file errato/manomesso".into(),
        );
    }
    let bytes = v
        .get("bytes")
        .and_then(|v| v.as_u64())
        .ok_or("Manifest FSI senza dimensione")?;
    let fetched_at = v
        .get("fetched_at_unix_ms")
        .and_then(|v| v.as_u64())
        .ok_or("Manifest FSI senza timestamp acquisizione")?;
    if bytes == 0
        || fetched_at == 0
        || fs::metadata(csv_path)
            .map_err(|e| format!("FSI CSV stat: {e}"))?
            .len()
            != bytes
    {
        return Err("Dimensione CSV non corrisponde al manifest FSI".into());
    }
    let parts = as_of.split('-').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err("Data FSI as-of non valida".into());
    }
    let (y, m, d) = (
        parts[0].parse::<i32>().map_err(|_| "Anno non valido")?,
        parts[1].parse::<i32>().map_err(|_| "Mese non valido")?,
        parts[2].parse::<i32>().map_err(|_| "Giorno non valido")?,
    );
    if y != report_month.year || m != i32::from(report_month.month) {
        return Err("Manifest FSI associato a periodo diverso dal report".into());
    }
    let declared = days_from_civil(y, m, d);
    let acquired = (fetched_at / 86_400_000) as i64;
    if (declared - acquired).abs() > 1 {
        return Err("FSI --fsi-as-of non corrisponde alla data registrata dal manifest".into());
    }
    Ok(FsiManifestProof {
        source_key: source_key.to_owned(),
        fetched_at_unix_ms: fetched_at,
        manifest_blake3: blake3::hash(&data).to_hex().to_string(),
        verified: true,
    })
}

fn enrich_official(cfg: &Config, mut current: Snapshot) -> Result<Snapshot, String> {
    let mut covered = Coverage {
        membership_records: current.players.len(),
        members_linked_to_allin: 0,
        members_with_fide_id: 0,
        fide_records_matched: 0,
        standard_rated: 0,
        rapid_rated: 0,
        blitz_rated: 0,
        membership_types: Default::default(),
        club_provinces: Default::default(),
        residence_provinces: Default::default(),
        distinct_clubs: 0,
    };
    if let Some(path) = &cfg.fsi_csv {
        let members = official::read_fsi(path)?;
        let old_players = std::mem::take(&mut current.players);
        let old_index = IdentityIndex::build(&old_players);
        let mut players = Vec::with_capacity(members.len());
        let mut club_ids = std::collections::HashSet::<String>::new();
        for member in members {
            if cfg.mode == Mode::Club && member.club != cfg.club.as_deref().unwrap_or("") {
                return Err(format!(
                    "FSI CSV contiene società {} diversa da --club: importazione rifiutata",
                    member.club
                ));
            }
            if cfg.mode == Mode::Top3 && member.club_region != cfg.region {
                return Err(format!(
                    "FSI CSV contiene regione {} diversa da --region: importazione rifiutata",
                    member.club_region
                ));
            }
            club_ids.insert(member.club.clone());
            *covered
                .membership_types
                .entry(member.membership_type.clone())
                .or_default() += 1;
            *covered
                .club_provinces
                .entry(member.club_province.clone())
                .or_default() += 1;
            *covered
                .residence_provinces
                .entry(member.residence_province.clone())
                .or_default() += 1;
            let idx = old_index.fsi.get(&member.fsi_id).copied().flatten();
            let old = idx
                .and_then(|n| old_players.get(n))
                .filter(|p| p.year.zip(member.birth_year).is_none_or(|(a, b)| a == b));
            if old.is_some() {
                covered.members_linked_to_allin += 1;
            }
            let fide_id = old.and_then(|p| p.fide_id.clone());
            if fide_id.is_some() {
                covered.members_with_fide_id += 1;
            }
            players.push(Player {
                name: member.name,
                fsi_id: Some(member.fsi_id),
                fide_id,
                year: member.birth_year,
                sex: old.map(|p| p.sex.clone()).unwrap_or_default(),
                region: member.club_region,
                club: member.club,
                membership_year: Some(cfg.month.year),
                category: member.category,
                italian: old.map_or(0, |p| p.italian),
                standard: old.map_or(0, |p| p.standard),
                rapid: old.map_or(0, |p| p.rapid),
                blitz: old.map_or(0, |p| p.blitz),
            });
        }
        if cfg.mode == Mode::Top3 && club_ids.len() < 2 {
            return Err("Fonte FSI regionale troppo ristretta (un solo club): usare export regionale completo".into());
        }
        covered.membership_records = players.len();
        covered.distinct_clubs = club_ids.len();
        current.fsi_source = Some(local_source(
            path,
            "fsi-members",
            players.len(),
            "official-fsi-scoped-export",
        )?);
        if let (Some(manifest), Some(source)) = (&cfg.fsi_manifest, current.fsi_source.as_ref()) {
            let expected_key = match cfg.mode {
                Mode::Top3 => format!(
                    "fsi-members-region-{}-{}",
                    cfg.region.to_ascii_lowercase(),
                    cfg.month.year
                ),
                Mode::Club => format!(
                    "fsi-members-club-{}-{}",
                    cfg.club.as_deref().unwrap_or(""),
                    cfg.month.year
                ),
            };
            current.fsi_manifest_proof = Some(verify_fsi_manifest(
                manifest,
                source,
                path,
                &expected_key,
                cfg.fsi_as_of.as_deref().unwrap_or(""),
                cfg.month,
            )?);
        }
        current.players = players;
        current.index = IdentityIndex::build(&current.players);
    }
    if let Some(path) = &cfg.fide_xml {
        let ids = current
            .players
            .iter()
            .filter_map(|p| p.fide_id.clone())
            .collect::<std::collections::HashSet<_>>();
        if ids.is_empty() {
            return Err(
                "Nessun FIDE ID riconciliato da Allin: rating ufficiale non assegnabile".into(),
            );
        }
        let ratings = official::read_fide_xml(path, &ids)?;
        if ratings.found.is_empty() {
            return Err(
                "FIDE XML non contiene nessuno dei giocatori collegati: report rifiutato".into(),
            );
        }
        covered.fide_records_matched = ratings.found.len();
        for p in &mut current.players {
            // Never fall back to stale operational Allin ratings in official mode.
            p.standard = 0;
            p.rapid = 0;
            p.blitz = 0;
            if let Some(ref fide) = p.fide_id {
                if let Some(r) = ratings.found.get(fide) {
                    if p.year.zip(r.birth_year).is_some_and(|(a, b)| a != b) {
                        // Flagging is safer than assigning an unrelated player's rating.
                        continue;
                    }
                    p.standard = r.standard;
                    p.rapid = r.rapid;
                    p.blitz = r.blitz;
                    if !r.sex.is_empty() {
                        p.sex = r.sex.clone();
                    }
                    if p.year.is_none() {
                        p.year = r.birth_year;
                    }
                }
            }
        }
        current.fide_source = Some(local_source(
            path,
            "fide-rating-list",
            ratings.seen,
            "official-fide-monthly-xml-user-supplied",
        )?);
        current.index = IdentityIndex::build(&current.players);
    }
    covered.members_with_fide_id = current
        .players
        .iter()
        .filter(|p| p.fide_id.is_some())
        .count();
    covered.standard_rated = current.players.iter().filter(|p| rated(p.standard)).count();
    covered.rapid_rated = current.players.iter().filter(|p| rated(p.rapid)).count();
    covered.blitz_rated = current.players.iter().filter(|p| rated(p.blitz)).count();
    current.coverage = Some(covered);
    Ok(current)
}

fn execute(cfg: Config) -> Result<(), String> {
    let current = parse_snapshot(&cfg.snapshot)?;
    let current = if cfg.fsi_csv.is_some() || cfg.fide_xml.is_some() {
        enrich_official(&cfg, current)?
    } else {
        current
    };
    let previous = cfg
        .previous
        .as_ref()
        .map(|p| parse_snapshot(p))
        .transpose()?;
    if let Some(p) = &previous {
        if p.source.blake3 == current.source.blake3 {
            return Err(
                "Gli snapshot corrente e precedente sono identici: confronto rifiutato".into(),
            );
        }
    }
    let r = report(&cfg, current, previous);
    let bytes = if cfg.format == "csv" {
        render_csv(&r)?
    } else {
        let mut bytes = serde_json::to_vec_pretty(&r).map_err(|e| format!("JSON: {e}"))?;
        bytes.push(b'\n');
        bytes
    };
    emit(cfg.output.as_deref(), &bytes)
}
pub fn run<I: IntoIterator<Item = String>>(args: I) -> ExitCode {
    match Config::from_args(args.into_iter().collect()) {
        Ok(None) => {
            help();
            ExitCode::SUCCESS
        }
        Ok(Some(cfg)) => match execute(cfg) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ERRORE rankings: {e}");
                ExitCode::from(1)
            }
        },
        Err(e) => {
            eprintln!("ERRORE rankings: {e}");
            ExitCode::from(2)
        }
    }
}
#[cfg(test)]
mod unit_tests {
    use super::*;
    #[test]
    fn age_bands_include_younger_u8_and_age_boundaries() {
        assert_eq!(cohort(2026, Some(2021)), Some("U08"));
        assert_eq!(cohort(2026, Some(2018)), Some("U08"));
        assert_eq!(cohort(2026, Some(2017)), Some("U10"));
        assert_eq!(cohort(2026, Some(2008)), Some("U18"));
        assert_eq!(cohort(2026, Some(2007)), None);
        assert_eq!(cohort(2026, Some(2027)), None);
    }
    #[test]
    fn comparisons_require_explicit_previous_month() {
        let v = vec![
            "top3",
            "--snapshot",
            "x",
            "--month",
            "2026-11",
            "--previous",
            "p",
        ];
        assert!(Config::from_args(v.into_iter().map(str::to_owned).collect()).is_err());
    }
    #[test]
    fn sentinel_is_not_a_rating() {
        assert!(!rated(0));
        assert!(!rated(1399));
        assert!(rated(1400));
    }
}
