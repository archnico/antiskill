use super::load_verified;
use crate::inspector::common::{create_unique_temporary, ensure_within, validate_relative_path};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub(in crate::inspector) struct ExportedOccurrence {
    pub(in crate::inspector) occurrence_id: String,
    pub(in crate::inspector) blake3: String,
    pub(in crate::inspector) bytes: u64,
    pub(in crate::inspector) output: PathBuf,
}

pub(in crate::inspector) fn export_jsonl(
    root: &Path,
    occurrences: &Path,
    output: Option<&Path>,
) -> Result<Vec<u8>, String> {
    let verified = load_verified(root, occurrences)?;
    let mut bytes = Vec::new();
    for occurrence in &verified.manifest.occurrences {
        serde_json::to_writer(&mut bytes, occurrence)
            .map_err(|error| format!("Serializzazione JSONL occurrence: {error}"))?;
        bytes.push(b'\n');
    }
    if let Some(path) = output {
        publish_output(root, path, &bytes, ".occurrences-jsonl-export")?;
        Ok(Vec::new())
    } else {
        Ok(bytes)
    }
}

pub(in crate::inspector) fn export_one(
    root: &Path,
    occurrences: &Path,
    occurrence_id: &str,
    output: &Path,
) -> Result<ExportedOccurrence, String> {
    let verified = load_verified(root, occurrences)?;
    let occurrence = verified
        .manifest
        .occurrences
        .iter()
        .find(|record| record.occurrence_id == occurrence_id)
        .ok_or("occurrence-id inesistente")?;
    let source = &verified.catalog.interpretation.parse.tokenization;
    source.verify_source()?;
    let bytes = source.read_source_range(occurrence.source_start, occurrence.source_end)?;
    let size = u64::try_from(bytes.len()).map_err(|_| "Occurrence esportata oltre u64")?;
    let expected_size = occurrence
        .source_end
        .checked_sub(occurrence.source_start)
        .ok_or("Intervallo occurrence invertito")?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    if size != expected_size || hash != occurrence.source_game_blake3 {
        return Err("Slice occurrence diversa da hash o dimensione dichiarati".into());
    }
    source.verify_source()?;
    publish_output(root, output, &bytes, ".occurrence-export")?;
    Ok(ExportedOccurrence {
        occurrence_id: occurrence.occurrence_id.clone(),
        blake3: hash,
        bytes: size,
        output: output.to_path_buf(),
    })
}

pub(in crate::inspector) fn publish_output(
    root: &Path,
    output: &Path,
    bytes: &[u8],
    label: &str,
) -> Result<(), String> {
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let resolved = if output.is_absolute() {
        output.to_path_buf()
    } else {
        root.join(validate_relative_path(
            output.to_str().ok_or("Output non UTF-8")?,
            "output",
        )?)
    };
    let parent = resolved
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or("Output senza parent")?;
    let canonical_parent =
        fs::canonicalize(parent).map_err(|error| format!("Parent output non valido: {error}"))?;
    ensure_within(&canonical_root, &canonical_parent, "parent output")?;
    let name = resolved.file_name().ok_or("Output senza nome")?;
    if !matches!(
        Path::new(name).components().next(),
        Some(Component::Normal(_))
    ) {
        return Err("Output non sicuro".into());
    }
    let target = canonical_parent.join(name);
    match fs::symlink_metadata(&target) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => return Err("L'output esiste già".into()),
        Err(error) => return Err(format!("Impossibile verificare output: {error}")),
    }
    let (mut file, temporary) = create_unique_temporary(&canonical_parent, label)?;
    let write = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Scrittura export occurrence: {error}"));
    drop(file);
    if let Err(error) = write {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let link = fs::hard_link(&temporary, &target);
    let cleanup = fs::remove_file(&temporary);
    match link {
        Ok(()) => cleanup.map_err(|error| format!("Pulizia temporaneo export: {error}")),
        Err(error) => {
            let _ = cleanup;
            Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
                "L'output esiste già".into()
            } else {
                format!("Pubblicazione export occurrence: {error}")
            })
        }
    }
}
