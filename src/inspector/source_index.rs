use super::common::{
    create_unique_temporary, ensure_within, path_to_manifest_string, validate_relative_path,
};
use super::{
    catalog, interpretation, mainlines, occurrences, parsing, segmentation, semantic_probe,
    tokenization,
};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};

const MAGIC: &[u8; 8] = b"ASKIDX1\0";
const VERSION: u32 = 1;
const HEADER_BYTES: u64 = 56;
const RECORD_BYTES: u64 = 88;

pub(in crate::inspector) fn expected_index_bytes(games: u64) -> Result<u64, String> {
    HEADER_BYTES
        .checked_add(
            games
                .checked_mul(RECORD_BYTES)
                .ok_or("Dimensione SourceIndex oltre u64")?,
        )
        .ok_or("Dimensione SourceIndex oltre u64".into())
}
const WORKERS: usize = 3;
const QUEUE_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::inspector) struct SourceIndexRecordV1 {
    pub(in crate::inspector) ordinal: u64,
    pub(in crate::inspector) segment_ordinal: u64,
    pub(in crate::inspector) source_start: u64,
    pub(in crate::inspector) source_end: u64,
    pub(in crate::inspector) source_game_blake3: String,
    pub(in crate::inspector) canonical_mainline_id: Option<String>,
    pub(in crate::inspector) comparable: bool,
    pub(in crate::inspector) direct_fast: bool,
}

#[derive(Debug)]
pub(in crate::inspector) struct CreatedSourceIndexV1 {
    pub(in crate::inspector) path: PathBuf,
    pub(in crate::inspector) blake3: String,
    pub(in crate::inspector) bytes: u64,
    pub(in crate::inspector) games: u64,
}

#[derive(Debug, Default)]
pub(in crate::inspector) struct ProductProjectionV1 {
    pub(in crate::inspector) white: Vec<String>,
    pub(in crate::inspector) black: Vec<String>,
    pub(in crate::inspector) event: Vec<String>,
    pub(in crate::inspector) date: Vec<String>,
    pub(in crate::inspector) result: Vec<String>,
    pub(in crate::inspector) round: Vec<String>,
    pub(in crate::inspector) mainline_results: Vec<String>,
}

struct IndexedSummary {
    segment_ordinal: u64,
    summary: semantic_probe::GameSummary,
}

struct WorkItem {
    ordinal: u64,
    segment_ordinal: u64,
    stream: tokenization::TokenStream,
    blob: Vec<u8>,
}

struct IndexWriter {
    writer: BufWriter<File>,
    hasher: blake3::Hasher,
    bytes: u64,
    records: u64,
}

impl IndexWriter {
    fn new(file: File, source_blake3: &str, source_bytes: u64) -> Result<Self, String> {
        let mut this = Self {
            writer: BufWriter::with_capacity(32 * 1024, file),
            hasher: blake3::Hasher::new(),
            bytes: 0,
            records: 0,
        };
        this.push(MAGIC)?;
        this.push(&VERSION.to_le_bytes())?;
        this.push(&(RECORD_BYTES as u32).to_le_bytes())?;
        this.push(&source_bytes.to_le_bytes())?;
        this.push(&decode_hash(source_blake3)?)?;
        Ok(this)
    }

    fn push(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writer
            .write_all(bytes)
            .map_err(|e| format!("Scrittura SourceIndex: {e}"))?;
        self.hasher.update(bytes);
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or("SourceIndex oltre u64")?;
        Ok(())
    }

    fn record(&mut self, item: &IndexedSummary) -> Result<(), String> {
        let game = &item.summary;
        let game_bytes = game
            .source_end
            .checked_sub(game.source_start)
            .ok_or("Intervallo SourceIndex invertito")?;
        let game_bytes = u32::try_from(game_bytes)
            .map_err(|_| "Partita oltre 4 GiB non rappresentabile nel SourceIndex".to_owned())?;
        self.push(&game.source_start.to_le_bytes())?;
        self.push(&game_bytes.to_le_bytes())?;
        self.push(&item.segment_ordinal.to_le_bytes())?;
        self.push(&decode_hash(&game.source_game_blake3)?)?;
        let canonical = game
            .identity
            .as_deref()
            .map(decode_hash)
            .transpose()?
            .unwrap_or([0; 32]);
        self.push(&canonical)?;
        self.push(&[u8::from(game.comparable), u8::from(game.direct_fast), 0, 0])?;
        self.records = self
            .records
            .checked_add(1)
            .ok_or("Numero record SourceIndex oltre u64")?;
        Ok(())
    }

