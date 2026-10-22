use super::common::{
    deserialize_logical_json_with_identity, ensure_within, logical_json_bytes, logical_json_reader,
    path_to_manifest_string, publish_compressed_json_stream, resolve_workspace_path, unix_time_ms,
    validate_relative_path, ProgressiveCompareWriter, BUFFER_BYTES,
};
use super::source::load_source_manifest;
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

const SEGMENTATION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct SegmentationManifest {
    pub(super) schema_version: u32,
    pub(super) source_manifest: String,
    pub(super) source_manifest_blake3: String,
    pub(super) source_manifest_bytes: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) segmented_at_unix_ms: u128,
    pub(super) segments: Vec<Segment>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Segment {
    pub(super) ordinal: u64,
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) kind: String,
}

#[derive(Debug)]
pub(super) struct CreatedSegmentation {
    pub(super) manifest: SegmentationManifestHeader,
    pub(super) segments: usize,
    pub(super) game_candidates: usize,
    pub(super) path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct SegmentationManifestHeader {
    pub(super) schema_version: u32,
    pub(super) source_manifest: String,
    pub(super) source_manifest_blake3: String,
    pub(super) source_manifest_bytes: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) segmented_at_unix_ms: u128,
    #[serde(rename = "segments", skip_serializing)]
    _segments: IgnoredAny,
}

#[derive(Debug)]
pub(super) struct LoadedSegmentationManifest {
    pub(super) manifest: SegmentationManifest,
    pub(super) object_path: PathBuf,
}

#[derive(Debug)]
pub(super) struct LoadedStreamingSegmentation {
    pub(super) manifest: SegmentationManifestHeader,
    pub(super) canonical_path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) object_path: PathBuf,
}

pub(super) fn segment_source(
    root: &Path,
    source_manifest_path: &Path,
) -> Result<CreatedSegmentation, String> {
    let source = load_source_manifest(root, source_manifest_path, None)?;
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let relative_manifest = source
        .canonical_path
        .strip_prefix(&canonical_root)
        .map_err(|_| "Manifest sorgente fuori dal workspace".to_owned())?;
    let source_manifest = path_to_manifest_string(relative_manifest);
    validate_relative_path(&source_manifest, "source_manifest")?;
    let manifest = SegmentationManifestHeader {
        schema_version: SEGMENTATION_SCHEMA_VERSION,
        source_manifest,
        source_manifest_blake3: source.manifest_blake3,
        source_manifest_bytes: source.manifest_bytes,
        source_blake3: source.manifest.blake3.clone(),
        source_bytes: source.manifest.bytes,
        segmented_at_unix_ms: unix_time_ms()?,
        _segments: IgnoredAny,
    };
    validate_segmentation_header(&manifest)?;
    let directory = root.join("inspector/segmentations");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Impossibile creare {}: {error}", directory.display()))?;
    let canonical_directory = fs::canonicalize(&directory)
        .map_err(|error| format!("Directory segmentazioni non valida: {error}"))?;
    ensure_within(
        &canonical_root,
        &canonical_directory,
        "directory segmentazioni",
    )?;
    let counts = RefCell::new(SegmentationCounts::default());
    let streaming = StreamingSegmentation {
        schema_version: manifest.schema_version,
        source_manifest: &manifest.source_manifest,
        source_manifest_blake3: &manifest.source_manifest_blake3,
        source_manifest_bytes: manifest.source_manifest_bytes,
        source_blake3: &manifest.source_blake3,
        source_bytes: manifest.source_bytes,
        segmented_at_unix_ms: manifest.segmented_at_unix_ms,
        segments: StreamingSegments {
            object_path: &source.object_path,
            expected_blake3: &manifest.source_blake3,
            expected_bytes: manifest.source_bytes,
            counts: &counts,
        },
    };
    let (published, _, _, _) =
        publish_compressed_json_stream(&canonical_root, &directory, "Segmentation", |writer| {
            serde_json::to_writer_pretty(&mut *writer, &streaming)
                .map_err(|error| format!("Errore serializzando la segmentazione: {error}"))?;
            writer
                .write_all(b"\n")
                .map_err(|error| format!("Errore serializzando la segmentazione: {error}"))
        })?;
    verify_source_object(
        &source.object_path,
        manifest.source_bytes,
        &manifest.source_blake3,
    )?;
    let counts = counts.into_inner();
    Ok(CreatedSegmentation {
        manifest,
        segments: counts.segments,
        game_candidates: counts.game_candidates,
        path: PathBuf::from("inspector")
            .join("segmentations")
            .join(published.file_name().unwrap()),
    })
}

#[derive(Serialize)]
struct StreamingSegmentation<'a, T> {
    schema_version: u32,
    source_manifest: &'a str,
    source_manifest_blake3: &'a str,
    source_manifest_bytes: u64,
    source_blake3: &'a str,
    source_bytes: u64,
    segmented_at_unix_ms: u128,
    segments: T,
}

#[derive(Default)]
struct SegmentationCounts {
    segments: usize,
    game_candidates: usize,
}

struct StreamingSegments<'a> {
    object_path: &'a Path,
    expected_blake3: &'a str,
    expected_bytes: u64,
    counts: &'a RefCell<SegmentationCounts>,
}

impl Serialize for StreamingSegments<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut sequence = serializer.serialize_seq(None)?;
        let mut counts = SegmentationCounts::default();
        let (bytes, hash) = lexical_segments_file(self.object_path, |segment| {
            sequence
                .serialize_element(&segment)
                .map_err(|error| error.to_string())?;
            counts.segments += 1;
            if segment.kind == "game_candidate" {
                counts.game_candidates += 1;
            }
            Ok(())
        })
        .map_err(serde::ser::Error::custom)?;
        if bytes != self.expected_bytes || hash != self.expected_blake3 {
            return Err(serde::ser::Error::custom(
                "L'oggetto non corrisponde a hash e dimensione del manifest",
            ));
        }
        *self.counts.borrow_mut() = counts;
        sequence.end()
    }
}

