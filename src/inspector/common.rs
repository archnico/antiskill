use serde::de::DeserializeOwned;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const BUFFER_BYTES: usize = 64 * 1024;

pub(super) fn fresh_chain_enabled() -> bool {
    std::env::var_os("ANTISKILL_FRESH_CHAIN").is_some()
}
pub(super) const COMPRESSED_JSON_SUFFIX: &str = ".json.xz";
pub(super) const LEGACY_JSON_SUFFIX: &str = ".json";

pub(super) fn logical_json_reader(path: &Path, label: &str) -> Result<Box<dyn Read>, String> {
    let file = File::open(path).map_err(|e| format!("Impossibile leggere {label}: {e}"))?;
    logical_json_reader_for_file(path, label, file)
}

pub(super) fn deserialize_logical_json_with_identity<T: DeserializeOwned>(
    path: &Path,
    label: &str,
) -> Result<(T, String, u64), String> {
    let reader = logical_json_reader(path, label)?;
    let mut identity = LogicalIdentityReader::new(reader);
    let value = serde_json::from_reader(BufReader::with_capacity(BUFFER_BYTES, &mut identity))
        .map_err(|error| error.to_string())?;
    let (hash, bytes) = identity.finish();
    Ok((value, hash, bytes))
}

struct LogicalIdentityReader<R> {
    inner: R,
    hasher: blake3::Hasher,
    bytes: u64,
}

impl<R> LogicalIdentityReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: blake3::Hasher::new(),
            bytes: 0,
        }
    }

    fn finish(self) -> (String, u64) {
        (self.hasher.finalize().to_hex().to_string(), self.bytes)
    }
}

impl<R: Read> Read for LogicalIdentityReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.hasher.update(&buffer[..read]);
        self.bytes = self
            .bytes
            .checked_add(read as u64)
            .ok_or_else(|| std::io::Error::other("Dimensione JSON logica oltre u64"))?;
        Ok(read)
    }
}

fn logical_json_reader_for_file(
    path: &Path,
    label: &str,
    file: File,
) -> Result<Box<dyn Read>, String> {
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| format!("Percorso {label} non UTF-8"))?;
    if name.ends_with(COMPRESSED_JSON_SUFFIX) {
        Ok(Box::new(xz2::read::XzDecoder::new(file)))
    } else if name.ends_with(LEGACY_JSON_SUFFIX) {
        Ok(Box::new(file))
    } else {
        Err(format!("Suffisso {label} non supportato: {name}"))
    }
}

pub(super) fn logical_json_bytes(path: &Path, label: &str) -> Result<Vec<u8>, String> {
    let mut reader = logical_json_reader(path, label)?;
    let mut logical = Vec::new();
    reader
        .read_to_end(&mut logical)
        .map_err(|e| format!("JSON/XZ {label} non valido: {e}"))?;
    Ok(logical)
}

#[cfg(test)]
fn xz_bytes(logical: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 3);
    encoder
        .write_all(logical)
        .map_err(|e| format!("Compressione XZ: {e}"))?;
    encoder
        .finish()
        .map_err(|e| format!("Finalizzazione XZ: {e}"))
}

fn xz_encoder<W: Write>(writer: W) -> Result<xz2::write::XzEncoder<W>, String> {
    let preset = std::env::var("ANTISKILL_XZ_PRESET")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value <= 9)
        .unwrap_or(3);
    let available = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);
    let threads = std::env::var("ANTISKILL_XZ_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1)
        .min(available) as u32;
    if threads == 1 {
        return Ok(xz2::write::XzEncoder::new(writer, preset));
    }
    let mut builder = xz2::stream::MtStreamBuilder::new();
    builder.threads(threads).preset(preset);
    let stream = builder
        .encoder()
        .map_err(|error| format!("Configurazione XZ multithread: {error}"))?;
    Ok(xz2::write::XzEncoder::new_stream(writer, stream))
}

