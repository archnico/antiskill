mod export;
mod extraction;
mod manifest;
#[cfg(test)]
mod tests;
mod validation;

use crate::inspector::catalog::manifest::CatalogGameRecordV1;
use crate::inspector::catalog::{
    load_streaming_catalog, load_verified_catalog, LoadedStreamingCatalog, VerifiedCatalog,
};
use crate::inspector::common::{
    canonical_artifact_path, deserialize_logical_json_with_identity, ensure_within,
    logical_json_reader, path_to_manifest_string, prepare_directory_chain,
    publish_compressed_json_stream, resolve_workspace_path, validate_relative_path,
    ProgressiveCompareWriter,
};
use crate::inspector::interpretation::manifest::GameRecordV1;
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

pub(in crate::inspector) fn probe_occurrence_identity(
    source_blake3: &str,
    source_bytes: u64,
    source_start: u64,
    source_end: u64,
    blob: &[u8],
) -> (String, String) {
    let source_game_blake3 = blake3::hash(blob).to_hex().to_string();
    let occurrence_id = extraction::occurrence_id(
        source_blake3,
        source_bytes,
        source_start,
        source_end,
        &source_game_blake3,
    );
    (source_game_blake3, occurrence_id)
}

pub(in crate::inspector) fn probe_occurrence(
    ordinal: u64,
    source_blake3: &str,
    source_bytes: u64,
    catalog_game: &CatalogGameRecordV1,
    game: &GameRecordV1,
    tree: &ParseTree,
    stream: &TokenStream,
) -> Result<GameOccurrenceV1, String> {
    let zero = "0000000000000000000000000000000000000000000000000000000000000000";
    let provenance = OccurrenceProvenanceV1 {
        source_manifest_blake3: zero.to_owned(),
        source_manifest_bytes: 1,
        segmentation_manifest_blake3: zero.to_owned(),
        segmentation_manifest_bytes: 1,
        tokenization_manifest_blake3: zero.to_owned(),
        tokenization_manifest_bytes: 1,
        parse_manifest_blake3: zero.to_owned(),
        parse_manifest_bytes: 1,
        interpretation_manifest_blake3: zero.to_owned(),
        interpretation_manifest_bytes: 1,
        catalog_manifest_blake3: zero.to_owned(),
        catalog_manifest_bytes: 1,
    };
    let occurrence = extraction::occurrence(
        ordinal,
        (source_blake3, source_bytes),
        catalog_game,
        game,
        tree,
        stream,
        &provenance,
    )?;
    validation::validate_occurrence(&occurrence, (source_blake3, source_bytes), &provenance)?;
    Ok(occurrence)
}

#[derive(Clone, Debug)]
pub(in crate::inspector) struct ProbeOccurrenceCompactFields {
    pub(in crate::inspector) source_start: u64,
    pub(in crate::inspector) source_end: u64,
    pub(in crate::inspector) source_game_blake3: String,
}

pub(in crate::inspector) fn probe_occurrence_compact_fields(
    occurrence: &GameOccurrenceV1,
) -> ProbeOccurrenceCompactFields {
    ProbeOccurrenceCompactFields {
        source_start: occurrence.source_start,
        source_end: occurrence.source_end,
        source_game_blake3: occurrence.source_game_blake3.clone(),
    }
}

pub(super) struct CreatedOccurrences {
    pub(super) path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) new_manifest: bool,
    pub(super) occurrences: usize,
}

pub(in crate::inspector) struct VerifiedOccurrences {
    pub(in crate::inspector) manifest: OccurrenceManifestV1,
    pub(in crate::inspector) catalog: VerifiedCatalog,
    pub(in crate::inspector) canonical_path: PathBuf,
    pub(in crate::inspector) manifest_blake3: String,
    pub(in crate::inspector) manifest_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) struct ProductDuplicateOccurrenceProjectionV1 {
    pub(in crate::inspector) occurrence_id: String,
    pub(in crate::inspector) source_game_blake3: String,
    pub(in crate::inspector) game_bytes: u64,
    pub(in crate::inspector) date: Vec<String>,
    pub(in crate::inspector) white: Vec<String>,
    pub(in crate::inspector) black: Vec<String>,
    pub(in crate::inspector) result: Vec<String>,
    pub(in crate::inspector) event: Vec<String>,
}

