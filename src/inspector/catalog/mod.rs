mod extraction;
pub(in crate::inspector) mod manifest;
pub(in crate::inspector) mod query;
#[cfg(test)]
mod tests;
mod validation;

use crate::inspector::common::{
    canonical_artifact_path, deserialize_logical_json_with_identity, ensure_within,
    logical_json_bytes, logical_json_reader, path_to_manifest_string, prepare_directory_chain,
    publish_compressed_json_stream, resolve_workspace_path, validate_relative_path,
    ProgressiveCompareWriter,
};
use crate::inspector::interpretation::load_streaming_interpretation;
use crate::inspector::interpretation::manifest::{GameRecordV1, LoadedStreamingInterpretation};
use crate::inspector::parsing::model::ParseTree;
use crate::inspector::tokenization::TokenStream;
use manifest::*;
use serde::de::IgnoredAny;
use serde::ser::{SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) struct CreatedCatalog {
    pub(super) path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) new_manifest: bool,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    pub(super) interpretation_manifest_blake3: String,
    pub(super) interpretation_manifest_bytes: u64,
    pub(super) counts: CatalogCounts,
}
pub(in crate::inspector) fn probe_catalog_game(
    index: u64,
    game: &GameRecordV1,
    tree: &ParseTree,
    stream: &TokenStream,
    blob: &[u8],
    source_bytes: u64,
) -> Result<CatalogGameRecordV1, String> {
    let record = extraction::as_catalog_record(index, game, tree, stream, blob)?;
    validation::validate_catalog_game(
        usize::try_from(index).map_err(|_| "Indice probe oltre usize".to_owned())?,
        &record,
        source_bytes,
    )?;
    Ok(record)
}

impl CreatedCatalog {
    pub(super) fn cli_fields(&self) -> Vec<(&'static str, String)> {
        let c = &self.counts;
        vec![
            ("SOURCE_BLAKE3", self.source_blake3.clone()),
            ("SOURCE_BYTES", self.source_bytes.to_string()),
            (
                "INTERPRETATION_BLAKE3",
                self.interpretation_manifest_blake3.clone(),
            ),
            (
                "INTERPRETATION_BYTES",
                self.interpretation_manifest_bytes.to_string(),
            ),
            ("GAMES", c.games.to_string()),
            ("TAGS", c.tags.to_string()),
            ("DECODED_TAGS", c.decoded_tags.to_string()),
            ("UNDECODABLE_TAGS", c.undecodable_tags.to_string()),
            ("CATALOG_BLAKE3", self.manifest_blake3.clone()),
            ("CATALOG_BYTES", self.manifest_bytes.to_string()),
            ("NEW_MANIFEST", self.new_manifest.to_string()),
            ("CATALOG", path_to_manifest_string(&self.path)),
        ]
    }
}

fn normalized(root: &Path, path: &Path, label: &str) -> Result<String, String> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| format!("{label} fuori dal workspace"))?;
    let s = rel
        .to_str()
        .ok_or_else(|| format!("Percorso {label} non UTF-8"))?;
    validate_relative_path(s, label)?;
    Ok(path_to_manifest_string(rel))
}

