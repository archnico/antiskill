use crate::archive::game_id::PublicGameIdV1;
use crate::inspector;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

enum ExportRequest {
    Single {
        game_id: PublicGameIdV1,
        output: PathBuf,
    },
    Batch {
        ids_file: PathBuf,
        output_dir: PathBuf,
    },
}

struct ResolvedBatchGame {
    game_id: PublicGameIdV1,
    bytes: Vec<u8>,
}

pub(crate) fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if matches!(args.as_slice(), [argument] if matches!(argument.as_str(), "help" | "--help" | "-h"))
    {
        print_help();
        return ExitCode::SUCCESS;
    }
    let result = parse_args(&args).and_then(|request| execute(Path::new("."), request));
    match result {
        Ok(ExportSuccess::Single { game_id, output }) => {
            println!("Partita esportata.");
            println!("{game_id} → {}", output.display());
            ExitCode::SUCCESS
        }
        Ok(ExportSuccess::Batch { games, output_dir }) => {
            println!("Partite esportate: {games}.");
            println!("{}", output_dir.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERRORE export: {error}");
            ExitCode::from(1)
        }
    }
}

enum ExportSuccess {
    Single {
        game_id: PublicGameIdV1,
        output: PathBuf,
    },
    Batch {
        games: usize,
        output_dir: PathBuf,
    },
}

fn parse_args(args: &[String]) -> Result<ExportRequest, String> {
    match args {
        [batch, ids_file, output_dir] if batch == "--batch" => Ok(ExportRequest::Batch {
            ids_file: PathBuf::from(ids_file),
            output_dir: PathBuf::from(output_dir),
        }),
        [game_id, output] if game_id != "--batch" => {
            Ok(ExportRequest::Single {
                game_id: game_id.parse()?,
                output: PathBuf::from(output),
            })
        }
        _ => Err(
            "Sono richiesti esattamente GAME_ID e OUTPUT, oppure --batch IDS_FILE OUTPUT_DIR. Uso: antiskill export GAME_ID OUTPUT oppure antiskill export --batch IDS_FILE OUTPUT_DIR".into(),
        ),
    }
}