pub(in crate::inspector) fn product_duplicate_projection(
    occurrence: &GameOccurrenceV1,
) -> Result<ProductDuplicateOccurrenceProjectionV1, String> {
    let game_bytes = occurrence
        .source_end
        .checked_sub(occurrence.source_start)
        .ok_or("Intervallo occurrence invertito")?;
    let values = |name: &str| {
        occurrence
            .tags
            .iter()
            .filter(|tag| {
                tag.decode_status == crate::inspector::catalog::manifest::TagDecodeStatusV1::Decoded
                    && tag
                        .name
                        .as_deref()
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
            })
            .filter_map(|tag| tag.value.clone())
            .collect::<Vec<_>>()
    };
    Ok(ProductDuplicateOccurrenceProjectionV1 {
        occurrence_id: occurrence.occurrence_id.clone(),
        source_game_blake3: occurrence.source_game_blake3.clone(),
        game_bytes,
        date: values("Date"),
        white: values("White"),
        black: values("Black"),
        result: values("Result"),
        event: values("Event"),
    })
}

fn normalized(root: &Path, path: &Path, label: &str) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("{label} fuori dal workspace"))?;
    let value = relative
        .to_str()
        .ok_or_else(|| format!("Percorso {label} non UTF-8"))?;
    validate_relative_path(value, label)?;
    Ok(path_to_manifest_string(relative))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::inspector) struct StreamingOccurrencesManifestHeader {
    pub(in crate::inspector) schema_version: u32,
    pub(in crate::inspector) occurrence_profile: String,
    pub(in crate::inspector) catalog_manifest: String,
    pub(in crate::inspector) catalog_manifest_blake3: String,
    pub(in crate::inspector) catalog_manifest_bytes: u64,
    pub(in crate::inspector) interpretation_manifest: String,
    pub(in crate::inspector) interpretation_manifest_blake3: String,
    pub(in crate::inspector) interpretation_manifest_bytes: u64,
    pub(in crate::inspector) parse_manifest: String,
    pub(in crate::inspector) parse_manifest_blake3: String,
    pub(in crate::inspector) parse_manifest_bytes: u64,
    pub(in crate::inspector) tokenization_manifest: String,
    pub(in crate::inspector) tokenization_manifest_blake3: String,
    pub(in crate::inspector) tokenization_manifest_bytes: u64,
    pub(in crate::inspector) segmentation_manifest: String,
    pub(in crate::inspector) segmentation_manifest_blake3: String,
    pub(in crate::inspector) segmentation_manifest_bytes: u64,
    pub(in crate::inspector) source_manifest: String,
    pub(in crate::inspector) source_manifest_blake3: String,
    pub(in crate::inspector) source_manifest_bytes: u64,
    pub(in crate::inspector) source_blake3: String,
    pub(in crate::inspector) source_bytes: u64,
    #[serde(rename = "occurrences")]
    _occurrences: IgnoredAny,
}

pub(in crate::inspector) struct LoadedStreamingOccurrences {
    pub(in crate::inspector) header: StreamingOccurrencesManifestHeader,
    pub(in crate::inspector) canonical_path: PathBuf,
    pub(in crate::inspector) manifest_blake3: String,
    pub(in crate::inspector) manifest_bytes: u64,
    pub(in crate::inspector) catalog: LoadedStreamingCatalog,
}