    fn finish(mut self) -> Result<(String, u64, u64), String> {
        self.writer
            .flush()
            .map_err(|e| format!("Flush SourceIndex: {e}"))?;
        self.writer
            .get_ref()
            .sync_all()
            .map_err(|e| format!("Sync SourceIndex: {e}"))?;
        let expected = HEADER_BYTES
            .checked_add(
                self.records
                    .checked_mul(RECORD_BYTES)
                    .ok_or("Dimensione SourceIndex oltre u64")?,
            )
            .ok_or("Dimensione SourceIndex oltre u64")?;
        if expected != self.bytes {
            return Err("Dimensione SourceIndex interna incoerente".into());
        }
        Ok((
            self.hasher.finalize().to_hex().to_string(),
            self.bytes,
            self.records,
        ))
    }
}

fn decode_hash(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 {
        return Err("Hash SourceIndex non lungo 64 caratteri".into());
    }
    let mut out = [0u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        let pair = &value.as_bytes()[i * 2..i * 2 + 2];
        let n = |b: u8| match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            _ => Err("Hash SourceIndex non esadecimale lowercase".to_owned()),
        };
        *slot = (n(pair[0])? << 4) | n(pair[1])?;
    }
    Ok(out)
}

fn encode_hash(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 15) as usize] as char);
    }
    out
}

