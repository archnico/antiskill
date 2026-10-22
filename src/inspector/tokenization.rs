use super::common::{
    deserialize_logical_json_with_identity, ensure_within, logical_json_bytes,
    path_to_manifest_string, publish_compressed_json_stream, resolve_workspace_path, unix_time_ms,
    validate_relative_path, BUFFER_BYTES,
};
use super::segmentation::{
    load_segmentation_manifest, load_streaming_segmentation, LoadedSegmentationManifest,
    LoadedStreamingSegmentation, Segment,
};
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const TOKENIZATION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct TokenizationManifest {
    pub(super) schema_version: u32,
    pub(super) segmentation_manifest: String,
    pub(super) segmentation_manifest_blake3: String,
    pub(super) segmentation_manifest_bytes: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) tokenized_at_unix_ms: u128,
    pub(super) streams: Vec<TokenStream>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct TokenStream {
    pub(super) segment_ordinal: u64,
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) tokens: Vec<Token>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) ordinal: u64,
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) kind: TokenKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum TokenKind {
    Whitespace,
    LeftBracket,
    RightBracket,
    LeftParenthesis,
    RightParenthesis,
    QuotedString,
    UnterminatedQuotedString,
    BraceComment,
    UnterminatedBraceComment,
    SemicolonComment,
    EscapeLine,
    MoveNumberCandidate,
    ResultCandidate,
    NagNumeric,
    NagSymbolic,
    Symbol,
    Unknown,
}

#[derive(Debug)]
pub(super) struct CreatedTokenization {
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) game_candidates: usize,
    pub(super) token_streams: usize,
    pub(super) tokens: usize,
    pub(super) unknown_tokens: usize,
    pub(super) unterminated_tokens: usize,
    pub(super) path: PathBuf,
}

#[derive(Debug)]
pub(super) struct LoadedTokenizationManifest {
    pub(super) manifest: TokenizationManifest,
    pub(super) segmentation: LoadedSegmentationManifest,
}

#[derive(Deserialize)]
pub(super) struct StreamingTokenizationManifest {
    pub(super) schema_version: u32,
    pub(super) segmentation_manifest: String,
    pub(super) segmentation_manifest_blake3: String,
    pub(super) segmentation_manifest_bytes: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) tokenized_at_unix_ms: u128,
    #[serde(rename = "streams")]
    _streams: IgnoredAny,
}

pub(super) struct LoadedStreamingTokenization {
    pub(super) manifest: StreamingTokenizationManifest,
    pub(super) canonical_path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) segmentation: LoadedStreamingSegmentation,
}

pub(super) fn tokenize(
    root: &Path,
    segmentation_path: &Path,
) -> Result<CreatedTokenization, String> {
    let loaded = load_streaming_segmentation(root, segmentation_path, None)?;
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let relative_segmentation = loaded
        .canonical_path
        .strip_prefix(&canonical_root)
        .map_err(|_| "Segmentazione fuori dal workspace".to_owned())?;
    let segmentation_manifest = path_to_manifest_string(relative_segmentation);
    validate_relative_path(&segmentation_manifest, "segmentation_manifest")?;
    loaded.verify_source()?;

    let directory = root.join("inspector/tokenizations");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Impossibile creare {}: {error}", directory.display()))?;
    let canonical_directory = fs::canonicalize(&directory)
        .map_err(|error| format!("Directory tokenizzazioni non valida: {error}"))?;
    ensure_within(
        &canonical_root,
        &canonical_directory,
        "directory tokenizzazioni",
    )?;

    let counts = RefCell::new(TokenizeCounts::default());
    let manifest = StreamingTokenization {
        schema_version: TOKENIZATION_SCHEMA_VERSION,
        segmentation_manifest: &segmentation_manifest,
        segmentation_manifest_blake3: &loaded.manifest_blake3,
        segmentation_manifest_bytes: loaded.manifest_bytes,
        source_blake3: &loaded.manifest.source_blake3,
        source_bytes: loaded.manifest.source_bytes,
        tokenized_at_unix_ms: unix_time_ms()?,
        streams: StreamingStreams {
            loaded: &loaded,
            counts: &counts,
        },
    };
    let (path, _, _, _) =
        publish_compressed_json_stream(&canonical_root, &directory, "Tokenization", |writer| {
            serde_json::to_writer_pretty(&mut *writer, &manifest)
                .map_err(|error| format!("Errore serializzando la tokenizzazione: {error}"))?;
            writer
                .write_all(b"\n")
                .map_err(|error| format!("Errore serializzando la tokenizzazione: {error}"))
        })?;
    loaded.verify_source()?;
    let counts = counts.into_inner();
    Ok(CreatedTokenization {
        source_blake3: loaded.manifest.source_blake3.clone(),
        source_bytes: loaded.manifest.source_bytes,
        game_candidates: counts.streams,
        token_streams: counts.streams,
        tokens: counts.tokens,
        unknown_tokens: counts.unknown,
        unterminated_tokens: counts.unterminated,
        path: PathBuf::from("inspector")
            .join("tokenizations")
            .join(path.file_name().unwrap()),
    })
}

#[derive(Serialize)]
struct StreamingTokenization<'a, T> {
    schema_version: u32,
    segmentation_manifest: &'a str,
    segmentation_manifest_blake3: &'a str,
    segmentation_manifest_bytes: u64,
    source_blake3: &'a str,
    source_bytes: u64,
    tokenized_at_unix_ms: u128,
    streams: T,
}

struct StreamingStreams<'a> {
    loaded: &'a LoadedStreamingSegmentation,
    counts: &'a RefCell<TokenizeCounts>,
}

impl Serialize for StreamingStreams<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut sequence = serializer.serialize_seq(None)?;
        let result = self.loaded.for_each_segment(|segment| {
            if segment.kind == "game_candidate" {
                let stream = tokenize_streaming_file_segment(self.loaded, &segment)?;
                validate_stream(&stream, self.loaded.manifest.source_bytes)?;
                sequence
                    .serialize_element(&stream)
                    .map_err(|error| error.to_string())?;
                self.counts.borrow_mut().add(&stream);
            }
            Ok(())
        });
        result.map_err(serde::ser::Error::custom)?;
        sequence.end()
    }
}

#[derive(Default)]
struct TokenizeCounts {
    streams: usize,
    tokens: usize,
    unknown: usize,
    unterminated: usize,
}