pub(in crate::inspector) fn load_streaming_occurrences(
    root: &Path,
    path: &Path,
    expected: Option<(&str, u64)>,
) -> Result<LoadedStreamingOccurrences, String> {
    let path_value = path.to_str().ok_or("Percorso GameOccurrence non UTF-8")?;
    validate_relative_path(path_value, "occurrences")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let canonical_path =
        fs::canonicalize(&resolved).map_err(|e| format!("GameOccurrence non valida: {e}"))?;
    ensure_within(&canonical_root, &canonical_path, "occurrences")?;
    let metadata =
        fs::symlink_metadata(&resolved).map_err(|e| format!("GameOccurrence non valida: {e}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("GameOccurrence non è un file regolare reale".into());
    }
    let (header, manifest_blake3, manifest_bytes): (StreamingOccurrencesManifestHeader, _, _) =
        deserialize_logical_json_with_identity(&canonical_path, "GameOccurrence")
            .map_err(|error| format!("Manifest GameOccurrence non valido: {error}"))?;
    if let Some((h, s)) = expected {
        if h != manifest_blake3 || s != manifest_bytes {
            return Err("L'occurrence non corrisponde all'identità attesa".to_owned());
        }
    }
    let actual_path = normalized(&canonical_root, &canonical_path, "occurrences")?;
    let expected_path = canonical_artifact_path("occurrences", &manifest_blake3, &canonical_path)?;
    if actual_path != expected_path {
        return Err("Percorso GameOccurrence non canonico".into());
    }
    validation::validate_header(&refs_from_header(&header))?;
    let catalog_path = validate_relative_path(&header.catalog_manifest, "catalog_manifest")?;
    let catalog = load_streaming_catalog(root, &catalog_path)?;
    if catalog.manifest_blake3 != header.catalog_manifest_blake3
        || catalog.manifest_bytes != header.catalog_manifest_bytes
    {
        return Err("Catalog diverso dall'identità GameOccurrence".into());
    }
    if header.interpretation_manifest != catalog.header.interpretation_manifest
        || header.interpretation_manifest_blake3 != catalog.header.interpretation_manifest_blake3
        || header.interpretation_manifest_bytes != catalog.header.interpretation_manifest_bytes
        || header.parse_manifest != catalog.interpretation.manifest.parse_manifest
        || header.parse_manifest_blake3 != catalog.interpretation.manifest.parse_manifest_blake3
        || header.parse_manifest_bytes != catalog.interpretation.manifest.parse_manifest_bytes
        || header.tokenization_manifest
            != catalog.interpretation.parse.manifest.tokenization_manifest
        || header.tokenization_manifest_blake3
            != catalog
                .interpretation
                .parse
                .manifest
                .tokenization_manifest_blake3
        || header.tokenization_manifest_bytes
            != catalog
                .interpretation
                .parse
                .manifest
                .tokenization_manifest_bytes
    {
        return Err("Anello provenance GameOccurrence incoerente".into());
    }
    let segmentation = &catalog.interpretation.parse.tokenization.segmentation;
    if header.segmentation_manifest
        != catalog
            .interpretation
            .parse
            .tokenization
            .manifest
            .segmentation_manifest
        || header.segmentation_manifest_blake3
            != catalog
                .interpretation
                .parse
                .tokenization
                .manifest
                .segmentation_manifest_blake3
        || header.segmentation_manifest_bytes
            != catalog
                .interpretation
                .parse
                .tokenization
                .manifest
                .segmentation_manifest_bytes
        || header.source_manifest != segmentation.manifest.source_manifest
        || header.source_manifest_blake3 != segmentation.manifest.source_manifest_blake3
        || header.source_manifest_bytes != segmentation.manifest.source_manifest_bytes
        || header.source_blake3 != catalog.header.source_blake3
        || header.source_bytes != catalog.header.source_bytes
    {
        return Err("Provenienza sorgente GameOccurrence incoerente".into());
    }
    // Streaming load validates identity and provenance only. Semantic Catalog
    // verification is fused with occurrence consumption/production downstream.
    Ok(LoadedStreamingOccurrences {
        header,
        canonical_path,
        manifest_blake3,
        manifest_bytes,
        catalog,
    })
}

#[derive(Serialize)]
struct StreamingOccurrencesManifest<'a, T> {
    schema_version: u32,
    occurrence_profile: &'a str,
    catalog_manifest: &'a str,
    catalog_manifest_blake3: &'a str,
    catalog_manifest_bytes: u64,
    interpretation_manifest: &'a str,
    interpretation_manifest_blake3: &'a str,
    interpretation_manifest_bytes: u64,
    parse_manifest: &'a str,
    parse_manifest_blake3: &'a str,
    parse_manifest_bytes: u64,
    tokenization_manifest: &'a str,
    tokenization_manifest_blake3: &'a str,
    tokenization_manifest_bytes: u64,
    segmentation_manifest: &'a str,
    segmentation_manifest_blake3: &'a str,
    segmentation_manifest_bytes: u64,
    source_manifest: &'a str,
    source_manifest_blake3: &'a str,
    source_manifest_bytes: u64,
    source_blake3: &'a str,
    source_bytes: u64,
    occurrences: T,
}

pub(in crate::inspector) struct StreamingOccurrencesRefs<'a> {
    pub(in crate::inspector) schema_version: u32,
    pub(in crate::inspector) occurrence_profile: &'a str,
    pub(in crate::inspector) catalog_manifest: &'a str,
    pub(in crate::inspector) catalog_manifest_blake3: &'a str,
    pub(in crate::inspector) catalog_manifest_bytes: u64,
    pub(in crate::inspector) interpretation_manifest: &'a str,
    pub(in crate::inspector) interpretation_manifest_blake3: &'a str,
    pub(in crate::inspector) interpretation_manifest_bytes: u64,
    pub(in crate::inspector) parse_manifest: &'a str,
    pub(in crate::inspector) parse_manifest_blake3: &'a str,
    pub(in crate::inspector) parse_manifest_bytes: u64,
    pub(in crate::inspector) tokenization_manifest: &'a str,
    pub(in crate::inspector) tokenization_manifest_blake3: &'a str,
    pub(in crate::inspector) tokenization_manifest_bytes: u64,
    pub(in crate::inspector) segmentation_manifest: &'a str,
    pub(in crate::inspector) segmentation_manifest_blake3: &'a str,
    pub(in crate::inspector) segmentation_manifest_bytes: u64,
    pub(in crate::inspector) source_manifest: &'a str,
    pub(in crate::inspector) source_manifest_blake3: &'a str,
    pub(in crate::inspector) source_manifest_bytes: u64,
    pub(in crate::inspector) source_blake3: &'a str,
    pub(in crate::inspector) source_bytes: u64,
}