pub(super) fn publish_compressed_json(
    canonical_root: &Path,
    directory: &Path,
    hash: &str,
    logical: &[u8],
    label: &str,
) -> Result<(PathBuf, bool), String> {
    let compressed = directory.join(format!("{hash}{COMPRESSED_JSON_SUFFIX}"));
    if let Some(existing) = existing_real_target(canonical_root, directory, &compressed, label)? {
        if logical_json_matches(existing, logical, label)? {
            return Ok((compressed, false));
        }
        return Err("Collisione content-addressed con contenuto logico differente".into());
    }
    let legacy = directory.join(format!("{hash}{LEGACY_JSON_SUFFIX}"));
    if let Some(existing) = existing_real_target(canonical_root, directory, &legacy, label)? {
        if logical_json_matches(existing, logical, label)? {
            return Ok((legacy, false));
        }
        return Err("Collisione content-addressed legacy con contenuto logico differente".into());
    }
    let (file, temporary) = create_unique_temporary(directory, ".compressed-json")?;
    let write_result = (|| {
        let mut encoder = xz_encoder(file)?;
        encoder
            .write_all(logical)
            .map_err(|e| format!("Compressione XZ: {e}"))?;
        let encoded = encoder
            .finish()
            .map_err(|e| format!("Finalizzazione XZ: {e}"))?;
        encoded
            .sync_all()
            .map_err(|e| format!("Sincronizzazione XZ: {e}"))
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let linked = fs::hard_link(&temporary, &compressed);
    let cleanup = fs::remove_file(&temporary);
    match linked {
        Ok(()) => cleanup.map_err(|e| format!("Impossibile rimuovere temporaneo: {e}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            cleanup.map_err(|e| format!("Impossibile rimuovere temporaneo: {e}"))?;
            let existing = existing_real_target(canonical_root, directory, &compressed, label)?
                .ok_or("Target scomparso dopo AlreadyExists")?;
            if !logical_json_matches(existing, logical, label)? {
                return Err("Collisione content-addressed con contenuto logico differente".into());
            }
            return Ok((compressed, false));
        }
        Err(error) => {
            let _ = cleanup;
            return Err(format!(
                "Impossibile pubblicare {}: {error}",
                compressed.display()
            ));
        }
    }
    let published = existing_real_target(canonical_root, directory, &compressed, label)?
        .ok_or("Target scomparso dopo pubblicazione")?;
    if !logical_json_matches(published, logical, label)? {
        return Err("Verifica logica dopo pubblicazione fallita".into());
    }
    Ok((compressed, true))
}

pub(super) fn publish_compressed_json_stream<F>(
    canonical_root: &Path,
    directory: &Path,
    label: &str,
    write_logical: F,
) -> Result<(PathBuf, bool, String, u64), String>
where
    F: FnOnce(&mut dyn Write) -> Result<(), String>,
{
    let (file, temporary) = create_unique_temporary(directory, ".compressed-json")?;
    let result = (|| {
        let mut encoder = xz_encoder(file)?;
        let mut writer = LogicalHashWriter {
            inner: &mut encoder,
            hasher: blake3::Hasher::new(),
            bytes: 0,
        };
        write_logical(&mut writer)?;
        writer
            .flush()
            .map_err(|error| format!("Compressione XZ: {error}"))?;
        let hash = writer.hasher.finalize().to_hex().to_string();
        let bytes = writer.bytes;
        drop(writer);
        let encoded = encoder
            .finish()
            .map_err(|error| format!("Finalizzazione XZ: {error}"))?;
        encoded
            .sync_all()
            .map_err(|error| format!("Sincronizzazione XZ: {error}"))?;

        let compressed = directory.join(format!("{hash}{COMPRESSED_JSON_SUFFIX}"));
        if let Some(existing) = existing_real_target(canonical_root, directory, &compressed, label)?
        {
            if compressed_logical_matches(existing, &temporary, label)? {
                return Ok((compressed, false, hash, bytes));
            }
            return Err("Collisione content-addressed con contenuto logico differente".into());
        }
        let legacy = directory.join(format!("{hash}{LEGACY_JSON_SUFFIX}"));
        if let Some(existing) = existing_real_target(canonical_root, directory, &legacy, label)? {
            if compressed_logical_matches(existing, &temporary, label)? {
                return Ok((legacy, false, hash, bytes));
            }
            return Err(
                "Collisione content-addressed legacy con contenuto logico differente".into(),
            );
        }

        match fs::hard_link(&temporary, &compressed) {
            Ok(()) => {
                let verification =
                    existing_real_target(canonical_root, directory, &compressed, label).and_then(
                        |published| {
                            let published =
                                published.ok_or("Target scomparso dopo pubblicazione")?;
                            fresh_stream_link_matches(published, &temporary, label)
                        },
                    );
                match verification {
                    Ok(true) => Ok((compressed, true, hash, bytes)),
                    Ok(false) => {
                        fs::remove_file(&compressed).map_err(|error| {
                            format!("Verifica logica fallita e rimozione incompleta: {error}")
                        })?;
                        Err("Verifica logica dopo pubblicazione fallita".into())
                    }
                    Err(error) => {
                        fs::remove_file(&compressed).map_err(|cleanup| {
                            format!("{error}; rimozione artefatto incompleta: {cleanup}")
                        })?;
                        Err(error)
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = existing_real_target(canonical_root, directory, &compressed, label)?
                    .ok_or("Target scomparso dopo AlreadyExists")?;
                if !compressed_logical_matches(existing, &temporary, label)? {
                    return Err(
                        "Collisione content-addressed con contenuto logico differente".into(),
                    );
                }
                Ok((compressed, false, hash, bytes))
            }
            Err(error) => Err(format!(
                "Impossibile pubblicare {}: {error}",
                compressed.display()
            )),
        }
    })();
    let cleanup = fs::remove_file(&temporary);
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(error)) => Err(format!("Impossibile rimuovere temporaneo: {error}")),
        (Err(error), _) => Err(error),
    }
}

struct LogicalHashWriter<'a, W> {
    inner: &'a mut W,
    hasher: blake3::Hasher,
    bytes: u64,
}

impl<W: Write> Write for LogicalHashWriter<'_, W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.hasher.update(&buffer[..written]);
        self.bytes = self
            .bytes
            .checked_add(written as u64)
            .ok_or_else(|| std::io::Error::other("Dimensione JSON logica oltre u64"))?;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(unix)]
fn fresh_stream_link_matches(
    target: ExistingTarget,
    temporary: &Path,
    _label: &str,
) -> Result<bool, String> {
    use std::os::unix::fs::MetadataExt;

    // A successful hard_link must initially name the same inode as the completed temporary.
    // Comparing file identities preserves the existing race check without decompressing the
    // just-written logical artifact twice. A later path replacement was already possible after
    // the old content comparison returned, so this does not widen the post-open TOCTOU window.
    let target_metadata = target
        .file
        .metadata()
        .map_err(|error| format!("Impossibile verificare handle pubblicato: {error}"))?;
    let temporary_metadata = fs::metadata(temporary)
        .map_err(|error| format!("Impossibile verificare temporaneo pubblicato: {error}"))?;
    Ok(target_metadata.dev() == temporary_metadata.dev()
        && target_metadata.ino() == temporary_metadata.ino())
}

#[cfg(not(unix))]
fn fresh_stream_link_matches(
    target: ExistingTarget,
    temporary: &Path,
    label: &str,
) -> Result<bool, String> {
    compressed_logical_matches(target, temporary, label)
}

fn compressed_logical_matches(
    target: ExistingTarget,
    compressed: &Path,
    label: &str,
) -> Result<bool, String> {
    let mut actual = logical_json_reader_for_file(&target.path, label, target.file)?;
    let expected_file = File::open(compressed)
        .map_err(|error| format!("Impossibile rileggere il temporaneo {label}: {error}"))?;
    let mut expected = xz2::read::XzDecoder::new(expected_file);
    let mut actual_buffer = [0_u8; BUFFER_BYTES];
    let mut expected_buffer = [0_u8; BUFFER_BYTES];
    loop {
        let actual_read = read_full_chunk(&mut actual, &mut actual_buffer)
            .map_err(|error| format!("JSON/XZ {label} non valido: {error}"))?;
        let expected_read = read_full_chunk(&mut expected, &mut expected_buffer)
            .map_err(|error| format!("Temporaneo XZ {label} non valido: {error}"))?;
        if actual_read != expected_read
            || actual_buffer[..actual_read] != expected_buffer[..expected_read]
        {
            return Ok(false);
        }
        if actual_read == 0 {
            return Ok(true);
        }
    }
}

fn read_full_chunk(reader: &mut dyn Read, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = reader.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

fn logical_json_matches(
    target: ExistingTarget,
    expected: &[u8],
    label: &str,
) -> Result<bool, String> {
    let mut reader = logical_json_reader_for_file(&target.path, label, target.file)?;
    let mut offset = 0_usize;
    let mut buffer = [0_u8; BUFFER_BYTES];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| format!("JSON/XZ {label} non valido: {e}"))?;
        if read == 0 {
            return Ok(offset == expected.len());
        }
        let end = offset
            .checked_add(read)
            .ok_or("Dimensione JSON logica oltre usize")?;
        if expected.get(offset..end) != Some(&buffer[..read]) {
            return Ok(false);
        }
        offset = end;
    }
}