pub(super) fn verify_segmentation(root: &Path, segmentation_path: &Path) -> Result<(), String> {
    let loaded = load_streaming_segmentation(root, segmentation_path, None)?;
    loaded.for_each_segment(|_| Ok(()))?;
    Ok(())
}

pub(super) fn load_streaming_segmentation(
    root: &Path,
    path: &Path,
    expected_identity: Option<(&str, u64)>,
) -> Result<LoadedStreamingSegmentation, String> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let resolved = resolve_workspace_path(root, path)?;
    let canonical_path = fs::canonicalize(&resolved)
        .map_err(|error| format!("Segmentazione non valida {}: {error}", resolved.display()))?;
    ensure_within(&canonical_root, &canonical_path, "segmentation")?;
    let (manifest, manifest_blake3, manifest_bytes): (SegmentationManifestHeader, _, _) =
        deserialize_logical_json_with_identity(&canonical_path, "Segmentation")
            .map_err(|error| format!("Manifest di segmentazione non valido: {error}"))?;
    if let Some((expected_blake3, expected_bytes)) = expected_identity {
        if manifest_blake3 != expected_blake3 || manifest_bytes != expected_bytes {
            return Err("La segmentazione è cambiata rispetto alla tokenizzazione".to_owned());
        }
    }
    validate_segmentation_header(&manifest)?;
    let source_path = validate_relative_path(&manifest.source_manifest, "source_manifest")?;
    let source = load_source_manifest(
        root,
        &source_path,
        Some((
            &manifest.source_manifest_blake3,
            manifest.source_manifest_bytes,
        )),
    )?;
    if source.manifest.blake3 != manifest.source_blake3
        || source.manifest.bytes != manifest.source_bytes
    {
        return Err("Segmentazione incoerente con il manifest sorgente".to_owned());
    }
    verify_source_object(
        &source.object_path,
        manifest.source_bytes,
        &manifest.source_blake3,
    )?;
    let loaded = LoadedStreamingSegmentation {
        manifest,
        canonical_path,
        manifest_blake3,
        manifest_bytes,
        object_path: source.object_path,
    };
    loaded.verify_canonical()?;
    Ok(loaded)
}

/// Convenience loader for legacy callers and tests that explicitly need all segments.
/// Product ingest uses `load_streaming_segmentation`.
pub(super) fn load_segmentation_manifest(
    root: &Path,
    path: &Path,
    expected_identity: Option<(&str, u64)>,
) -> Result<LoadedSegmentationManifest, String> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let resolved = resolve_workspace_path(root, path)?;
    let canonical_segmentation = fs::canonicalize(&resolved)
        .map_err(|error| format!("Segmentazione non valida {}: {error}", resolved.display()))?;
    ensure_within(&canonical_root, &canonical_segmentation, "segmentation")?;
    let data = logical_json_bytes(&canonical_segmentation, "Segmentation")?;
    let manifest_bytes = u64::try_from(data.len())
        .map_err(|_| "Dimensione della segmentazione oltre u64".to_owned())?;
    let manifest_blake3 = blake3::hash(&data).to_hex().to_string();
    if let Some((expected_blake3, expected_bytes)) = expected_identity {
        if manifest_blake3 != expected_blake3 || manifest_bytes != expected_bytes {
            return Err("La segmentazione è cambiata rispetto alla tokenizzazione".to_owned());
        }
    }
    let manifest: SegmentationManifest = serde_json::from_slice(&data)
        .map_err(|error| format!("Manifest di segmentazione non valido: {error}"))?;
    validate_segmentation_intervals(&manifest)?;
    let source_path = validate_relative_path(&manifest.source_manifest, "source_manifest")?;
    let source = load_source_manifest(
        root,
        &source_path,
        Some((
            &manifest.source_manifest_blake3,
            manifest.source_manifest_bytes,
        )),
    )?;
    if source.manifest.blake3 != manifest.source_blake3
        || source.manifest.bytes != manifest.source_bytes
    {
        return Err("Segmentazione incoerente con il manifest sorgente".to_owned());
    }
    Ok(LoadedSegmentationManifest {
        manifest,
        object_path: source.object_path,
    })
}

fn validate_segmentation_intervals(manifest: &SegmentationManifest) -> Result<(), String> {
    validate_segmentation_fields(
        manifest.schema_version,
        &manifest.source_manifest,
        &manifest.source_manifest_blake3,
        manifest.source_manifest_bytes,
        &manifest.source_blake3,
        manifest.source_bytes,
    )?;
    let mut validation = SegmentValidation::new(manifest.source_bytes);
    for segment in &manifest.segments {
        validation.accept(segment)?;
    }
    validation.finish()
}

fn validate_segmentation_header(manifest: &SegmentationManifestHeader) -> Result<(), String> {
    validate_segmentation_fields(
        manifest.schema_version,
        &manifest.source_manifest,
        &manifest.source_manifest_blake3,
        manifest.source_manifest_bytes,
        &manifest.source_blake3,
        manifest.source_bytes,
    )
}

fn validate_segmentation_fields(
    schema_version: u32,
    source_manifest: &str,
    source_manifest_blake3: &str,
    source_manifest_bytes: u64,
    source_blake3: &str,
    source_bytes: u64,
) -> Result<(), String> {
    if schema_version != SEGMENTATION_SCHEMA_VERSION {
        return Err(format!(
            "Versione segmentazione non supportata: {}",
            schema_version
        ));
    }
    validate_relative_path(source_manifest, "source_manifest")?;
    validate_blake3(
        source_manifest_blake3,
        "Hash BLAKE3 del manifest sorgente non valido",
    )?;
    if source_manifest_bytes == 0 {
        return Err("La dimensione del manifest sorgente deve essere positiva".to_owned());
    }
    if source_bytes == 0 {
        return Err("La segmentazione deve coprire un blob non vuoto".to_owned());
    }
    validate_blake3(source_blake3, "Hash BLAKE3 della segmentazione non valido")
}

