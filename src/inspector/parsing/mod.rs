use super::common::{
    deserialize_logical_json_with_identity, ensure_within, logical_json_reader,
    path_to_manifest_string, publish_compressed_json_stream, resolve_workspace_path, unix_time_ms,
    validate_relative_path,
};
use super::tokenization::{load_streaming_tokenization, load_tokenization_manifest, TokenStream};
use model::{NodeStatus, ParseManifest, ParseTree, PARSE_SCHEMA_VERSION};
use parser::parse_stream;
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use validation::validate_tree;
use validation::{validate_parse_manifest, validate_parse_shape, validate_parse_shape_fields};

pub(super) mod model;
mod parser;
mod validation;

pub(in crate::inspector) fn parse_stream_direct(
    stream: &super::tokenization::TokenStream,
) -> Result<model::ParseTree, String> {
    let tree = parser::parse_stream(stream)?;
    validation::validate_tree(&tree)?;
    Ok(tree)
}

#[cfg(test)]
mod tests;
#[derive(Debug)]
pub(super) struct CreatedParse {
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) game_candidates: usize,
    pub(super) parse_trees: usize,
    pub(super) nodes: usize,
    pub(super) valid_nodes: usize,
    pub(super) recovered_nodes: usize,
    pub(super) incomplete_nodes: usize,
    pub(super) unparsed_nodes: usize,
    pub(super) diagnostics: usize,
    pub(super) path: PathBuf,
}

#[derive(Debug)]
pub(super) struct LoadedParseManifest {
    #[allow(dead_code, reason = "materialized trees only serve test extract_games")]
    pub(super) manifest: ParseManifest,
    pub(super) tokenization: super::tokenization::LoadedTokenizationManifest,
}

pub(super) fn parse(root: &Path, tokenization_path: &Path) -> Result<CreatedParse, String> {
    let loaded = load_streaming_tokenization(root, tokenization_path, None)?;
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let relative_tokenization = loaded
        .canonical_path
        .strip_prefix(&canonical_root)
        .map_err(|_| "Tokenizzazione fuori dal workspace".to_owned())?;
    let tokenization_manifest = path_to_manifest_string(relative_tokenization);
    validate_relative_path(&tokenization_manifest, "tokenization_manifest")?;
    let parsed_at_unix_ms = unix_time_ms()?;
    let source_bytes = loaded.manifest.source_bytes;
    let counts = RefCell::new(ParseCounts::default());

    let directory = root.join("inspector/parses");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Impossibile creare {}: {error}", directory.display()))?;
    let canonical_directory = fs::canonicalize(&directory)
        .map_err(|error| format!("Directory parse non valida: {error}"))?;
    ensure_within(&canonical_root, &canonical_directory, "directory parse")?;
    let name = {
        let trees = StreamingTrees {
            tokenization: &loaded,
            counts: &counts,
        };
        let manifest = StreamingParseManifest {
            schema_version: PARSE_SCHEMA_VERSION,
            tokenization_manifest: &tokenization_manifest,
            tokenization_manifest_blake3: &loaded.manifest_blake3,
            tokenization_manifest_bytes: loaded.manifest_bytes,
            source_blake3: &loaded.manifest.source_blake3,
            source_bytes,
            parsed_at_unix_ms,
            trees,
        };
        publish_parse_manifest(&canonical_root, &directory, &manifest)?
    };
    let counts = counts.into_inner();
    Ok(CreatedParse {
        source_blake3: loaded.manifest.source_blake3.clone(),
        source_bytes,
        game_candidates: counts.trees,
        parse_trees: counts.trees,
        nodes: counts.nodes,
        valid_nodes: counts.valid,
        recovered_nodes: counts.recovered,
        incomplete_nodes: counts.incomplete,
        unparsed_nodes: counts.unparsed,
        diagnostics: counts.diagnostics,
        path: PathBuf::from("inspector").join("parses").join(name),
    })
}

pub(super) fn verify_parse(root: &Path, parse_path: &Path) -> Result<(), String> {
    let loaded = load_streaming_parse(root, parse_path, None)?;
    loaded.for_each_tree(|_, _| Ok(()))?;
    Ok(())
}