impl TokenizeCounts {
    fn add(&mut self, stream: &TokenStream) {
        self.streams += 1;
        self.tokens += stream.tokens.len();
        for token in &stream.tokens {
            if token.kind == TokenKind::Unknown {
                self.unknown += 1;
            }
            if matches!(
                token.kind,
                TokenKind::UnterminatedQuotedString | TokenKind::UnterminatedBraceComment
            ) {
                self.unterminated += 1;
            }
        }
    }
}

pub(super) fn verify_tokenization(root: &Path, tokenization_path: &Path) -> Result<(), String> {
    let loaded = load_streaming_tokenization(root, tokenization_path, None)?;
    loaded.for_each_stream(|_| Ok(()))?;
    Ok(())
}

pub(super) fn load_tokenization_manifest(
    root: &Path,
    tokenization_path: &Path,
    expected_identity: Option<(&str, u64)>,
) -> Result<LoadedTokenizationManifest, String> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let resolved = resolve_workspace_path(root, tokenization_path)?;
    let canonical_tokenization = fs::canonicalize(&resolved)
        .map_err(|error| format!("Tokenizzazione non valida {}: {error}", resolved.display()))?;
    ensure_within(&canonical_root, &canonical_tokenization, "tokenization")?;
    let data = logical_json_bytes(&canonical_tokenization, "Tokenization")?;
    let manifest_bytes = u64::try_from(data.len())
        .map_err(|_| "Dimensione della tokenizzazione oltre u64".to_owned())?;
    let manifest_blake3 = blake3::hash(&data).to_hex().to_string();
    if let Some((expected_blake3, expected_bytes)) = expected_identity {
        if manifest_blake3 != expected_blake3 || manifest_bytes != expected_bytes {
            return Err("La tokenizzazione è cambiata rispetto alla parse".to_owned());
        }
    }
    let manifest: TokenizationManifest = serde_json::from_slice(&data)
        .map_err(|error| format!("Manifest di tokenizzazione non valido: {error}"))?;
    validate_tokenization_shape(&manifest)?;
    let segmentation_path =
        validate_relative_path(&manifest.segmentation_manifest, "segmentation_manifest")?;
    let loaded = load_segmentation_manifest(
        root,
        &segmentation_path,
        Some((
            &manifest.segmentation_manifest_blake3,
            manifest.segmentation_manifest_bytes,
        )),
    )?;
    validate_tokenization(&manifest, &loaded)?;
    verify_blob(&loaded)?;
    for (stream, segment) in manifest.streams.iter().zip(
        loaded
            .manifest
            .segments
            .iter()
            .filter(|segment| segment.kind == "game_candidate"),
    ) {
        if stream != &tokenize_file_segment(&loaded, segment)? {
            return Err("Tokenizzazione diversa dal risultato del lexer".to_owned());
        }
    }
    verify_blob(&loaded)?;
    Ok(LoadedTokenizationManifest {
        manifest,
        segmentation: loaded,
    })
}

pub(super) fn load_streaming_tokenization(
    root: &Path,
    tokenization_path: &Path,
    expected_identity: Option<(&str, u64)>,
) -> Result<LoadedStreamingTokenization, String> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let resolved = resolve_workspace_path(root, tokenization_path)?;
    let canonical_path = fs::canonicalize(&resolved)
        .map_err(|error| format!("Tokenizzazione non valida {}: {error}", resolved.display()))?;
    ensure_within(&canonical_root, &canonical_path, "tokenization")?;
    let (manifest, manifest_blake3, manifest_bytes): (StreamingTokenizationManifest, _, _) =
        deserialize_logical_json_with_identity(&canonical_path, "Tokenization")
            .map_err(|error| format!("Manifest di tokenizzazione non valido: {error}"))?;
    if let Some((expected_blake3, expected_bytes)) = expected_identity {
        if manifest_blake3 != expected_blake3 || manifest_bytes != expected_bytes {
            return Err("La tokenizzazione è cambiata rispetto alla parse".to_owned());
        }
    }
    validate_streaming_shape(&manifest)?;
    let segmentation_path =
        validate_relative_path(&manifest.segmentation_manifest, "segmentation_manifest")?;
    let segmentation = load_streaming_segmentation(
        root,
        &segmentation_path,
        Some((
            &manifest.segmentation_manifest_blake3,
            manifest.segmentation_manifest_bytes,
        )),
    )?;
    if manifest.source_blake3 != segmentation.manifest.source_blake3
        || manifest.source_bytes != segmentation.manifest.source_bytes
    {
        return Err("Tokenizzazione incoerente con la segmentazione".to_owned());
    }
    segmentation.verify_source()?;
    Ok(LoadedStreamingTokenization {
        manifest,
        canonical_path,
        manifest_blake3,
        manifest_bytes,
        segmentation,
    })
}

impl LoadedStreamingTokenization {
    pub(super) fn verify_source(&self) -> Result<(), String> {
        self.segmentation.verify_source()
    }

    pub(super) fn read_source_range(&self, start: u64, end: u64) -> Result<Vec<u8>, String> {
        read_streaming_source_range(&self.segmentation, start, end)
    }

    pub(super) fn for_each_stream<F>(&self, mut consume: F) -> Result<usize, String>
    where
        F: FnMut(TokenStream) -> Result<(), String>,
    {
        self.segmentation.verify_source()?;
        let reader = super::common::logical_json_reader(&self.canonical_path, "Tokenization")?;
        let mut reader = IdentityReader::new(reader);
        let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
        let seed = ManifestSeed {
            loaded: self,
            consume: &mut consume,
        };
        let count = seed
            .deserialize(&mut deserializer)
            .map_err(|error| format!("Manifest di tokenizzazione non valido: {error}"))?;
        deserializer
            .end()
            .map_err(|error| format!("Manifest di tokenizzazione non valido: {error}"))?;
        let (hash, bytes) = reader.finish();
        if hash != self.manifest_blake3 || bytes != self.manifest_bytes {
            return Err("La tokenizzazione è cambiata durante la lettura".to_owned());
        }
        self.segmentation.verify_source()?;
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
            .ok_or_else(|| std::io::Error::other("Dimensione tokenizzazione oltre u64"))?;
        Ok(read)
    }
}