struct SegmentValidation {
    source_bytes: u64,
    expected_ordinal: u64,
    expected_start: u64,
}

impl SegmentValidation {
    fn new(source_bytes: u64) -> Self {
        Self {
            source_bytes,
            expected_ordinal: 0,
            expected_start: 0,
        }
    }

    fn accept(&mut self, segment: &Segment) -> Result<(), String> {
        if segment.ordinal != self.expected_ordinal {
            return Err("Ordinali dei segmenti non consecutivi".to_owned());
        }
        if segment.start >= segment.end {
            return Err("Intervallo di segmento vuoto o invertito".to_owned());
        }
        if segment.start != self.expected_start {
            return Err("Segmenti con buco, sovrapposizione o ordine errato".to_owned());
        }
        if segment.kind != "game_candidate" && segment.kind != "unclassified" {
            return Err(format!("Kind di segmento non supportato: {}", segment.kind));
        }
        self.expected_ordinal += 1;
        self.expected_start = segment.end;
        Ok(())
    }

    fn finish(self) -> Result<(), String> {
        if self.expected_ordinal == 0 {
            return Err("La segmentazione deve coprire un blob non vuoto".to_owned());
        }
        if self.expected_start != self.source_bytes {
            return Err("L'ultimo segmento non termina a source_bytes".to_owned());
        }
        Ok(())
    }
}

fn verify_source_object(
    path: &Path,
    expected_bytes: u64,
    expected_hash: &str,
) -> Result<(), String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Impossibile aprire {}: {error}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Errore leggendo il SourceBlob: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes = bytes
            .checked_add(read as u64)
            .ok_or("Dimensione dell'oggetto oltre u64")?;
    }
    if bytes != expected_bytes || hasher.finalize().to_hex().as_str() != expected_hash {
        return Err("L'oggetto non corrisponde a hash e dimensione della segmentazione".to_owned());
    }
    Ok(())
}

impl LoadedStreamingSegmentation {
    fn verify_canonical(&self) -> Result<(), String> {
        let counts = RefCell::new(SegmentationCounts::default());
        let manifest = StreamingSegmentation {
            schema_version: self.manifest.schema_version,
            source_manifest: &self.manifest.source_manifest,
            source_manifest_blake3: &self.manifest.source_manifest_blake3,
            source_manifest_bytes: self.manifest.source_manifest_bytes,
            source_blake3: &self.manifest.source_blake3,
            source_bytes: self.manifest.source_bytes,
            segmented_at_unix_ms: self.manifest.segmented_at_unix_ms,
            segments: StreamingSegments {
                object_path: &self.object_path,
                expected_blake3: &self.manifest.source_blake3,
                expected_bytes: self.manifest.source_bytes,
                counts: &counts,
            },
        };
        let mut stored = logical_json_reader(&self.canonical_path, "Segmentation")?;
        let mut writer = ProgressiveCompareWriter::new(&mut *stored);
        serde_json::to_writer_pretty(&mut writer, &manifest)
            .map_err(|error| format!("Segmentazione non canonica: {error}"))?;
        writer
            .write_all(b"\n")
            .map_err(|error| format!("Segmentazione non canonica: {error}"))?;
        writer.finish("Segmentation", &self.manifest_blake3, self.manifest_bytes)
    }

    pub(super) fn verify_source(&self) -> Result<(), String> {
        verify_source_object(
            &self.object_path,
            self.manifest.source_bytes,
            &self.manifest.source_blake3,
        )
    }

    pub(super) fn for_each_segment<F>(&self, mut consume: F) -> Result<usize, String>
    where
        F: FnMut(Segment) -> Result<(), String>,
    {
        self.verify_source()?;
        let mut reader =
            IdentityReader::new(logical_json_reader(&self.canonical_path, "Segmentation")?);
        let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
        let count = SegmentationSeed {
            loaded: self,
            consume: &mut consume,
        }
        .deserialize(&mut deserializer)
        .map_err(|error| format!("Manifest di segmentazione non valido: {error}"))?;
        deserializer
            .end()
            .map_err(|error| format!("Manifest di segmentazione non valido: {error}"))?;
        let (hash, bytes) = reader.finish();
        if hash != self.manifest_blake3 || bytes != self.manifest_bytes {
            return Err("La segmentazione è cambiata durante la lettura".to_owned());
        }
        self.verify_source()?;
        Ok(count)
    }
}

struct IdentityReader<R> {
    inner: R,
    hasher: blake3::Hasher,
    bytes: u64,
}

impl<R> IdentityReader<R> {
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

impl<R: Read> Read for IdentityReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.hasher.update(&buffer[..read]);
        self.bytes = self
            .bytes
            .checked_add(read as u64)
            .ok_or_else(|| std::io::Error::other("Dimensione segmentazione oltre u64"))?;
        Ok(read)
    }
}

struct SegmentationSeed<'a, F> {
    loaded: &'a LoadedStreamingSegmentation,
    consume: &'a mut F,
}

impl<'de, F> DeserializeSeed<'de> for SegmentationSeed<'_, F>
where
    F: FnMut(Segment) -> Result<(), String>,
{
    type Value = usize;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(SegmentationVisitor {
            loaded: self.loaded,
            consume: self.consume,
        })
    }
}

struct SegmentationVisitor<'a, F> {
    loaded: &'a LoadedStreamingSegmentation,
    consume: &'a mut F,
}