#[derive(Deserialize)]
pub(super) struct StreamingParseManifestHeader {
    pub(super) schema_version: u32,
    pub(super) tokenization_manifest: String,
    pub(super) tokenization_manifest_blake3: String,
    pub(super) tokenization_manifest_bytes: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) parsed_at_unix_ms: u128,
    #[serde(rename = "trees")]
    _trees: IgnoredAny,
}

pub(super) struct LoadedStreamingParse {
    pub(super) manifest: StreamingParseManifestHeader,
    pub(super) canonical_path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) tokenization: super::tokenization::LoadedStreamingTokenization,
}

pub(super) fn load_streaming_parse(
    root: &Path,
    parse_path: &Path,
    expected_identity: Option<(&str, u64)>,
) -> Result<LoadedStreamingParse, String> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let resolved = resolve_workspace_path(root, parse_path)?;
    let canonical_path = fs::canonicalize(&resolved)
        .map_err(|error| format!("Parse non valida {}: {error}", resolved.display()))?;
    ensure_within(&canonical_root, &canonical_path, "parse")?;
    let (manifest, manifest_blake3, manifest_bytes): (StreamingParseManifestHeader, _, _) =
        deserialize_logical_json_with_identity(&canonical_path, "Parse")
            .map_err(|error| format!("Manifest parse non valido: {error}"))?;
    if let Some((expected_blake3, expected_bytes)) = expected_identity {
        if manifest_blake3 != expected_blake3 || manifest_bytes != expected_bytes {
            return Err("La parse è cambiata rispetto all'identità attesa".to_owned());
        }
    }
    validate_parse_shape_fields(
        manifest.schema_version,
        &manifest.tokenization_manifest,
        &manifest.tokenization_manifest_blake3,
        manifest.tokenization_manifest_bytes,
        &manifest.source_blake3,
        manifest.source_bytes,
    )?;
    let tokenization_path =
        validate_relative_path(&manifest.tokenization_manifest, "tokenization_manifest")?;
    let tokenization = load_streaming_tokenization(
        root,
        &tokenization_path,
        Some((
            &manifest.tokenization_manifest_blake3,
            manifest.tokenization_manifest_bytes,
        )),
    )?;
    if manifest.source_blake3 != tokenization.manifest.source_blake3
        || manifest.source_bytes != tokenization.manifest.source_bytes
    {
        return Err("Parse incoerente con la tokenizzazione".to_owned());
    }
    Ok(LoadedStreamingParse {
        manifest,
        canonical_path,
        manifest_blake3,
        manifest_bytes,
        tokenization,
    })
}

impl LoadedStreamingParse {
    pub(super) fn for_each_tree<F>(&self, mut consume: F) -> Result<usize, String>
    where
        F: FnMut(ParseTree, TokenStream) -> Result<(), String>,
    {
        self.tokenization.verify_source()?;
        let reader = logical_json_reader(&self.canonical_path, "Parse")?;
        let mut reader = IdentityReader::new(reader);
        let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
        let seed = TreesSeed {
            loaded: self,
            consume: &mut consume,
        };
        let count = seed
            .deserialize(&mut deserializer)
            .map_err(|error| format!("Manifest parse non valido: {error}"))?;
        deserializer
            .end()
            .map_err(|error| format!("Manifest parse non valido: {error}"))?;
        let (hash, bytes) = reader.finish();
        if hash != self.manifest_blake3 || bytes != self.manifest_bytes {
            return Err("La parse è cambiata durante la lettura".to_owned());
        }
        self.tokenization.verify_source()?;
        Ok(count)
    }
}

struct TreesSeed<'a, F> {
    loaded: &'a LoadedStreamingParse,
    consume: &'a mut F,
}

impl<'de, F> DeserializeSeed<'de> for TreesSeed<'_, F>
where
    F: FnMut(ParseTree, TokenStream) -> Result<(), String>,
{
    type Value = usize;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(TreesVisitor {
            loaded: self.loaded,
            consume: self.consume,
        })
    }
}

struct TreesVisitor<'a, F> {
    loaded: &'a LoadedStreamingParse,
    consume: &'a mut F,
}

