use super::common::{
    create_unique_temporary, ensure_within, path_to_manifest_string, publish_numbered_link,
    resolve_workspace_path, unix_time_ms, validate_relative_path, BUFFER_BYTES,
};
use crate::object_store::{store_object, StoredObject};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct InspectorManifest {
    pub(super) schema_version: u32,
    pub(super) source_path: String,
    pub(super) imported_at_unix_ms: u128,
    pub(super) blake3: String,
    pub(super) bytes: u64,
    pub(super) object_path: String,
    pub(super) new_object: bool,
    pub(super) bytes_added: u64,
}

#[derive(Debug)]
pub(super) struct ImportedSource {
    pub(super) manifest: InspectorManifest,
    pub(super) manifest_path: PathBuf,
}

pub(super) struct StoredSource {
    source_path: String,
    pub(super) blake3: String,
    pub(super) bytes: u64,
    stored: StoredObject,
}

impl StoredSource {
    pub(super) fn object_path(&self) -> &Path {
        &self.stored.object_path
    }
}

#[derive(Debug)]
pub(super) struct LoadedSourceManifest {
    pub(super) manifest: InspectorManifest,
    pub(super) canonical_path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) object_path: PathBuf,
}

pub(super) fn import_source(root: &Path, source: &Path) -> Result<ImportedSource, String> {
    publish_stored_source(root, store_source(root, source)?)
}

pub(super) fn store_source(root: &Path, source: &Path) -> Result<StoredSource, String> {
    let source_path = source
        .to_str()
        .ok_or_else(|| {
            format!(
                "Il percorso sorgente non è UTF-8 valido: {}",
                source.display()
            )
        })?
        .to_owned();
    let source_file = File::open(source)
        .map_err(|error| format!("Impossibile aprire {}: {error}", source.display()))?;
    let stored = store_object(root, source_file)?;
    Ok(StoredSource {
        source_path,
        blake3: stored.blake3.clone(),
        bytes: stored.bytes,
        stored,
    })
}

pub(super) fn publish_stored_source(
    root: &Path,
    source: StoredSource,
) -> Result<ImportedSource, String> {
    let imported_at_unix_ms = unix_time_ms()?;
    let bytes_added = if source.stored.new_object {
        source.stored.bytes
    } else {
        0
    };

    let manifest = InspectorManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        source_path: source.source_path,
        imported_at_unix_ms,
        blake3: source.stored.blake3.clone(),
        bytes: source.stored.bytes,
        object_path: path_to_manifest_string(&source.stored.object_relative),
        new_object: source.stored.new_object,
        bytes_added,
    };

    let manifest_dir = root.join("inspector/manifests");
    fs::create_dir_all(&manifest_dir)
        .map_err(|error| format!("Impossibile creare {}: {error}", manifest_dir.display()))?;
    let prefix = format!(
        "{}-{}-{}",
        imported_at_unix_ms,
        std::process::id(),
        &source.stored.blake3[..12]
    );
    let manifest_name = publish_numbered_manifest(&manifest_dir, &prefix, &manifest)?;
    let manifest_path = PathBuf::from("inspector")
        .join("manifests")
        .join(manifest_name);

    Ok(ImportedSource {
        manifest,
        manifest_path,
    })
}