struct ManifestSeed<'a, F> {
    loaded: &'a LoadedStreamingTokenization,
    consume: &'a mut F,
}

impl<'de, F> DeserializeSeed<'de> for ManifestSeed<'_, F>
where
    F: FnMut(TokenStream) -> Result<(), String>,
{
    type Value = usize;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(ManifestVisitor {
            loaded: self.loaded,
            consume: self.consume,
        })
    }
}

struct ManifestVisitor<'a, F> {
    loaded: &'a LoadedStreamingTokenization,
    consume: &'a mut F,
}

impl<'de, F> Visitor<'de> for ManifestVisitor<'_, F>
where
    F: FnMut(TokenStream) -> Result<(), String>,
{
    type Value = usize;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("manifest di tokenizzazione")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = [false; 8];
        let mut stream_count = None;
        while let Some(key) = map.next_key::<String>()? {
            let index = match key.as_str() {
                "schema_version" => 0,
                "segmentation_manifest" => 1,
                "segmentation_manifest_blake3" => 2,
                "segmentation_manifest_bytes" => 3,
                "source_blake3" => 4,
                "source_bytes" => 5,
                "tokenized_at_unix_ms" => 6,
                "streams" => 7,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                    continue;
                }
            };
            if std::mem::replace(&mut seen[index], true) {
                return Err(serde::de::Error::custom(format!(
                    "campo duplicato nella tokenizzazione: {key}"
                )));
            }
            match index {
                0 => compare_field(
                    map.next_value::<u32>()?,
                    self.loaded.manifest.schema_version,
                )?,
                1 => compare_field(
                    map.next_value::<String>()?,
                    self.loaded.manifest.segmentation_manifest.clone(),
                )?,
                2 => compare_field(
                    map.next_value::<String>()?,
                    self.loaded.manifest.segmentation_manifest_blake3.clone(),
                )?,
                3 => compare_field(
                    map.next_value::<u64>()?,
                    self.loaded.manifest.segmentation_manifest_bytes,
                )?,
                4 => compare_field(
                    map.next_value::<String>()?,
                    self.loaded.manifest.source_blake3.clone(),
                )?,
                5 => compare_field(map.next_value::<u64>()?, self.loaded.manifest.source_bytes)?,
                6 => compare_field(
                    map.next_value::<u128>()?,
                    self.loaded.manifest.tokenized_at_unix_ms,
                )?,
                7 => {
                    stream_count = Some(map.next_value_seed(StreamsSeed {
                        loaded: self.loaded,
                        consume: &mut *self.consume,
                    })?);
                }
                _ => unreachable!(),
            }
        }
        if seen.iter().any(|value| !value) {
            return Err(serde::de::Error::custom(
                "campo obbligatorio della tokenizzazione assente",
            ));
        }
        Ok(stream_count.expect("streams marcato come presente"))
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
            "metadati della tokenizzazione cambiati durante la lettura",
        ))
    }
}

struct StreamsSeed<'a, F> {
    loaded: &'a LoadedStreamingTokenization,
    consume: &'a mut F,
}

impl<'de, F> DeserializeSeed<'de> for StreamsSeed<'_, F>
where
    F: FnMut(TokenStream) -> Result<(), String>,
{
    type Value = usize;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(StreamsVisitor {
            loaded: self.loaded,
            consume: self.consume,
        })
    }
}

struct StreamsVisitor<'a, F> {
    loaded: &'a LoadedStreamingTokenization,
    consume: &'a mut F,
}

impl<'de, F> Visitor<'de> for StreamsVisitor<'_, F>
where
    F: FnMut(TokenStream) -> Result<(), String>,
{
    type Value = usize;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sequenza di stream token")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut count = 0_usize;
        self.loaded
            .segmentation
            .for_each_segment(|segment| {
                if segment.kind != "game_candidate" {
                    return Ok(());
                }
                let stream = sequence
                    .next_element::<TokenStream>()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "Stream token assente".to_owned())?;
                validate_stream(&stream, self.loaded.manifest.source_bytes)?;
                if stream.segment_ordinal != segment.ordinal
                    || stream.start != segment.start
                    || stream.end != segment.end
                {
                    return Err(
                        "Stream assente, duplicato, fuori ordine o non game_candidate".to_owned(),
                    );
                }
                let expected =
                    tokenize_streaming_file_segment(&self.loaded.segmentation, &segment)?;
                if stream != expected {
                    return Err("Tokenizzazione diversa dal risultato del lexer".to_owned());
                }
                (self.consume)(stream)?;
                count += 1;
                Ok(())
            })
            .map_err(serde::de::Error::custom)?;
        if sequence.next_element::<IgnoredAny>()?.is_some() {
            return Err(serde::de::Error::custom(
                "Stream token inatteso o duplicato",
            ));
        }
        Ok(count)
    }
}

fn validate_streaming_shape(manifest: &StreamingTokenizationManifest) -> Result<(), String> {
    if manifest.schema_version != TOKENIZATION_SCHEMA_VERSION {
        return Err(format!(
            "Versione tokenizzazione non supportata: {}",
            manifest.schema_version
        ));
    }
    validate_relative_path(&manifest.segmentation_manifest, "segmentation_manifest")?;
    validate_blake3(
        &manifest.segmentation_manifest_blake3,
        "Hash BLAKE3 della segmentazione non valido",
    )?;
    if manifest.segmentation_manifest_bytes == 0 {
        return Err("La dimensione della segmentazione deve essere positiva".to_owned());
    }
    validate_blake3(
        &manifest.source_blake3,
        "Hash BLAKE3 della sorgente non valido",
    )?;
    if manifest.source_bytes == 0 {
        return Err("La dimensione della sorgente deve essere positiva".to_owned());
    }
    Ok(())
}

impl LoadedTokenizationManifest {
    pub(super) fn verify_source(&self) -> Result<(), String> {
        verify_blob(&self.segmentation)
    }

    pub(super) fn read_source_range(&self, start: u64, end: u64) -> Result<Vec<u8>, String> {
        read_source_range(&self.segmentation, start, end)
    }
}

