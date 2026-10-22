use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const BUFFER_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct StoredObject {
    pub blake3: String,
    pub bytes: u64,
    pub object_relative: PathBuf,
    pub object_path: PathBuf,
    pub new_object: bool,
}

pub(crate) fn store_object<R: Read>(root: &Path, mut reader: R) -> Result<StoredObject, String> {
    let temporary_dir = root.join("raw/tmp");
    fs::create_dir_all(&temporary_dir)
        .map_err(|error| format!("Impossibile creare {}: {error}", temporary_dir.display()))?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Orologio precedente a UNIX_EPOCH: {error}"))?
        .as_nanos();

    let temporary_path = temporary_dir.join(format!(".object-{}-{nonce}.tmp", std::process::id()));

    let mut temporary_file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary_path)
        .map_err(|error| {
            format!(
                "Impossibile creare il file temporaneo {}: {error}",
                temporary_path.display()
            )
        })?;

    let (hash, bytes) = match copy_and_hash(&mut reader, &mut temporary_file) {
        Ok(result) => result,
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }
    };

    if bytes == 0 {
        let _ = fs::remove_file(&temporary_path);
        return Err("L'oggetto sorgente è vuoto".to_owned());
    }

    if let Err(error) = temporary_file.sync_all() {
        drop(temporary_file);
        let _ = fs::remove_file(&temporary_path);
        return Err(format!(
            "Errore sincronizzando l'oggetto temporaneo: {error}"
        ));
    }
    drop(temporary_file);

    let object_relative = PathBuf::from("raw")
        .join("objects")
        .join("blake3")
        .join(&hash[..2])
        .join(&hash);

    let object_path = root.join(&object_relative);
    let object_parent = object_path
        .parent()
        .ok_or("Percorso oggetto senza directory padre")?;

    fs::create_dir_all(object_parent)
        .map_err(|error| format!("Impossibile creare {}: {error}", object_parent.display()))?;

    let new_object = match fs::hard_link(&temporary_path, &object_path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            match verified_files_equal(&temporary_path, &object_path) {
                Ok(true) => false,
                Ok(false) => {
                    let _ = fs::remove_file(&temporary_path);
                    return Err("Collisione object store con byte differenti".into());
                }
                Err(error) => {
                    let _ = fs::remove_file(&temporary_path);
                    return Err(error);
                }
            }
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            return Err(format!(
                "Impossibile registrare l'oggetto {}: {error}",
                object_path.display()
            ));
        }
    };

    fs::remove_file(&temporary_path).map_err(|error| {
        format!(
            "Impossibile rimuovere il file temporaneo {}: {error}",
            temporary_path.display()
        )
    })?;

    Ok(StoredObject {
        blake3: hash,
        bytes,
        object_relative,
        object_path,
        new_object,
    })
}

fn verified_files_equal(expected: &Path, existing: &Path) -> Result<bool, String> {
    let metadata = fs::symlink_metadata(existing)
        .map_err(|error| format!("Impossibile verificare {}: {error}", existing.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Target object store esistente non è un file regolare reale".into());
    }
    let expected_bytes = fs::metadata(expected)
        .map_err(|error| format!("Metadati oggetto temporaneo: {error}"))?
        .len();
    if metadata.len() != expected_bytes {
        return Ok(false);
    }
    let mut expected_file =
        File::open(expected).map_err(|error| format!("Apertura oggetto temporaneo: {error}"))?;
    let mut existing_file =
        File::open(existing).map_err(|error| format!("Apertura oggetto esistente: {error}"))?;
    let mut expected_buffer = [0_u8; BUFFER_BYTES];
    let mut existing_buffer = [0_u8; BUFFER_BYTES];
    let mut remaining = expected_bytes;
    while remaining > 0 {
        let wanted = usize::try_from(remaining.min(BUFFER_BYTES as u64))
            .map_err(|_| "Dimensione confronto non rappresentabile")?;
        expected_file
            .read_exact(&mut expected_buffer[..wanted])
            .map_err(|error| format!("Lettura oggetto temporaneo: {error}"))?;
        existing_file
            .read_exact(&mut existing_buffer[..wanted])
            .map_err(|error| format!("Lettura oggetto esistente: {error}"))?;
        if expected_buffer[..wanted] != existing_buffer[..wanted] {
            return Ok(false);
        }
        remaining -= wanted as u64;
    }
    Ok(true)
}

fn copy_and_hash<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Result<(String, u64), String> {
    let mut hasher = blake3::Hasher::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; BUFFER_BYTES];

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("Errore leggendo la sorgente: {error}"))?;

        if read == 0 {
            break;
        }

        writer
            .write_all(&buffer[..read])
            .map_err(|error| format!("Errore scrivendo l'oggetto: {error}"))?;

        hasher.update(&buffer[..read]);
        bytes = bytes
            .checked_add(read as u64)
            .ok_or("Dimensione dell'oggetto oltre u64")?;
    }

    Ok((hasher.finalize().to_hex().to_string(), bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn test_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("orologio precedente a UNIX_EPOCH")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "antiskill-object-store-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn identical_content_is_stored_only_once() {
        let root = test_root();
        let bytes = b"[Event \"Test\"]\n\n1. e4 e5 1-0\n";

        let first = store_object(&root, Cursor::new(bytes)).expect("primo oggetto");
        let second = store_object(&root, Cursor::new(bytes)).expect("secondo oggetto");

        assert!(first.new_object);
        assert!(!second.new_object);
        assert_eq!(first.blake3, second.blake3);
        assert_eq!(first.bytes, bytes.len() as u64);
        assert_eq!(first.object_path, second.object_path);
        assert_eq!(
            fs::read(&first.object_path).expect("lettura oggetto"),
            bytes
        );

        fs::remove_dir_all(root).expect("pulizia directory test");
    }

    #[test]
    fn empty_content_is_rejected() {
        let root = test_root();

        let error = store_object(&root, Cursor::new([]))
            .expect_err("l'oggetto vuoto deve essere rifiutato");

        assert!(error.contains("vuoto"));

        if root.exists() {
            fs::remove_dir_all(root).expect("pulizia directory test");
        }
    }

    #[test]
    fn corrupted_existing_content_addressed_object_fails_closed() {
        let root = test_root();
        let bytes = b"[Event \"Expected\"]\n";
        let hash = blake3::hash(bytes).to_hex().to_string();
        let object = root.join("raw/objects/blake3").join(&hash[..2]).join(&hash);
        fs::create_dir_all(object.parent().unwrap()).unwrap();
        let mut corrupted = bytes.to_vec();
        corrupted[0] ^= 1;
        fs::write(&object, &corrupted).unwrap();

        assert!(store_object(&root, Cursor::new(bytes)).is_err());
        assert_eq!(fs::read(&object).unwrap(), corrupted);
        assert!(fs::read_dir(root.join("raw/tmp")).unwrap().next().is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
