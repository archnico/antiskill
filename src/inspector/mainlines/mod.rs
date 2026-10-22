mod export;
mod extraction;
mod manifest;
#[cfg(test)]
mod tests;
mod validation;

use crate::inspector::common::{
    canonical_artifact_path, deserialize_logical_json_with_identity, ensure_within,
    logical_json_reader, path_to_manifest_string, prepare_directory_chain,
    publish_compressed_json_stream, resolve_workspace_path, validate_relative_path,
    ProgressiveCompareWriter,
};
use crate::inspector::interpretation::manifest::GameRecordV1;
use crate::inspector::occurrences::{self, LoadedStreamingOccurrences, VerifiedOccurrences};
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

#[derive(Debug)]
pub(in crate::inspector) struct ProbeMainlineSummary {
    pub(in crate::inspector) comparable: bool,
    pub(in crate::inspector) moves: usize,
    pub(in crate::inspector) results: usize,
    pub(in crate::inspector) identity: Option<String>,
    pub(in crate::inspector) result_values: Vec<String>,
}

pub(in crate::inspector) fn probe_mainline(
    ordinal: u64,
    occurrence: &occurrences::GameOccurrenceV1,
    game: &GameRecordV1,
    tree: &ParseTree,
    stream: &TokenStream,
    blob: &[u8],
) -> Result<ProbeMainlineSummary, String> {
    let zero = "0000000000000000000000000000000000000000000000000000000000000000";
    let record = extraction::extract_one_from_blob_for_probe(
        zero, 1, zero, 1, occurrence, game, tree, stream, blob,
    )?;
    validation::validate_mainline(
        usize::try_from(ordinal).map_err(|_| "Ordinal probe oltre usize".to_owned())?,
        &record,
        zero,
        1,
        zero,
        1,
    )?;
    Ok(ProbeMainlineSummary {
        comparable: record.comparable,
        moves: record.moves.len(),
        results: record.results.len(),
        identity: record.canonical_mainline_id,
        result_values: record
            .results
            .iter()
            .map(|result| result.value.clone())
            .collect(),
    })
}

pub(super) struct CreatedMainlines {
    pub(super) path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) new_manifest: bool,
    pub(super) mainlines: usize,
    pub(super) comparable: usize,
}

struct VerifiedMainlines {
    manifest: MainlineManifestV1,
    occurrences: VerifiedOccurrences,
    canonical_path: PathBuf,
    manifest_blake3: String,
    manifest_bytes: u64,
}