pub(in crate::inspector) fn refs_from_header<'a>(
    header: &'a StreamingOccurrencesManifestHeader,
) -> StreamingOccurrencesRefs<'a> {
    StreamingOccurrencesRefs {
        schema_version: header.schema_version,
        occurrence_profile: &header.occurrence_profile,
        catalog_manifest: &header.catalog_manifest,
        catalog_manifest_blake3: &header.catalog_manifest_blake3,
        catalog_manifest_bytes: header.catalog_manifest_bytes,
        interpretation_manifest: &header.interpretation_manifest,
        interpretation_manifest_blake3: &header.interpretation_manifest_blake3,
        interpretation_manifest_bytes: header.interpretation_manifest_bytes,
        parse_manifest: &header.parse_manifest,
        parse_manifest_blake3: &header.parse_manifest_blake3,
        parse_manifest_bytes: header.parse_manifest_bytes,
        tokenization_manifest: &header.tokenization_manifest,
        tokenization_manifest_blake3: &header.tokenization_manifest_blake3,
        tokenization_manifest_bytes: header.tokenization_manifest_bytes,
        segmentation_manifest: &header.segmentation_manifest,
        segmentation_manifest_blake3: &header.segmentation_manifest_blake3,
        segmentation_manifest_bytes: header.segmentation_manifest_bytes,
        source_manifest: &header.source_manifest,
        source_manifest_blake3: &header.source_manifest_blake3,
        source_manifest_bytes: header.source_manifest_bytes,
        source_blake3: &header.source_blake3,
        source_bytes: header.source_bytes,
    }
}

fn refs_from_created<'a>(
    catalog: &'a LoadedStreamingCatalog,
    catalog_manifest: &'a str,
) -> Result<StreamingOccurrencesRefs<'a>, String> {
    let segmentation = &catalog.interpretation.parse.tokenization.segmentation;
    Ok(StreamingOccurrencesRefs {
        schema_version: OCCURRENCE_SCHEMA_VERSION,
        occurrence_profile: OCCURRENCE_PROFILE,
        catalog_manifest,
        catalog_manifest_blake3: &catalog.manifest_blake3,
        catalog_manifest_bytes: catalog.manifest_bytes,
        interpretation_manifest: &catalog.header.interpretation_manifest,
        interpretation_manifest_blake3: &catalog.header.interpretation_manifest_blake3,
        interpretation_manifest_bytes: catalog.header.interpretation_manifest_bytes,
        parse_manifest: &catalog.interpretation.manifest.parse_manifest,
        parse_manifest_blake3: &catalog.interpretation.manifest.parse_manifest_blake3,
        parse_manifest_bytes: catalog.interpretation.manifest.parse_manifest_bytes,
        tokenization_manifest: &catalog.interpretation.parse.manifest.tokenization_manifest,
        tokenization_manifest_blake3: &catalog
            .interpretation
            .parse
            .manifest
            .tokenization_manifest_blake3,
        tokenization_manifest_bytes: catalog
            .interpretation
            .parse
            .manifest
            .tokenization_manifest_bytes,
        segmentation_manifest: &catalog
            .interpretation
            .parse
            .tokenization
            .manifest
            .segmentation_manifest,
        segmentation_manifest_blake3: &catalog
            .interpretation
            .parse
            .tokenization
            .manifest
            .segmentation_manifest_blake3,
        segmentation_manifest_bytes: catalog
            .interpretation
            .parse
            .tokenization
            .manifest
            .segmentation_manifest_bytes,
        source_manifest: &segmentation.manifest.source_manifest,
        source_manifest_blake3: &segmentation.manifest.source_manifest_blake3,
        source_manifest_bytes: segmentation.manifest.source_manifest_bytes,
        source_blake3: &catalog.header.source_blake3,
        source_bytes: catalog.header.source_bytes,
    })
}