fn verify_blob(loaded: &LoadedSegmentationManifest) -> Result<(), String> {
    let mut file = File::open(&loaded.object_path).map_err(|error| {
        format!(
            "Impossibile aprire {}: {error}",
            loaded.object_path.display()
        )
    })?;
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
    if bytes != loaded.manifest.source_bytes
        || hasher.finalize().to_hex().as_str() != loaded.manifest.source_blake3
    {
        return Err("L'oggetto non corrisponde a hash e dimensione della segmentazione".to_owned());
    }
    Ok(())
}

fn read_segment(loaded: &LoadedSegmentationManifest, segment: &Segment) -> Result<Vec<u8>, String> {
    read_source_range(loaded, segment.start, segment.end)
}

fn read_source_range(
    loaded: &LoadedSegmentationManifest,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, String> {
    read_source_file_range(
        &loaded.object_path,
        loaded.manifest.source_bytes,
        start,
        end,
    )
}

fn read_source_file_range(
    object_path: &Path,
    source_bytes: u64,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, String> {
    let length = end
        .checked_sub(start)
        .ok_or("Intervallo del segmento invertito")?;
    if start >= end || end > source_bytes {
        return Err("Intervallo del segmento fuori dal SourceBlob".into());
    }
    let length = usize::try_from(length)
        .map_err(|_| "Dimensione del segmento non rappresentabile".to_owned())?;
    let mut bytes = vec![0_u8; length];
    let mut file = File::open(object_path)
        .map_err(|error| format!("Impossibile aprire {}: {error}", object_path.display()))?;
    file.seek(SeekFrom::Start(start))
        .map_err(|error| format!("Impossibile raggiungere il segmento: {error}"))?;
    file.read_exact(&mut bytes)
        .map_err(|error| format!("Segmento troncato nel SourceBlob: {error}"))?;
    Ok(bytes)
}

pub(super) fn tokenize_file_segment(
    loaded: &LoadedSegmentationManifest,
    segment: &Segment,
) -> Result<TokenStream, String> {
    tokenize_segment_bytes(&read_segment(loaded, segment)?, segment)
}

pub(super) fn tokenize_streaming_file_segment(
    loaded: &LoadedStreamingSegmentation,
    segment: &Segment,
) -> Result<TokenStream, String> {
    tokenize_segment_bytes(
        &read_streaming_source_range(loaded, segment.start, segment.end)?,
        segment,
    )
}

pub(in crate::inspector) fn tokenize_direct_segment_bytes(
    segment: &Segment,
    bytes: &[u8],
) -> Result<TokenStream, String> {
    tokenize_segment_bytes(bytes, segment)
}

pub(in crate::inspector) fn tokenize_streaming_file_segment_with_bytes(
    loaded: &LoadedStreamingSegmentation,
    segment: &Segment,
) -> Result<(TokenStream, Vec<u8>), String> {
    let bytes = read_streaming_source_range(loaded, segment.start, segment.end)?;
    let stream = tokenize_segment_bytes(&bytes, segment)?;
    Ok((stream, bytes))
}

fn read_streaming_source_range(
    loaded: &LoadedStreamingSegmentation,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, String> {
    read_source_file_range(
        &loaded.object_path,
        loaded.manifest.source_bytes,
        start,
        end,
    )
}

fn validate_tokenization(
    manifest: &TokenizationManifest,
    segmentation: &LoadedSegmentationManifest,
) -> Result<(), String> {
    validate_tokenization_shape(manifest)?;
    if manifest.source_blake3 != segmentation.manifest.source_blake3
        || manifest.source_bytes != segmentation.manifest.source_bytes
    {
        return Err("Tokenizzazione incoerente con la segmentazione".to_owned());
    }
    let candidate_count = segmentation
        .manifest
        .segments
        .iter()
        .filter(|segment| segment.kind == "game_candidate")
        .count();
    if manifest.streams.len() != candidate_count {
        return Err("Numero di stream diverso dai game_candidate".to_owned());
    }
    for (stream, segment) in manifest.streams.iter().zip(
        segmentation
            .manifest
            .segments
            .iter()
            .filter(|segment| segment.kind == "game_candidate"),
    ) {
        if stream.segment_ordinal != segment.ordinal {
            return Err("Stream assente, duplicato, fuori ordine o non game_candidate".to_owned());
        }
        if stream.start != segment.start || stream.end != segment.end {
            return Err("Intervallo dello stream diverso dal game_candidate".to_owned());
        }
        validate_stream(stream, manifest.source_bytes)?;
    }
    Ok(())
}

fn validate_tokenization_shape(manifest: &TokenizationManifest) -> Result<(), String> {
    if manifest.schema_version != TOKENIZATION_SCHEMA_VERSION {
        return Err(format!(
            "Versione tokenizzazione non supportata: {}",
            manifest.schema_version
        ));
    }
    validate_relative_path(&manifest.segmentation_manifest, "segmentation_manifest")?;
    validate_blake3(
        &manifest.segmentation_manifest_blake3,
        "Hash BLAKE3 della segmentazione non valido",
    )?;
    if manifest.segmentation_manifest_bytes == 0 {
        return Err("La dimensione della segmentazione deve essere positiva".to_owned());
    }
    validate_blake3(
        &manifest.source_blake3,
        "Hash BLAKE3 della sorgente non valido",
    )?;
    if manifest.source_bytes == 0 {
        return Err("La dimensione della sorgente deve essere positiva".to_owned());
    }
    Ok(())
}

fn validate_stream(stream: &TokenStream, source_bytes: u64) -> Result<(), String> {
    if stream.start >= stream.end || stream.end > source_bytes {
        return Err("Intervallo dello stream non valido o fuori dal blob".to_owned());
    }
    if stream.tokens.is_empty() {
        return Err("Uno stream deve contenere almeno un token".to_owned());
    }
    let mut expected_start = stream.start;
    for (index, token) in stream.tokens.iter().enumerate() {
        if token.ordinal != index as u64 {
            return Err("Ordinali dei token non consecutivi".to_owned());
        }
        if token.start >= token.end || token.end > source_bytes {
            return Err("Intervallo di token non valido o fuori dal blob".to_owned());
        }
        if token.start != expected_start {
            return Err("Token con buco, sovrapposizione o ordine errato".to_owned());
        }
        expected_start = token.end;
    }
    if expected_start != stream.end {
        return Err("L'ultimo token non termina alla fine dello stream".to_owned());
    }
    Ok(())
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

#[cfg(test)]
fn tokenize_segment(blob: &[u8], segment: &Segment) -> Result<TokenStream, String> {
    let start = usize::try_from(segment.start)
        .map_err(|_| "Offset iniziale del segmento non rappresentabile".to_owned())?;
    let end = usize::try_from(segment.end)
        .map_err(|_| "Offset finale del segmento non rappresentabile".to_owned())?;
    let bytes = blob
        .get(start..end)
        .ok_or("Intervallo del segmento fuori dal blob")?;
    tokenize_segment_bytes(bytes, segment)
}

fn tokenize_segment_bytes(bytes: &[u8], segment: &Segment) -> Result<TokenStream, String> {
    let expected = segment
        .end
        .checked_sub(segment.start)
        .ok_or("Intervallo segmento invertito")?;
    if u64::try_from(bytes.len()).map_err(|_| "Dimensione segmento oltre u64")? != expected {
        return Err("Byte del segmento di dimensione inattesa".into());
    }
    let reserve_div = std::env::var("ANTISKILL_PROBE_TOKEN_DIV")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3)
        .max(1);
    let reserve_slack = std::env::var("ANTISKILL_PROBE_TOKEN_SLACK")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(8);
    let reserve_max = std::env::var("ANTISKILL_PROBE_TOKEN_MAX")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2048)
        .max(16);
    let estimated_tokens = bytes
        .len()
        .saturating_div(reserve_div)
        .saturating_add(reserve_slack)
        .clamp(16, reserve_max);
    let mut tokens = Vec::with_capacity(estimated_tokens);
    let mut index = 0_usize;
    while index < bytes.len() {
        let token_start = index;
        let (token_end, kind) = next_token(bytes, index);
        debug_assert!(token_end > token_start);
        tokens.push(Token {
            ordinal: tokens.len() as u64,
            start: segment.start + token_start as u64,
            end: segment.start + token_end as u64,
            kind,
        });
        index = token_end;
    }
    Ok(TokenStream {
        segment_ordinal: segment.ordinal,
        start: segment.start,
        end: segment.end,
        tokens,
    })
}

fn next_token(bytes: &[u8], index: usize) -> (usize, TokenKind) {
    if let Some(token) = recognized_token(bytes, index) {
        return token;
    }
    let mut end = index + 1;
    while end < bytes.len() && recognized_token(bytes, end).is_none() {
        end += 1;
    }
    (end, TokenKind::Unknown)
}

fn recognized_token(bytes: &[u8], index: usize) -> Option<(usize, TokenKind)> {
    let byte = bytes[index];
    if byte == b'%' && (index == 0 || bytes[index - 1] == b'\n') {
        return Some((line_content_end(bytes, index), TokenKind::EscapeLine));
    }
    if byte.is_ascii_whitespace() {
        return Some((
            take_while(bytes, index, u8::is_ascii_whitespace),
            TokenKind::Whitespace,
        ));
    }
    if byte == b'{' {
        if let Some(relative) = bytes[index + 1..].iter().position(|value| *value == b'}') {
            return Some((index + relative + 2, TokenKind::BraceComment));
        }
        return Some((bytes.len(), TokenKind::UnterminatedBraceComment));
    }
    if byte == b';' {
        return Some((line_content_end(bytes, index), TokenKind::SemicolonComment));
    }
    if byte == b'"' {
        let mut cursor = index + 1;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'\\' if cursor + 1 < bytes.len() => cursor += 2,
                b'"' => return Some((cursor + 1, TokenKind::QuotedString)),
                _ => cursor += 1,
            }
        }
        return Some((bytes.len(), TokenKind::UnterminatedQuotedString));
    }
    let punctuation = match byte {
        b'[' => Some(TokenKind::LeftBracket),
        b']' => Some(TokenKind::RightBracket),
        b'(' => Some(TokenKind::LeftParenthesis),
        b')' => Some(TokenKind::RightParenthesis),
        _ => None,
    };
    if let Some(kind) = punctuation {
        return Some((index + 1, kind));
    }
    if byte == b'$' {
        let end = take_while(bytes, index + 1, u8::is_ascii_digit);
        if end > index + 1 {
            return Some((end, TokenKind::NagNumeric));
        }
    }
    for nag in [b"!!".as_slice(), b"??", b"!?", b"?!", b"!", b"?"] {
        if bytes[index..].starts_with(nag) {
            return Some((index + nag.len(), TokenKind::NagSymbolic));
        }
    }
    for result in [b"1/2-1/2".as_slice(), b"1-0", b"0-1", b"*"] {
        if bytes[index..].starts_with(result) {
            return Some((index + result.len(), TokenKind::ResultCandidate));
        }
    }
    if byte.is_ascii_digit() {
        let digits_end = take_while(bytes, index, u8::is_ascii_digit);
        let dots_end = take_while(bytes, digits_end, |value| *value == b'.');
        if dots_end > digits_end {
            return Some((dots_end, TokenKind::MoveNumberCandidate));
        }
    }
    if is_symbol_byte(&byte) {
        return Some((take_while(bytes, index, is_symbol_byte), TokenKind::Symbol));
    }
    None
}