pub(in crate::inspector) struct VerifiedMainlineChain {
    pub(in crate::inspector) occurrences: VerifiedOccurrences,
    pub(in crate::inspector) canonical_path: PathBuf,
    pub(in crate::inspector) manifest_blake3: String,
    pub(in crate::inspector) manifest_bytes: u64,
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
pub(super) struct StreamingMainlinesManifestHeader {
    pub(super) schema_version: u32,
    pub(super) mainline_profile: String,
    pub(super) occurrences_manifest: String,
    pub(super) occurrences_manifest_blake3: String,
    pub(super) occurrences_manifest_bytes: u64,
    pub(super) interpretation_manifest: String,
    pub(super) interpretation_manifest_blake3: String,
    pub(super) interpretation_manifest_bytes: u64,
    pub(super) source_blake3: String,
    pub(super) source_bytes: u64,
    #[serde(rename = "mainlines")]
    _mainlines: IgnoredAny,
}

pub(super) struct LoadedStreamingMainlines {
    pub(super) header: StreamingMainlinesManifestHeader,
    pub(super) canonical_path: PathBuf,
    pub(super) manifest_blake3: String,
    pub(super) manifest_bytes: u64,
    pub(super) occurrences: LoadedStreamingOccurrences,
}

pub(super) fn load_streaming_mainlines(
    root: &Path,
    path: &Path,
    expected: Option<(&str, u64)>,
) -> Result<LoadedStreamingMainlines, String> {
    let path_value = path
        .to_str()
        .ok_or("Percorso canonical mainline non UTF-8")?;
    validate_relative_path(path_value, "mainlines")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Workspace non valido: {e}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let canonical_path =
        fs::canonicalize(&resolved).map_err(|e| format!("Canonical mainline non valida: {e}"))?;
    ensure_within(&canonical_root, &canonical_path, "mainlines")?;
    let metadata = fs::symlink_metadata(&resolved)
        .map_err(|e| format!("Canonical mainline non valida: {e}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Canonical mainline non è un file regolare reale".into());
    }
    let (header, manifest_blake3, manifest_bytes): (StreamingMainlinesManifestHeader, _, _) =
        deserialize_logical_json_with_identity(&canonical_path, "Canonical mainline")
            .map_err(|error| format!("Manifest canonical mainline non valido: {error}"))?;
    if let Some((h, s)) = expected {
        if h != manifest_blake3 || s != manifest_bytes {
            return Err("La canonical mainline non corrisponde all'identità attesa".to_owned());
        }
    }
    let actual_path = normalized(&canonical_root, &canonical_path, "mainlines")?;
    let expected_path = canonical_artifact_path("mainlines", &manifest_blake3, &canonical_path)?;
    if actual_path != expected_path {
        return Err("Percorso canonical mainline non canonico".into());
    }
    validation::validate_header(&refs_from_header(&header))?;
    let occurrences_path =
        validate_relative_path(&header.occurrences_manifest, "occurrences_manifest")?;
    let occurrences = occurrences::load_streaming_occurrences(
        root,
        &occurrences_path,
        Some((
            &header.occurrences_manifest_blake3,
            header.occurrences_manifest_bytes,
        )),
    )?;
    if header.interpretation_manifest != occurrences.catalog.header.interpretation_manifest
        || header.interpretation_manifest_blake3
            != occurrences.catalog.header.interpretation_manifest_blake3
        || header.interpretation_manifest_bytes
            != occurrences.catalog.header.interpretation_manifest_bytes
        || header.source_blake3 != occurrences.catalog.header.source_blake3
        || header.source_bytes != occurrences.catalog.header.source_bytes
    {
        return Err("Anello provenance canonical mainline incoerente".into());
    }
    Ok(LoadedStreamingMainlines {
        header,
        canonical_path,
        manifest_blake3,
        manifest_bytes,
        occurrences,
    })
}

#[derive(Serialize)]
struct StreamingMainlinesManifest<'a, T> {
    schema_version: u32,
    mainline_profile: &'a str,
    occurrences_manifest: &'a str,
    occurrences_manifest_blake3: &'a str,
    occurrences_manifest_bytes: u64,
    interpretation_manifest: &'a str,
    interpretation_manifest_blake3: &'a str,
    interpretation_manifest_bytes: u64,
    source_blake3: &'a str,
    source_bytes: u64,
    mainlines: T,
}

pub(in crate::inspector) struct StreamingMainlinesRefs<'a> {
    pub(in crate::inspector) schema_version: u32,
    pub(in crate::inspector) mainline_profile: &'a str,
    pub(in crate::inspector) occurrences_manifest: &'a str,
    pub(in crate::inspector) occurrences_manifest_blake3: &'a str,
    pub(in crate::inspector) occurrences_manifest_bytes: u64,
    pub(in crate::inspector) interpretation_manifest: &'a str,
    pub(in crate::inspector) interpretation_manifest_blake3: &'a str,
    pub(in crate::inspector) interpretation_manifest_bytes: u64,
    pub(in crate::inspector) source_blake3: &'a str,
    pub(in crate::inspector) source_bytes: u64,
}

fn refs_from_header<'a>(
    header: &'a StreamingMainlinesManifestHeader,
) -> StreamingMainlinesRefs<'a> {
    StreamingMainlinesRefs {
        schema_version: header.schema_version,
        mainline_profile: &header.mainline_profile,
        occurrences_manifest: &header.occurrences_manifest,
        occurrences_manifest_blake3: &header.occurrences_manifest_blake3,
        occurrences_manifest_bytes: header.occurrences_manifest_bytes,
        interpretation_manifest: &header.interpretation_manifest,
        interpretation_manifest_blake3: &header.interpretation_manifest_blake3,
        interpretation_manifest_bytes: header.interpretation_manifest_bytes,
        source_blake3: &header.source_blake3,
        source_bytes: header.source_bytes,
    }
}

