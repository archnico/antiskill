//! Locally supplied official-source parsers. No unrestricted download or publication.
//! FSI export is a headerless 15-column CSV. FIDE input is the extracted XML
//! from an official monthly rating-list archive supplied by an authorized user.
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};

pub(super) struct FsiMember {
    pub(super) fsi_id: String,
    pub(super) name: String,
    pub(super) birth_year: Option<i32>,
    pub(super) category: String,
    pub(super) club: String,
    pub(super) club_region: String,
    pub(super) membership_type: String,
    pub(super) residence_province: String,
    pub(super) club_province: String,
}
pub(super) struct FidePlayer {
    pub(super) fide_id: String,
    pub(super) birth_year: Option<i32>,
    pub(super) sex: String,
    pub(super) standard: i32,
    pub(super) rapid: i32,
    pub(super) blitz: i32,
}
pub(super) struct FideResult {
    pub(super) seen: usize,
    pub(super) found: HashMap<String, FidePlayer>,
}
fn decode_field(bytes: &[u8]) -> String {
    // The FSI endpoint returns legacy single-byte Italian text on some rows.
    // Do not replace accented names with U+FFFD.
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.trim().to_string();
    }
    bytes
        .iter()
        .map(|b| char::from(*b))
        .collect::<String>()
        .trim()
        .to_string()
}
fn positive_year(s: &str, what: &str) -> Result<Option<i32>, String> {
    if s.is_empty() || s == "0" {
        return Ok(None);
    }
    let y = s
        .parse::<i32>()
        .map_err(|_| format!("{what}: anno non numerico"))?;
    if !(1850..=2100).contains(&y) {
        return Err(format!("{what}: anno fuori intervallo"));
    }
    Ok(Some(y))
}
pub(super) fn read_fsi(path: &Path) -> Result<Vec<FsiMember>, String> {
    let bytes = fs::read(path).map_err(|e| format!("Apertura CSV FSI {}: {e}", path.display()))?;
    if bytes.is_empty() || bytes.len() > 25_000_000 || bytes.starts_with(b"<") {
        return Err("CSV FSI assente, HTML o dimensione non attendibile".into());
    }
    let mut rd = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b';')
        .from_reader(bytes.as_slice());
    let mut data = Vec::new();
    let mut ids = HashSet::new();
    for (line, result) in rd.byte_records().enumerate() {
        let row = result.map_err(|e| format!("CSV FSI riga {}: {e}", line + 1))?;
        if row.len() != 15 {
            return Err(format!(
                "CSV FSI riga {}: attese 15 colonne, trovate {}",
                line + 1,
                row.len()
            ));
        }
        let col = |i: usize| decode_field(row.get(i).unwrap_or_default());
        let id = col(0);
        if id.is_empty() || !id.bytes().all(|x| x.is_ascii_digit()) || id == "0" {
            return Err(format!("ID FSI invalido alla riga {}", line + 1));
        }
        if !ids.insert(id.clone()) {
            return Err(format!("ID FSI duplicato nel file: {id}"));
        }
        let club = col(8);
        if club.is_empty() {
            return Err(format!(
                "Codice società FSI mancante alla riga {}",
                line + 1
            ));
        }
        let region = col(11).to_uppercase();
        let club_region = match region.as_str() {
            "TOSCANA" => "TOS",
            "EMILIA ROMAGNA" => "EMI",
            "LAZIO" => "LAZ",
            "LOMBARDIA" => "LOM",
            "PIEMONTE" => "PIE",
            "VENETO" => "VEN",
            "SICILIA" => "SIC",
            "SARDEGNA" => "SAR",
            "PUGLIA" => "PUG",
            "LIGURIA" => "LIG",
            "UMBRIA" => "UMB",
            "MARCHE" => "MAR",
            "ABRUZZO" => "ABR",
            "BASILICATA" => "BAS",
            "CALABRIA" => "CAL",
            "CAMPANIA" => "CAM",
            "MOLISE" => "MOL",
            "TRENTINO" => "TAA",
            "ALTO ADIGE" => "AAT",
            "FRIULI VENEZIA G." => "FVG",
            "VALLE D'AOSTA" => "VDA",
            _ => return Err(format!("Regione società FSI sconosciuta nel CSV: {region}")),
        };
        let surname = col(1);
        let given = col(2);
        if surname.is_empty() || given.is_empty() {
            return Err(format!("Nome FSI incompleto alla riga {}", line + 1));
        }
        data.push(FsiMember {
            fsi_id: id,
            name: format!("{surname} {given}"),
            birth_year: positive_year(&col(3), "FSI")?,
            category: col(4),
            club,
            club_region: club_region.into(),
            membership_type: col(12),
            residence_province: col(5),
            club_province: col(10),
        });
    }
    if data.is_empty() {
        return Err("Nessun tesserato nella fonte FSI".into());
    }
    Ok(data)
}
fn field_number(row: &HashMap<String, String>, keys: &[&str], field: &str) -> Result<i32, String> {
    let val = keys
        .iter()
        .filter_map(|k| row.get(*k))
        .find(|s| !s.trim().is_empty())
        .map(String::as_str)
        .unwrap_or("0");
    val.trim()
        .parse::<i32>()
        .map_err(|_| format!("FIDE {field}: rating non numerico: {val}"))
}
fn parse_player(row: &HashMap<String, String>) -> Result<Option<FidePlayer>, String> {
    let id = row
        .get("fideid")
        .or_else(|| row.get("id_number"))
        .map(|x| x.trim())
        .unwrap_or("");
    if id.is_empty() {
        return Ok(None);
    }
    if !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("FIDE ID non numerico".into());
    }
    let birth = row
        .get("birthday")
        .or_else(|| row.get("born"))
        .map(String::as_str)
        .unwrap_or("");
    let sex = row.get("sex").map(|s| s.to_uppercase()).unwrap_or_default();
    // A few official FIDE rows carry implausible or missing birth years.
    // Such years are UNKNOWN, not grounds to abort a 2-million-player release.
    let birth_year = birth
        .trim()
        .parse::<i32>()
        .ok()
        .filter(|&y| (1850..=2100).contains(&y));
    Ok(Some(FidePlayer {
        fide_id: id.into(),
        birth_year,
        sex,
        standard: field_number(row, &["rating", "std_rating", "srtng"], "Standard")?,
        rapid: field_number(row, &["rapid_rating", "rrtng"], "Rapid")?,
        blitz: field_number(row, &["blitz_rating", "brtng"], "Blitz")?,
    }))
}
pub(super) fn read_fide_xml(path: &Path, wanted: &HashSet<String>) -> Result<FideResult, String> {
    let mut file =
        File::open(path).map_err(|e| format!("Apertura FIDE {}: {e}", path.display()))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| format!("Lettura intestazione FIDE: {e}"))?;
    if &magic == b"PK\x03\x04" {
        return read_fide_zip(path, wanted);
    }
    let f = File::open(path).map_err(|e| format!("Apertura XML FIDE: {e}"))?;
    read_fide_reader(BufReader::new(f), wanted)
}
fn read_fide_zip(path: &Path, wanted: &HashSet<String>) -> Result<FideResult, String> {
    // Open exactly one XML member, never extract an 800MB archive to disk.
    let archive = Command::new("unzip")
        .arg("-Z")
        .arg("-1")
        .arg(path)
        .output()
        .map_err(|e| format!("Listing ZIP FIDE (unzip richiesto): {e}"))?;
    if !archive.status.success() {
        return Err("ZIP FIDE invalido (listing fallito)".into());
    }
    let names = String::from_utf8(archive.stdout).map_err(|_| "Nomi ZIP FIDE non UTF8")?;
    let items: Vec<&str> = names.lines().filter(|s| !s.trim().is_empty()).collect();
    if items.len() != 1
        || !items[0].to_ascii_lowercase().ends_with(".xml")
        || items[0].starts_with('/')
        || items[0].contains("..")
    {
        return Err("ZIP FIDE deve contenere un solo membro XML senza percorsi sospetti".into());
    }
    let mut child = Command::new("unzip")
        .arg("-p")
        .arg(path)
        .arg(items[0])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Streaming ZIP FIDE: {e}"))?;
    let reader = child.stdout.take().ok_or("Pipe ZIP FIDE non disponibile")?;
    let parsed = read_fide_reader(BufReader::with_capacity(128 * 1024, reader), wanted);
    if let Err(error) = parsed {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("Parsing FIDE ZIP: {error}"));
    }
    let status = child.wait().map_err(|e| format!("Attesa unzip: {e}"))?;
    if !status.success() {
        return Err("ZIP FIDE: estrazione stream non integra".into());
    }
    parsed
}
fn read_fide_reader<R: BufRead>(source: R, wanted: &HashSet<String>) -> Result<FideResult, String> {
    let mut reader = Reader::from_reader(source);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::with_capacity(16384);
    let mut inside = false;
    let mut field = String::new();
    let mut row = HashMap::<String, String>::new();
    let mut seen = 0usize;
    let mut found = HashMap::new();
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| format!("XML FIDE: {e}"))?
        {
            Event::Start(t) => {
                let local = t.local_name();
                let tag = local.as_ref();
                if tag == "player" {
                    if inside {
                        return Err("XML FIDE: player annidati".into());
                    }
                    inside = true;
                    row.clear();
                    field.clear();
                } else if inside {
                    // Ignore name, titles, activity and games for rating-only imports.
                    field = match tag {
                        "fideid" | "id_number" | "birthday" | "born" | "sex" | "rating"
                        | "std_rating" | "srtng" | "rapid_rating" | "rrtng" | "blitz_rating"
                        | "brtng" => tag.to_owned(),
                        _ => String::new(),
                    };
                }
            }
            Event::Text(t) if inside && !field.is_empty() => {
                let known = field == "fideid"
                    || field == "id_number"
                    || row
                        .get("fideid")
                        .or_else(|| row.get("id_number"))
                        .is_some_and(|id| wanted.contains(id));
                if known {
                    row.entry(field.clone()).or_default().push_str(t.as_ref());
                }
            }
            Event::CData(t) if inside && !field.is_empty() => {
                let known = field == "fideid"
                    || field == "id_number"
                    || row
                        .get("fideid")
                        .or_else(|| row.get("id_number"))
                        .is_some_and(|id| wanted.contains(id));
                if known {
                    row.entry(field.clone()).or_default().push_str(t.as_ref());
                }
            }
            Event::End(t) => {
                let local = t.local_name();
                let tag = local.as_ref();
                if tag == "player" && inside {
                    seen += 1;
                    if row
                        .get("fideid")
                        .or_else(|| row.get("id_number"))
                        .is_some_and(|id| wanted.contains(id))
                    {
                        if let Some(player) = parse_player(&row)? {
                            if found.insert(player.fide_id.clone(), player).is_some() {
                                return Err("FIDE XML: ID duplicato per persona richiesta".into());
                            }
                        }
                    }
                    inside = false;
                    field.clear();
                    row.clear();
                } else if inside && tag == field {
                    field.clear();
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if inside {
        return Err("XML FIDE troncato, player non chiuso".into());
    }
    if seen == 0 {
        return Err("FIDE XML senza record <player>: formato non supportato".into());
    }
    Ok(FideResult { seen, found })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_real_fsi_layout() {
        let fixture=b"900001;COGNOME FITTIZIO;NOME;2018;NC;FI;TOSCANA;ITA;17029;A.S.D. SINTETICA;FI;TOSCANA;TG;100;TG\n";
        let p = std::env::temp_dir().join(format!("fsi-fixture-{}", std::process::id()));
        fs::write(&p, fixture).unwrap();
        let list = read_fsi(&p).unwrap();
        let _ = fs::remove_file(p);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].fsi_id, "900001");
        assert_eq!(list[0].club_region, "TOS");
    }
    #[test]
    fn parses_combined_fide_xml_using_requested_ids_only() {
        let s = r#"<?xml version="1.0" encoding="UTF-8"?><playerslist>
<player><fideid>2800001</fideid><name>Test Player</name><country>ITA</country>
<sex>F</sex><birthday>2010</birthday><rating>1850</rating><rapid_rating>1901</rapid_rating><blitz_rating>1750</blitz_rating></player>
<player><fideid>2800002</fideid><name>Other</name><birthday>2018</birthday><rating>1502</rating></player></playerslist>"#;
        let p = std::env::temp_dir().join(format!("fide-fixture-{}.xml", std::process::id()));
        fs::write(&p, s).unwrap();
        let set = HashSet::from(["2800001".to_string()]);
        let r = read_fide_xml(&p, &set).unwrap();
        let _ = fs::remove_file(p);
        assert_eq!(r.seen, 2);
        assert_eq!(r.found.len(), 1);
        assert_eq!(r.found["2800001"].rapid, 1901);
        assert_eq!(r.found["2800001"].standard, 1850);
    }
}