fn files_equal(a: &Path, b: &Path) -> Result<bool, String> {
    let ma = fs::metadata(a).map_err(|e| format!("Stat SourceIndex: {e}"))?;
    let mb = fs::metadata(b).map_err(|e| format!("Stat SourceIndex: {e}"))?;
    if ma.len() != mb.len() {
        return Ok(false);
    }
    let mut a = BufReader::new(File::open(a).map_err(|e| format!("Apertura SourceIndex: {e}"))?);
    let mut b = BufReader::new(File::open(b).map_err(|e| format!("Apertura SourceIndex: {e}"))?);
    let mut aa = [0u8; 65536];
    let mut bb = [0u8; 65536];
    loop {
        let na = a.read(&mut aa).map_err(|e| e.to_string())?;
        let nb = b.read(&mut bb).map_err(|e| e.to_string())?;
        if na != nb || aa[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
    }
}

fn collect_results(
    rx: Receiver<Result<IndexedSummary, String>>,
    file: File,
    source_blake3: &str,
    source_bytes: u64,
) -> Result<(String, u64, u64), String> {
    let mut writer = IndexWriter::new(file, source_blake3, source_bytes)?;
    let mut pending = BTreeMap::new();
    let mut next = 0u64;
    let mut first_error = None;
    while let Ok(result) = rx.recv() {
        match result {
            Ok(item) if first_error.is_none() => {
                if pending.insert(item.summary.ordinal, item).is_some() {
                    first_error = Some("Ordinal duplicato nel SourceIndex".into());
                    continue;
                }
                while let Some(item) = pending.remove(&next) {
                    if let Err(e) = writer.record(&item) {
                        first_error = Some(e);
                        break;
                    }
                    next = next.checked_add(1).ok_or("Ordinal SourceIndex oltre u64")?;
                }
            }
            Ok(_) => {}
            Err(e) if first_error.is_none() => first_error = Some(e),
            Err(_) => {}
        }
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    if !pending.is_empty() {
        return Err("Gap di ordinal nel SourceIndex".into());
    }
    writer.finish()
}

fn worker_loop(
    rx: Receiver<WorkItem>,
    tx: SyncSender<Result<IndexedSummary, String>>,
    source_blake3: String,
    source_bytes: u64,
) {
    while let Ok(item) = rx.recv() {
        let result = semantic_probe::process_fused_game(
            item.ordinal,
            item.stream,
            item.blob,
            &source_blake3,
            source_bytes,
        )
        .map(|summary| IndexedSummary {
            segment_ordinal: item.segment_ordinal,
            summary,
        });
        if tx.send(result).is_err() {
            break;
        }
    }
}

pub(in crate::inspector) fn create(
    root: &Path,
    object_path: &Path,
    source_blake3: &str,
    source_bytes: u64,
) -> Result<CreatedSourceIndexV1, String> {
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let canonical_object =
        fs::canonicalize(object_path).map_err(|e| format!("SourceBlob non valido: {e}"))?;
    ensure_within(&canonical_root, &canonical_object, "SourceBlob")?;
    let directory = root.join("inspector/source-indexes");
    fs::create_dir_all(&directory).map_err(|e| format!("Creazione SourceIndex: {e}"))?;
    let canonical_directory = fs::canonicalize(&directory)
        .map_err(|e| format!("Directory SourceIndex non valida: {e}"))?;
    ensure_within(
        &canonical_root,
        &canonical_directory,
        "directory SourceIndex",
    )?;
    let (file, temp) = create_unique_temporary(&canonical_directory, ".source-index")?;
    let built = std::thread::scope(|scope| -> Result<(String, u64, u64), String> {
        let (result_tx, result_rx) =
            mpsc::sync_channel::<Result<IndexedSummary, String>>(WORKERS * QUEUE_DEPTH * 2);
        let collector_hash = source_blake3.to_owned();
        let collector =
            scope.spawn(move || collect_results(result_rx, file, &collector_hash, source_bytes));
        let mut senders = Vec::with_capacity(WORKERS);
        let mut handles = Vec::with_capacity(WORKERS);
        for _ in 0..WORKERS {
            let (tx, rx) = mpsc::sync_channel::<WorkItem>(QUEUE_DEPTH);
            senders.push(tx);
            let rt = result_tx.clone();
            let hash = source_blake3.to_owned();
            handles.push(scope.spawn(move || worker_loop(rx, rt, hash, source_bytes)));
        }
        drop(result_tx);
        let mut ordinal = 0u64;
        let mut producer_error = None;
        let scan =
            segmentation::for_each_game_candidate_blob(&canonical_object, |segment, blob| {
                let stream = tokenization::tokenize_direct_segment_bytes(&segment, &blob)?;
                let idx = (ordinal as usize) % WORKERS;
                senders[idx]
                    .send(WorkItem {
                        ordinal,
                        segment_ordinal: segment.ordinal,
                        stream,
                        blob,
                    })
                    .map_err(|_| "Worker SourceIndex terminato prematuramente".to_owned())?;
                ordinal = ordinal
                    .checked_add(1)
                    .ok_or("Ordinal SourceIndex oltre u64")?;
                Ok(())
            });
        match scan {
            Ok((bytes, hash, games))
                if bytes == source_bytes && hash == source_blake3 && games == ordinal => {}
            Ok(_) => {
                producer_error =
                    Some("SourceBlob diverso dall'identità dichiarata durante SourceIndex".into())
            }
            Err(e) => producer_error = Some(e),
        }
        drop(senders);
        for h in handles {
            if h.join().is_err() && producer_error.is_none() {
                producer_error = Some("Worker SourceIndex in panic".into());
            }
        }
        let result = collector
            .join()
            .map_err(|_| "Collector SourceIndex in panic".to_owned())??;
        if let Some(e) = producer_error {
            return Err(e);
        }
        Ok(result)
    });
    let (hash, bytes, games) = match built {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_file(&temp);
            return Err(e);
        }
    };
    let target = canonical_directory.join(format!("{hash}.bin"));
    match fs::hard_link(&temp, &target) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if !files_equal(&temp, &target)? {
                let _ = fs::remove_file(&temp);
                return Err("Collisione content-addressed SourceIndex".into());
            }
        }
        Err(e) => {
            let _ = fs::remove_file(&temp);
            return Err(format!("Pubblicazione SourceIndex: {e}"));
        }
    }
    fs::remove_file(&temp).map_err(|e| format!("Pulizia temporaneo SourceIndex: {e}"))?;
    let relative = target
        .strip_prefix(&canonical_root)
        .map_err(|_| "SourceIndex fuori workspace")?;
    Ok(CreatedSourceIndexV1 {
        path: PathBuf::from(path_to_manifest_string(relative)),
        blake3: hash,
        bytes,
        games,
    })
}

fn source_object_path(root: &Path, hash: &str) -> Result<PathBuf, String> {
    decode_hash(hash)?;
    Ok(root.join("raw/objects/blake3").join(&hash[..2]).join(hash))
}

fn read_exact_hash<R: Read>(
    reader: &mut R,
    buf: &mut [u8],
    hasher: &mut blake3::Hasher,
) -> Result<(), String> {
    reader
        .read_exact(buf)
        .map_err(|e| format!("Lettura SourceIndex: {e}"))?;
    hasher.update(buf);
    Ok(())
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().expect("4 byte"))
}
fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b.try_into().expect("8 byte"))
}