fn refs_from_occurrences<'a>(
    occurrences: &'a LoadedStreamingOccurrences,
    occurrences_manifest: &'a str,
) -> Result<StreamingMainlinesRefs<'a>, String> {
    Ok(StreamingMainlinesRefs {
        schema_version: MAINLINE_SCHEMA_VERSION,
        mainline_profile: MAINLINE_PROFILE,
        occurrences_manifest,
        occurrences_manifest_blake3: &occurrences.manifest_blake3,
        occurrences_manifest_bytes: occurrences.manifest_bytes,
        interpretation_manifest: &occurrences.catalog.header.interpretation_manifest,
        interpretation_manifest_blake3: &occurrences.catalog.header.interpretation_manifest_blake3,
        interpretation_manifest_bytes: occurrences.catalog.header.interpretation_manifest_bytes,
        source_blake3: &occurrences.catalog.header.source_blake3,
        source_bytes: occurrences.catalog.header.source_bytes,
    })
}

#[derive(Debug, Default, Clone)]
pub(in crate::inspector) struct MainlineCounts {
    pub(in crate::inspector) mainlines: usize,
    pub(in crate::inspector) comparable: usize,
}

fn serialize_mainlines_with<W, F>(
    writer: &mut W,
    refs: &StreamingMainlinesRefs<'_>,
    occurrences: &LoadedStreamingOccurrences,
    counts: &RefCell<MainlineCounts>,
    on_mainline: &RefCell<F>,
) -> Result<(), String>
where
    W: Write + ?Sized,
    F: FnMut(CanonicalMainlineV1, &occurrences::GameOccurrenceV1) -> Result<(), String>,
{
    let manifest = StreamingMainlinesManifest {
        schema_version: refs.schema_version,
        mainline_profile: refs.mainline_profile,
        occurrences_manifest: refs.occurrences_manifest,
        occurrences_manifest_blake3: refs.occurrences_manifest_blake3,
        occurrences_manifest_bytes: refs.occurrences_manifest_bytes,
        interpretation_manifest: refs.interpretation_manifest,
        interpretation_manifest_blake3: refs.interpretation_manifest_blake3,
        interpretation_manifest_bytes: refs.interpretation_manifest_bytes,
        source_blake3: refs.source_blake3,
        source_bytes: refs.source_bytes,
        mainlines: ChangedMainlines {
            refs,
            occurrences,
            counts,
            on_mainline,
        },
    };
    serde_json::to_writer_pretty(&mut *writer, &manifest)
        .map_err(|error| format!("Errore serializzando le canonical mainline: {error}"))?;
    writer
        .write_all(b"\n")
        .map_err(|error| format!("Errore serializzando le canonical mainline: {error}"))?;
    Ok(())
}

struct ChangedMainlines<'a, F> {
    refs: &'a StreamingMainlinesRefs<'a>,
    occurrences: &'a LoadedStreamingOccurrences,
    counts: &'a RefCell<MainlineCounts>,
    on_mainline: &'a RefCell<F>,
}

impl<F> Serialize for ChangedMainlines<'_, F>
where
    F: FnMut(CanonicalMainlineV1, &occurrences::GameOccurrenceV1) -> Result<(), String>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(None)?;
        let tokenization = &self.occurrences.catalog.interpretation.parse.tokenization;
        let cx = extraction::MainlineExtractionContext {
            occurrences_manifest_blake3: self.refs.occurrences_manifest_blake3,
            occurrences_manifest_bytes: self.refs.occurrences_manifest_bytes,
            interpretation_manifest_blake3: self.refs.interpretation_manifest_blake3,
            interpretation_manifest_bytes: self.refs.interpretation_manifest_bytes,
            tokenization,
        };
        let mut consume = |occurrence: occurrences::GameOccurrenceV1,
                           interpretation_game: &GameRecordV1,
                           tree: &ParseTree,
                           stream: &TokenStream| {
            let ordinal = occurrence.ordinal;
            let index = usize::try_from(ordinal).map_err(|_| "Ordinal oltre usize".to_owned())?;
            let mainline =
                extraction::extract_one(&cx, &occurrence, interpretation_game, tree, stream)?;
            validation::validate_mainline(
                index,
                &mainline,
                self.refs.occurrences_manifest_blake3,
                self.refs.occurrences_manifest_bytes,
                self.refs.interpretation_manifest_blake3,
                self.refs.interpretation_manifest_bytes,
            )?;
            sequence
                .serialize_element(&mainline)
                .map_err(|error| error.to_string())?;
            self.counts.borrow_mut().mainlines += 1;
            if mainline.comparable {
                self.counts.borrow_mut().comparable += 1;
            }
            (self.on_mainline.borrow_mut())(mainline, &occurrence)
                .map_err(|error| error.to_string())
        };
        let result = if crate::inspector::common::fresh_chain_enabled() {
            self.occurrences
                .for_each_fresh_occurrence_with_context(&mut consume)
        } else {
            self.occurrences
                .for_each_verified_occurrence_with_context(&mut consume)
        };
        result.map_err(serde::ser::Error::custom)?;
        sequence.end()
    }
}