pub(super) fn canonical_artifact_path(
    directory: &str,
    hash: &str,
    path: &Path,
) -> Result<String, String> {
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("Nome artefatto non UTF-8")?;
    let expected_xz = format!("{hash}{COMPRESSED_JSON_SUFFIX}");
    let expected_json = format!("{hash}{LEGACY_JSON_SUFFIX}");
    if name != expected_xz && name != expected_json {
        return Err("Nome artefatto non canonico".into());
    }
    Ok(format!("inspector/{directory}/{name}"))
}

#[cfg(test)]
pub(super) fn replace_logical_json(path: &Path, logical: &[u8]) {
    let name = path.file_name().and_then(|v| v.to_str()).unwrap();
    let bytes = if name.ends_with(COMPRESSED_JSON_SUFFIX) {
        xz_bytes(logical).unwrap()
    } else {
        logical.to_vec()
    };
    fs::write(path, bytes).unwrap();
}

pub(super) struct ProgressiveCompareWriter<'a> {
    stored: &'a mut dyn Read,
    hasher: blake3::Hasher,
    bytes: u64,
    buffer: Vec<u8>,
}

impl<'a> ProgressiveCompareWriter<'a> {
    pub(super) fn new(stored: &'a mut dyn Read) -> Self {
        Self {
            stored,
            hasher: blake3::Hasher::new(),
            bytes: 0,
            buffer: Vec::with_capacity(BUFFER_BYTES),
        }
    }