pub(super) fn create_catalog(root: &Path, path: &Path) -> Result<CreatedCatalog, String> {
    let loaded = load_streaming_interpretation(root, path, None)?;
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let interpretation_manifest = normalized(
        &canonical_root,
        &loaded.canonical_path,
        "interpretation_manifest",
    )?;
    let directory = prepare_directory_chain(
        root,
        &canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("catalogs", "directory catalogs"),
        ],
    )?;
    let counts = RefCell::new(CatalogCounts::default());
    let (target, new_manifest, hash, size) =
        publish_compressed_json_stream(&canonical_root, &directory, "Catalog", |writer| {
            let tally = serialize_catalog_into(
                writer,
                &loaded,
                &interpretation_manifest,
                &counts,
                |_, _, _, _| Ok(()),
            )?;
            *counts.borrow_mut() = tally;
            Ok(())
        })?;
    let counts = counts.into_inner();
    Ok(CreatedCatalog {
        path: PathBuf::from("inspector/catalogs")
            .join(target.file_name().ok_or("Artefatto Catalog senza nome")?),
        manifest_blake3: hash,
        manifest_bytes: size,
        new_manifest,
        source_blake3: loaded.parse.tokenization.manifest.source_blake3.clone(),
        source_bytes: loaded.parse.tokenization.manifest.source_bytes,
        interpretation_manifest_blake3: loaded.manifest_blake3.clone(),
        interpretation_manifest_bytes: loaded.manifest_bytes,
        counts,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamingCatalogManifestHeader {
    pub(super) schema_version: u32,
    pub(super) catalog_profile: String,
    pub(super) interpretation_manifest: String,
    pub(super) interpretation_manifest_blake3: String,
    pub(super) interpretation_manifest_bytes: u64,
    pub(super) parse_manifest_blake3: String,
    pub(super) parse_manifest_bytes: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    #[serde(rename = "games")]
    _games: IgnoredAny,
}

pub(super) struct LoadedStreamingCatalog {
    pub(super) header: StreamingCatalogManifestHeader,
    pub(super) canonical_path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) interpretation: LoadedStreamingInterpretation,
}

pub(super) fn load_streaming_catalog(
    root: &Path,
    path: &Path,
) -> Result<LoadedStreamingCatalog, String> {
    let path_value = path.to_str().ok_or("Percorso Catalog non UTF-8")?;
    validate_relative_path(path_value, "catalog")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let canonical_path =
        fs::canonicalize(&resolved).map_err(|e| format!("Catalog non valido: {e}"))?;
    ensure_within(&canonical_root, &canonical_path, "catalog")?;
    let metadata =
        fs::symlink_metadata(&resolved).map_err(|e| format!("Catalog non valido: {e}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Catalog non è un file regolare reale".into());
    }
    let (header, manifest_blake3, manifest_bytes): (StreamingCatalogManifestHeader, _, _) =
        deserialize_logical_json_with_identity(&canonical_path, "Catalog")
            .map_err(|error| format!("Manifest Catalog non valido: {error}"))?;
    let actual_path = normalized(&canonical_root, &canonical_path, "catalog")?;
    let canonical_expected =
        canonical_artifact_path("catalogs", &manifest_blake3, &canonical_path)?;
    if actual_path != canonical_expected {
        return Err("Percorso Catalog non canonico".into());
    }
    validation::validate_header(&refs_from_header(&header))?;
    let interpretation_path =
        validate_relative_path(&header.interpretation_manifest, "interpretation_manifest")?;
    let interpretation = load_streaming_interpretation(
        root,
        &interpretation_path,
        Some((
            &header.interpretation_manifest_blake3,
            header.interpretation_manifest_bytes,
        )),
    )?;
    if interpretation.manifest.parse_manifest_blake3 != header.parse_manifest_blake3
        || interpretation.manifest.parse_manifest_bytes != header.parse_manifest_bytes
        || interpretation.manifest.source_blake3 != header.source_blake3
        || interpretation.manifest.source_bytes != header.source_bytes
    {
        return Err("Catalog incoerente con l'interpretazione".into());
    }
    // Streaming load validates identity and provenance only. Semantic byte-exact
    // verification is fused with record consumption downstream.
    Ok(LoadedStreamingCatalog {
        header,
        canonical_path,
        manifest_blake3,
        manifest_bytes,
        interpretation,
    })
}

#[derive(Serialize)]
struct StreamingCatalogManifest<'a, T> {
    schema_version: u32,
    catalog_profile: &'a str,
    interpretation_manifest: &'a str,
    interpretation_manifest_blake3: &'a str,
    interpretation_manifest_bytes: u64,
    parse_manifest_blake3: &'a str,
    parse_manifest_bytes: u64,
    source_blake3: &'a str,
    source_bytes: u64,
    games: T,
}

struct StreamingCatalogRefs<'a> {
    pub(in crate::inspector) schema_version: u32,
    pub(in crate::inspector) catalog_profile: &'a str,
    pub(in crate::inspector) interpretation_manifest: &'a str,
    pub(in crate::inspector) interpretation_manifest_blake3: &'a str,
    pub(in crate::inspector) interpretation_manifest_bytes: u64,
    pub(in crate::inspector) parse_manifest_blake3: &'a str,
    pub(in crate::inspector) parse_manifest_bytes: u64,
    pub(in crate::inspector) source_blake3: &'a str,
    pub(in crate::inspector) source_bytes: u64,
}