fn serialize_mainlines_into<W, F>(
    writer: &mut W,
    occurrences: &LoadedStreamingOccurrences,
    refs: &StreamingMainlinesRefs<'_>,
    counts: &RefCell<MainlineCounts>,
    on_mainline: F,
) -> Result<MainlineCounts, String>
where
    W: Write + ?Sized,
    F: FnMut(CanonicalMainlineV1, &occurrences::GameOccurrenceV1) -> Result<(), String>,
{
    let on_mainline = RefCell::new(on_mainline);
    serialize_mainlines_with(writer, refs, occurrences, counts, &on_mainline)?;
    Ok(counts.borrow().clone())
}

impl LoadedStreamingMainlines {
    pub(super) fn for_each_verified_mainline<F>(&self, mut on_mainline: F) -> Result<usize, String>
    where
        F: FnMut(CanonicalMainlineV1) -> Result<(), String>,
    {
        self.for_each_verified_mainline_with_occurrence(|mainline, _| on_mainline(mainline))
    }

    pub(super) fn for_each_verified_mainline_with_occurrence<F>(
        &self,
        on_mainline: F,
    ) -> Result<usize, String>
    where
        F: FnMut(CanonicalMainlineV1, &occurrences::GameOccurrenceV1) -> Result<(), String>,
    {
        let refs = refs_from_header(&self.header);
        let mut stored = logical_json_reader(&self.canonical_path, "Canonical mainline")?;
        let mut compare = ProgressiveCompareWriter::new(&mut stored);
        let counts = RefCell::new(MainlineCounts::default());
        let on_mainline = RefCell::new(on_mainline);
        serialize_mainlines_with(
            &mut compare,
            &refs,
            &self.occurrences,
            &counts,
            &on_mainline,
        )?;
        compare.finish(
            "Canonical mainline",
            &self.manifest_blake3,
            self.manifest_bytes,
        )?;
        Ok(counts.into_inner().mainlines)
    }

    pub(super) fn for_each_verified_product_logical_game_row<F>(
        &self,
        mut on_row: F,
    ) -> Result<usize, String>
    where
        F: FnMut(crate::inspector::ProductLogicalGameRowV1) -> Result<(), String>,
    {
        self.for_each_verified_mainline_with_occurrence(|mainline, occurrence| {
            let projection = occurrences::product_duplicate_projection(occurrence)?;
            if mainline.occurrence_id != projection.occurrence_id {
                return Err("Occurrence e canonical mainline non allineate".into());
            }
            let representative_quality = crate::inspector::product_representative_quality(
                &projection.white,
                &projection.black,
                &projection.date,
                &projection.event,
                &projection.result,
            );
            on_row(crate::inspector::ProductLogicalGameRowV1 {
                occurrence_id: mainline.occurrence_id,
                canonical_mainline_id: mainline.canonical_mainline_id,
                comparable: mainline.comparable,
                representative_quality,
            })
        })
    }

    pub(super) fn for_each_verified_product_duplicate_row<F>(
        &self,
        mut on_row: F,
    ) -> Result<usize, String>
    where
        F: FnMut(crate::inspector::ProductDuplicateRowV1) -> Result<(), String>,
    {
        self.for_each_verified_mainline_with_occurrence(|mainline, occurrence| {
            let projection = occurrences::product_duplicate_projection(occurrence)?;
            if mainline.occurrence_id != projection.occurrence_id {
                return Err("Occurrence e canonical mainline non allineate".into());
            }
            on_row(crate::inspector::ProductDuplicateRowV1 {
                occurrence_id: projection.occurrence_id,
                source_game_blake3: projection.source_game_blake3,
                game_bytes: projection.game_bytes,
                canonical_mainline_id: mainline.canonical_mainline_id,
                comparable: mainline.comparable,
                mainline_results: mainline
                    .results
                    .into_iter()
                    .map(|result| result.value)
                    .collect(),
                date: projection.date,
                white: projection.white,
                black: projection.black,
                result: projection.result,
                event: projection.event,
            })
        })
    }
}