    pub(super) fn finish(
        self,
        label: &str,
        baseline_blake3: &str,
        baseline_bytes: u64,
    ) -> Result<(), String> {
        let mut trailing = [0_u8; BUFFER_BYTES];
        let read = self
            .stored
            .read(&mut trailing)
            .map_err(|error| format!("JSON/XZ {label} non valido: {error}"))?;
        if read != 0 {
            return Err(format!("{label} memorizzato più lungo del previsto"));
        }
        let hash = self.hasher.finalize().to_hex().to_string();
        if hash != baseline_blake3 || self.bytes != baseline_bytes {
            return Err(format!("{label} diverso dall'identità attesa"));
        }
        Ok(())
    }
}

impl Write for ProgressiveCompareWriter<'_> {
    fn write(&mut self, expected: &[u8]) -> std::io::Result<usize> {
        self.buffer.resize(expected.len(), 0);
        let read = read_full_chunk(self.stored, &mut self.buffer)
            .map_err(|error| std::io::Error::other(format!("JSON/XZ non valido: {error}")))?;
        if read != expected.len() || self.buffer[..read] != expected[..] {
            return Err(std::io::Error::other(
                "Byte differenti dall'artefatto memorizzato",
            ));
        }
        self.hasher.update(expected);
        self.bytes = self
            .bytes
            .checked_add(expected.len() as u64)
            .ok_or_else(|| std::io::Error::other("Dimensione JSON logica oltre u64"))?;
        Ok(expected.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn create_unique_temporary(
    directory: &Path,
    label: &str,
) -> Result<(File, PathBuf), String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Orologio precedente a UNIX_EPOCH: {error}"))?
        .as_nanos();
    for suffix in 0_u32.. {
        let path = directory.join(format!(
            "{label}-{}-{nonce}-{suffix}.tmp",
            std::process::id()
        ));
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Impossibile creare il file temporaneo {}: {error}",
                    path.display()
                ))
            }
        }
    }
    unreachable!("l'intervallo u32 non può essere esaurito in pratica")
}