pub(in crate::inspector) fn for_each_verified_record<F>(
    root: &Path,
    path: &Path,
    index_blake3: &str,
    index_bytes: u64,
    source_blake3: &str,
    source_bytes: u64,
    mut on_record: F,
) -> Result<u64, String>
where
    F: FnMut(&SourceIndexRecordV1, &[u8]) -> Result<(), String>,
{
    let rel = validate_relative_path(
        path.to_str().ok_or("Percorso SourceIndex non UTF-8")?,
        "SourceIndex",
    )?;
    let expected = PathBuf::from("inspector/source-indexes").join(format!("{index_blake3}.bin"));
    if rel != expected {
        return Err("Percorso SourceIndex non canonico".into());
    }
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let resolved = root.join(rel);
    let meta =
        fs::symlink_metadata(&resolved).map_err(|e| format!("SourceIndex non valido: {e}"))?;
    if meta.file_type().is_symlink() || !meta.is_file() || meta.len() != index_bytes {
        return Err("Identità fisica SourceIndex non valida".into());
    }
    let canonical_index =
        fs::canonicalize(&resolved).map_err(|e| format!("SourceIndex non valido: {e}"))?;
    ensure_within(&canonical_root, &canonical_index, "SourceIndex")?;
    if index_bytes < HEADER_BYTES || !(index_bytes - HEADER_BYTES).is_multiple_of(RECORD_BYTES) {
        return Err("Dimensione SourceIndex non canonica".into());
    }
    let records = (index_bytes - HEADER_BYTES) / RECORD_BYTES;
    let source_path = source_object_path(root, source_blake3)?;
    let sm =
        fs::symlink_metadata(&source_path).map_err(|e| format!("SourceBlob non valido: {e}"))?;
    if sm.file_type().is_symlink() || !sm.is_file() || sm.len() != source_bytes {
        return Err("Identità fisica SourceBlob non valida".into());
    }
    let canonical_source =
        fs::canonicalize(&source_path).map_err(|e| format!("SourceBlob non valido: {e}"))?;
    ensure_within(&canonical_root, &canonical_source, "SourceBlob")?;
    let mut idx = BufReader::with_capacity(
        64 * 1024,
        File::open(&canonical_index).map_err(|e| e.to_string())?,
    );
    let mut ih = blake3::Hasher::new();
    let mut header = [0u8; 56];
    read_exact_hash(&mut idx, &mut header, &mut ih)?;
    if &header[..8] != MAGIC
        || u32le(&header[8..12]) != VERSION
        || u32le(&header[12..16]) as u64 != RECORD_BYTES
        || u64le(&header[16..24]) != source_bytes
        || encode_hash(&header[24..56]) != source_blake3
    {
        return Err("Header SourceIndex non valido".into());
    }
    let mut src = BufReader::with_capacity(
        64 * 1024,
        File::open(&canonical_source).map_err(|e| e.to_string())?,
    );
    let mut sh = blake3::Hasher::new();
    let mut source_pos = 0u64;
    let mut prior_end = 0u64;
    let mut scratch = [0u8; 65536];
    for ordinal in 0..records {
        let mut raw = [0u8; 88];
        read_exact_hash(&mut idx, &mut raw, &mut ih)?;
        let start = u64le(&raw[0..8]);
        let len = u64::from(u32le(&raw[8..12]));
        let segment = u64le(&raw[12..20]);
        let end = start
            .checked_add(len)
            .ok_or("Intervallo SourceIndex oltre u64")?;
        let comparable = raw[84] != 0;
        let direct_fast = raw[85] != 0;
        if raw[86] != 0 || raw[87] != 0 || len == 0 || start < prior_end || end > source_bytes {
            return Err("Record SourceIndex non canonico".into());
        }
        let canonical_raw = &raw[52..84];
        let canonical = if canonical_raw.iter().all(|b| *b == 0) {
            None
        } else {
            Some(encode_hash(canonical_raw))
        };
        if comparable != canonical.is_some() {
            return Err("Confrontabilità SourceIndex incoerente".into());
        }
        while source_pos < start {
            let take = usize::try_from((start - source_pos).min(scratch.len() as u64)).unwrap();
            src.read_exact(&mut scratch[..take])
                .map_err(|e| format!("Lettura SourceBlob: {e}"))?;
            sh.update(&scratch[..take]);
            source_pos += take as u64;
        }
        if source_pos != start {
            return Err("Record SourceIndex sovrapposto".into());
        }
        let usize_len =
            usize::try_from(len).map_err(|_| "Partita non rappresentabile in memoria")?;
        let mut blob = vec![0u8; usize_len];
        src.read_exact(&mut blob)
            .map_err(|e| format!("Lettura partita SourceBlob: {e}"))?;
        sh.update(&blob);
        source_pos = end;
        let game_hash = blake3::hash(&blob).to_hex().to_string();
        if game_hash != encode_hash(&raw[20..52]) {
            return Err("Hash partita SourceIndex non valido".into());
        }
        let record = SourceIndexRecordV1 {
            ordinal,
            segment_ordinal: segment,
            source_start: start,
            source_end: end,
            source_game_blake3: game_hash,
            canonical_mainline_id: canonical,
            comparable,
            direct_fast,
        };
        on_record(&record, &blob)?;
        prior_end = end;
    }
    loop {
        let n = src
            .read(&mut scratch)
            .map_err(|e| format!("Lettura SourceBlob: {e}"))?;
        if n == 0 {
            break;
        }
        sh.update(&scratch[..n]);
        source_pos = source_pos
            .checked_add(n as u64)
            .ok_or("SourceBlob oltre u64")?;
    }
    if source_pos != source_bytes || sh.finalize().to_hex().as_str() != source_blake3 {
        return Err("SourceBlob diverso dall'identità dichiarata".into());
    }
    let mut extra = [0u8; 1];
    if idx.read(&mut extra).map_err(|e| e.to_string())? != 0 {
        return Err("Byte extra nel SourceIndex".into());
    }
    if ih.finalize().to_hex().as_str() != index_blake3 {
        return Err("Hash SourceIndex non valido".into());
    }
    Ok(records)
}