fn refs_from_header<'a>(header: &'a StreamingCatalogManifestHeader) -> StreamingCatalogRefs<'a> {
    StreamingCatalogRefs {
        schema_version: header.schema_version,
        catalog_profile: &header.catalog_profile,
        interpretation_manifest: &header.interpretation_manifest,
        interpretation_manifest_blake3: &header.interpretation_manifest_blake3,
        interpretation_manifest_bytes: header.interpretation_manifest_bytes,
        parse_manifest_blake3: &header.parse_manifest_blake3,
        parse_manifest_bytes: header.parse_manifest_bytes,
        source_blake3: &header.source_blake3,
        source_bytes: header.source_bytes,
    }
}

fn refs_from_created<'a>(
    loaded: &'a LoadedStreamingInterpretation,
    interpretation_manifest: &'a str,
) -> StreamingCatalogRefs<'a> {
    StreamingCatalogRefs {
        schema_version: CATALOG_SCHEMA_VERSION,
        catalog_profile: CATALOG_PROFILE,
        interpretation_manifest,
        interpretation_manifest_blake3: &loaded.manifest_blake3,
        interpretation_manifest_bytes: loaded.manifest_bytes,
        parse_manifest_blake3: &loaded.parse.manifest_blake3,
        parse_manifest_bytes: loaded.parse.manifest_bytes,
        source_blake3: &loaded.parse.tokenization.manifest.source_blake3,
        source_bytes: loaded.parse.tokenization.manifest.source_bytes,
    }
}

fn serialize_catalog_with<W, F>(
    writer: &mut W,
    refs: &StreamingCatalogRefs<'_>,
    loaded: &LoadedStreamingInterpretation,
    counts: &RefCell<CatalogCounts>,
    on_game: &RefCell<F>,
) -> Result<(), String>
where
    W: Write + ?Sized,
    F: FnMut(CatalogGameRecordV1, &GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    let manifest = StreamingCatalogManifest {
        schema_version: refs.schema_version,
        catalog_profile: refs.catalog_profile,
        interpretation_manifest: refs.interpretation_manifest,
        interpretation_manifest_blake3: refs.interpretation_manifest_blake3,
        interpretation_manifest_bytes: refs.interpretation_manifest_bytes,
        parse_manifest_blake3: refs.parse_manifest_blake3,
        parse_manifest_bytes: refs.parse_manifest_bytes,
        source_blake3: refs.source_blake3,
        source_bytes: refs.source_bytes,
        games: ChangedCatalogs {
            loaded,
            counts,
            on_game,
        },
    };
    serde_json::to_writer_pretty(&mut *writer, &manifest)
        .map_err(|error| format!("Errore serializzando il Catalog: {error}"))?;
    writer
        .write_all(b"\n")
        .map_err(|error| format!("Errore serializzando il Catalog: {error}"))?;
    Ok(())
}

struct ChangedCatalogs<'a, F> {
    loaded: &'a LoadedStreamingInterpretation,
    counts: &'a RefCell<CatalogCounts>,
    on_game: &'a RefCell<F>,
}

fn require_increasing_segment_ordinal(
    previous: &mut Option<u64>,
    current: u64,
) -> Result<(), String> {
    if previous.is_some_and(|ordinal| current <= ordinal) {
        return Err("segment_ordinal non strettamente crescente".into());
    }
    *previous = Some(current);
    Ok(())
}