impl<'de, F> Visitor<'de> for SegmentationVisitor<'_, F>
where
    F: FnMut(Segment) -> Result<(), String>,
{
    type Value = usize;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("manifest di segmentazione")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = [false; 8];
        let mut count = None;
        while let Some(key) = map.next_key::<String>()? {
            let index = match key.as_str() {
                "schema_version" => 0,
                "source_manifest" => 1,
                "source_manifest_blake3" => 2,
                "source_manifest_bytes" => 3,
                "source_blake3" => 4,
                "source_bytes" => 5,
                "segmented_at_unix_ms" => 6,
                "segments" => 7,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                    continue;
                }
            };
            if std::mem::replace(&mut seen[index], true) {
                return Err(serde::de::Error::custom(format!(
                    "campo duplicato nella segmentazione: {key}"
                )));
            }
            match index {
                0 => compare_field(
                    map.next_value::<u32>()?,
                    self.loaded.manifest.schema_version,
                )?,
                1 => compare_field(
                    map.next_value::<String>()?,
                    self.loaded.manifest.source_manifest.clone(),
                )?,
                2 => compare_field(
                    map.next_value::<String>()?,
                    self.loaded.manifest.source_manifest_blake3.clone(),
                )?,
                3 => compare_field(
                    map.next_value::<u64>()?,
                    self.loaded.manifest.source_manifest_bytes,
                )?,
                4 => compare_field(
                    map.next_value::<String>()?,
                    self.loaded.manifest.source_blake3.clone(),
                )?,
                5 => compare_field(map.next_value::<u64>()?, self.loaded.manifest.source_bytes)?,
                6 => compare_field(
                    map.next_value::<u128>()?,
                    self.loaded.manifest.segmented_at_unix_ms,
                )?,
                7 => {
                    count = Some(map.next_value_seed(SegmentsSeed {
                        source_bytes: self.loaded.manifest.source_bytes,
                        consume: &mut *self.consume,
                    })?);
                }
                _ => unreachable!(),
            }
        }
        if seen.iter().any(|value| !value) {
            return Err(serde::de::Error::custom(
                "campo obbligatorio della segmentazione assente",
            ));
        }
        Ok(count.expect("segments marcato come presente"))
    }
}

fn compare_field<E, T>(actual: T, expected: T) -> Result<(), E>
where
    E: serde::de::Error,
    T: PartialEq,
{
    if actual == expected {
        Ok(())
    } else {
        Err(E::custom(
            "metadati della segmentazione cambiati durante la lettura",
        ))
    }
}

struct SegmentsSeed<'a, F> {
    source_bytes: u64,
    consume: &'a mut F,
}

impl<'de, F> DeserializeSeed<'de> for SegmentsSeed<'_, F>
where
    F: FnMut(Segment) -> Result<(), String>,
{
    type Value = usize;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(SegmentsVisitor {
            source_bytes: self.source_bytes,
            consume: self.consume,
        })
    }
}

struct SegmentsVisitor<'a, F> {
    source_bytes: u64,
    consume: &'a mut F,
}

impl<'de, F> Visitor<'de> for SegmentsVisitor<'_, F>
where
    F: FnMut(Segment) -> Result<(), String>,
{
    type Value = usize;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sequenza di segmenti")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut validation = SegmentValidation::new(self.source_bytes);
        let mut count = 0_usize;
        while let Some(segment) = sequence.next_element::<Segment>()? {
            validation
                .accept(&segment)
                .map_err(serde::de::Error::custom)?;
            (self.consume)(segment).map_err(serde::de::Error::custom)?;
            count += 1;
        }
        validation.finish().map_err(serde::de::Error::custom)?;
        Ok(count)
    }
}

fn validate_blake3(value: &str, error: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(error.to_owned())
    }
}

struct OwnedByteLine {
    start: u64,
    end: u64,
    bytes: Vec<u8>,
}

impl OwnedByteLine {
    fn content(&self) -> &[u8] {
        let mut end = self.bytes.len();
        if self.bytes.last() == Some(&b'\n') {
            end -= 1;
        }
        if end > 0 && self.bytes[end - 1] == b'\r' {
            end -= 1;
        }
        &self.bytes[..end]
    }
}

fn read_owned_line<R: BufRead>(
    reader: &mut R,
    offset: &mut u64,
    hasher: &mut blake3::Hasher,
) -> Result<Option<OwnedByteLine>, String> {
    let mut bytes = Vec::new();
    let read = reader
        .read_until(b'\n', &mut bytes)
        .map_err(|error| format!("Errore leggendo il SourceBlob: {error}"))?;
    if read == 0 {
        return Ok(None);
    }
    let start = *offset;
    *offset = (*offset)
        .checked_add(read as u64)
        .ok_or("Dimensione del SourceBlob oltre u64")?;
    hasher.update(&bytes);
    Ok(Some(OwnedByteLine {
        start,
        end: *offset,
        bytes,
    }))
}

fn content_is_blank(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_whitespace)
}

fn content_is_tag_pair(line: &[u8]) -> bool {
    let mut index = 0;
    while index < line.len() && matches!(line[index], b' ' | b'\t') {
        index += 1;
    }
    if line.get(index) != Some(&b'[') {
        return false;
    }
    index += 1;
    let name_start = index;
    while index < line.len() && (line[index].is_ascii_alphanumeric() || line[index] == b'_') {
        index += 1;
    }
    if index == name_start || !line[index..].starts_with(b" \"") {
        return false;
    }
    index += 2;
    let mut escaped = false;
    while index < line.len() {
        match line[index] {
            b'"' if !escaped => {
                index += 1;
                if line.get(index) != Some(&b']') {
                    return false;
                }
                index += 1;
                return line[index..]
                    .iter()
                    .all(|byte| matches!(byte, b' ' | b'\t'));
            }
            b'\\' if !escaped => escaped = true,
            _ => escaped = false,
        }
        index += 1;
    }
    false
}

