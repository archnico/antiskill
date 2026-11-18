//! Reproducible local rankings from immutable Allin CSV snapshots.
//! This vertical consumes sources; it does not assume that Allin is an official FIDE rating list.
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

NOTE:
  --month identifica il periodo attribuito dall'operatore allo snapshot: non
  verifica automaticamente la data di pubblicazione del file.
  La colonna Elo FIDE di Allin è una copia operativa, NON la lista FIDE ufficiale.
  Le variazioni sono tra snapshot, non variazioni provvisorie di torneo.
  Senza snapshot precedente i delta rimangono null (mai falsi nuovi ingressi).
  I dati grezzi conservano i rispettivi diritti: nessuna pubblicazione automatica.
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
        while let Some(arg) = it.next() {
            let val = match arg.as_str() {
                "--snapshot" | "--allin" | "--previous" | "--previous-allin" | "--month"
                | "--previous-month" | "--region" | "--club" | "--under" | "--rating"
                | "--limit" | "--format" | "--output" => it
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
        scope:match cfg.mode {Mode::Top3=>"top3-region".into(),Mode::Club=>"club".into()},
        rating:cfg.rating.label(),
        filters:match cfg.mode {Mode::Top3=>format!("membership-region={}, membership-year={}",cfg.region,cfg.month.year),
            Mode::Club=>format!("club={}, membership-year={}, under={}",cfg.club.as_deref().unwrap_or(""),cfg.month.year,
                cfg.under.map(|u|u.to_string()).unwrap_or_else(||"all".into()))},
        note:"Allin operational snapshot; verify FIDE ratings and FSI memberships against authoritative releases before public publication.",
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
fn execute(cfg: Config) -> Result<(), String> {
    let current = parse_snapshot(&cfg.snapshot)?;
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