impl<'de, F> Visitor<'de> for TreesVisitor<'_, F>
where
    F: FnMut(ParseTree, TokenStream) -> Result<(), String>,
{
    type Value = usize;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("manifest di parse")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = [false; 8];
        let mut tree_count = None;
        while let Some(key) = map.next_key::<String>()? {
            let index = match key.as_str() {
                "schema_version" => 0,
                "tokenization_manifest" => 1,
                "tokenization_manifest_blake3" => 2,
                "tokenization_manifest_bytes" => 3,
                "source_blake3" => 4,
                "source_bytes" => 5,
                "parsed_at_unix_ms" => 6,
                "trees" => 7,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                    continue;
                }
            };
            if std::mem::replace(&mut seen[index], true) {
                return Err(serde::de::Error::custom(format!(
                    "campo duplicato nella parse: {key}"
                )));
            }
            match index {
                0 => compare_field(
                    map.next_value::<u32>()?,
                    self.loaded.manifest.schema_version,
                )?,
                1 => compare_field(
                    map.next_value::<String>()?,
                    self.loaded.manifest.tokenization_manifest.clone(),
                )?,
                2 => compare_field(
                    map.next_value::<String>()?,
                    self.loaded.manifest.tokenization_manifest_blake3.clone(),
                )?,
                3 => compare_field(
                    map.next_value::<u64>()?,
                    self.loaded.manifest.tokenization_manifest_bytes,
                )?,
                4 => compare_field(
                    map.next_value::<String>()?,
                    self.loaded.manifest.source_blake3.clone(),
                )?,
                5 => compare_field(map.next_value::<u64>()?, self.loaded.manifest.source_bytes)?,
                6 => compare_field(
                    map.next_value::<u128>()?,
                    self.loaded.manifest.parsed_at_unix_ms,
                )?,
                7 => {
                    tree_count = Some(map.next_value_seed(TreesSeqSeed {
                        loaded: self.loaded,
                        consume: &mut *self.consume,
                    })?);
                }
                _ => unreachable!(),
            }
        }
        if seen.iter().any(|value| !value) {
            return Err(serde::de::Error::custom(
                "campo obbligatorio della parse assente",
            ));
        }
        Ok(tree_count.expect("trees marcato come presente"))
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
            "metadati della parse cambiati durante la lettura",
        ))
    }
}

struct TreesSeqSeed<'a, F> {
    loaded: &'a LoadedStreamingParse,
    consume: &'a mut F,
}

impl<'de, F> DeserializeSeed<'de> for TreesSeqSeed<'_, F>
where
    F: FnMut(ParseTree, TokenStream) -> Result<(), String>,
{
    type Value = usize;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(TreesSeqVisitor {
            loaded: self.loaded,
            consume: self.consume,
        })
    }
}

struct TreesSeqVisitor<'a, F> {
    loaded: &'a LoadedStreamingParse,
    consume: &'a mut F,
}

impl<'de, F> Visitor<'de> for TreesSeqVisitor<'_, F>
where
    F: FnMut(ParseTree, TokenStream) -> Result<(), String>,
{
    type Value = usize;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sequenza di alberi parse")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut count = 0_usize;
        self.loaded
            .tokenization
            .segmentation
            .for_each_segment(|segment| {
                if segment.kind != "game_candidate" {
                    return Ok(());
                }
                let tree = sequence
                    .next_element::<ParseTree>()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "Albero parse assente".to_owned())?;
                validate_tree(&tree)?;
                if tree.segment_ordinal != segment.ordinal {
                    return Err("Albero assente, duplicato o fuori ordine".to_owned());
                }
                let stream = super::tokenization::tokenize_streaming_file_segment(
                    &self.loaded.tokenization.segmentation,
                    &segment,
                )?;
                if tree.token_count != stream.tokens.len() as u64 || tree != parse_stream(&stream)?
                {
                    return Err("Parse diversa dal risultato del parser".to_owned());
                }
                (self.consume)(tree, stream)?;
                count += 1;
                Ok(())
            })
            .map_err(serde::de::Error::custom)?;
        if sequence.next_element::<IgnoredAny>()?.is_some() {
            return Err(serde::de::Error::custom("Albero inatteso o duplicato"));
        }
        Ok(count)
    }
}

