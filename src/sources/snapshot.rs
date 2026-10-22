use crate::object_store::store_object;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub(crate) struct SnapshotMetadata {
    pub source_key: String,
    pub source_url: String,
    pub fetched_at_unix_ms: u128,
    pub http_status: u16,
    pub content_type: Option<String>,
    pub content_length_header: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_disposition: Option<String>,
    pub rights_class: String,
    pub retrieved_by: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct AcquisitionManifest {
    pub schema_version: u32,
    pub source_key: String,
    pub source_url: String,
    pub fetched_at_unix_ms: u128,
    pub http_status: u16,
    pub content_type: Option<String>,
    pub content_length_header: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_disposition: Option<String>,
    pub rights_class: String,
    pub retrieved_by: String,
    pub blake3: String,
    pub bytes: u64,
    pub object_path: String,
    pub new_object: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LatestReference {
    schema_version: u32,
    source_key: String,
    fetched_at_unix_ms: u128,
    manifest_path: String,
    blake3: String,
}

#[derive(Clone, Debug)]
pub(crate) struct VerifiedSnapshot {
    pub fetched_at_unix_ms: u128,
    pub blake3: String,
    pub bytes: u64,
    pub object_path: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct StoredSnapshot {
    pub manifest: AcquisitionManifest,
    pub manifest_path: PathBuf,
    pub object_path: PathBuf,
}

pub(crate) fn store_snapshot<R: Read>(
    root: &Path,
    metadata: SnapshotMetadata,
    mut reader: R,
) -> Result<StoredSnapshot, String> {
    validate_source_key(&metadata.source_key)?;

    let stored_object = store_object(root, &mut reader)?;
    let raw_root = root.join("raw");
    let hash = stored_object.blake3;
    let bytes = stored_object.bytes;
    let object_relative = stored_object.object_relative;
    let object_path = stored_object.object_path;
    let new_object = stored_object.new_object;

    let manifest = AcquisitionManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        source_key: metadata.source_key.clone(),
        source_url: metadata.source_url,
        fetched_at_unix_ms: metadata.fetched_at_unix_ms,
        http_status: metadata.http_status,
        content_type: metadata.content_type,
        content_length_header: metadata.content_length_header,
        etag: metadata.etag,
        last_modified: metadata.last_modified,
        content_disposition: metadata.content_disposition,
        rights_class: metadata.rights_class,
        retrieved_by: metadata.retrieved_by,
        blake3: hash.clone(),
        bytes,
        object_path: path_to_manifest_string(&object_relative),
        new_object,
    };

    let manifest_relative = PathBuf::from("raw")
        .join("manifests")
        .join(&metadata.source_key)
        .join(format!(
            "{}-{}-{}.json",
            metadata.fetched_at_unix_ms,
            std::process::id(),
            &hash[..12]
        ));
    let manifest_path = root.join(&manifest_relative);
    write_json_new(&manifest_path, &manifest)?;

    let latest = LatestReference {
        schema_version: MANIFEST_SCHEMA_VERSION,
        source_key: metadata.source_key.clone(),
        fetched_at_unix_ms: metadata.fetched_at_unix_ms,
        manifest_path: path_to_manifest_string(&manifest_relative),
        blake3: hash,
    };
    let latest_path = raw_root
        .join("refs")
        .join(format!("{}.json", metadata.source_key));
    write_json_replace(&latest_path, &latest)?;

    Ok(StoredSnapshot {
        manifest,
        manifest_path,
        object_path,
    })
}

pub(crate) fn load_verified_latest_snapshot(
    root: &Path,
    source_key: &str,
) -> Result<VerifiedSnapshot, String> {
    validate_source_key(source_key)?;
    let latest_path = root.join("raw/refs").join(format!("{source_key}.json"));
    let latest: LatestReference = read_json_regular(&latest_path, "riferimento latest")?;
    if latest.schema_version != MANIFEST_SCHEMA_VERSION || latest.source_key != source_key {
        return Err(format!("Riferimento latest incoerente per {source_key}"));
    }
    let manifest_path = safe_relative_join(root, &latest.manifest_path, "manifest")?;
    let manifest: AcquisitionManifest = read_json_regular(&manifest_path, "manifest snapshot")?;
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION
        || manifest.source_key != source_key
        || manifest.fetched_at_unix_ms != latest.fetched_at_unix_ms
        || manifest.blake3 != latest.blake3
    {
        return Err(format!("Manifest snapshot incoerente per {source_key}"));
    }
    let object_path = safe_relative_join(root, &manifest.object_path, "oggetto snapshot")?;
    let (actual_hash, actual_bytes) = hash_regular_file(&object_path)?;
    if actual_hash != manifest.blake3 || actual_bytes != manifest.bytes {
        return Err(format!("Oggetto snapshot corrotto per {source_key}"));
    }
    Ok(VerifiedSnapshot {
        fetched_at_unix_ms: manifest.fetched_at_unix_ms,
        blake3: actual_hash,
        bytes: actual_bytes,
        object_path,
        manifest_path,
    })
}

fn validate_source_key(source_key: &str) -> Result<(), String> {
    let valid = !source_key.is_empty()
        && source_key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');

    if valid {
        Ok(())
    } else {
        Err(format!("Chiave sorgente non valida: {source_key}"))
    }
}

fn read_json_regular<T>(path: &Path, label: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Impossibile leggere {label} {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{label} non è un file regolare reale: {}",
            path.display()
        ));
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("Impossibile leggere {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{label} JSON non valido {}: {error}", path.display()))
}

