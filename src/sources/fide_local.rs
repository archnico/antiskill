//! Local FIDE rating release import, without re-downloading copyrighted data.
//! The operator supplies an already available, legally acquired ZIP XML list.
use super::snapshot::{store_snapshot, SnapshotMetadata};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct Config {
    root: PathBuf,
    file: PathBuf,
    month: String,
    kind: String,
    url: String,
}
fn official_download_url(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("https://ratings.fide.com/download/") else {
        return false;
    };
    !rest.is_empty()
        && rest
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
        && rest.ends_with(".zip")
}
impl Config {
    fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Self, String> {
        let mut it = args.into_iter();
        let (mut root, mut file, mut month, mut kind, mut url) = (None, None, None, None, None);
        while let Some(arg) = it.next() {
            let val = it.next().ok_or_else(|| format!("Manca {arg}"))?;
            match arg.as_str() {
                "--root" => root = Some(PathBuf::from(val)),
                "--file" => file = Some(PathBuf::from(val)),
                "--month" => month = Some(val),
                "--kind" => kind = Some(val),
                "--url" => url = Some(val),
                _ => return Err(format!("Opzione sconosciuta: {arg}")),
            }
        }
        let root = root.ok_or("--root obbligatorio")?;
        let file = file.ok_or("--file obbligatorio")?;
        let month = month.ok_or("--month YYYY-MM obbligatorio")?;
        let kind = kind.ok_or("--kind standard|rapid|blitz|combined obbligatorio")?;
        let url = url.ok_or("--url FIDE originale obbligatorio")?;
        if !month.is_ascii()
            || month.len() != 7
            || month.as_bytes()[4] != b'-'
            || month[0..4]
                .parse::<u16>()
                .ok()
                .filter(|y| (1950..=2100).contains(y))
                .is_none()
            || month[5..]
                .parse::<u8>()
                .ok()
                .filter(|m| (1..=12).contains(m))
                .is_none()
        {
            return Err("Formato YYYY-MM non valido".into());
        }
        if !matches!(kind.as_str(), "standard" | "rapid" | "blitz" | "combined") {
            return Err("Tipo FIDE non supportato".into());
        }
        if !official_download_url(&url) {
            return Err(
                "URL deve essere un file ZIP ufficiale su https://ratings.fide.com/download/"
                    .into(),
            );
        }
        Ok(Self {
            root,
            file,
            month,
            kind,
            url,
        })
    }
    fn key(&self) -> String {
        format!("fide-rating-{}-{}", self.kind, self.month)
    }
}
fn check_zip(path: &PathBuf) -> Result<(), String> {
    let mut f = File::open(path).map_err(|e| format!("FIDE ZIP: {e}"))?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)
        .map_err(|e| format!("ZIP troncato: {e}"))?;
    if magic != [b'P', b'K', 3, 4] {
        return Err("File FIDE non è ZIP".into());
    }
    let size = f.metadata().map_err(|e| format!("ZIP stat: {e}"))?.len();
    if !(200..=250_000_000).contains(&size) {
        return Err("Dimensione ZIP inattesa".into());
    }
    let output = Command::new("unzip")
        .arg("-Z")
        .arg("-1")
        .arg(path)
        .output()
        .map_err(|e| format!("Serve unzip per controllare FIDE ZIP: {e}"))?;
    if !output.status.success() {
        return Err("ZIP FIDE non leggibile".into());
    }
    let names = String::from_utf8(output.stdout).map_err(|_| "ZIP contiene nomi non UTF8")?;
    let names: Vec<&str> = names.lines().filter(|v| !v.is_empty()).collect();
    if names.len() != 1
        || !names[0].to_ascii_lowercase().ends_with(".xml")
        || names[0].contains("..")
        || names[0].starts_with('/')
    {
        return Err("ZIP FIDE deve contenere un unico membro XML non sospetto".into());
    }
    Ok(())
}
pub(super) fn run<I: IntoIterator<Item = String>>(args: I) -> Result<(), String> {
    let args: Vec<String> = args.into_iter().collect();
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("antiskill sources import fide-local --root DIR --file rating.zip --month YYYY-MM --kind standard|rapid|blitz|combined --url https://ratings.fide.com/download/NAME.zip\nImporta ZIP XML FIDE già ottenuto legalmente. Nessun download, nessuna redistribuzione. URL e periodo dichiarati dall'operatore: da certificare prima dell'uso pubblico.");
        return Ok(());
    }
    let cfg = Config::parse(args)?;
    check_zip(&cfg.file)?;
    let acquired = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Clock: {e}"))?
        .as_millis();
    let file = File::open(&cfg.file).map_err(|e| format!("FIDE file: {e}"))?;
    let source_key = cfg.key();
    let saved = store_snapshot(
        &cfg.root,
        SnapshotMetadata {
            source_key: source_key.clone(),
            source_url: cfg.url,
            fetched_at_unix_ms: acquired,
            http_status: 0,
            content_type: Some("application/zip".into()),
            content_length_header: None,
            etag: None,
            last_modified: None,
            content_disposition: None,
            rights_class: "internal".into(),
            retrieved_by: "antiskill-local-import-operator-declared".into(),
        },
        file,
    )?;
    println!("SOURCE\t{source_key}\nSTATUS\tlocal-import-unverified-origin\nBYTES\t{}\nBLAKE3\t{}\nOBJECT\t{}\nMANIFEST\t{}",
        saved.manifest.bytes,saved.manifest.blake3,
        saved.object_path.display(),saved.manifest_path.display());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_scoped_official_url_and_explicit_month() {
        let good = vec![
            "--root",
            "/tmp/x",
            "--file",
            "/tmp/y.zip",
            "--month",
            "2026-10",
            "--kind",
            "standard",
            "--url",
            "https://ratings.fide.com/download/standard_oct26frl_xml.zip",
        ];
        assert!(Config::parse(good.into_iter().map(str::to_owned)).is_ok());
        assert!(!official_download_url("https://example.com/a.zip"));
        assert!(!official_download_url(
            "https://ratings.fide.com.evil/download/a.zip"
        ));
        assert!(!official_download_url(
            "https://ratings.fide.com/download/../../private.zip"
        ));
        assert!(Config::parse(
            ["--root", "/tmp", "--month", "2026-13"]
                .into_iter()
                .map(str::to_owned)
        )
        .is_err());
    }
}