fn publish_numbered_manifest(
    directory: &Path,
    prefix: &str,
    manifest: &InspectorManifest,
) -> Result<PathBuf, String> {
    let (mut temporary_file, temporary_path) = create_unique_temporary(directory, ".manifest")?;
    let write_result = (|| {
        serde_json::to_writer_pretty(&mut temporary_file, manifest)
            .map_err(|error| format!("Errore serializzando il manifest temporaneo: {error}"))?;
        temporary_file
            .write_all(b"\n")
            .map_err(|error| format!("Errore scrivendo il manifest temporaneo: {error}"))?;
        temporary_file
            .sync_all()
            .map_err(|error| format!("Errore sincronizzando il manifest temporaneo: {error}"))
    })();
    drop(temporary_file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    let publish_result = publish_numbered_link(directory, prefix, &temporary_path);
    let cleanup_result = fs::remove_file(&temporary_path).map_err(|error| {
        format!(
            "Impossibile rimuovere il manifest temporaneo {}: {error}",
            temporary_path.display()
        )
    });
    match (publish_result, cleanup_result) {
        (Ok(path), Ok(())) => Ok(path),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

pub(super) fn load_source_manifest(
    root: &Path,
    path: &Path,
    expected_identity: Option<(&str, u64)>,
) -> Result<LoadedSourceManifest, String> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let resolved = resolve_workspace_path(root, path)?;
    let canonical_manifest = fs::canonicalize(&resolved)
        .map_err(|error| format!("Manifest non valido {}: {error}", resolved.display()))?;
    ensure_within(&canonical_root, &canonical_manifest, "manifest")?;
    let manifest_data = fs::read(&canonical_manifest).map_err(|error| {
        format!(
            "Impossibile leggere il manifest {}: {error}",
            canonical_manifest.display()
        )
    })?;
    let manifest_bytes = u64::try_from(manifest_data.len())
        .map_err(|_| "Dimensione del manifest oltre u64".to_owned())?;
    let manifest_blake3 = blake3::hash(&manifest_data).to_hex().to_string();
    if let Some((expected_blake3, expected_bytes)) = expected_identity {
        if manifest_blake3 != expected_blake3 || manifest_bytes != expected_bytes {
            return Err("Il manifest sorgente è cambiato rispetto alla segmentazione".to_owned());
        }
    }
    let manifest: InspectorManifest = serde_json::from_slice(&manifest_data)
        .map_err(|error| format!("Manifest Inspector non valido: {error}"))?;
    validate_manifest(&manifest)?;
    let object_relative = validate_relative_path(&manifest.object_path, "object_path")?;
    let object_path = root.join(object_relative);
    let canonical_object = fs::canonicalize(&object_path)
        .map_err(|error| format!("Oggetto non valido {}: {error}", object_path.display()))?;
    ensure_within(&canonical_root, &canonical_object, "object_path")?;
    Ok(LoadedSourceManifest {
        manifest,
        canonical_path: canonical_manifest,
        manifest_blake3,
        manifest_bytes,
        object_path: canonical_object,
    })
}

pub(super) fn export_source(
    root: &Path,
    manifest_path: &Path,
    output: &Path,
) -> Result<(), String> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let manifest_path = resolve_workspace_path(root, manifest_path)?;
    let canonical_manifest = fs::canonicalize(&manifest_path)
        .map_err(|error| format!("Manifest non valido {}: {error}", manifest_path.display()))?;
    ensure_within(&canonical_root, &canonical_manifest, "manifest")?;

    let manifest: InspectorManifest =
        serde_json::from_reader(File::open(&canonical_manifest).map_err(|error| {
            format!(
                "Impossibile aprire il manifest {}: {error}",
                canonical_manifest.display()
            )
        })?)
        .map_err(|error| format!("Manifest Inspector non valido: {error}"))?;
    validate_manifest(&manifest)?;

    let object_relative = validate_relative_path(&manifest.object_path, "object_path")?;
    let object_path = root.join(object_relative);
    let canonical_object = fs::canonicalize(&object_path)
        .map_err(|error| format!("Oggetto non valido {}: {error}", object_path.display()))?;
    ensure_within(&canonical_root, &canonical_object, "object_path")?;
    verify_object(&canonical_object, &manifest)?;

    let mut input = File::open(&canonical_object)
        .map_err(|error| format!("Impossibile aprire {}: {error}", canonical_object.display()))?;
    let output_directory = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let (mut temporary_file, temporary_path) =
        create_unique_temporary(output_directory, ".inspector-export")?;
    let copy_result = (|| {
        std::io::copy(&mut input, &mut temporary_file)
            .map_err(|error| format!("Errore esportando {}: {error}", output.display()))?;
        temporary_file
            .sync_all()
            .map_err(|error| format!("Errore sincronizzando {}: {error}", output.display()))
    })();
    drop(temporary_file);

    let publish_result = copy_result.and_then(|()| {
        fs::hard_link(&temporary_path, output).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                format!("L'output esiste già: {}", output.display())
            } else {
                format!("Impossibile pubblicare {}: {error}", output.display())
            }
        })
    });
    let cleanup_result = fs::remove_file(&temporary_path).map_err(|error| {
        format!(
            "Impossibile rimuovere l'export temporaneo {}: {error}",
            temporary_path.display()
        )
    });
    match (publish_result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
    }
}