fn decoded_simple_value(raw: &[u8]) -> Option<String> {
    if raw.len() < 2 || raw[0] != b'"' || *raw.last()? != b'"' {
        return None;
    }
    let mut out = Vec::with_capacity(raw.len() - 2);
    let mut i = 1;
    while i + 1 < raw.len() {
        if raw[i] == b'\\' {
            i += 1;
            if i + 1 >= raw.len() || !matches!(raw[i], b'\\' | b'"') {
                return None;
            }
            out.push(raw[i]);
            i += 1;
        } else {
            out.push(raw[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn fast_projection(
    stream: &tokenization::TokenStream,
    blob: &[u8],
) -> Result<ProductProjectionV1, String> {
    let mut p = ProductProjectionV1::default();
    let tokens = &stream.tokens;
    let local = |t: &tokenization::Token| -> Result<&[u8], String> {
        let s = usize::try_from(t.start - stream.start).map_err(|_| "Offset token")?;
        let e = usize::try_from(t.end - stream.start).map_err(|_| "Offset token")?;
        blob.get(s..e).ok_or("Token fuori blob".into())
    };
    let mut i = 0usize;
    while i < tokens.len() {
        while i < tokens.len() && tokens[i].kind == tokenization::TokenKind::Whitespace {
            i += 1;
        }
        if i >= tokens.len() || tokens[i].kind != tokenization::TokenKind::LeftBracket {
            break;
        }
        i += 1;
        while i < tokens.len() && tokens[i].kind == tokenization::TokenKind::Whitespace {
            i += 1;
        }
        let name = tokens
            .get(i)
            .filter(|t| t.kind == tokenization::TokenKind::Symbol)
            .ok_or("Tag fast non canonico")?;
        let name = std::str::from_utf8(local(name)?).ok().map(str::to_owned);
        i += 1;
        while i < tokens.len() && tokens[i].kind == tokenization::TokenKind::Whitespace {
            i += 1;
        }
        let value = tokens
            .get(i)
            .filter(|t| t.kind == tokenization::TokenKind::QuotedString)
            .ok_or("Tag fast senza valore")?;
        let value = decoded_simple_value(local(value)?);
        i += 1;
        while i < tokens.len() && tokens[i].kind == tokenization::TokenKind::Whitespace {
            i += 1;
        }
        if tokens
            .get(i)
            .is_none_or(|t| t.kind != tokenization::TokenKind::RightBracket)
        {
            return Err("Tag fast senza chiusura".into());
        }
        i += 1;
        if let (Some(n), Some(v)) = (name, value) {
            match n.to_ascii_lowercase().as_str() {
                "white" => p.white.push(v),
                "black" => p.black.push(v),
                "event" => p.event.push(v),
                "date" => p.date.push(v),
                "result" => p.result.push(v),
                "round" => p.round.push(v),
                _ => {}
            }
        }
    }
    for token in &tokens[i..] {
        if token.kind == tokenization::TokenKind::ResultCandidate {
            if let Ok(v) = std::str::from_utf8(local(token)?) {
                p.mainline_results.push(v.to_owned());
            }
        }
    }
    Ok(p)
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn strict_header_tag(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let line = trim_ascii(line.strip_suffix(b"\r").unwrap_or(line));
    if line.first() != Some(&b'[') || line.last() != Some(&b']') {
        return None;
    }
    let inner = &line[1..line.len() - 1];
    let split = inner.iter().position(u8::is_ascii_whitespace)?;
    let name = &inner[..split];
    if name.is_empty()
        || !name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        return None;
    }
    let rest = trim_ascii(&inner[split..]);
    if rest.first() != Some(&b'"') {
        return None;
    }
    let mut escaped = false;
    let mut close = None;
    for (index, byte) in rest.iter().enumerate().skip(1) {
        if escaped {
            if !matches!(*byte, b'\\' | b'"') {
                return None;
            }
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else if *byte == b'"' {
            close = Some(index);
            break;
        }
    }
    let close = close?;
    if escaped || !trim_ascii(&rest[close + 1..]).is_empty() {
        return None;
    }
    Some((name, &rest[1..close]))
}

fn raw_value_is_informative(value: &[u8]) -> bool {
    let value = trim_ascii(value);
    !value.is_empty()
        && value
            .iter()
            .any(|byte| byte.is_ascii_alphanumeric() || !byte.is_ascii())
}

fn eq_ascii_case(name: &[u8], expected: &[u8]) -> bool {
    name.eq_ignore_ascii_case(expected)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) struct HeaderPlayerSideEvidenceV1 {
    pub(in crate::inspector) side: crate::inspector::PlayerSideV1,
    pub(in crate::inspector) names: Vec<String>,
    pub(in crate::inspector) fide_id_values: Vec<String>,
}

fn decode_strict_header_value(value: &[u8]) -> Option<String> {
    let mut out = Vec::with_capacity(value.len());
    let mut index = 0usize;
    while index < value.len() {
        if value[index] == b'\\' {
            index = index.checked_add(1)?;
            let escaped = *value.get(index)?;
            if !matches!(escaped, b'\\' | b'"') {
                return None;
            }
            out.push(escaped);
        } else {
            out.push(value[index]);
        }
        index = index.checked_add(1)?;
    }
    String::from_utf8(out).ok()
}

pub(in crate::inspector) fn player_header_evidence_from_blob(
    blob: &[u8],
) -> Vec<HeaderPlayerSideEvidenceV1> {
    let mut white_names = Vec::new();
    let mut black_names = Vec::new();
    let mut white_fide = Vec::new();
    let mut black_fide = Vec::new();
    for line in blob.split(|byte| *byte == b'\n') {
        let trimmed = trim_ascii(line.strip_suffix(b"\r").unwrap_or(line));
        if trimmed.is_empty() {
            continue;
        }
        let Some((name, value)) = strict_header_tag(trimmed) else {
            break;
        };
        let Some(value) = decode_strict_header_value(value) else {
            continue;
        };
        if eq_ascii_case(name, b"White") {
            white_names.push(value);
        } else if eq_ascii_case(name, b"Black") {
            black_names.push(value);
        } else if eq_ascii_case(name, b"WhiteFideId") {
            white_fide.push(value);
        } else if eq_ascii_case(name, b"BlackFideId") {
            black_fide.push(value);
        }
    }
    vec![
        HeaderPlayerSideEvidenceV1 {
            side: crate::inspector::PlayerSideV1::White,
            names: white_names,
            fide_id_values: white_fide,
        },
        HeaderPlayerSideEvidenceV1 {
            side: crate::inspector::PlayerSideV1::Black,
            names: black_names,
            fide_id_values: black_fide,
        },
    ]
}

pub(in crate::inspector) fn representative_quality_from_blob(blob: &[u8]) -> u8 {
    let mut white = false;
    let mut black = false;
    let mut date = false;
    let mut event = false;
    let mut result = false;
    for line in blob.split(|byte| *byte == b'\n') {
        let trimmed = trim_ascii(line.strip_suffix(b"\r").unwrap_or(line));
        if trimmed.is_empty() {
            continue;
        }
        let Some((name, value)) = strict_header_tag(trimmed) else {
            break;
        };
        if !raw_value_is_informative(value) {
            continue;
        }
        if eq_ascii_case(name, b"White") {
            white = true;
        } else if eq_ascii_case(name, b"Black") {
            black = true;
        } else if eq_ascii_case(name, b"Date") {
            date = true;
        } else if eq_ascii_case(name, b"Event") {
            event = true;
        } else if eq_ascii_case(name, b"Result") {
            result = true;
        }
    }
    u8::from(white) * 8
        + u8::from(black) * 8
        + u8::from(date) * 4
        + u8::from(event) * 2
        + u8::from(result)
}

pub(in crate::inspector) fn product_projection(
    record: &SourceIndexRecordV1,
    blob: &[u8],
    source_blake3: &str,
    source_bytes: u64,
) -> Result<ProductProjectionV1, String> {
    let segment = segmentation::Segment {
        ordinal: record.segment_ordinal,
        start: record.source_start,
        end: record.source_end,
        kind: "game_candidate".into(),
    };
    let stream = tokenization::tokenize_direct_segment_bytes(&segment, blob)?;
    if record.direct_fast {
        return fast_projection(&stream, blob);
    }
    let tree = parsing::parse_stream_direct(&stream)?;
    let prof = interpretation::interpret_one_direct_profiled(record.ordinal, &tree, &stream, blob)?;
    let game = prof.record;
    let cg =
        catalog::probe_catalog_game(record.ordinal, &game, &tree, &stream, blob, source_bytes)?;
    let mut p = ProductProjectionV1::default();
    for tag in &cg.tags {
        if tag.decode_status != catalog::manifest::TagDecodeStatusV1::Decoded {
            continue;
        }
        let (Some(name), Some(value)) = (tag.name.as_deref(), tag.value.as_deref()) else {
            continue;
        };
        match name.to_ascii_lowercase().as_str() {
            "white" => p.white.push(value.to_owned()),
            "black" => p.black.push(value.to_owned()),
            "event" => p.event.push(value.to_owned()),
            "date" => p.date.push(value.to_owned()),
            "result" => p.result.push(value.to_owned()),
            "round" => p.round.push(value.to_owned()),
            _ => {}
        }
    }
    let occ = occurrences::probe_occurrence(
        record.ordinal,
        source_blake3,
        source_bytes,
        &cg,
        &game,
        &tree,
        &stream,
    )?;
    let ml = mainlines::probe_mainline(record.ordinal, &occ, &game, &tree, &stream, blob)?;
    p.mainline_results = ml.result_values;
    Ok(p)
}

#[cfg(test)]
mod representative_quality_tests {
    use super::{decoded_simple_value, representative_quality_from_blob};

    #[test]
    fn decoded_simple_value_accepts_valid_utf8() {
        assert_eq!(
            decoded_simple_value("\"4° Rapid Città Sintetica\"".as_bytes()).as_deref(),
            Some("4° Rapid Città Sintetica")
        );
    }

    #[test]
    fn representative_quality_is_conservative_and_metadata_only() {
        let poor = b"[Event \"\"]\n[Date \"????.??.??\"]\n[White \"\"]\n[Black \"?\"]\n[Result \"0-1\"]\n\n1. e4 e5 0-1\n";
        let rich = "[Event \"Torneo Aurora\"]\n[Date \"2024.02.09\"]\n[White \"Esempio, Auròra\"]\n[Black \"Beta, Bruno\"]\n[Result \"0-1\"]\n\n1. d4 d5 0-1\n".as_bytes();
        assert_eq!(representative_quality_from_blob(poor), 1);
        assert_eq!(representative_quality_from_blob(rich), 23);
    }

    #[test]
    fn representative_quality_stops_at_non_header_content() {
        let blob = b"[White \"Alpha\"]\n\n1. e4 e5\n[Black \"Beta\"]\n";
        assert_eq!(representative_quality_from_blob(blob), 8);
    }

    #[test]
    fn malformed_or_unsupported_header_syntax_never_gains_quality() {
        assert_eq!(representative_quality_from_blob(b"[White Alpha]\n"), 0);
        assert_eq!(representative_quality_from_blob(b"[White \"A\\q\"]\n"), 0);
    }
}