fn line_content_end(bytes: &[u8], index: usize) -> usize {
    bytes[index..]
        .iter()
        .position(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(bytes.len(), |relative| index + relative)
}

fn take_while<F>(bytes: &[u8], mut index: usize, predicate: F) -> usize
where
    F: Fn(&u8) -> bool,
{
    while index < bytes.len() && predicate(&bytes[index]) {
        index += 1;
    }
    index
}

fn is_symbol_byte(byte: &u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'#' | b'=' | b':' | b'/' | b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspector::segmentation::segment_source;
    use crate::inspector::source::import_source;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture {
        root: PathBuf,
        source_manifest: PathBuf,
        segmentation: PathBuf,
        tokenization: PathBuf,
    }

    fn test_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("orologio precedente a UNIX_EPOCH")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "antiskill-tokenization-{}-{nonce}",
            std::process::id()
        ))
    }

    fn make_fixture(bytes: &[u8]) -> Fixture {
        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let source = root.join("source.pgn");
        fs::write(&source, bytes).expect("scrittura sorgente");
        let imported = import_source(&root, &source).expect("import");
        let segmented = segment_source(&root, &imported.manifest_path).expect("segmentazione");
        let tokenized = tokenize(&root, &segmented.path).expect("tokenizzazione");
        Fixture {
            root,
            source_manifest: imported.manifest_path,
            segmentation: segmented.path,
            tokenization: tokenized.path,
        }
    }

    fn read_tokenization(root: &Path, path: &Path) -> TokenizationManifest {
        serde_json::from_slice(
            &crate::inspector::common::logical_json_bytes(&root.join(path), "test tokenizzazione")
                .expect("lettura tokenizzazione"),
        )
        .expect("deserializzazione tokenizzazione")
    }

    fn kinds(stream: &TokenStream) -> Vec<TokenKind> {
        stream.tokens.iter().map(|token| token.kind).collect()
    }

    fn write_tokenization(root: &Path, path: &Path, manifest: &TokenizationManifest) {
        crate::inspector::common::replace_logical_json(
            &root.join(path),
            &serde_json::to_vec_pretty(manifest).expect("serializzazione tokenizzazione"),
        );
    }

    #[test]
    fn lexer_covers_supported_tokens_non_utf8_and_line_rules_losslessly() {
        let bytes = b"[Event \"A\\\"B\"]\r\n%escape\r\n1... e4$12!? (e5) 1-0 ;semi\xff\r\n{br\xff} x%y @\xfe\n";
        let segment = Segment {
            ordinal: 7,
            start: 0,
            end: bytes.len() as u64,
            kind: "game_candidate".to_owned(),
        };
        let stream = tokenize_segment(bytes, &segment).expect("tokenizzazione");
        let token_kinds = kinds(&stream);

        for expected in [
            TokenKind::LeftBracket,
            TokenKind::QuotedString,
            TokenKind::RightBracket,
            TokenKind::Whitespace,
            TokenKind::EscapeLine,
            TokenKind::MoveNumberCandidate,
            TokenKind::NagNumeric,
            TokenKind::NagSymbolic,
            TokenKind::LeftParenthesis,
            TokenKind::RightParenthesis,
            TokenKind::ResultCandidate,
            TokenKind::SemicolonComment,
            TokenKind::BraceComment,
            TokenKind::Symbol,
            TokenKind::Unknown,
        ] {
            assert!(
                token_kinds.contains(&expected),
                "token assente: {expected:?}"
            );
        }
        assert_eq!(
            token_kinds
                .iter()
                .filter(|kind| **kind == TokenKind::EscapeLine)
                .count(),
            1,
            "% non in prima colonna non deve essere escape_line"
        );
        let semicolon = stream
            .tokens
            .iter()
            .find(|token| token.kind == TokenKind::SemicolonComment)
            .expect("commento semicolon");
        assert!(bytes[semicolon.start as usize..semicolon.end as usize].contains(&0xff));
        let brace = stream
            .tokens
            .iter()
            .find(|token| token.kind == TokenKind::BraceComment)
            .expect("commento brace");
        assert!(bytes[brace.start as usize..brace.end as usize].contains(&0xff));
        let reconstructed = stream
            .tokens
            .iter()
            .flat_map(|token| bytes[token.start as usize..token.end as usize].iter())
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(reconstructed, bytes);
    }

    #[test]
    fn unterminated_constructs_stop_at_the_segment_boundary() {
        for (bytes, expected) in [
            (
                b"\"unterminated\xff".as_slice(),
                TokenKind::UnterminatedQuotedString,
            ),
            (
                b"{unterminated\xff".as_slice(),
                TokenKind::UnterminatedBraceComment,
            ),
        ] {
            let segment = Segment {
                ordinal: 0,
                start: 10,
                end: 10 + bytes.len() as u64,
                kind: "game_candidate".to_owned(),
            };
            let mut blob = vec![0; 10];
            blob.extend_from_slice(bytes);
            blob.extend_from_slice(b"must-not-be-read");
            let stream = tokenize_segment(&blob, &segment).expect("tokenizzazione");
            assert_eq!(stream.tokens.len(), 1);
            assert_eq!(stream.tokens[0].kind, expected);
            assert_eq!(stream.tokens[0].end, segment.end);
        }
    }

    #[test]
    fn unknown_is_maximal_and_stops_before_recognized_tokens() {
        let bytes = b"\xff\xfe\xfd@,& whitespace";
        let segment = Segment {
            ordinal: 0,
            start: 0,
            end: bytes.len() as u64,
            kind: "game_candidate".to_owned(),
        };
        let stream = tokenize_segment(bytes, &segment).expect("tokenizzazione");
        assert_eq!(
            kinds(&stream),
            vec![TokenKind::Unknown, TokenKind::Whitespace, TokenKind::Symbol]
        );
        assert_eq!(&bytes[..stream.tokens[0].end as usize], b"\xff\xfe\xfd@,&");
        assert_eq!(
            stream.tokens[0].end, stream.tokens[1].start,
            "unknown deve fermarsi prima del whitespace"
        );
        let reconstructed = stream
            .tokens
            .iter()
            .flat_map(|token| bytes[token.start as usize..token.end as usize].iter())
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(reconstructed, bytes);

        let bytes = b"@word";
        let segment = Segment {
            ordinal: 0,
            start: 0,
            end: bytes.len() as u64,
            kind: "game_candidate".to_owned(),
        };
        let stream = tokenize_segment(bytes, &segment).expect("tokenizzazione");
        assert_eq!(kinds(&stream), vec![TokenKind::Unknown, TokenKind::Symbol]);
        assert_eq!(stream.tokens[0].end, 1);
    }

    #[test]
    fn pipeline_tokenizes_only_candidates_and_is_repeatable() {
        let bytes = b"outside\xff\n[Event \"One\"]\r\n\r\n1. e4$1!? (e5) 1-0\r\n\r\noutside-tail";
        let fixture = make_fixture(bytes);
        let first = read_tokenization(&fixture.root, &fixture.tokenization);
        let second_created = tokenize(&fixture.root, &fixture.segmentation).expect("seconda");
        let second = read_tokenization(&fixture.root, &second_created.path);

        assert_ne!(fixture.tokenization, second_created.path);
        assert_eq!(first.streams, second.streams);
        assert_eq!(first.streams.len(), 1);
        assert!(first.streams[0].start > 0);
        assert!(first.streams[0].end < bytes.len() as u64);
        assert_eq!(
            first.streams[0]
                .tokens
                .iter()
                .map(|token| token.end - token.start)
                .sum::<u64>(),
            first.streams[0].end - first.streams[0].start
        );
        verify_tokenization(&fixture.root, &fixture.tokenization).expect("verifica");
        fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    }

    #[test]
    fn product_tokenization_cannot_regress_to_full_segmentation_loading() {
        let source = include_str!("tokenization.rs");
        let product = source
            .split("pub(super) fn tokenize(")
            .nth(1)
            .expect("funzione tokenize")
            .split("#[derive(Serialize)]")
            .next()
            .expect("corpo tokenize");
        let streaming_sequence = source
            .split("impl Serialize for StreamingStreams")
            .nth(1)
            .expect("serializer stream")
            .split("#[derive(Default)]")
            .next()
            .expect("corpo serializer");

        assert!(product.contains("load_streaming_segmentation"));
        assert!(!product.contains("load_segmentation_manifest"));
        assert!(streaming_sequence.contains("for_each_segment"));
        assert!(!streaming_sequence.contains(".segments"));
    }

    #[test]
    fn segment_access_uses_exact_absolute_range_and_rejects_truncation() {
        let root = test_root();
        fs::create_dir_all(&root).expect("creazione workspace");
        let source_path = root.join("source.pgn");
        let bytes = b"prefix\n[Event \"One\"]\r\n\r\n1. e4 {\xff} *\r\n\r\ntail";
        fs::write(&source_path, bytes).expect("scrittura sorgente");
        let imported = import_source(&root, &source_path).expect("import");
        let segmented = segment_source(&root, &imported.manifest_path).expect("segmentazione");
        let loaded = load_segmentation_manifest(&root, &segmented.path, None).expect("caricamento");
        let segment = loaded
            .manifest
            .segments
            .iter()
            .find(|segment| segment.kind == "game_candidate")
            .expect("game candidate")
            .clone();

        let actual = read_segment(&loaded, &segment).expect("lettura segmento");
        assert_eq!(actual, bytes[segment.start as usize..segment.end as usize]);
        assert_eq!(
            tokenize_segment_bytes(&actual, &segment).expect("tokenizzazione locale"),
            tokenize_segment(bytes, &segment).expect("tokenizzazione legacy")
        );

        fs::OpenOptions::new()
            .write(true)
            .open(&loaded.object_path)
            .expect("apertura oggetto")
            .set_len(segment.end - 1)
            .expect("troncamento oggetto");
        assert!(read_segment(&loaded, &segment).is_err());
        fs::remove_dir_all(root).expect("pulizia workspace");
    }

    #[test]
    fn validation_rejects_token_interval_and_kind_corruptions() {
        let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
        let original = read_tokenization(&fixture.root, &fixture.tokenization);
        let loaded = load_segmentation_manifest(&fixture.root, &fixture.segmentation, None)
            .expect("segmentazione");

        let mut gap = original.clone();
        gap.streams[0].tokens[1].start += 1;
        assert!(validate_tokenization(&gap, &loaded).is_err());

        let mut overlap = original.clone();
        overlap.streams[0].tokens[1].start -= 1;
        assert!(validate_tokenization(&overlap, &loaded).is_err());

        let mut ordinal = original.clone();
        ordinal.streams[0].tokens[0].ordinal = 1;
        assert!(validate_tokenization(&ordinal, &loaded).is_err());

        let mut empty = original.clone();
        empty.streams[0].tokens[0].end = empty.streams[0].tokens[0].start;
        assert!(validate_tokenization(&empty, &loaded).is_err());

        let mut outside = original.clone();
        outside.streams[0].tokens.last_mut().expect("ultimo").end = outside.source_bytes + 1;
        assert!(validate_tokenization(&outside, &loaded).is_err());

        let mut json = serde_json::to_value(&original).expect("JSON");
        json["streams"][0]["tokens"][0]["kind"] = serde_json::Value::String("future".to_owned());
        assert!(serde_json::from_value::<TokenizationManifest>(json).is_err());

        fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    }

    #[test]
    fn validation_rejects_missing_duplicate_and_unclassified_streams() {
        let fixture = make_fixture(b"head\n[Event \"One\"]\n\n1. e4 *\n\ntail");
        let original = read_tokenization(&fixture.root, &fixture.tokenization);
        let loaded = load_segmentation_manifest(&fixture.root, &fixture.segmentation, None)
            .expect("segmentazione");

        let mut missing = original.clone();
        missing.streams.clear();
        assert!(validate_tokenization(&missing, &loaded).is_err());

        let mut duplicate = original.clone();
        duplicate.streams.push(duplicate.streams[0].clone());
        assert!(validate_tokenization(&duplicate, &loaded).is_err());

        let unclassified = loaded
            .manifest
            .segments
            .iter()
            .find(|segment| segment.kind == "unclassified")
            .expect("segmento unclassified");
        let mut wrong = original;
        wrong.streams[0].segment_ordinal = unclassified.ordinal;
        wrong.streams[0].start = unclassified.start;
        wrong.streams[0].end = unclassified.end;
        assert!(validate_tokenization(&wrong, &loaded).is_err());

        fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    }

    #[test]
    fn verification_rejects_structurally_valid_lexical_changes() {
        let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
        let original = read_tokenization(&fixture.root, &fixture.tokenization);
        let loaded = load_segmentation_manifest(&fixture.root, &fixture.segmentation, None)
            .expect("segmentazione");

        let mut changed_kind = original.clone();
        let whitespace = changed_kind.streams[0]
            .tokens
            .iter_mut()
            .find(|token| token.kind == TokenKind::Whitespace)
            .expect("whitespace");
        whitespace.kind = TokenKind::Symbol;
        assert!(validate_tokenization(&changed_kind, &loaded).is_ok());
        write_tokenization(&fixture.root, &fixture.tokenization, &changed_kind);
        assert!(verify_tokenization(&fixture.root, &fixture.tokenization).is_err());

        let mut merged = original.clone();
        let second = merged.streams[0].tokens.remove(1);
        merged.streams[0].tokens[0].end = second.end;
        for (ordinal, token) in merged.streams[0].tokens.iter_mut().enumerate() {
            token.ordinal = ordinal as u64;
        }
        assert!(validate_tokenization(&merged, &loaded).is_ok());
        write_tokenization(&fixture.root, &fixture.tokenization, &merged);
        assert!(verify_tokenization(&fixture.root, &fixture.tokenization).is_err());

        let mut shifted = original;
        let boundary = shifted.streams[0]
            .tokens
            .windows(2)
            .position(|pair| pair[0].end - pair[0].start > 1)
            .expect("confine spostabile");
        shifted.streams[0].tokens[boundary].end -= 1;
        shifted.streams[0].tokens[boundary + 1].start -= 1;
        assert!(validate_tokenization(&shifted, &loaded).is_ok());
        write_tokenization(&fixture.root, &fixture.tokenization, &shifted);
        assert!(verify_tokenization(&fixture.root, &fixture.tokenization).is_err());

        fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    }

    #[test]
    fn verification_rejects_changed_segmentation_and_source_chain() {
        let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
        let segmentation_path = fixture.root.join(&fixture.segmentation);
        let mut segmentation: serde_json::Value = serde_json::from_slice(
            &crate::inspector::common::logical_json_bytes(&segmentation_path, "segmentazione test")
                .expect("lettura segmentazione"),
        )
        .expect("JSON segmentazione");
        segmentation["segmented_at_unix_ms"] = serde_json::Value::from(1_u64);
        fs::write(
            &segmentation_path,
            serde_json::to_vec_pretty(&segmentation).expect("serializzazione"),
        )
        .expect("modifica segmentazione");
        assert!(verify_tokenization(&fixture.root, &fixture.tokenization).is_err());
        fs::remove_dir_all(fixture.root).expect("pulizia workspace");

        let fixture = make_fixture(b"[Event \"Two\"]\n\n1. d4 *\n");
        let source_manifest_path = fixture.root.join(&fixture.source_manifest);
        let mut source_manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(&source_manifest_path).expect("lettura manifest sorgente"),
        )
        .expect("JSON manifest sorgente");
        source_manifest["source_path"] = serde_json::Value::String("changed.pgn".to_owned());
        fs::write(
            source_manifest_path,
            serde_json::to_vec_pretty(&source_manifest).expect("serializzazione"),
        )
        .expect("modifica manifest sorgente");
        assert!(verify_tokenization(&fixture.root, &fixture.tokenization).is_err());
        fs::remove_dir_all(fixture.root).expect("pulizia workspace");

        let fixture = make_fixture(b"[Event \"Three\"]\n\n1. c4 *\n");
        let loaded = load_segmentation_manifest(&fixture.root, &fixture.segmentation, None)
            .expect("segmentazione");
        fs::write(&loaded.object_path, b"altered").expect("modifica oggetto");
        assert!(verify_tokenization(&fixture.root, &fixture.tokenization).is_err());
        fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    }

    #[test]
    fn verification_rejects_path_traversal() {
        let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
        let path = fixture.root.join(&fixture.tokenization);
        let mut manifest = read_tokenization(&fixture.root, &fixture.tokenization);
        manifest.segmentation_manifest = "../outside.json".to_owned();
        fs::write(
            &path,
            serde_json::to_vec(&manifest).expect("serializzazione"),
        )
        .expect("modifica tokenizzazione");
        assert!(verify_tokenization(&fixture.root, &fixture.tokenization).is_err());
        fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    }

    #[cfg(unix)]
    #[test]
    fn tokenization_and_segmentation_symlinks_outside_workspace_are_rejected() {
        use std::os::unix::fs::symlink;

        let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
        let outside = fixture.root.with_extension("outside-tokenization.json");
        fs::write(
            &outside,
            fs::read(fixture.root.join(&fixture.tokenization)).expect("tokenizzazione"),
        )
        .expect("file esterno");
        let link = fixture.root.join("tokenization-link.json");
        symlink(&outside, &link).expect("symlink tokenizzazione");
        assert!(verify_tokenization(&fixture.root, Path::new("tokenization-link.json")).is_err());
        fs::remove_file(&link).expect("rimozione symlink");

        let outside_segmentation = fixture.root.with_extension("outside-segmentation.json");
        fs::write(
            &outside_segmentation,
            fs::read(fixture.root.join(&fixture.segmentation)).expect("segmentazione"),
        )
        .expect("segmentazione esterna");
        let segmentation_link = fixture.root.join("inspector/segmentation-link.json");
        symlink(&outside_segmentation, &segmentation_link).expect("symlink segmentazione");
        let mut manifest = read_tokenization(&fixture.root, &fixture.tokenization);
        manifest.segmentation_manifest = "inspector/segmentation-link.json".to_owned();
        manifest.segmentation_manifest_blake3 =
            blake3::hash(&fs::read(&outside_segmentation).expect("lettura segmentazione esterna"))
                .to_hex()
                .to_string();
        manifest.segmentation_manifest_bytes =
            fs::metadata(&outside_segmentation).expect("metadati").len();
        fs::write(
            fixture.root.join(&fixture.tokenization),
            serde_json::to_vec(&manifest).expect("serializzazione"),
        )
        .expect("modifica tokenizzazione");
        assert!(verify_tokenization(&fixture.root, &fixture.tokenization).is_err());

        fs::remove_dir_all(fixture.root).expect("pulizia workspace");
        fs::remove_file(outside).expect("pulizia esterno");
        fs::remove_file(outside_segmentation).expect("pulizia segmentazione esterna");
    }

    #[cfg(unix)]
    #[test]
    fn tokenization_publication_directory_symlink_outside_workspace_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = test_root();
        let outside = root.with_extension("outside-tokenizations");
        fs::create_dir_all(root.join("inspector")).expect("directory inspector");
        fs::create_dir_all(&outside).expect("directory esterna");
        symlink(&outside, root.join("inspector/tokenizations")).expect("symlink directory");
        let source = root.join("source.pgn");
        fs::write(&source, b"[Event \"One\"]\n\n1. e4 *\n").expect("sorgente");
        let imported = import_source(&root, &source).expect("import");
        let segmented = segment_source(&root, &imported.manifest_path).expect("segmentazione");

        assert!(tokenize(&root, &segmented.path).is_err());
        assert!(
            fs::read_dir(&outside)
                .expect("lettura directory esterna")
                .next()
                .is_none(),
            "nessuna tokenizzazione deve essere pubblicata fuori dal workspace"
        );

        fs::remove_dir_all(root).expect("pulizia workspace");
        fs::remove_dir_all(outside).expect("pulizia directory esterna");
    }
}