fn for_each_computed_catalog_game<F>(
    loaded: &LoadedStreamingInterpretation,
    trust_fresh: bool,
    mut on_game: F,
) -> Result<CatalogCounts, String>
where
    F: FnMut(CatalogGameRecordV1, &GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    let source_bytes = loaded.parse.tokenization.manifest.source_bytes;
    let mut previous_end = None;
    let mut previous_segment_ordinal = None;
    let mut counts = CatalogCounts::default();
    let mut consume = |record: GameRecordV1, tree: &ParseTree, stream: &TokenStream| {
        let raw = loaded
            .parse
            .tokenization
            .read_source_range(stream.start, stream.end)?;
        require_increasing_segment_ordinal(&mut previous_segment_ordinal, stream.segment_ordinal)?;
        if previous_end.is_some_and(|end| end > stream.start) {
            return Err("Intervalli partita non ordinati o sovrapposti".into());
        }
        previous_end = Some(stream.end);
        let record_id = record.id;
        let catalog_record = extraction::as_catalog_record(record_id, &record, tree, stream, &raw)?;
        validation::validate_catalog_game(
            usize::try_from(record_id).map_err(|_| "Indice gioco oltre usize".to_owned())?,
            &catalog_record,
            source_bytes,
        )?;
        counts.add_game(&catalog_record);
        on_game(catalog_record, &record, tree, stream)
    };
    if trust_fresh {
        loaded.for_each_fresh_game(&mut consume)?;
    } else {
        loaded.for_each_verified_game(&mut consume)?;
    }
    Ok(counts)
}

impl<F> Serialize for ChangedCatalogs<'_, F>
where
    F: FnMut(CatalogGameRecordV1, &GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(None)?;
        let trust_fresh = crate::inspector::common::fresh_chain_enabled();
        let counts = for_each_computed_catalog_game(
            self.loaded,
            trust_fresh,
            |catalog_record, record, tree, stream| {
                sequence
                    .serialize_element(&catalog_record)
                    .map_err(|error| error.to_string())?;
                (self.on_game.borrow_mut())(catalog_record, record, tree, stream)
                    .map_err(|error| error.to_string())
            },
        )
        .map_err(serde::ser::Error::custom)?;
        *self.counts.borrow_mut() = counts;
        sequence.end()
    }
}

fn serialize_catalog_into<W, F>(
    writer: &mut W,
    loaded: &LoadedStreamingInterpretation,
    interpretation_manifest: &str,
    counts: &RefCell<CatalogCounts>,
    on_game: F,
) -> Result<CatalogCounts, String>
where
    W: Write + ?Sized,
    F: FnMut(CatalogGameRecordV1, &GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    let refs = refs_from_created(loaded, interpretation_manifest);
    let on_game = RefCell::new(on_game);
    serialize_catalog_with(writer, &refs, loaded, counts, &on_game)?;
    Ok(counts.borrow().clone())
}

#[cfg(test)]
fn serialized(m: &CatalogManifestV1) -> Result<Vec<u8>, String> {
    let mut b =
        serde_json::to_vec_pretty(m).map_err(|e| format!("Serializzazione Catalog: {e}"))?;
    b.push(b'\n');
    Ok(b)
}

#[cfg(test)]
struct LoadedCatalog {
    manifest: CatalogManifestV1,
}
pub(super) struct VerifiedCatalog {
    pub(super) manifest: CatalogManifestV1,
    pub(super) interpretation: crate::inspector::interpretation::manifest::VerifiedInterpretation,
    pub(super) canonical_path: PathBuf,
    #[allow(
        dead_code,
        reason = "byte identity is part of the internal verified API"
    )]
    pub(super) bytes: Vec<u8>,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
}
#[cfg(test)]
fn load_catalog(root: &Path, path: &Path) -> Result<LoadedCatalog, String> {
    let path_value = path.to_str().ok_or("Percorso Catalog non UTF-8")?;
    validate_relative_path(path_value, "catalog")?;
    let cr = fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let canonical = fs::canonicalize(&resolved).map_err(|e| format!("Catalog non valido: {e}"))?;
    ensure_within(&cr, &canonical, "catalog")?;
    let meta = fs::symlink_metadata(&resolved).map_err(|e| format!("Catalog non valido: {e}"))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err("Catalog non è un file regolare reale".into());
    }
    let bytes = logical_json_bytes(&canonical, "Catalog")?;
    let manifest =
        serde_json::from_slice(&bytes).map_err(|e| format!("Manifest Catalog non valido: {e}"))?;
    validation::validate(&manifest)?;
    Ok(LoadedCatalog { manifest })
}
pub(super) fn load_verified_catalog(root: &Path, path: &Path) -> Result<VerifiedCatalog, String> {
    let loaded = load_streaming_catalog(root, path)?;
    let interpretation_path = validate_relative_path(
        &loaded.header.interpretation_manifest,
        "interpretation_manifest",
    )?;
    let interpretation =
        crate::inspector::interpretation::load_verified_interpretation(root, &interpretation_path)?;
    if interpretation.manifest_blake3 != loaded.header.interpretation_manifest_blake3
        || interpretation.manifest_bytes != loaded.header.interpretation_manifest_bytes
    {
        return Err("Interpretation diversa dall'identità Catalog".into());
    }
    let mut games = Vec::new();
    loaded.for_each_verified_catalog_game(|game| {
        games.push(game);
        Ok(())
    })?;
    let manifest = CatalogManifestV1 {
        schema_version: loaded.header.schema_version,
        catalog_profile: loaded.header.catalog_profile,
        interpretation_manifest: loaded.header.interpretation_manifest,
        interpretation_manifest_blake3: loaded.header.interpretation_manifest_blake3,
        interpretation_manifest_bytes: loaded.header.interpretation_manifest_bytes,
        parse_manifest_blake3: loaded.header.parse_manifest_blake3,
        parse_manifest_bytes: loaded.header.parse_manifest_bytes,
        source_blake3: loaded.header.source_blake3,
        source_bytes: loaded.header.source_bytes,
        games,
    };
    validation::validate(&manifest)?;
    let bytes = logical_json_bytes(&loaded.canonical_path, "Catalog")?;
    Ok(VerifiedCatalog {
        manifest,
        interpretation,
        canonical_path: loaded.canonical_path,
        bytes,
        manifest_blake3: loaded.manifest_blake3,
        manifest_bytes: loaded.manifest_bytes,
    })
}