pub(in crate::inspector) fn provenance_from_refs(
    refs: &StreamingOccurrencesRefs<'_>,
) -> OccurrenceProvenanceV1 {
    OccurrenceProvenanceV1 {
        source_manifest_blake3: refs.source_manifest_blake3.to_owned(),
        source_manifest_bytes: refs.source_manifest_bytes,
        segmentation_manifest_blake3: refs.segmentation_manifest_blake3.to_owned(),
        segmentation_manifest_bytes: refs.segmentation_manifest_bytes,
        tokenization_manifest_blake3: refs.tokenization_manifest_blake3.to_owned(),
        tokenization_manifest_bytes: refs.tokenization_manifest_bytes,
        parse_manifest_blake3: refs.parse_manifest_blake3.to_owned(),
        parse_manifest_bytes: refs.parse_manifest_bytes,
        interpretation_manifest_blake3: refs.interpretation_manifest_blake3.to_owned(),
        interpretation_manifest_bytes: refs.interpretation_manifest_bytes,
        catalog_manifest_blake3: refs.catalog_manifest_blake3.to_owned(),
        catalog_manifest_bytes: refs.catalog_manifest_bytes,
    }
}

#[derive(Debug, Default, Clone)]
pub(in crate::inspector) struct OccurrenceCounts {
    pub(in crate::inspector) occurrences: usize,
}

fn serialize_occurrences_with<W, F>(
    writer: &mut W,
    refs: &StreamingOccurrencesRefs<'_>,
    catalog: &LoadedStreamingCatalog,
    provenance: &OccurrenceProvenanceV1,
    counts: &RefCell<OccurrenceCounts>,
    on_occurrence: &RefCell<F>,
) -> Result<(), String>
where
    W: Write + ?Sized,
    F: FnMut(GameOccurrenceV1, &GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    let manifest = StreamingOccurrencesManifest {
        schema_version: refs.schema_version,
        occurrence_profile: refs.occurrence_profile,
        catalog_manifest: refs.catalog_manifest,
        catalog_manifest_blake3: refs.catalog_manifest_blake3,
        catalog_manifest_bytes: refs.catalog_manifest_bytes,
        interpretation_manifest: refs.interpretation_manifest,
        interpretation_manifest_blake3: refs.interpretation_manifest_blake3,
        interpretation_manifest_bytes: refs.interpretation_manifest_bytes,
        parse_manifest: refs.parse_manifest,
        parse_manifest_blake3: refs.parse_manifest_blake3,
        parse_manifest_bytes: refs.parse_manifest_bytes,
        tokenization_manifest: refs.tokenization_manifest,
        tokenization_manifest_blake3: refs.tokenization_manifest_blake3,
        tokenization_manifest_bytes: refs.tokenization_manifest_bytes,
        segmentation_manifest: refs.segmentation_manifest,
        segmentation_manifest_blake3: refs.segmentation_manifest_blake3,
        segmentation_manifest_bytes: refs.segmentation_manifest_bytes,
        source_manifest: refs.source_manifest,
        source_manifest_blake3: refs.source_manifest_blake3,
        source_manifest_bytes: refs.source_manifest_bytes,
        source_blake3: refs.source_blake3,
        source_bytes: refs.source_bytes,
        occurrences: ChangedOccurrences {
            catalog,
            provenance,
            counts,
            on_occurrence,
        },
    };
    serde_json::to_writer_pretty(&mut *writer, &manifest)
        .map_err(|error| format!("Errore serializzando le GameOccurrence: {error}"))?;
    writer
        .write_all(b"\n")
        .map_err(|error| format!("Errore serializzando le GameOccurrence: {error}"))?;
    Ok(())
}

struct ChangedOccurrences<'a, F> {
    catalog: &'a LoadedStreamingCatalog,
    provenance: &'a OccurrenceProvenanceV1,
    counts: &'a RefCell<OccurrenceCounts>,
    on_occurrence: &'a RefCell<F>,
}

fn for_each_computed_occurrence<F>(
    catalog: &LoadedStreamingCatalog,
    provenance: &OccurrenceProvenanceV1,
    trust_fresh: bool,
    mut on_occurrence: F,
) -> Result<OccurrenceCounts, String>
where
    F: FnMut(GameOccurrenceV1, &GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    let source_blake3 = catalog.header.source_blake3.as_str();
    let source_bytes = catalog.header.source_bytes;
    let mut previous_end = None;
    let mut counts = OccurrenceCounts::default();
    let mut consume = |catalog_game: CatalogGameRecordV1,
                       interpretation_game: &GameRecordV1,
                       tree: &ParseTree,
                       stream: &TokenStream| {
        let ordinal = catalog_game.id;
        let occurrence = extraction::occurrence(
            ordinal,
            (source_blake3, source_bytes),
            &catalog_game,
            interpretation_game,
            tree,
            stream,
            provenance,
        )?;
        validation::validate_occurrence(&occurrence, (source_blake3, source_bytes), provenance)?;
        if previous_end.is_some_and(|end| end > occurrence.source_start) {
            return Err("Intervallo occurrence non ordinato o sovrapposto".into());
        }
        previous_end = Some(occurrence.source_end);
        counts.occurrences += 1;
        on_occurrence(occurrence, interpretation_game, tree, stream)
    };
    if trust_fresh {
        catalog.for_each_fresh_game(&mut consume)?;
    } else {
        catalog.for_each_game(&mut consume)?;
    }
    Ok(counts)
}