pub(super) fn load_parse_manifest(
    root: &Path,
    parse_path: &Path,
    expected_identity: Option<(&str, u64)>,
) -> Result<LoadedParseManifest, String> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Workspace non valido {}: {error}", root.display()))?;
    let resolved = resolve_workspace_path(root, parse_path)?;
    let canonical_parse = fs::canonicalize(&resolved)
        .map_err(|error| format!("Parse non valida {}: {error}", resolved.display()))?;
    ensure_within(&canonical_root, &canonical_parse, "parse")?;
    let (manifest, manifest_blake3, manifest_bytes) = read_parse_manifest(&canonical_parse)?;
    if let Some((expected_blake3, expected_bytes)) = expected_identity {
        if manifest_blake3 != expected_blake3 || manifest_bytes != expected_bytes {
            return Err("La parse è cambiata rispetto all'identità attesa".to_owned());
        }
    }
    validate_parse_shape(&manifest)?;
    let tokenization_path =
        validate_relative_path(&manifest.tokenization_manifest, "tokenization_manifest")?;
    let loaded = load_tokenization_manifest(
        root,
        &tokenization_path,
        Some((
            &manifest.tokenization_manifest_blake3,
            manifest.tokenization_manifest_bytes,
        )),
    )?;
    validate_parse_manifest(&manifest, &loaded)?;
    for (tree, stream) in manifest.trees.iter().zip(&loaded.manifest.streams) {
        if tree != &parse_stream(stream)? {
            return Err("Parse diversa dal risultato del parser".to_owned());
        }
    }
    Ok(LoadedParseManifest {
        manifest,
        tokenization: loaded,
    })
}

fn read_parse_manifest(path: &Path) -> Result<(ParseManifest, String, u64), String> {
    let reader = logical_json_reader(path, "Parse")?;
    let mut reader = IdentityReader::new(reader);
    let manifest = {
        let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
        let manifest = ParseManifest::deserialize(&mut deserializer)
            .map_err(|error| format!("Manifest parse non valido: {error}"))?;
        deserializer
            .end()
            .map_err(|error| format!("Manifest parse non valido: {error}"))?;
        manifest
    };
    let (hash, bytes) = reader.finish();
    Ok((manifest, hash, bytes))
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
            .ok_or_else(|| std::io::Error::other("Dimensione parse oltre u64"))?;
        Ok(read)
    }
}

fn publish_parse_manifest<T: Serialize>(
    canonical_root: &Path,
    directory: &Path,
    manifest: &T,
) -> Result<PathBuf, String> {
    let (path, _, _, _) =
        publish_compressed_json_stream(canonical_root, directory, "Parse", |writer| {
            serde_json::to_writer_pretty(&mut *writer, manifest)
                .map_err(|error| format!("Errore serializzando la parse: {error}"))?;
            writer
                .write_all(b"\n")
                .map_err(|error| format!("Errore serializzando la parse: {error}"))
        })?;
    Ok(path.file_name().unwrap().into())
}

#[derive(Serialize)]
struct StreamingParseManifest<'a, T> {
    schema_version: u32,
    tokenization_manifest: &'a str,
    tokenization_manifest_blake3: &'a str,
    tokenization_manifest_bytes: u64,
    source_blake3: &'a str,
    source_bytes: u64,
    parsed_at_unix_ms: u128,
    trees: T,
}

struct StreamingTrees<'a> {
    tokenization: &'a super::tokenization::LoadedStreamingTokenization,
    counts: &'a RefCell<ParseCounts>,
}

impl Serialize for StreamingTrees<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(None)?;
        let result = self.tokenization.for_each_stream(|stream| {
            let tree = parse_stream(&stream)?;
            validation::validate_tree(&tree)?;
            sequence
                .serialize_element(&tree)
                .map_err(|error| error.to_string())?;
            self.counts.borrow_mut().add_tree(&tree);
            Ok(())
        });
        result.map_err(serde::ser::Error::custom)?;
        sequence.end()
    }
}

#[derive(Default)]
struct ParseCounts {
    trees: usize,
    nodes: usize,
    valid: usize,
    recovered: usize,
    incomplete: usize,
    unparsed: usize,
    diagnostics: usize,
}

impl ParseCounts {
    fn add_tree(&mut self, tree: &model::ParseTree) {
        self.trees += 1;
        self.diagnostics += tree.diagnostics.len();
        for node in &tree.nodes {
            self.nodes += 1;
            self.valid += usize::from(node.status == NodeStatus::Valid);
            self.recovered += usize::from(node.status == NodeStatus::Recovered);
            self.incomplete += usize::from(node.status == NodeStatus::Incomplete);
            self.unparsed += usize::from(node.status == NodeStatus::Unparsed);
        }
    }
}