impl LoadedStreamingCatalog {
    pub(super) fn for_each_game<F>(&self, on_game: F) -> Result<usize, String>
    where
        F: FnMut(
            CatalogGameRecordV1,
            &GameRecordV1,
            &ParseTree,
            &TokenStream,
        ) -> Result<(), String>,
    {
        self.interpretation.parse.tokenization.verify_source()?;
        let header = &self.header;
        let refs = refs_from_header(header);
        let mut stored = logical_json_reader(&self.canonical_path, "Catalog")?;
        let mut compare = ProgressiveCompareWriter::new(&mut stored);
        let counts = RefCell::new(CatalogCounts::default());
        let on_game = RefCell::new(on_game);
        serialize_catalog_with(&mut compare, &refs, &self.interpretation, &counts, &on_game)?;
        compare.finish("Catalog", &self.manifest_blake3, self.manifest_bytes)?;
        self.interpretation.parse.tokenization.verify_source()?;
        Ok(counts.into_inner().games)
    }

    pub(super) fn for_each_fresh_game<F>(&self, on_game: F) -> Result<usize, String>
    where
        F: FnMut(
            CatalogGameRecordV1,
            &GameRecordV1,
            &ParseTree,
            &TokenStream,
        ) -> Result<(), String>,
    {
        Ok(for_each_computed_catalog_game(&self.interpretation, true, on_game)?.games)
    }

    pub(super) fn for_each_verified_catalog_game<F>(&self, on_game: F) -> Result<usize, String>
    where
        F: FnMut(CatalogGameRecordV1) -> Result<(), String>,
    {
        let mut on_game = on_game;
        self.for_each_game(|game, _, _, _| on_game(game))
    }
}

pub(super) fn verify_catalog(root: &Path, path: &Path) -> Result<(), String> {
    let loaded = load_streaming_catalog(root, path)?;
    loaded.for_each_verified_catalog_game(|_| Ok(()))?;
    Ok(())
}
pub(super) fn query_catalog(q: &Query) -> Result<String, String> {
    query::run(q)
}
pub(super) use query::{Query, QueryFilters};