pub(super) fn publish_numbered_link(
    directory: &Path,
    prefix: &str,
    temporary_path: &Path,
) -> Result<PathBuf, String> {
    for suffix in 0_u32.. {
        let name = if suffix == 0 {
            format!("{prefix}.json")
        } else {
            format!("{prefix}-{suffix}.json")
        };
        let path = directory.join(name);
        match fs::hard_link(temporary_path, &path) {
            Ok(()) => {
                return Ok(path
                    .file_name()
                    .expect("il manifest pubblicato ha un nome")
                    .into())
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Impossibile pubblicare {}: {error}",
                    path.display()
                ));
            }
        }
    }
    unreachable!("l'intervallo u32 non può essere esaurito in pratica")
}

pub(super) fn resolve_workspace_path(root: &Path, path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let safe = validate_relative_path(
            path.to_str()
                .ok_or("Il percorso manifest non è UTF-8 valido")?,
            "manifest",
        )?;
        Ok(root.join(safe))
    }
}

pub(super) fn validate_relative_path(value: &str, label: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    let safe = !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if safe {
        Ok(path.to_path_buf())
    } else {
        Err(format!("Percorso {label} non sicuro: {value}"))
    }
}

pub(super) fn ensure_within(root: &Path, path: &Path, label: &str) -> Result<(), String> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err(format!(
            "Percorso {label} fuori dal workspace: {}",
            path.display()
        ))
    }
}

pub(super) fn unix_time_ms() -> Result<u128, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Orologio precedente a UNIX_EPOCH: {error}"))?
        .as_millis())
}

pub(super) fn path_to_manifest_string(path: &Path) -> String {
    path.components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .expect("i percorsi interni dell'object store sono UTF-8")
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn prepare_real_directory(
    canonical_root: &Path,
    parent: &Path,
    name: &str,
    label: &str,
) -> Result<PathBuf, String> {
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_dir() => {}
        Ok(_) => return Err(format!("{label} non è una directory reale")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&path)
            .map_err(|error| format!("Impossibile creare {}: {error}", path.display()))?,
        Err(error) => {
            return Err(format!(
                "Impossibile verificare {}: {error}",
                path.display()
            ))
        }
    }
    let canonical = fs::canonicalize(&path)
        .map_err(|error| format!("Directory {label} non valida: {error}"))?;
    ensure_within(canonical_root, &canonical, label)?;
    Ok(canonical)
}

pub(super) fn prepare_directory_chain(
    root: &Path,
    canonical_root: &Path,
    components: &[(&str, &str)],
) -> Result<PathBuf, String> {
    let mut parent = root.to_path_buf();
    for (name, label) in components {
        parent = prepare_real_directory(canonical_root, &parent, name, label)?;
    }
    Ok(parent)
}

struct ExistingTarget {
    path: PathBuf,
    file: File,
}