fn execute(root: &Path, request: ExportRequest) -> Result<ExportSuccess, String> {
    match request {
        ExportRequest::Single { game_id, output } => {
            let resolved = resolve_game(root, &game_id)?;
            let published = publish_explicit_destination(root, &output, &resolved.bytes)?;
            Ok(ExportSuccess::Single {
                game_id,
                output: published,
            })
        }
        ExportRequest::Batch {
            ids_file,
            output_dir,
        } => {
            let game_ids = parse_ids_file(root, &ids_file)?;
            let games = game_ids
                .into_iter()
                .map(|game_id| {
                    let bytes = resolve_game(root, &game_id)?.bytes;
                    Ok(ResolvedBatchGame { game_id, bytes })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let output_dir = publish_batch_destination(root, &output_dir, &games)?;
            Ok(ExportSuccess::Batch {
                games: games.len(),
                output_dir,
            })
        }
    }
}

fn resolve_game(
    root: &Path,
    game_id: &PublicGameIdV1,
) -> Result<inspector::ResolvedPublicGame, String> {
    let resolved = inspector::resolve_active_public_game(root, game_id.occurrence_id())?;
    if resolved.occurrence_id != game_id.occurrence_id() {
        return Err("Game ID risolto incoerente".into());
    }
    Ok(resolved)
}

fn parse_ids_file(root: &Path, ids_file: &Path) -> Result<Vec<PublicGameIdV1>, String> {
    let path = if ids_file.is_absolute() {
        ids_file.to_path_buf()
    } else {
        root.join(ids_file)
    };
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Lettura IDS_FILE come UTF-8: {error}"))?;
    let mut seen = HashSet::new();
    let mut game_ids = Vec::new();
    for (index, line) in contents.lines().enumerate() {
        let value = line.trim();
        if value.is_empty() {
            continue;
        }
        let game_id = value
            .parse::<PublicGameIdV1>()
            .map_err(|error| format!("IDS_FILE riga {}: {error}", index + 1))?;
        if !seen.insert(game_id.clone()) {
            return Err(format!("Game ID duplicato in IDS_FILE: {game_id}"));
        }
        game_ids.push(game_id);
    }
    if game_ids.is_empty() {
        return Err("IDS_FILE non contiene Game ID".into());
    }
    Ok(game_ids)
}

fn print_help() {
    println!(
        "\
Esporta l'occurrence originale byte-esatta dalla Collection attiva.

USO:
    antiskill export GAME_ID OUTPUT
    antiskill export --batch IDS_FILE OUTPUT_DIR

GAME_ID si copia dall'output di `antiskill search`.
Il workspace è la directory corrente.
OUTPUT può essere relativo al workspace o assoluto e quindi esterno;
il parent deve esistere e i file esistenti non vengono mai sovrascritti.

In modalità batch, IDS_FILE contiene un ID g1 per riga non vuota.
OUTPUT_DIR deve essere una nuova directory; ogni occurrence originale byte-esatta
viene scritta come OUTPUT_DIR/<GAME_ID>.pgn.
"
    );
}

fn publish_batch_destination(
    root: &Path,
    output: &Path,
    games: &[ResolvedBatchGame],
) -> Result<PathBuf, String> {
    publish_batch_destination_with(root, output, games, write_batch_file)
}

fn publish_batch_destination_with<F>(
    root: &Path,
    output: &Path,
    games: &[ResolvedBatchGame],
    mut write_file: F,
) -> Result<PathBuf, String>
where
    F: FnMut(&Path, &[u8]) -> Result<(), String>,
{
    let target = validate_new_destination(root, output, "La directory di output esiste già")?;
    let parent = target.parent().expect("destinazione validata con parent");
    let staging = create_staging_directory(parent)?;
    let prepared = (|| {
        for game in games {
            let path = staging.join(format!("{}.pgn", game.game_id));
            write_file(&path, &game.bytes)?;
        }
        sync_directory(&staging)?;
        rename_no_replace(&staging, &target)?;
        let _ = sync_directory(parent);
        Ok(())
    })();
    if let Err(error) = prepared {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    Ok(target)
}

fn write_batch_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("Creazione file batch: {error}"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Scrittura export batch: {error}"))
}

fn publish_explicit_destination(
    root: &Path,
    output: &Path,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    let target = validate_new_destination(root, output, "Il file di destinazione esiste già")?;
    let canonical_parent = target.parent().expect("destinazione validata con parent");
    let (mut temporary_file, temporary_path) = create_temporary(canonical_parent)?;
    let written = temporary_file
        .write_all(bytes)
        .and_then(|()| temporary_file.sync_all())
        .map_err(|error| format!("Scrittura export: {error}"));
    drop(temporary_file);
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    let published = fs::hard_link(&temporary_path, &target);
    let cleanup = fs::remove_file(&temporary_path);
    match published {
        Ok(()) => cleanup
            .map_err(|error| format!("Pulizia temporaneo export: {error}"))
            .map(|()| target),
        Err(error) => {
            let _ = cleanup;
            Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
                "Il file di destinazione esiste già".into()
            } else {
                format!("Pubblicazione export: {error}")
            })
        }
    }
}

fn validate_new_destination(
    root: &Path,
    output: &Path,
    exists_message: &str,
) -> Result<PathBuf, String> {
    let resolved = if output.is_absolute() {
        output.to_path_buf()
    } else {
        root.join(output)
    };
    let parent = resolved
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or("Destinazione senza parent")?;
    let metadata = fs::symlink_metadata(parent).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "Parent della destinazione inesistente".to_owned()
        } else {
            format!("Parent della destinazione non verificabile: {error}")
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Parent della destinazione non è una directory reale".into());
    }
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| format!("Parent della destinazione non verificabile: {error}"))?;
    let name = resolved
        .file_name()
        .filter(|name| {
            matches!(
                Path::new(name).components().next(),
                Some(Component::Normal(_))
            )
        })
        .ok_or("Nome file di destinazione non valido")?;
    let target = canonical_parent.join(name);
    match fs::symlink_metadata(&target) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => return Err(exists_message.into()),
        Err(error) => return Err(format!("Impossibile verificare la destinazione: {error}")),
    }
    Ok(target)
}

fn create_temporary(parent: &Path) -> Result<(File, PathBuf), String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Orologio precedente a UNIX_EPOCH: {error}"))?
        .as_nanos();
    for suffix in 0_u32.. {
        let path = parent.join(format!(
            ".antiskill-export-{}-{nonce}-{suffix}.tmp",
            std::process::id()
        ));
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Creazione temporaneo export: {error}")),
        }
    }
    unreachable!("l'intervallo u32 non può essere esaurito in pratica")
}