fn emit_segment<F>(
    ordinal: &mut u64,
    start: u64,
    end: u64,
    kind: &str,
    emit: &mut F,
) -> Result<(), String>
where
    F: FnMut(Segment) -> Result<(), String>,
{
    if start < end {
        emit(Segment {
            ordinal: *ordinal,
            start,
            end,
            kind: kind.to_owned(),
        })?;
        *ordinal = ordinal
            .checked_add(1)
            .ok_or("Numero di segmenti oltre u64")?;
    }
    Ok(())
}

pub(in crate::inspector) fn for_each_game_candidate_blob<F>(
    path: &Path,
    mut emit: F,
) -> Result<(u64, String, u64), String>
where
    F: FnMut(Segment, Vec<u8>) -> Result<(), String>,
{
    let file = File::open(path)
        .map_err(|error| format!("Impossibile aprire {}: {error}", path.display()))?;
    let source_buffer = std::env::var("ANTISKILL_PROBE_SOURCE_BUFFER")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(BUFFER_BYTES)
        .max(4096);
    let mut reader = BufReader::with_capacity(source_buffer, file);
    let mut hasher = blake3::Hasher::new();
    let mut source_bytes = 0_u64;
    let mut line = read_owned_line(&mut reader, &mut source_bytes, &mut hasher)?;
    let mut segment_ordinal = 0_u64;
    let mut game_ordinal = 0_u64;
    let mut classified_end = 0_u64;

    while line.is_some() {
        if !content_is_tag_pair(line.as_ref().expect("linea presente").content()) {
            line = read_owned_line(&mut reader, &mut source_bytes, &mut hasher)?;
            continue;
        }
        let candidate_start = line.as_ref().expect("linea presente").start;
        let blob_reserve = std::env::var("ANTISKILL_PROBE_BLOB_RESERVE")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(1024);
        let mut blob = Vec::with_capacity(blob_reserve);
        while line
            .as_ref()
            .is_some_and(|value| content_is_tag_pair(value.content()))
        {
            blob.extend_from_slice(&line.as_ref().expect("linea presente").bytes);
            line = read_owned_line(&mut reader, &mut source_bytes, &mut hasher)?;
        }
        while line
            .as_ref()
            .is_some_and(|value| content_is_blank(value.content()))
        {
            blob.extend_from_slice(&line.as_ref().expect("linea presente").bytes);
            line = read_owned_line(&mut reader, &mut source_bytes, &mut hasher)?;
        }
        let mut candidate_end = None;
        while line.as_ref().is_some_and(|value| {
            !content_is_blank(value.content()) && !content_is_tag_pair(value.content())
        }) {
            let value = line.as_ref().expect("linea presente");
            candidate_end = Some(value.end);
            blob.extend_from_slice(&value.bytes);
            line = read_owned_line(&mut reader, &mut source_bytes, &mut hasher)?;
        }
        if let Some(end) = candidate_end {
            if classified_end < candidate_start {
                segment_ordinal = segment_ordinal
                    .checked_add(1)
                    .ok_or("Numero di segmenti oltre u64")?;
            }
            let segment = Segment {
                ordinal: segment_ordinal,
                start: candidate_start,
                end,
                kind: "game_candidate".to_owned(),
            };
            emit(segment, blob)?;
            game_ordinal = game_ordinal
                .checked_add(1)
                .ok_or("Numero di game_candidate oltre u64")?;
            segment_ordinal = segment_ordinal
                .checked_add(1)
                .ok_or("Numero di segmenti oltre u64")?;
            classified_end = end;
        }
    }
    Ok((
        source_bytes,
        hasher.finalize().to_hex().to_string(),
        game_ordinal,
    ))
}

fn lexical_segments_file<F>(path: &Path, mut emit: F) -> Result<(u64, String), String>
where
    F: FnMut(Segment) -> Result<(), String>,
{
    let file = File::open(path)
        .map_err(|error| format!("Impossibile aprire {}: {error}", path.display()))?;
    let mut reader = BufReader::with_capacity(BUFFER_BYTES, file);
    let mut hasher = blake3::Hasher::new();
    let mut source_bytes = 0_u64;
    let mut line = read_owned_line(&mut reader, &mut source_bytes, &mut hasher)?;
    let mut ordinal = 0_u64;
    let mut classified_end = 0_u64;

    while line.is_some() {
        if !content_is_tag_pair(line.as_ref().expect("linea presente").content()) {
            line = read_owned_line(&mut reader, &mut source_bytes, &mut hasher)?;
            continue;
        }
        let candidate_start = line.as_ref().expect("linea presente").start;
        while line
            .as_ref()
            .is_some_and(|value| content_is_tag_pair(value.content()))
        {
            line = read_owned_line(&mut reader, &mut source_bytes, &mut hasher)?;
        }
        while line
            .as_ref()
            .is_some_and(|value| content_is_blank(value.content()))
        {
            line = read_owned_line(&mut reader, &mut source_bytes, &mut hasher)?;
        }
        let mut candidate_end = None;
        while line.as_ref().is_some_and(|value| {
            !content_is_blank(value.content()) && !content_is_tag_pair(value.content())
        }) {
            candidate_end = line.as_ref().map(|value| value.end);
            line = read_owned_line(&mut reader, &mut source_bytes, &mut hasher)?;
        }
        if let Some(end) = candidate_end {
            emit_segment(
                &mut ordinal,
                classified_end,
                candidate_start,
                "unclassified",
                &mut emit,
            )?;
            emit_segment(
                &mut ordinal,
                candidate_start,
                end,
                "game_candidate",
                &mut emit,
            )?;
            classified_end = end;
        }
    }
    emit_segment(
        &mut ordinal,
        classified_end,
        source_bytes,
        "unclassified",
        &mut emit,
    )?;
    Ok((source_bytes, hasher.finalize().to_hex().to_string()))
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct ByteLine {
    start: usize,
    content_end: usize,
    end: usize,
}

#[cfg(test)]
fn byte_lines(bytes: &[u8]) -> Vec<ByteLine> {
    let mut lines = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let newline = bytes[start..].iter().position(|byte| *byte == b'\n');
        let end = newline.map_or(bytes.len(), |offset| start + offset + 1);
        let mut content_end = newline.map_or(end, |offset| start + offset);
        if content_end > start && bytes[content_end - 1] == b'\r' {
            content_end -= 1;
        }
        lines.push(ByteLine {
            start,
            content_end,
            end,
        });
        start = end;
    }
    lines
}