fn existing_real_target(
    canonical_root: &Path,
    directory: &Path,
    target: &Path,
    label: &str,
) -> Result<Option<ExistingTarget>, String> {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Impossibile verificare {}: {error}",
                target.display()
            ))
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "Target {label} esistente non è un file regolare reale"
        ));
    }
    let canonical =
        fs::canonicalize(target).map_err(|error| format!("Target {label} non valido: {error}"))?;
    ensure_within(canonical_root, &canonical, &format!("target {label}"))?;
    let canonical_directory = fs::canonicalize(directory)
        .map_err(|error| format!("Directory target {label} non valida: {error}"))?;
    ensure_within(
        canonical_root,
        &canonical_directory,
        &format!("directory target {label}"),
    )?;
    if canonical.parent() != Some(canonical_directory.as_path()) {
        return Err(format!("Target {label} fuori dalla directory verificata"));
    }
    let file = File::open(&canonical)
        .map_err(|error| format!("Impossibile aprire target {label}: {error}"))?;
    if !file
        .metadata()
        .map_err(|error| format!("Impossibile verificare handle {label}: {error}"))?
        .is_file()
    {
        return Err(format!("Handle target {label} non è un file regolare"));
    }
    Ok(Some(ExistingTarget {
        path: canonical,
        file,
    }))
}

#[cfg(test)]
mod compressed_storage_tests {
    use super::*;

    fn root() -> PathBuf {
        let base = std::env::temp_dir();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        for suffix in 0_u32.. {
            let path = base.join(format!(
                "antiskill-compressed-storage-{}-{nonce}-{suffix}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return path,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!(
                    "impossibile creare la directory temporanea {}: {error}",
                    path.display()
                ),
            }
        }

        unreachable!("intervallo u32 esaurito")
    }