pub(super) fn create(root: &Path, occurrence_path: &Path) -> Result<CreatedMainlines, String> {
    let occurrences = occurrences::load_streaming_occurrences(root, occurrence_path, None)?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let occurrences_manifest = normalized(
        &canonical_root,
        &occurrences.canonical_path,
        "occurrences_manifest",
    )?;
    let refs = refs_from_occurrences(&occurrences, &occurrences_manifest)?;
    let directory = prepare_directory_chain(
        root,
        &canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("mainlines", "directory mainlines"),
        ],
    )?;
    let counts = RefCell::new(MainlineCounts::default());
    let (target, new_manifest, hash, size) = publish_compressed_json_stream(
        &canonical_root,
        &directory,
        "Canonical mainline",
        |writer| {
            let tally =
                serialize_mainlines_into(writer, &occurrences, &refs, &counts, |_, _| Ok(()))?;
            *counts.borrow_mut() = tally;
            Ok(())
        },
    )?;
    let counts = counts.into_inner();
    Ok(CreatedMainlines {
        path: PathBuf::from("inspector/mainlines")
            .join(target.file_name().ok_or("Artefatto mainline senza nome")?),
        manifest_blake3: hash,
        manifest_bytes: size,
        new_manifest,
        mainlines: counts.mainlines,
        comparable: counts.comparable,
    })
}

fn load_verified(root: &Path, path: &Path) -> Result<VerifiedMainlines, String> {
    let loaded = load_streaming_mainlines(root, path, None)?;
    let mut mainlines = Vec::new();
    loaded.for_each_verified_mainline(|mainline| {
        mainlines.push(mainline);
        Ok(())
    })?;
    let manifest = MainlineManifestV1 {
        schema_version: loaded.header.schema_version,
        mainline_profile: loaded.header.mainline_profile,
        occurrences_manifest: loaded.header.occurrences_manifest,
        occurrences_manifest_blake3: loaded.header.occurrences_manifest_blake3.clone(),
        occurrences_manifest_bytes: loaded.header.occurrences_manifest_bytes,
        interpretation_manifest: loaded.header.interpretation_manifest,
        interpretation_manifest_blake3: loaded.header.interpretation_manifest_blake3,
        interpretation_manifest_bytes: loaded.header.interpretation_manifest_bytes,
        source_blake3: loaded.header.source_blake3,
        source_bytes: loaded.header.source_bytes,
        mainlines,
    };
    validation::validate(&manifest)?;
    let occurrences_path =
        validate_relative_path(&manifest.occurrences_manifest, "occurrences_manifest")?;
    let occurrences = occurrences::load_verified(root, &occurrences_path)?;
    if occurrences.manifest_blake3 != loaded.header.occurrences_manifest_blake3
        || occurrences.manifest_bytes != loaded.header.occurrences_manifest_bytes
    {
        return Err("Occurrences diverse dall'identità canonical mainline".into());
    }
    Ok(VerifiedMainlines {
        manifest,
        occurrences,
        canonical_path: loaded.canonical_path,
        manifest_blake3: loaded.manifest_blake3,
        manifest_bytes: loaded.manifest_bytes,
    })
}

pub(in crate::inspector) fn load_verified_collection_chain(
    root: &Path,
    path: &Path,
) -> Result<VerifiedMainlineChain, String> {
    let verified = load_verified(root, path)?;
    let VerifiedMainlines {
        occurrences,
        canonical_path,
        manifest_blake3,
        manifest_bytes,
        ..
    } = verified;
    Ok(VerifiedMainlineChain {
        occurrences,
        canonical_path,
        manifest_blake3,
        manifest_bytes,
    })
}