impl<F> Serialize for ChangedOccurrences<'_, F>
where
    F: FnMut(GameOccurrenceV1, &GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(None)?;
        let trust_fresh = crate::inspector::common::fresh_chain_enabled();
        let counts = for_each_computed_occurrence(
            self.catalog,
            self.provenance,
            trust_fresh,
            |occurrence, interpretation_game, tree, stream| {
                sequence
                    .serialize_element(&occurrence)
                    .map_err(|error| error.to_string())?;
                (self.on_occurrence.borrow_mut())(occurrence, interpretation_game, tree, stream)
                    .map_err(|error| error.to_string())
            },
        )
        .map_err(serde::ser::Error::custom)?;
        *self.counts.borrow_mut() = counts;
        sequence.end()
    }
}

fn serialize_occurrences_into<W, F>(
    writer: &mut W,
    catalog: &LoadedStreamingCatalog,
    refs: &StreamingOccurrencesRefs<'_>,
    provenance: &OccurrenceProvenanceV1,
    counts: &RefCell<OccurrenceCounts>,
    on_occurrence: F,
) -> Result<OccurrenceCounts, String>
where
    W: Write + ?Sized,
    F: FnMut(GameOccurrenceV1, &GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
{
    let on_occurrence = RefCell::new(on_occurrence);
    serialize_occurrences_with(writer, refs, catalog, provenance, counts, &on_occurrence)?;
    Ok(counts.borrow().clone())
}

impl LoadedStreamingOccurrences {
    pub(super) fn for_each_verified_occurrence<F>(
        &self,
        mut on_occurrence: F,
    ) -> Result<usize, String>
    where
        F: FnMut(GameOccurrenceV1) -> Result<(), String>,
    {
        self.for_each_verified_occurrence_with_context(|occurrence, _, _, _| {
            on_occurrence(occurrence)
        })
    }

    pub(super) fn for_each_fresh_occurrence_with_context<F>(
        &self,
        on_occurrence: F,
    ) -> Result<usize, String>
    where
        F: FnMut(GameOccurrenceV1, &GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
    {
        let refs = refs_from_header(&self.header);
        let provenance = provenance_from_refs(&refs);
        Ok(
            for_each_computed_occurrence(&self.catalog, &provenance, true, on_occurrence)?
                .occurrences,
        )
    }

    pub(super) fn for_each_verified_occurrence_with_context<F>(
        &self,
        on_occurrence: F,
    ) -> Result<usize, String>
    where
        F: FnMut(GameOccurrenceV1, &GameRecordV1, &ParseTree, &TokenStream) -> Result<(), String>,
    {
        let refs = refs_from_header(&self.header);
        let provenance = provenance_from_refs(&refs);
        let mut stored = logical_json_reader(&self.canonical_path, "GameOccurrence")?;
        let mut compare = ProgressiveCompareWriter::new(&mut stored);
        let counts = RefCell::new(OccurrenceCounts::default());
        let on_occurrence = RefCell::new(on_occurrence);
        serialize_occurrences_with(
            &mut compare,
            &refs,
            &self.catalog,
            &provenance,
            &counts,
            &on_occurrence,
        )?;
        compare.finish("GameOccurrence", &self.manifest_blake3, self.manifest_bytes)?;
        Ok(counts.into_inner().occurrences)
    }
}

pub(super) fn create(root: &Path, catalog_path: &Path) -> Result<CreatedOccurrences, String> {
    let catalog = load_streaming_catalog(root, catalog_path)?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let catalog_manifest =
        normalized(&canonical_root, &catalog.canonical_path, "catalog_manifest")?;
    let refs = refs_from_created(&catalog, &catalog_manifest)?;
    let provenance = provenance_from_refs(&refs);
    let directory = prepare_directory_chain(
        root,
        &canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("occurrences", "directory occurrences"),
        ],
    )?;
    let counts = RefCell::new(OccurrenceCounts::default());
    let (target, new_manifest, hash, size) =
        publish_compressed_json_stream(&canonical_root, &directory, "GameOccurrence", |writer| {
            let tally = serialize_occurrences_into(
                writer,
                &catalog,
                &refs,
                &provenance,
                &counts,
                |_, _, _, _| Ok(()),
            )?;
            *counts.borrow_mut() = tally;
            Ok(())
        })?;
    Ok(CreatedOccurrences {
        path: PathBuf::from("inspector/occurrences").join(
            target
                .file_name()
                .ok_or("Artefatto occurrence senza nome")?,
        ),
        manifest_blake3: hash,
        manifest_bytes: size,
        new_manifest,
        occurrences: counts.into_inner().occurrences,
    })
}

pub(super) fn load_verified(root: &Path, path: &Path) -> Result<VerifiedOccurrences, String> {
    let loaded = load_streaming_occurrences(root, path, None)?;
    let catalog_path = validate_relative_path(&loaded.header.catalog_manifest, "catalog_manifest")?;
    let catalog = load_verified_catalog(root, &catalog_path)?;
    if catalog.manifest_blake3 != loaded.header.catalog_manifest_blake3
        || catalog.manifest_bytes != loaded.header.catalog_manifest_bytes
    {
        return Err("Catalog diverso dall'identità GameOccurrence".into());
    }
    let mut occurrences = Vec::new();
    loaded.for_each_verified_occurrence(|occurrence| {
        occurrences.push(occurrence);
        Ok(())
    })?;
    let manifest = OccurrenceManifestV1 {
        schema_version: loaded.header.schema_version,
        occurrence_profile: loaded.header.occurrence_profile,
        catalog_manifest: loaded.header.catalog_manifest,
        catalog_manifest_blake3: loaded.header.catalog_manifest_blake3,
        catalog_manifest_bytes: loaded.header.catalog_manifest_bytes,
        interpretation_manifest: loaded.header.interpretation_manifest,
        interpretation_manifest_blake3: loaded.header.interpretation_manifest_blake3,
        interpretation_manifest_bytes: loaded.header.interpretation_manifest_bytes,
        parse_manifest: loaded.header.parse_manifest,
        parse_manifest_blake3: loaded.header.parse_manifest_blake3,
        parse_manifest_bytes: loaded.header.parse_manifest_bytes,
        tokenization_manifest: loaded.header.tokenization_manifest,
        tokenization_manifest_blake3: loaded.header.tokenization_manifest_blake3,
        tokenization_manifest_bytes: loaded.header.tokenization_manifest_bytes,
        segmentation_manifest: loaded.header.segmentation_manifest,
        segmentation_manifest_blake3: loaded.header.segmentation_manifest_blake3,
        segmentation_manifest_bytes: loaded.header.segmentation_manifest_bytes,
        source_manifest: loaded.header.source_manifest,
        source_manifest_blake3: loaded.header.source_manifest_blake3,
        source_manifest_bytes: loaded.header.source_manifest_bytes,
        source_blake3: loaded.header.source_blake3,
        source_bytes: loaded.header.source_bytes,
        occurrences,
    };
    validation::validate(&manifest)?;
    Ok(VerifiedOccurrences {
        manifest,
        catalog,
        canonical_path: loaded.canonical_path,
        manifest_blake3: loaded.manifest_blake3,
        manifest_bytes: loaded.manifest_bytes,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) struct VerifiedOccurrenceProvenanceV1 {
    pub(in crate::inspector) source_manifest_blake3: String,
    pub(in crate::inspector) source_manifest_bytes: u64,
    pub(in crate::inspector) segmentation_manifest_blake3: String,
    pub(in crate::inspector) segmentation_manifest_bytes: u64,
    pub(in crate::inspector) tokenization_manifest_blake3: String,
    pub(in crate::inspector) tokenization_manifest_bytes: u64,
    pub(in crate::inspector) parse_manifest_blake3: String,
    pub(in crate::inspector) parse_manifest_bytes: u64,
    pub(in crate::inspector) interpretation_manifest_blake3: String,
    pub(in crate::inspector) interpretation_manifest_bytes: u64,
    pub(in crate::inspector) catalog_manifest_blake3: String,
    pub(in crate::inspector) catalog_manifest_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::inspector) struct VerifiedOccurrenceIdentityV1 {
    pub(in crate::inspector) ordinal: u64,
    pub(in crate::inspector) occurrence_id: String,
    pub(in crate::inspector) source_blake3: String,
    pub(in crate::inspector) source_bytes: u64,
    pub(in crate::inspector) source_game_blake3: String,
    pub(in crate::inspector) game_bytes: u64,
    pub(in crate::inspector) provenance: VerifiedOccurrenceProvenanceV1,
}

pub(in crate::inspector) fn verified_identity_projection(
    verified: &VerifiedOccurrences,
) -> Result<Vec<VerifiedOccurrenceIdentityV1>, String> {
    verified
        .manifest
        .occurrences
        .iter()
        .enumerate()
        .map(|(index, occurrence)| {
            let ordinal = u64::try_from(index).map_err(|_| "Ordinal occurrence oltre u64")?;

            if occurrence.ordinal != ordinal {
                return Err("Ordinal occurrence non allineato".into());
            }

            if occurrence.source_blake3 != verified.manifest.source_blake3
                || occurrence.source_bytes != verified.manifest.source_bytes
            {
                return Err("Identità sorgente occurrence incoerente".into());
            }

            let game_bytes = occurrence
                .source_end
                .checked_sub(occurrence.source_start)
                .ok_or("Intervallo occurrence invertito")?;

            let expected_id = extraction::occurrence_id(
                &occurrence.source_blake3,
                occurrence.source_bytes,
                occurrence.source_start,
                occurrence.source_end,
                &occurrence.source_game_blake3,
            );

            if occurrence.occurrence_id != expected_id {
                return Err("occurrence_id diverso dal ricalcolo".into());
            }

            let provenance = &occurrence.provenance;
            if provenance.source_manifest_blake3 != verified.manifest.source_manifest_blake3
                || provenance.source_manifest_bytes != verified.manifest.source_manifest_bytes
                || provenance.segmentation_manifest_blake3
                    != verified.manifest.segmentation_manifest_blake3
                || provenance.segmentation_manifest_bytes
                    != verified.manifest.segmentation_manifest_bytes
                || provenance.tokenization_manifest_blake3
                    != verified.manifest.tokenization_manifest_blake3
                || provenance.tokenization_manifest_bytes
                    != verified.manifest.tokenization_manifest_bytes
                || provenance.parse_manifest_blake3 != verified.manifest.parse_manifest_blake3
                || provenance.parse_manifest_bytes != verified.manifest.parse_manifest_bytes
                || provenance.interpretation_manifest_blake3
                    != verified.manifest.interpretation_manifest_blake3
                || provenance.interpretation_manifest_bytes
                    != verified.manifest.interpretation_manifest_bytes
                || provenance.catalog_manifest_blake3 != verified.manifest.catalog_manifest_blake3
                || provenance.catalog_manifest_bytes != verified.manifest.catalog_manifest_bytes
            {
                return Err("Provenance occurrence incoerente".into());
            }

            Ok(VerifiedOccurrenceIdentityV1 {
                ordinal,
                occurrence_id: occurrence.occurrence_id.clone(),
                source_blake3: occurrence.source_blake3.clone(),
                source_bytes: occurrence.source_bytes,
                source_game_blake3: occurrence.source_game_blake3.clone(),
                game_bytes,
                provenance: VerifiedOccurrenceProvenanceV1 {
                    source_manifest_blake3: provenance.source_manifest_blake3.clone(),
                    source_manifest_bytes: provenance.source_manifest_bytes,
                    segmentation_manifest_blake3: provenance.segmentation_manifest_blake3.clone(),
                    segmentation_manifest_bytes: provenance.segmentation_manifest_bytes,
                    tokenization_manifest_blake3: provenance.tokenization_manifest_blake3.clone(),
                    tokenization_manifest_bytes: provenance.tokenization_manifest_bytes,
                    parse_manifest_blake3: provenance.parse_manifest_blake3.clone(),
                    parse_manifest_bytes: provenance.parse_manifest_bytes,
                    interpretation_manifest_blake3: provenance
                        .interpretation_manifest_blake3
                        .clone(),
                    interpretation_manifest_bytes: provenance.interpretation_manifest_bytes,
                    catalog_manifest_blake3: provenance.catalog_manifest_blake3.clone(),
                    catalog_manifest_bytes: provenance.catalog_manifest_bytes,
                },
            })
        })
        .collect()
}

pub(super) fn verify(root: &Path, path: &Path) -> Result<(), String> {
    let loaded = load_streaming_occurrences(root, path, None)?;
    loaded.for_each_verified_occurrence(|_| Ok(()))?;
    Ok(())
}

pub(in crate::inspector) use export::publish_output;
pub(super) use export::{export_jsonl, export_one};
pub(in crate::inspector) use manifest::{GameOccurrenceV1, OccurrenceProvenanceV1};