    #[test]
    fn single_pass_deserialize_identity_matches_plain_and_xz_logical_bytes() {
        let root = root();
        let logical = b"{\n  \"value\": 7\n}\n";
        let expected_hash = blake3::hash(logical).to_hex().to_string();

        let plain = root.join("identity.json");
        fs::write(&plain, logical).unwrap();
        let (plain_value, plain_hash, plain_bytes): (serde_json::Value, _, _) =
            deserialize_logical_json_with_identity(&plain, "test").unwrap();
        assert_eq!(plain_value["value"], 7);
        assert_eq!(plain_hash, expected_hash);
        assert_eq!(plain_bytes, logical.len() as u64);

        let compressed = root.join("identity.json.xz");
        fs::write(&compressed, xz_bytes(logical).unwrap()).unwrap();
        let (xz_value, xz_hash, xz_logical_bytes): (serde_json::Value, _, _) =
            deserialize_logical_json_with_identity(&compressed, "test").unwrap();
        assert_eq!(xz_value, plain_value);
        assert_eq!(xz_hash, expected_hash);
        assert_eq!(xz_logical_bytes, logical.len() as u64);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn single_pass_deserialize_identity_rejects_trailing_garbage_and_corrupt_xz() {
        let root = root();
        let trailing = root.join("trailing.json");
        fs::write(&trailing, b"{\"value\":7}\nnot-json").unwrap();
        assert!(
            deserialize_logical_json_with_identity::<serde_json::Value>(&trailing, "test").is_err()
        );

        let logical = b"{\"value\":7}\n";
        let encoded = xz_bytes(logical).unwrap();
        let truncated = root.join("truncated.json.xz");
        fs::write(&truncated, &encoded[..encoded.len() - 3]).unwrap();
        assert!(
            deserialize_logical_json_with_identity::<serde_json::Value>(&truncated, "test")
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn single_pass_deserialize_identity_is_bounded_by_construction() {
        let source = include_str!("common.rs");
        let helper = source
            .split("pub(super) fn deserialize_logical_json_with_identity")
            .nth(1)
            .unwrap()
            .split("struct LogicalIdentityReader")
            .next()
            .unwrap();
        assert!(helper.contains("BufReader::with_capacity(BUFFER_BYTES"));
        assert!(!helper.contains("read_to_end"));
        assert!(!helper.contains("logical_json_bytes"));
        assert!(!helper.contains("Vec<"));
    }

    #[test]
    fn preset_three_round_trip_and_deterministic_stream() {
        let logical = b"{\n  \"schema_version\": 1\n}\n";
        let one = xz_bytes(logical).unwrap();
        let two = xz_bytes(logical).unwrap();
        assert_eq!(one, two);
        assert_eq!(&one[..6], &[0xfd, b'7', b'z', b'X', b'Z', 0]);
        let root = root();
        let path = root.join("object.json.xz");
        fs::write(&path, one).unwrap();
        assert_eq!(logical_json_bytes(&path, "test").unwrap(), logical);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_truncated_non_xz_and_unknown_suffix_are_rejected() {
        let root = root();
        let valid = xz_bytes(b"{}\n").unwrap();
        let mut corrupt = valid.clone();
        let middle = corrupt.len() / 2;
        corrupt[middle] ^= 1;
        for (name, bytes) in [
            ("truncated.json.xz", valid[..valid.len() - 3].to_vec()),
            ("corrupt.json.xz", corrupt),
            ("plain.json.xz", b"{}\n".to_vec()),
            ("unknown.xz", valid),
        ] {
            let path = root.join(name);
            fs::write(&path, bytes).unwrap();
            assert!(logical_json_bytes(&path, "test").is_err(), "{name}");
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn publication_reuses_compressed_and_legacy_without_migration() {
        let root = root();
        let canonical = fs::canonicalize(&root).unwrap();
        let logical = b"{}\n";
        let hash = blake3::hash(logical).to_hex().to_string();
        let (compressed, created) =
            publish_compressed_json(&canonical, &canonical, &hash, logical, "test").unwrap();
        assert!(created && compressed.ends_with(format!("{hash}.json.xz")));
        assert!(
            !publish_compressed_json(&canonical, &canonical, &hash, logical, "test")
                .unwrap()
                .1
        );
        fs::remove_file(compressed).unwrap();
        let legacy = root.join(format!("{hash}.json"));
        fs::write(&legacy, logical).unwrap();
        let (reused, created) =
            publish_compressed_json(&canonical, &canonical, &hash, logical, "test").unwrap();
        assert_eq!(
            fs::canonicalize(&reused).unwrap(),
            fs::canonicalize(&legacy).unwrap()
        );
        assert!(!created);
        assert!(!root.join(format!("{hash}.json.xz")).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn fresh_stream_publication_hard_link_identity_distinguishes_link_from_copy() {
        let root = root();
        let canonical = fs::canonicalize(&root).unwrap();
        let temporary = root.join("temporary.json.xz");
        fs::write(&temporary, xz_bytes(b"{}\n").unwrap()).unwrap();

        let linked = root.join("linked.json.xz");
        fs::hard_link(&temporary, &linked).unwrap();
        let linked_target = existing_real_target(&canonical, &canonical, &linked, "test")
            .unwrap()
            .unwrap();
        assert!(fresh_stream_link_matches(linked_target, &temporary, "test").unwrap());

        let copied = root.join("copied.json.xz");
        fs::copy(&temporary, &copied).unwrap();
        let copied_target = existing_real_target(&canonical, &canonical, &copied, "test")
            .unwrap()
            .unwrap();
        assert!(!fresh_stream_link_matches(copied_target, &temporary, "test").unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn streaming_publication_matches_reuses_and_cleans_up_failures() {
        let root = root();
        let canonical = fs::canonicalize(&root).unwrap();
        let logical = b"{\n  \"schema_version\": 1\n}\n";
        let (path, created, hash, bytes) =
            publish_compressed_json_stream(&canonical, &canonical, "test", |writer| {
                writer.write_all(logical).map_err(|error| error.to_string())
            })
            .unwrap();
        assert!(created);
        assert_eq!(hash, blake3::hash(logical).to_hex().to_string());
        assert_eq!(bytes, logical.len() as u64);
        assert_eq!(logical_json_bytes(&path, "test").unwrap(), logical);
        assert!(
            !publish_compressed_json_stream(&canonical, &canonical, "test", |writer| {
                writer.write_all(logical).map_err(|error| error.to_string())
            })
            .unwrap()
            .1
        );

        fs::remove_file(&path).unwrap();
        let legacy = root.join(format!("{hash}.json"));
        fs::write(&legacy, logical).unwrap();
        let (reused, created, _, _) =
            publish_compressed_json_stream(&canonical, &canonical, "test", |writer| {
                writer.write_all(logical).map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(
            fs::canonicalize(&reused).unwrap(),
            fs::canonicalize(&legacy).unwrap()
        );
        assert!(!created);

        let entries_before = fs::read_dir(&root).unwrap().count();
        assert!(
            publish_compressed_json_stream(&canonical, &canonical, "test", |writer| {
                writer
                    .write_all(b"partial")
                    .map_err(|error| error.to_string())?;
                Err("failure".to_owned())
            },)
            .is_err()
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), entries_before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn streaming_publication_rejects_content_addressed_collision() {
        let root = root();
        let canonical = fs::canonicalize(&root).unwrap();
        let logical = b"{}\n";
        let hash = blake3::hash(logical).to_hex().to_string();
        let collision = root.join(format!("{hash}.json.xz"));
        fs::write(&collision, xz_bytes(b"different\n").unwrap()).unwrap();
        assert!(
            publish_compressed_json_stream(&canonical, &canonical, "test", |writer| writer
                .write_all(logical)
                .map_err(|error| error.to_string()),)
            .is_err()
        );
        assert_eq!(
            logical_json_bytes(&collision, "test").unwrap(),
            b"different\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn streaming_publication_rejects_content_addressed_symlink() {
        use std::os::unix::fs::symlink;

        let root = root();
        let canonical = fs::canonicalize(&root).unwrap();
        let logical = b"{}\n";
        let hash = blake3::hash(logical).to_hex().to_string();
        let outside = root.with_extension("outside.json.xz");
        fs::write(&outside, xz_bytes(logical).unwrap()).unwrap();
        let target = root.join(format!("{hash}.json.xz"));
        symlink(&outside, &target).unwrap();
        assert!(
            publish_compressed_json_stream(&canonical, &canonical, "test", |writer| writer
                .write_all(logical)
                .map_err(|error| error.to_string()),)
            .is_err()
        );
        assert_eq!(logical_json_bytes(&outside, "test").unwrap(), logical);
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(outside).unwrap();
    }

    #[test]
    fn corrupt_compressed_is_preferred_to_valid_legacy_and_fails_closed() {
        let root = root();
        let canonical = fs::canonicalize(&root).unwrap();
        let logical = b"{}\n";
        let hash = blake3::hash(logical).to_hex().to_string();
        let compressed = root.join(format!("{hash}.json.xz"));
        let legacy = root.join(format!("{hash}.json"));
        fs::write(&compressed, b"not xz\n").unwrap();
        fs::write(&legacy, logical).unwrap();

        assert!(publish_compressed_json(&canonical, &canonical, &hash, logical, "test").is_err());
        assert_eq!(fs::read(&compressed).unwrap(), b"not xz\n");
        assert_eq!(fs::read(&legacy).unwrap(), logical);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn existing_content_addressed_symlink_is_rejected_without_following_it() {
        use std::os::unix::fs::symlink;

        let root = root();
        let canonical = fs::canonicalize(&root).unwrap();
        let logical = b"{}\n";
        let hash = blake3::hash(logical).to_hex().to_string();
        let outside = root.with_extension("outside.json.xz");
        fs::write(&outside, xz_bytes(logical).unwrap()).unwrap();
        let target = root.join(format!("{hash}.json.xz"));
        symlink(&outside, &target).unwrap();

        assert!(publish_compressed_json(&canonical, &canonical, &hash, logical, "test").is_err());
        assert_eq!(fs::read(&outside).unwrap(), xz_bytes(logical).unwrap());
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(outside).unwrap();
    }
}