fn dedup_reason_name(reason: NoncomparabilityReasonV1) -> &'static str {
    match reason {
        NoncomparabilityReasonV1::NonStandardVariant => "non_standard_variant",
        NoncomparabilityReasonV1::UnsupportedVariant => "unsupported_variant",
        NoncomparabilityReasonV1::MissingInitialPosition => "missing_initial_position",
        NoncomparabilityReasonV1::InvalidInitialPosition => "invalid_initial_position",
        NoncomparabilityReasonV1::AmbiguousInitialPosition => "ambiguous_initial_position",
        NoncomparabilityReasonV1::MainlineNotComplete => "mainline_not_complete",
        NoncomparabilityReasonV1::EmptyMainline => "empty_mainline",
        NoncomparabilityReasonV1::NonAppliedMove => "non_applied_move",
    }
}

pub(super) fn load_verified_for_dedup(
    root: &Path,
    path: &Path,
) -> Result<crate::inspector::VerifiedMainlineArtifactInputV1, String> {
    use crate::inspector::{
        VerifiedArtifactIdentityV1, VerifiedMainlineArtifactInputV1, VerifiedMainlineInputV1,
        VerifiedMainlineProvenanceInputV1,
    };

    let verified = load_verified(root, path)?;
    let identities = occurrences::verified_identity_projection(&verified.occurrences)?;

    if identities.len() != verified.manifest.mainlines.len() {
        return Err("Occurrence e canonical mainline hanno cardinalità diversa".into());
    }

    let mut records = Vec::with_capacity(identities.len());

    for (mainline, occurrence) in verified.manifest.mainlines.iter().zip(identities) {
        if mainline.ordinal != occurrence.ordinal
            || mainline.occurrence_id != occurrence.occurrence_id
        {
            return Err("Occurrence e canonical mainline non allineate".into());
        }

        let p = occurrence.provenance;

        records.push(VerifiedMainlineInputV1 {
            occurrence_id: occurrence.occurrence_id,
            canonical_mainline_id: mainline.canonical_mainline_id.clone(),
            comparable: mainline.comparable,
            noncomparability_reasons: mainline
                .noncomparability_reasons
                .iter()
                .copied()
                .map(dedup_reason_name)
                .map(str::to_owned)
                .collect(),
            source_blake3: occurrence.source_blake3,
            source_bytes: occurrence.source_bytes,
            source_game_blake3: occurrence.source_game_blake3,
            game_bytes: occurrence.game_bytes,
            results: mainline
                .results
                .iter()
                .map(|result| result.value.clone())
                .collect(),
            provenance: VerifiedMainlineProvenanceInputV1 {
                occurrences_manifest: VerifiedArtifactIdentityV1 {
                    blake3: verified.manifest.occurrences_manifest_blake3.clone(),
                    bytes: verified.manifest.occurrences_manifest_bytes,
                },
                interpretation_manifest: VerifiedArtifactIdentityV1 {
                    blake3: verified.manifest.interpretation_manifest_blake3.clone(),
                    bytes: verified.manifest.interpretation_manifest_bytes,
                },
                source_manifest: VerifiedArtifactIdentityV1 {
                    blake3: p.source_manifest_blake3,
                    bytes: p.source_manifest_bytes,
                },
                segmentation_manifest: VerifiedArtifactIdentityV1 {
                    blake3: p.segmentation_manifest_blake3,
                    bytes: p.segmentation_manifest_bytes,
                },
                tokenization_manifest: VerifiedArtifactIdentityV1 {
                    blake3: p.tokenization_manifest_blake3,
                    bytes: p.tokenization_manifest_bytes,
                },
                parse_manifest: VerifiedArtifactIdentityV1 {
                    blake3: p.parse_manifest_blake3,
                    bytes: p.parse_manifest_bytes,
                },
                catalog_manifest: VerifiedArtifactIdentityV1 {
                    blake3: p.catalog_manifest_blake3,
                    bytes: p.catalog_manifest_bytes,
                },
            },
        });
    }

    Ok(VerifiedMainlineArtifactInputV1 {
        artifact: VerifiedArtifactIdentityV1 {
            blake3: verified.manifest_blake3,
            bytes: verified.manifest_bytes,
        },
        records,
    })
}

pub(super) fn verify(root: &Path, path: &Path) -> Result<(), String> {
    let loaded = load_streaming_mainlines(root, path, None)?;
    loaded.for_each_verified_mainline(|_| Ok(()))?;
    Ok(())
}

pub(in crate::inspector) use export::export_jsonl;
pub(in crate::inspector) use manifest::CanonicalMainlineV1;
