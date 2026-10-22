use super::manifest::SourceObservationRecordV1;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) struct Candidate {
    pub path: PathBuf,
    pub relative: String,
    pub observation: SourceObservationRecordV1,
}
#[derive(Clone, Debug)]
pub(super) struct ScanFailure {
    pub relative: String,
    pub code: String,
    pub message: String,
}
pub(super) struct Scan {
    pub candidates: Vec<Candidate>,
    pub scanned_files: u64,
    pub pgn_files: u64,
    pub skipped_symlinks: u64,
    pub failures: Vec<ScanFailure>,
}

pub(super) fn directory(input: &Path) -> Result<Scan, String> {
    let meta =
        fs::symlink_metadata(input).map_err(|e| format!("Directory input non valida: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err("Input è un symlink".into());
    }
    if !meta.is_dir() {
        return Err("Input non è una directory".into());
    }
    let root = fs::canonicalize(input)
        .map_err(|e| format!("Directory input non canonicalizzabile: {e}"))?;
    let mut scan = Scan {
        candidates: Vec::new(),
        scanned_files: 0,
        pgn_files: 0,
        skipped_symlinks: 0,
        failures: Vec::new(),
    };
    walk(&root, &root, true, &mut scan)?;
    scan.candidates.sort_by(|a, b| a.relative.cmp(&b.relative));
    scan.failures
        .sort_by(|a, b| a.relative.cmp(&b.relative).then(a.code.cmp(&b.code)));
    Ok(scan)
}

fn walk(root: &Path, directory: &Path, is_root: bool, scan: &mut Scan) -> Result<(), String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if !is_root => {
            failure(
                scan,
                root,
                directory,
                "directory_read",
                format!("Impossibile leggere la sottodirectory: {error}"),
            );
            return Ok(());
        }
        Err(error) => return Err(format!("Impossibile leggere la directory input: {error}")),
    };
    let mut collected = Vec::new();
    for (ordinal, entry) in entries.enumerate() {
        match entry {
            Ok(entry) => collected.push(entry),
            Err(error) => scan.failures.push(ScanFailure {
                relative: format!("%entry-error-{ordinal}"),
                code: "directory_entry".into(),
                message: format!("Impossibile leggere una directory entry: {error}"),
            }),
        }
    }
    collected.sort_by_key(|e| e.file_name());
    for entry in collected {
        let path = entry.path();
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(error) => {
                failure(
                    scan,
                    root,
                    &path,
                    "file_type",
                    format!("Impossibile determinare il tipo: {error}"),
                );
                continue;
            }
        };
        if kind.is_symlink() {
            scan.skipped_symlinks += 1;
            continue;
        }
        if kind.is_dir() {
            walk(root, &path, false, scan)?;
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        scan.scanned_files += 1;
        if !is_pgn_extension(path.extension()) {
            continue;
        }
        scan.pgn_files += 1;
        let relative_path = path
            .strip_prefix(root)
            .map_err(|_| "Percorso entry fuori input")?;
        let relative = match relative_path.to_str() {
            Some(value) => portable(value),
            None => {
                failure(
                    scan,
                    root,
                    &path,
                    "non_utf8_path",
                    "Percorso PGN non UTF-8".into(),
                );
                continue;
            }
        };
        let canonical = match fs::canonicalize(&path) {
            Ok(value) => value,
            Err(error) => {
                scan.failures.push(ScanFailure {
                    relative,
                    code: "candidate_canonicalize".into(),
                    message: format!("PGN non canonicalizzabile: {error}"),
                });
                continue;
            }
        };
        let Some(canonical_path) = canonical.to_str().map(portable) else {
            scan.failures.push(ScanFailure {
                relative,
                code: "non_utf8_canonical_path".into(),
                message: "Percorso canonico PGN non UTF-8".into(),
            });
            continue;
        };
        scan.candidates.push(Candidate {
            path: canonical,
            relative: relative.clone(),
            observation: SourceObservationRecordV1 {
                canonical_path,
                input_relative_path: relative,
            },
        });
    }
    Ok(())
}

fn portable(value: &str) -> String {
    value.replace(std::path::MAIN_SEPARATOR, "/")
}

fn failure(scan: &mut Scan, root: &Path, path: &Path, code: &str, message: String) {
    let relative = path.strip_prefix(root).unwrap_or(path);
    scan.failures.push(ScanFailure {
        relative: diagnostic_path(relative),
        code: code.into(),
        message,
    });
}

fn is_pgn_extension(extension: Option<&OsStr>) -> bool {
    let Some(extension) = extension else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        extension.as_bytes().eq_ignore_ascii_case(b"pgn")
    }
    #[cfg(not(unix))]
    {
        extension
            .to_str()
            .is_some_and(|value| value.eq_ignore_ascii_case("pgn"))
    }
}

fn diagnostic_path(path: &Path) -> String {
    if let Some(value) = path.to_str() {
        return portable(value);
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut out = String::from("hex:");
        for byte in path.as_os_str().as_bytes() {
            use std::fmt::Write;
            write!(&mut out, "{byte:02x}").expect("String write");
        }
        out
    }
    #[cfg(not(unix))]
    {
        "non-utf8-path".into()
    }
}