fn create_staging_directory(parent: &Path) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Orologio precedente a UNIX_EPOCH: {error}"))?
        .as_nanos();
    for suffix in 0_u32.. {
        let path = parent.join(format!(
            ".antiskill-export-batch-{}-{nonce}-{suffix}.tmp",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Creazione staging export batch: {error}")),
        }
    }
    unreachable!("l'intervallo u32 non può essere esaurito in pratica")
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Sync directory export batch: {error}"))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn rename_no_replace(source: &Path, target: &Path) -> Result<(), String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| "Percorso staging non valido".to_owned())?;
    let target = CString::new(target.as_os_str().as_bytes())
        .map_err(|_| "Percorso output non valido".to_owned())?;
    // SAFETY: both pointers come from live CString values and use AT_FDCWD.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    rename_result(result)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn rename_no_replace(source: &Path, target: &Path) -> Result<(), String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| "Percorso staging non valido".to_owned())?;
    let target = CString::new(target.as_os_str().as_bytes())
        .map_err(|_| "Percorso output non valido".to_owned())?;
    // SAFETY: both pointers come from live CString values.
    let result = unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
    rename_result(result)
}

#[cfg(windows)]
fn rename_no_replace(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        #[link_name = "MoveFileW"]
        fn move_file_w(existing_file_name: *const u16, new_file_name: *const u16) -> i32;
    }

    fn wide_path(path: &Path, invalid_message: &str) -> Result<Vec<u16>, String> {
        let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
        if wide.contains(&0) {
            return Err(invalid_message.to_owned());
        }
        wide.push(0);
        Ok(wide)
    }

    let source = wide_path(source, "Percorso staging non valido")?;
    let target = wide_path(target, "Percorso output non valido")?;
    // SAFETY: both pointers reference live, NUL-terminated UTF-16 buffers.
    let result = unsafe { move_file_w(source.as_ptr(), target.as_ptr()) };
    if result != 0 {
        Ok(())
    } else {
        Err(publication_error(std::io::Error::last_os_error()))
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    windows
)))]
fn rename_no_replace(_source: &Path, _target: &Path) -> Result<(), String> {
    Err("Pubblicazione batch atomica senza overwrite non supportata su questa piattaforma".into())
}

#[cfg(unix)]
fn rename_result(result: libc::c_int) -> Result<(), String> {
    if result == 0 {
        Ok(())
    } else {
        Err(publication_error(std::io::Error::last_os_error()))
    }
}

fn publication_error(error: std::io::Error) -> String {
    if matches!(
        error.kind(),
        std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::DirectoryNotEmpty
    ) {
        "La directory di output esiste già".into()
    } else {
        format!("Pubblicazione export batch: {error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_export_uses_structured_api_without_inspector_subprocess_or_output() {
        let source = include_str!("export.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("inspector::resolve_active_public_game"));
        assert!(!production.contains("Command::new"));
        assert!(!production.contains("query-collection"));
        assert!(!production.contains("OCCURRENCE_EXPORT"));
    }

    #[test]
    fn windows_batch_publication_uses_native_no_replace_rename() {
        let source = include_str!("export.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let windows_implementation = production
            .split("#[cfg(windows)]")
            .nth(1)
            .unwrap()
            .split("#[cfg(not(any(")
            .next()
            .unwrap();

        assert!(windows_implementation.contains("MoveFileW"));
        assert!(!windows_implementation.contains("fs::rename"));
        assert!(!windows_implementation.contains("std::fs::rename"));
    }

    #[test]
    fn batch_publication_does_not_replace_a_racing_destination() {
        let parent = std::env::temp_dir().join(format!(
            "antiskill-export-race-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&parent).unwrap();
        let staging = parent.join("staging");
        let target = parent.join("target");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("game.pgn"), b"game").unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"keep").unwrap();

        assert!(rename_no_replace(&staging, &target).is_err());
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"keep");
        assert!(staging.exists());
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn batch_write_failure_cleans_staging_without_publishing_final_directory() {
        let parent = std::env::temp_dir().join(format!(
            "antiskill-export-write-failure-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&parent).unwrap();
        let target = parent.join("target");
        let ids = ["1".repeat(64), "2".repeat(64)];
        let games = ids
            .iter()
            .map(|id| ResolvedBatchGame {
                game_id: PublicGameIdV1::from_occurrence_id(id).unwrap(),
                bytes: id.as_bytes().to_vec(),
            })
            .collect::<Vec<_>>();
        let mut writes = 0;

        let error = publish_batch_destination_with(&parent, &target, &games, |path, bytes| {
            writes += 1;
            if writes == 2 {
                return Err("errore di scrittura simulato".into());
            }
            fs::write(path, bytes).map_err(|error| error.to_string())
        })
        .unwrap_err();

        assert!(error.contains("simulato"));
        assert!(!target.exists());
        assert!(fs::read_dir(&parent).unwrap().next().is_none());
        fs::remove_dir_all(parent).unwrap();
    }
}