#[cfg(test)]
fn is_blank(bytes: &[u8], line: ByteLine) -> bool {
    bytes[line.start..line.content_end]
        .iter()
        .all(u8::is_ascii_whitespace)
}

#[cfg(test)]
fn is_tag_pair(bytes: &[u8], line: ByteLine) -> bool {
    let line = &bytes[line.start..line.content_end];
    let mut index = 0;
    while index < line.len() && matches!(line[index], b' ' | b'\t') {
        index += 1;
    }
    if line.get(index) != Some(&b'[') {
        return false;
    }
    index += 1;
    let name_start = index;
    while index < line.len() && (line[index].is_ascii_alphanumeric() || line[index] == b'_') {
        index += 1;
    }
    if index == name_start || !line[index..].starts_with(b" \"") {
        return false;
    }
    index += 2;
    let mut escaped = false;
    while index < line.len() {
        match line[index] {
            b'"' if !escaped => {
                index += 1;
                if line.get(index) != Some(&b']') {
                    return false;
                }
                index += 1;
                return line[index..]
                    .iter()
                    .all(|byte| matches!(byte, b' ' | b'\t'));
            }
            b'\\' if !escaped => escaped = true,
            _ => escaped = false,
        }
        index += 1;
    }
    false
}

#[cfg(test)]
fn lexical_segments(bytes: &[u8]) -> Vec<Segment> {
    let lines = byte_lines(bytes);
    let mut candidate_ranges = Vec::<(usize, usize)>::new();
    let mut index = 0;
    while index < lines.len() {
        if !is_tag_pair(bytes, lines[index]) {
            index += 1;
            continue;
        }
        let start = lines[index].start;
        while index < lines.len() && is_tag_pair(bytes, lines[index]) {
            index += 1;
        }
        while index < lines.len() && is_blank(bytes, lines[index]) {
            index += 1;
        }
        let body_start = index;
        while index < lines.len()
            && !is_blank(bytes, lines[index])
            && !is_tag_pair(bytes, lines[index])
        {
            index += 1;
        }
        if index > body_start {
            candidate_ranges.push((start, lines[index - 1].end));
        }
    }

    let mut boundaries = Vec::new();
    let mut cursor = 0;
    for (start, end) in candidate_ranges {
        if cursor < start {
            boundaries.push((cursor, start, "unclassified"));
        }
        boundaries.push((start, end, "game_candidate"));
        cursor = end;
    }
    if cursor < bytes.len() {
        boundaries.push((cursor, bytes.len(), "unclassified"));
    }
    if boundaries.is_empty() && !bytes.is_empty() {
        boundaries.push((0, bytes.len(), "unclassified"));
    }
    boundaries
        .into_iter()
        .enumerate()
        .map(|(ordinal, (start, end, kind))| Segment {
            ordinal: ordinal as u64,
            start: start as u64,
            end: end as u64,
            kind: kind.to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspector::source::import_source;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn valid_segmentation() -> SegmentationManifest {
        SegmentationManifest {
            schema_version: SEGMENTATION_SCHEMA_VERSION,
            source_manifest: "inspector/manifests/source.json".to_owned(),
            source_manifest_blake3: "1".repeat(64),
            source_manifest_bytes: 100,
            source_blake3: "0".repeat(64),
            source_bytes: 10,
            segmented_at_unix_ms: 1,
            segments: vec![
                Segment {
                    ordinal: 0,
                    start: 0,
                    end: 4,
                    kind: "unclassified".to_owned(),
                },
                Segment {
                    ordinal: 1,
                    start: 4,
                    end: 10,
                    kind: "game_candidate".to_owned(),
                },
            ],
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
    fn lexical_segmentation_is_lossless_with_mixed_lines_and_non_utf8() {
        let bytes = b"preamble\r\n\
[Site \"First\"]\r\n[White \"A\"]\r\n\r\n1. e4 {\xff} *\r\n\
\r\n-- separator --\n\
[Event \"Second\"]\n\n1. d4 d5 *\n\
\ntrailing material";
        let segments = lexical_segments(bytes);
        assert_eq!(
            segments
                .iter()
                .filter(|segment| segment.kind == "game_candidate")
                .count(),
            2
        );
        assert_eq!(segments.first().expect("primo segmento").start, 0);
        assert_eq!(
            segments.last().expect("ultimo segmento").end,
            bytes.len() as u64
        );
        for pair in segments.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        let reconstructed: Vec<u8> = segments
            .iter()
            .flat_map(|segment| bytes[segment.start as usize..segment.end as usize].iter())
            .copied()
            .collect();
        assert_eq!(reconstructed, bytes);
        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.end - segment.start)
                .sum::<u64>(),
            bytes.len() as u64
        );
        assert_eq!(segments.last().expect("coda").kind, "unclassified");
    }

    #[test]
    fn document_without_candidate_is_one_unclassified_segment() {
        let bytes = b"not a game\r\n[broken \"tag]\n\xff";
        let segments = lexical_segments(bytes);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start, 0);
        assert_eq!(segments[0].end, bytes.len() as u64);
        assert_eq!(segments[0].kind, "unclassified");
    }

    #[test]
    fn streaming_segmentation_matches_legacy_across_buffer_and_line_boundaries() {
        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let path = root.join("boundary-source.pgn");
        let mut bytes = vec![b'x'; BUFFER_BYTES + 17];
        bytes.extend_from_slice(
            b"\r\n[Event \"One\"]\r\n[Site \"A\\\"B\"]\r\n\r\n1. e4 *\r\n\r\n\
[Event \"No body\"]\n\n[Event \"Two\"]\n\n1. d4 d5 *",
        );
        fs::write(&path, &bytes).expect("scrittura sorgente");

        let mut streamed = Vec::new();
        let (size, hash) = lexical_segments_file(&path, |segment| {
            streamed.push(segment);
            Ok(())
        })
        .expect("segmentazione stream");
        let legacy = lexical_segments(&bytes);
        let projection = |segments: &[Segment]| {
            segments
                .iter()
                .map(|segment| {
                    (
                        segment.ordinal,
                        segment.start,
                        segment.end,
                        segment.kind.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(projection(&streamed), projection(&legacy));
        assert_eq!(size, bytes.len() as u64);
        assert_eq!(hash, blake3::hash(&bytes).to_hex().to_string());
        fs::remove_dir_all(root).expect("pulizia workspace");
    }

    #[test]
    fn streaming_publication_is_canonical_v1_and_loads_without_segment_storage() {
        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let source_path = root.join("source.pgn");
        let bytes =
            b"head\n[Event \"One\"]\n\n1. e4 *\n\nmiddle\n[Event \"Two\"]\r\n\r\n1. d4 *\r\ntail";
        fs::write(&source_path, bytes).expect("sorgente");
        let imported = import_source(&root, &source_path).expect("import");
        let created = segment_source(&root, &imported.manifest_path).expect("segmentazione");
        let logical = logical_json_bytes(&root.join(&created.path), "test").expect("lettura");
        let legacy: SegmentationManifest =
            serde_json::from_slice(&logical).expect("manifest legacy");
        let mut expected = serde_json::to_vec_pretty(&legacy).expect("serializzazione legacy");
        expected.push(b'\n');

        assert_eq!(logical, expected);
        let loaded = load_streaming_segmentation(&root, &created.path, None).expect("header");
        let mut streamed = Vec::new();
        loaded
            .for_each_segment(|segment| {
                streamed.push(segment);
                Ok(())
            })
            .expect("segmenti");
        assert_eq!(
            streamed
                .iter()
                .map(|segment| (
                    segment.ordinal,
                    segment.start,
                    segment.end,
                    segment.kind.as_str()
                ))
                .collect::<Vec<_>>(),
            legacy
                .segments
                .iter()
                .map(|segment| (
                    segment.ordinal,
                    segment.start,
                    segment.end,
                    segment.kind.as_str()
                ))
                .collect::<Vec<_>>()
        );
        assert!(
            std::mem::size_of::<LoadedStreamingSegmentation>()
                < std::mem::size_of::<SegmentationManifest>() + 256,
            "il loader prodotto non deve contenere Vec<Segment>"
        );
        fs::remove_dir_all(root).expect("pulizia workspace");
    }

    #[test]
    fn streaming_segmentation_rejects_truncation_and_interval_corruption() {
        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let source_path = root.join("source.pgn");
        fs::write(&source_path, b"[Event \"One\"]\n\n1. e4 *\n").expect("sorgente");
        let imported = import_source(&root, &source_path).expect("import");
        let created = segment_source(&root, &imported.manifest_path).expect("segmentazione");
        let path = root.join(&created.path);
        let original = logical_json_bytes(&path, "test").expect("lettura");

        crate::inspector::common::replace_logical_json(&path, &original[..original.len() - 2]);
        assert!(load_streaming_segmentation(&root, &created.path, None).is_err());

        let mut manifest: SegmentationManifest =
            serde_json::from_slice(&original).expect("manifest");
        manifest.segments[0].end -= 1;
        crate::inspector::common::replace_logical_json(
            &path,
            &serde_json::to_vec_pretty(&manifest).expect("serializzazione"),
        );
        assert!(load_streaming_segmentation(&root, &created.path, None).is_err());
        fs::remove_dir_all(root).expect("pulizia workspace");
    }

    #[test]
    fn segmentation_validation_rejects_each_interval_corruption() {
        assert!(validate_segmentation_intervals(&valid_segmentation()).is_ok());

        let mut gap = valid_segmentation();
        gap.segments[1].start = 5;
        assert!(validate_segmentation_intervals(&gap).is_err());

        let mut overlap = valid_segmentation();
        overlap.segments[1].start = 3;
        assert!(validate_segmentation_intervals(&overlap).is_err());

        let mut ordinal = valid_segmentation();
        ordinal.segments[1].ordinal = 2;
        assert!(validate_segmentation_intervals(&ordinal).is_err());

        let mut short = valid_segmentation();
        short.segments[1].end = 9;
        assert!(validate_segmentation_intervals(&short).is_err());

        let mut empty = valid_segmentation();
        empty.segments[1].start = empty.segments[1].end;
        assert!(validate_segmentation_intervals(&empty).is_err());

        let mut unknown = valid_segmentation();
        unknown.segments[0].kind = "game".to_owned();
        assert!(validate_segmentation_intervals(&unknown).is_err());

        let mut traversal = valid_segmentation();
        traversal.source_manifest = "../source.json".to_owned();
        assert!(validate_segmentation_intervals(&traversal).is_err());
    }

    #[test]
    fn segmentation_references_source_and_verifies_unchanged_object() {
        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let source_path = root.join("source.pgn");
        let bytes = b"intro\n[Site \"Test\"]\n\n1. e4 *\n\nend\n";
        fs::write(&source_path, bytes).expect("scrittura sorgente");
        let imported = import_source(&root, &source_path).expect("import");
        let first = segment_source(&root, &imported.manifest_path).expect("segmentazione");
        let second = segment_source(&root, &imported.manifest_path).expect("seconda segmentazione");

        if first.manifest.segmented_at_unix_ms == second.manifest.segmented_at_unix_ms {
            assert_eq!(first.path, second.path);
        } else {
            assert_ne!(first.path, second.path);
        }
        assert_eq!(
            first.manifest.source_manifest,
            path_to_manifest_string(&imported.manifest_path)
        );
        let source_manifest_data =
            fs::read(root.join(&imported.manifest_path)).expect("lettura manifest sorgente");
        assert_eq!(
            first.manifest.source_manifest_blake3,
            blake3::hash(&source_manifest_data).to_hex().to_string()
        );
        assert_eq!(
            first.manifest.source_manifest_bytes,
            source_manifest_data.len() as u64
        );
        assert_eq!(first.manifest.source_blake3, imported.manifest.blake3);
        assert_eq!(first.manifest.source_bytes, imported.manifest.bytes);
        verify_segmentation(&root, &first.path).expect("verifica");

        let object = root.join(
            validate_relative_path(&imported.manifest.object_path, "object_path")
                .expect("object_path"),
        );
        fs::write(&object, b"altered").expect("alterazione oggetto");
        assert!(verify_segmentation(&root, &first.path).is_err());

        fs::remove_dir_all(root).expect("pulizia directory test");
    }

    #[test]
    fn altered_source_manifest_is_rejected_by_segmentation_verification() {
        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let source_path = root.join("source.pgn");
        fs::write(&source_path, b"[Site \"Test\"]\n\n1. e4 *\n").expect("sorgente");
        let imported = import_source(&root, &source_path).expect("import");
        let segmented = segment_source(&root, &imported.manifest_path).expect("segmentazione");
        let manifest_path = root.join(&imported.manifest_path);
        let mut altered = imported.manifest;
        altered.source_path.push_str(".renamed");
        fs::write(
            manifest_path,
            serde_json::to_vec(&altered).expect("serializzazione"),
        )
        .expect("alterazione manifest");
        assert!(verify_segmentation(&root, &segmented.path).is_err());
        fs::remove_dir_all(root).expect("pulizia directory test");
    }

    #[cfg(unix)]
    #[test]
    fn segmentation_path_symlink_outside_workspace_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let outside = root.with_extension("outside.json");
        fs::write(&outside, b"{}").expect("file esterno");
        let inside = root.join("outside-link.json");
        symlink(&outside, &inside).expect("symlink");
        assert!(segment_source(&root, Path::new("outside-link.json")).is_err());
        assert!(verify_segmentation(&root, Path::new("outside-link.json")).is_err());
        fs::remove_dir_all(root).expect("pulizia workspace");
        fs::remove_file(outside).expect("pulizia esterno");
    }

    #[cfg(unix)]
    #[test]
    fn source_manifest_field_symlink_outside_workspace_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let source_path = root.join("source.pgn");
        fs::write(&source_path, b"[Site \"Test\"]\n\n1. e4 *\n").expect("sorgente");
        let imported = import_source(&root, &source_path).expect("import");
        let segmented = segment_source(&root, &imported.manifest_path).expect("segmentazione");

        let outside = root.with_extension("outside-source-manifest.json");
        let outside_bytes =
            fs::read(root.join(&imported.manifest_path)).expect("lettura manifest sorgente");
        fs::write(&outside, &outside_bytes).expect("scrittura manifest esterno");
        let link = root.join("inspector/source-manifest-link.json");
        symlink(&outside, &link).expect("symlink manifest sorgente");

        let segmentation_path = root.join(&segmented.path);
        let mut segmentation: SegmentationManifest = serde_json::from_slice(
            &crate::inspector::common::logical_json_bytes(&segmentation_path, "test segmentazione")
                .expect("lettura segmentazione"),
        )
        .expect("deserializzazione segmentazione");
        segmentation.source_manifest = "inspector/source-manifest-link.json".to_owned();
        fs::write(
            &segmentation_path,
            serde_json::to_vec_pretty(&segmentation).expect("serializzazione segmentazione"),
        )
        .expect("aggiornamento segmentazione");

        assert!(verify_segmentation(&root, &segmented.path).is_err());
        assert_eq!(
            fs::read(&outside).expect("rilettura manifest esterno"),
            outside_bytes
        );

        fs::remove_dir_all(root).expect("pulizia workspace");
        fs::remove_file(outside).expect("pulizia manifest esterno");
    }

    #[cfg(unix)]
    #[test]
    fn segmentation_publication_directory_symlink_outside_workspace_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = test_root();
        let outside = root.with_extension("outside");
        fs::create_dir_all(root.join("inspector")).expect("directory inspector");
        fs::create_dir_all(&outside).expect("directory esterna");
        symlink(&outside, root.join("inspector/segmentations")).expect("symlink directory");
        let source_path = root.join("source.pgn");
        fs::write(&source_path, b"[Site \"Test\"]\n\n1. e4 *\n").expect("sorgente");
        let imported = import_source(&root, &source_path).expect("import");
        assert!(segment_source(&root, &imported.manifest_path).is_err());
        assert!(
            fs::read_dir(&outside)
                .expect("lettura directory esterna")
                .next()
                .is_none(),
            "nessun manifest deve essere pubblicato fuori dal workspace"
        );
        fs::remove_dir_all(root).expect("pulizia workspace");
        fs::remove_dir_all(outside).expect("pulizia esterno");
    }
}