pub(super) struct ExportedGame {
    pub game_id: u64,
    pub blake3: String,
    pub bytes: u64,
    pub output: PathBuf,
}
pub(super) fn export_game(
    root: &Path,
    catalog_path: &Path,
    game_id: u64,
    output: &Path,
) -> Result<ExportedGame, String> {
    let loaded = load_verified_catalog(root, catalog_path)?;
    let game = loaded
        .manifest
        .games
        .get(usize::try_from(game_id).map_err(|_| "game-id oltre usize")?)
        .filter(|g| g.id == game_id)
        .ok_or("game-id inesistente")?;
    let source = &loaded.interpretation.parse.tokenization;
    source.verify_source()?;
    let bytes = source.read_source_range(game.source_start, game.source_end)?;
    let size = u64::try_from(bytes.len()).map_err(|_| "Partita esportata oltre u64")?;
    let expected_size = game
        .source_end
        .checked_sub(game.source_start)
        .ok_or("Intervallo partita invertito")?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    if size != expected_size || hash != game.source_game_blake3 {
        return Err("source_game_blake3 non corrisponde ai byte".into());
    }
    source.verify_source()?;
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let resolved_output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        let safe = validate_relative_path(output.to_str().ok_or("Output non UTF-8")?, "output")?;
        root.join(safe)
    };
    let parent = resolved_output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or("Output senza parent")?;
    let canonical_parent =
        fs::canonicalize(parent).map_err(|e| format!("Parent output non valido: {e}"))?;
    ensure_within(&canonical_root, &canonical_parent, "parent output")?;
    let name = resolved_output
        .file_name()
        .ok_or("Output senza nome file")?;
    if !matches!(
        Path::new(name).components().next(),
        Some(std::path::Component::Normal(_))
    ) {
        return Err("Output non sicuro".into());
    }
    let target = canonical_parent.join(name);
    match fs::symlink_metadata(&target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => return Err("L'output esiste già".into()),
        Err(e) => return Err(format!("Impossibile verificare output: {e}")),
    }
    let (mut file, temp) = crate::inspector::common::create_unique_temporary(
        &canonical_parent,
        ".catalog-game-export",
    )?;
    use std::io::Write;
    let write = (|| {
        file.write_all(&bytes)
            .map_err(|e| format!("Errore export: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("Errore sync export: {e}"))
    })();
    drop(file);
    if let Err(e) = write {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    let link = fs::hard_link(&temp, &target);
    let cleanup = fs::remove_file(&temp);
    match link {
        Ok(()) => cleanup.map_err(|e| format!("Pulizia temporaneo export: {e}"))?,
        Err(e) => {
            let _ = cleanup;
            return Err(if e.kind() == std::io::ErrorKind::AlreadyExists {
                "L'output esiste già".into()
            } else {
                format!("Pubblicazione export: {e}")
            });
        }
    }
    Ok(ExportedGame {
        game_id,
        blake3: hash,
        bytes: u64::try_from(bytes.len()).map_err(|_| "Export oltre u64")?,
        output: output.to_path_buf(),
    })
}

pub(super) fn parse_query_args<I: IntoIterator<Item = String>>(args: I) -> Result<Query, String> {
    let mut root = None;
    let mut catalog = None;
    let mut filter_args = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = Some(PathBuf::from(it.next().ok_or("Manca valore per --root")?)),
            "--catalog" => {
                catalog = Some(PathBuf::from(
                    it.next().ok_or("Manca valore per --catalog")?,
                ))
            }
            "--player" | "--event" | "--date" | "--date-from" | "--date-to" => {
                let value = it.next().ok_or_else(|| format!("Manca valore per {a}"))?;
                filter_args.extend([a, value]);
            }
            _ => return Err(format!("Argomento inatteso: {a}")),
        }
    }
    let filters = parse_query_filters(filter_args)?;
    Ok(Query {
        root: root.ok_or("Manca --root WORKSPACE")?,
        catalog: catalog.ok_or("Manca --catalog CATALOG")?,
        filters,
    })
}

pub(in crate::inspector) fn parse_query_filters<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<QueryFilters, String> {
    let mut filters = QueryFilters::default();
    let mut it = args.into_iter();
    while let Some(argument) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| format!("Manca valore per {argument}"))?;
        match argument.as_str() {
            "--player" => filters.player = Some(value),
            "--event" => filters.event = Some(value),
            "--date" => filters.date = Some(query::parse_cli_date(&value)?),
            "--date-from" => filters.date_from = Some(query::parse_cli_date(&value)?),
            "--date-to" => filters.date_to = Some(query::parse_cli_date(&value)?),
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }
    query::validate_filters(&filters)?;
    Ok(filters)
}