fn validate_manifest(manifest: &InspectorManifest) -> Result<(), String> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "Versione manifest Inspector non supportata: {}",
            manifest.schema_version
        ));
    }
    if manifest.bytes == 0 {
        return Err("Il manifest dichiara un oggetto vuoto".to_owned());
    }
    if manifest.source_path.is_empty() {
        return Err("source_path del manifest è vuoto".to_owned());
    }
    let expected_bytes_added = if manifest.new_object {
        manifest.bytes
    } else {
        0
    };
    if manifest.bytes_added != expected_bytes_added {
        return Err("new_object e bytes_added del manifest sono incoerenti".to_owned());
    }
    if manifest.blake3.len() != 64
        || !manifest
            .blake3
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("Hash BLAKE3 del manifest non valido".to_owned());
    }
    let expected_path = PathBuf::from("raw")
        .join("objects")
        .join("blake3")
        .join(&manifest.blake3[..2])
        .join(&manifest.blake3);
    if validate_relative_path(&manifest.object_path, "object_path")? != expected_path {
        return Err("object_path non corrisponde al BLAKE3 dichiarato".to_owned());
    }
    Ok(())
}

fn verify_object(path: &Path, manifest: &InspectorManifest) -> Result<(), String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Impossibile aprire {}: {error}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Errore leggendo {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes = bytes
            .checked_add(read as u64)
            .ok_or("Dimensione dell'oggetto oltre u64")?;
    }
    let hash = hasher.finalize().to_hex().to_string();
    if bytes != manifest.bytes || hash != manifest.blake3 {
        Err("L'oggetto non corrisponde a hash e dimensione del manifest".to_owned())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn valid_manifest() -> InspectorManifest {
        let blake3 = "0".repeat(64);
        InspectorManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            source_path: "source.pgn".to_owned(),
            imported_at_unix_ms: 1,
            object_path: format!("raw/objects/blake3/00/{blake3}"),
            blake3,
            bytes: 1,
            new_object: true,
            bytes_added: 1,
        }
    }

    fn test_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("orologio precedente a UNIX_EPOCH")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "antiskill-inspector-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn rejects_unsafe_relative_paths() {
        assert!(validate_relative_path("../manifest.json", "manifest").is_err());
        assert!(validate_relative_path("raw/../object", "object_path").is_err());
        assert!(validate_relative_path("/absolute", "object_path").is_err());
    }

    #[test]
    fn import_export_round_trip_and_deduplication() {
        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let first_source = root.join("first.pgn");
        let second_source = root.join("second.pgn");
        let output = root.join("exported.pgn");
        let bytes = b"[Event \"Lossless\"]\r\n\r\n1. e4 {x} e5 *\r\n";
        fs::write(&first_source, bytes).expect("scrittura prima sorgente");
        fs::write(&second_source, bytes).expect("scrittura seconda sorgente");

        let first = import_source(&root, &first_source).expect("primo import");
        let repeated = import_source(&root, &first_source).expect("secondo import");
        let other_path = import_source(&root, &second_source).expect("import altro percorso");

        assert!(first.manifest.new_object);
        assert_eq!(first.manifest.bytes_added, bytes.len() as u64);
        assert!(!repeated.manifest.new_object);
        assert_eq!(repeated.manifest.bytes_added, 0);
        assert_eq!(first.manifest.blake3, repeated.manifest.blake3);
        assert_eq!(first.manifest.blake3, other_path.manifest.blake3);
        assert_ne!(first.manifest_path, repeated.manifest_path);
        assert_ne!(first.manifest_path, other_path.manifest_path);
        assert_ne!(first.manifest.source_path, other_path.manifest.source_path);

        export_source(&root, &first.manifest_path, &output).expect("export");
        assert_eq!(fs::read(&output).expect("lettura export"), bytes);
        fs::write(&output, b"preesistente").expect("sostituzione controllata output");
        assert!(export_source(&root, &first.manifest_path, &output).is_err());
        assert_eq!(
            fs::read(&output).expect("lettura output preesistente"),
            b"preesistente"
        );

        fs::remove_dir_all(root).expect("pulizia directory test");
    }

    #[test]
    fn rejects_inconsistent_manifest_accounting() {
        let mut newly_stored = valid_manifest();
        newly_stored.bytes_added = 0;
        assert!(validate_manifest(&newly_stored).is_err());

        let mut deduplicated = valid_manifest();
        deduplicated.new_object = false;
        assert!(validate_manifest(&deduplicated).is_err());
    }

    #[test]
    fn rejects_empty_fields_and_unsupported_version() {
        let mut manifest = valid_manifest();
        manifest.bytes = 0;
        manifest.bytes_added = 0;
        assert!(validate_manifest(&manifest).is_err());

        let mut manifest = valid_manifest();
        manifest.source_path.clear();
        assert!(validate_manifest(&manifest).is_err());

        let mut manifest = valid_manifest();
        manifest.schema_version += 1;
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn altered_object_is_rejected_without_output() {
        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let source = root.join("source.pgn");
        let output = root.join("exported.pgn");
        fs::write(&source, b"[Event \"Original\"]\n").expect("scrittura sorgente");
        let imported = import_source(&root, &source).expect("import");
        let object = root.join(
            validate_relative_path(&imported.manifest.object_path, "object_path")
                .expect("object_path valido"),
        );
        fs::write(&object, b"[Event \"Altered\"]\n").expect("alterazione oggetto");

        assert!(export_source(&root, &imported.manifest_path, &output).is_err());
        assert!(!output.exists());

        fs::remove_dir_all(root).expect("pulizia directory test");
    }

    #[cfg(unix)]
    #[test]
    fn object_symlink_outside_workspace_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let source = root.join("source.pgn");
        let outside = root.with_extension("outside");
        let output = root.join("exported.pgn");
        fs::write(&source, b"[Event \"Original\"]\n").expect("scrittura sorgente");
        let imported = import_source(&root, &source).expect("import");
        let object = root.join(
            validate_relative_path(&imported.manifest.object_path, "object_path")
                .expect("object_path valido"),
        );
        fs::write(&outside, b"[Event \"Original\"]\n").expect("scrittura file esterno");
        fs::remove_file(&object).expect("rimozione oggetto di test");
        symlink(&outside, &object).expect("creazione symlink");

        assert!(export_source(&root, &imported.manifest_path, &output).is_err());
        assert!(!output.exists());

        fs::remove_dir_all(root).expect("pulizia directory test");
        fs::remove_file(outside).expect("pulizia file esterno");
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_source_path_is_rejected_before_storage() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let source = root.join(OsString::from_vec(b"source-\xff.pgn".to_vec()));
        fs::write(&source, b"[Event \"Non UTF-8 path\"]\n").expect("scrittura sorgente");

        let error = import_source(&root, &source).expect_err("percorso non UTF-8 rifiutato");
        assert!(error.contains("UTF-8"));
        assert!(!root.join("raw").exists());
        assert!(!root.join("inspector").exists());

        fs::remove_dir_all(root).expect("pulizia directory test");
    }

    #[test]
    fn rejects_empty_source_and_unsafe_manifest_object_path() {
        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let empty = root.join("empty.pgn");
        fs::write(&empty, []).expect("scrittura sorgente vuota");
        assert!(import_source(&root, &empty).is_err());

        let malicious = root.join("inspector/manifests/malicious.json");
        fs::create_dir_all(malicious.parent().expect("directory manifest"))
            .expect("creazione directory manifest");
        let mut manifest = valid_manifest();
        manifest.object_path = "../outside".to_owned();
        fs::write(
            &malicious,
            serde_json::to_vec(&manifest).expect("serializzazione manifest"),
        )
        .expect("scrittura manifest");
        assert!(export_source(&root, Path::new("../outside.json"), &root.join("a.pgn")).is_err());
        assert!(export_source(&root, &malicious, &root.join("b.pgn")).is_err());

        fs::remove_dir_all(root).expect("pulizia directory test");
    }
}
