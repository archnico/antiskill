//! Targeted, auditable import of official FSI membership CSV exports.
//! The endpoint returns headerless, semicolon-delimited CSV with 15 columns.
//! No global crawl; a club or an affiliation region and year are mandatory.
use super::snapshot::{store_snapshot, SnapshotMetadata};
use std::io::Read;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const URL: &str = "https://www.federscacchi.com/fsi/FSI_Programmi/tesserati_csv.php";

#[derive(Clone, Debug)]
enum Scope {
    Club(String),
    Region(&'static str, &'static str),
}
#[derive(Clone, Debug)]
struct Config {
    root: PathBuf,
    year: u16,
    scope: Scope,
}
fn region_code(s: &str) -> Option<(&'static str, &'static str)> {
    match s.to_ascii_uppercase().as_str() {
        "TOS" => Some(("TOS", "17")),
        "EMI" => Some(("EMI", "5")),
        "LAZ" => Some(("LAZ", "7")),
        "LOM" => Some(("LOM", "9")),
        "PIE" => Some(("PIE", "12")),
        "VEN" => Some(("VEN", "20")),
        "SIC" => Some(("SIC", "15")),
        "SAR" => Some(("SAR", "14")),
        "PUG" => Some(("PUG", "13")),
        "LIG" => Some(("LIG", "8")),
        "UMB" => Some(("UMB", "18")),
        "MAR" => Some(("MAR", "10")),
        "ABR" => Some(("ABR", "1")),
        "BAS" => Some(("BAS", "2")),
        "CAL" => Some(("CAL", "3")),
        "CAM" => Some(("CAM", "4")),
        "FVG" => Some(("FVG", "6")),
        "MOL" => Some(("MOL", "11")),
        "TAA" => Some(("TAA", "16")), // Trentino; Alto Adige has FSI code 21
        "VDA" => Some(("VDA", "19")),
        "AAT" => Some(("AAT", "21")),
        _ => None,
    }
}
impl Config {
    fn parse<I: IntoIterator<Item = String>>(it: I) -> Result<Self, String> {
        let mut root = None;
        let mut year = None;
        let mut club = None;
        let mut region = None;
        let mut it = it.into_iter();
        while let Some(key) = it.next() {
            let value = it
                .next()
                .ok_or_else(|| format!("Manca il valore di {key}"))?;
            match key.as_str() {
                "--root" => root = Some(PathBuf::from(value)),
                "--year" => year = Some(value.parse::<u16>().map_err(|_| "Anno FSI non valido")?),
                "--club" => club = Some(value),
                "--region" => region = Some(value),
                _ => return Err(format!("Opzione sconosciuta {key}")),
            }
        }
        let root = root.ok_or("Specificare --root DIR")?;
        let year = year.ok_or("Specificare --year YYYY")?;
        if !(2000..=2100).contains(&year) {
            return Err("Anno FSI fuori intervallo".into());
        }
        let scope = match (club, region) {
            (Some(c), None)
                if c.len() >= 3 && c.len() <= 8 && c.bytes().all(|b| b.is_ascii_digit()) =>
            {
                Scope::Club(c)
            }
            (None, Some(r)) => {
                let (abbr, id) = region_code(&r).ok_or("Regione sconosciuta; es. TOS")?;
                Scope::Region(abbr, id)
            }
            _ => {
                return Err(
                    "Specificare esattamente uno tra --club NUMERO e --region CODICE".into(),
                )
            }
        };
        Ok(Self { root, year, scope })
    }
    fn body(&self) -> String {
        match &self.scope {
            Scope::Club(c) => format!("anno={}&cir={}", self.year, c),
            Scope::Region(_, id) => format!("anno={}&socreg={id}", self.year),
        }
    }
    fn key(&self) -> String {
        match &self.scope {
            Scope::Club(c) => format!("fsi-members-club-{c}-{}", self.year),
            Scope::Region(r, _) => format!(
                "fsi-members-region-{}-{}",
                r.to_ascii_lowercase(),
                self.year
            ),
        }
    }
}
pub(super) fn print_help() {
    println!(
        r#"antiskill sources sync fsi-members --root DIR --year YYYY (--club CODE | --region TOS)

Uses the FSI's official, scoped CSV export. The records remain private (rights: internal).
The source snapshot and acquisition manifest are immutable and content addressed.
For regional queries, --region describes the club's affiliation, not the player's residence.
This does not certify the *historical* status of memberships: preserve the acquisition date.
"#
    );
}
fn validate(body: &[u8], cfg: &Config) -> Result<usize, String> {
    if body.is_empty() || body.starts_with(b"<") || body.len() > 20_000_000 {
        return Err("Risposta FSI vuota, HTML, o eccessivamente grande: rifiutata".into());
    }
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b';')
        .from_reader(body);
    let mut count = 0;
    for result in rdr.byte_records() {
        let row = result.map_err(|e| format!("CSV FSI non valido: {e}"))?;
        if row.len() != 15 {
            return Err(format!("Riga FSI con {} colonne invece di 15", row.len()));
        }
        let id = row.get(0).unwrap_or_default();
        if id.is_empty() || !id.iter().all(|x| x.is_ascii_digit()) {
            return Err("Riga FSI senza ID federale valido".into());
        }
        let year = row.get(3).unwrap_or_default();
        if year.is_empty() || !year.iter().all(|x| x.is_ascii_digit()) {
            return Err("Riga FSI con anno di nascita non numerico".into());
        }
        if let Scope::Club(c) = &cfg.scope {
            if row.get(8).unwrap_or_default() != c.as_bytes() {
                return Err("FSI restituisce un circolo diverso dal filtro richiesto".into());
            }
        }
        if let Scope::Region(code, _) = &cfg.scope {
            if *code == "TOS" && row.get(11).unwrap_or_default() != b"TOSCANA" {
                return Err("FSI restituisce una regione diversa dal filtro richiesto".into());
            }
        }
        count += 1;
    }
    if count == 0 {
        return Err("Nessun tesserato restituito dalla FSI: acquisizione rifiutata".into());
    }
    Ok(count)
}
pub(super) fn run<I: IntoIterator<Item = String>>(args: I) -> Result<(), String> {
    let args: Vec<_> = args.into_iter().collect();
    if args
        .iter()
        .any(|a| matches!(a.as_str(), "--help" | "-h" | "help"))
    {
        print_help();
        return Ok(());
    }
    let cfg = Config::parse(args)?;
    let source_key = cfg.key();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Orologio di sistema: {e}"))?
        .as_millis();
    let user_agent = format!(
        "antiskill/{} (+https://github.com/archnico/antiskill)",
        env!("CARGO_PKG_VERSION")
    );
    let body = cfg.body();
    let mut response = ureq::post(URL)
        .header("User-Agent", &user_agent)
        .header("Accept", "text/csv,text/plain,*/*")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(body.as_bytes())
        .map_err(|e| format!("Richiesta export FSI fallita: {e}"))?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|x| x.to_str().ok())
        .map(str::to_owned);
    let last_modified = response
        .headers()
        .get("last-modified")
        .and_then(|x| x.to_str().ok())
        .map(str::to_owned);
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(20_000_001)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Lettura FSI: {e}"))?;
    let count = validate(&bytes, &cfg)?;
    let saved = store_snapshot(
        &cfg.root,
        SnapshotMetadata {
            source_key: source_key.clone(),
            source_url: URL.to_owned(),
            fetched_at_unix_ms: now,
            http_status: status,
            content_type,
            content_length_header: Some(bytes.len() as u64),
            etag: None,
            last_modified,
            content_disposition: None,
            rights_class: "internal".into(),
            retrieved_by: user_agent,
        },
        bytes.as_slice(),
    )?;
    println!(
        "SOURCE\t{source_key}\nCOUNT\t{count}\nBYTES\t{}\nBLAKE3\t{}\nOBJECT\t{}\nMANIFEST\t{}",
        saved.manifest.bytes,
        saved.manifest.blake3,
        saved.object_path.display(),
        saved.manifest_path.display()
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_scoped_requests_are_accepted() {
        assert!(Config::parse(
            ["--root", "/tmp/test", "--year", "2026", "--club", "17029"]
                .into_iter()
                .map(str::to_owned)
        )
        .is_ok());
        assert!(Config::parse(
            ["--root", "/tmp/test", "--year", "2026", "--region", "TOS"]
                .into_iter()
                .map(str::to_owned)
        )
        .is_ok());
        assert!(Config::parse(
            ["--root", "/tmp/test", "--year", "2026"]
                .into_iter()
                .map(str::to_owned)
        )
        .is_err());
    }
    #[test]
    fn validates_headerless_fsi() {
        let cfg = Config::parse(
            ["--root", "/tmp/x", "--year", "2026", "--club", "17029"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(
            validate(
                b"900001;COGNOME;NOME;2018;NC;FI;TOSCANA;ITA;17029;ASD;FI;TOSCANA;TG;18773;TG\n",
                &cfg
            ),
            Ok(1)
        );
        assert!(validate(b"<html>Bad</html>", &cfg).is_err());
        assert!(validate(
            b"900001;COGNOME;NOME;2018;NC;FI;TOSCANA;ITA;17030;ASD;FI;TOSCANA;TG;18773;TG\n",
            &cfg
        )
        .is_err());
    }
}