fn safe_relative_join(root: &Path, relative: &str, label: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!("Percorso {label} non sicuro: {relative}"));
    }
    Ok(root.join(path))
}

fn hash_regular_file(path: &Path) -> Result<(String, u64), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Metadati oggetto snapshot {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("Oggetto snapshot non regolare: {}", path.display()));
    }
    let mut file = File::open(path)
        .map_err(|error| format!("Apertura oggetto snapshot {}: {error}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Lettura oggetto snapshot {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes = bytes
            .checked_add(read as u64)
            .ok_or("Dimensione snapshot non rappresentabile")?;
    }
    Ok((hasher.finalize().to_hex().to_string(), bytes))
}

fn write_json_new<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path.parent().ok_or("Percorso JSON senza directory padre")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Impossibile creare {}: {error}", parent.display()))?;

    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("Impossibile creare {}: {error}", path.display()))?;
    write_json(file, path, value)
}

fn write_json_replace<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path.parent().ok_or("Percorso JSON senza directory padre")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Impossibile creare {}: {error}", parent.display()))?;

    let temporary_path = parent.join(format!(
        ".{}-{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("latest.json"),
        std::process::id()
    ));

    if temporary_path.exists() {
        fs::remove_file(&temporary_path)
            .map_err(|error| format!("Impossibile pulire {}: {error}", temporary_path.display()))?;
    }

    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary_path)
        .map_err(|error| format!("Impossibile creare {}: {error}", temporary_path.display()))?;
    write_json(file, &temporary_path, value)?;

    fs::rename(&temporary_path, path).map_err(|error| {
        format!(
            "Impossibile aggiornare il riferimento {}: {error}",
            path.display()
        )
    })
}

fn write_json<T: Serialize>(mut file: File, path: &Path, value: &T) -> Result<(), String> {
    serde_json::to_writer_pretty(&mut file, value)
        .map_err(|error| format!("Errore serializzando {}: {error}", path.display()))?;
    file.write_all(b"\n")
        .map_err(|error| format!("Errore scrivendo {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("Errore sincronizzando {}: {error}", path.display()))
}

fn path_to_manifest_string(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("orologio precedente a UNIX_EPOCH")
            .as_nanos();
        std::env::temp_dir().join(format!("antiskill-snapshot-{}-{nonce}", std::process::id()))
    }

    fn metadata(fetched_at_unix_ms: u128) -> SnapshotMetadata {
        SnapshotMetadata {
            source_key: "test-source".to_owned(),
            source_url: "https://example.invalid/source.csv".to_owned(),
            fetched_at_unix_ms,
            http_status: 200,
            content_type: Some("text/csv".to_owned()),
            content_length_header: None,
            etag: None,
            last_modified: None,
            content_disposition: None,
            rights_class: "test".to_owned(),
            retrieved_by: "antiskill-test".to_owned(),
        }
    }

    #[test]
    fn identical_acquisitions_share_one_object() {
        let root = test_root();
        let bytes = b"name;id\nAda;1\n";

        let first = store_snapshot(&root, metadata(1), Cursor::new(bytes)).expect("primo snapshot");
        let second =
            store_snapshot(&root, metadata(2), Cursor::new(bytes)).expect("secondo snapshot");

        assert!(first.manifest.new_object);
        assert!(!second.manifest.new_object);
        assert_eq!(first.object_path, second.object_path);
        assert_ne!(first.manifest_path, second.manifest_path);
        assert_eq!(
            fs::read(&first.object_path).expect("lettura oggetto"),
            bytes
        );

        let latest_path = root.join("raw/refs/test-source.json");
        let latest: LatestReference =
            serde_json::from_slice(&fs::read(&latest_path).expect("lettura riferimento latest"))
                .expect("parsing riferimento latest");
        assert_eq!(latest.fetched_at_unix_ms, 2);
        assert_eq!(latest.blake3, first.manifest.blake3);

        fs::remove_dir_all(root).expect("pulizia directory test");
    }

    #[test]
    fn rejects_unsafe_source_keys() {
        let root = test_root();
        let mut invalid = metadata(1);
        invalid.source_key = "../escape".to_owned();

        let error = store_snapshot(&root, invalid, Cursor::new(b"data"))
            .expect_err("la chiave deve essere rifiutata");
        assert!(error.contains("Chiave sorgente non valida"));
        assert!(!root.exists());
    }
}
