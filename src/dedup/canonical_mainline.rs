use crate::inspector::{
    load_verified_mainlines_for_dedup, VerifiedArtifactIdentityV1, VerifiedMainlineArtifactInputV1,
    VerifiedMainlineInputV1,
};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

const REASONS: [&str; 8] = [
    "non_standard_variant",
    "unsupported_variant",
    "missing_initial_position",
    "invalid_initial_position",
    "ambiguous_initial_position",
    "mainline_not_complete",
    "empty_mainline",
    "non_applied_move",
];

pub(super) fn load_verified_inputs(
    root: &Path,
    paths: &[PathBuf],
) -> Result<Vec<VerifiedMainlineArtifactInputV1>, String> {
    let inputs = paths
        .iter()
        .map(|path| load_verified_mainlines_for_dedup(root, path))
        .collect::<Result<Vec<_>, _>>()?;

    validate_verified_inputs(inputs)
}

pub(super) fn validate_verified_inputs(
    mut inputs: Vec<VerifiedMainlineArtifactInputV1>,
) -> Result<Vec<VerifiedMainlineArtifactInputV1>, String> {
    for input in &inputs {
        validate_identity("artefatto canonical mainline", &input.artifact)?;
        for record in &input.records {
            validate_record(record)?;
        }
    }

    inputs.sort_by(|left, right| {
        left.artifact
            .blake3
            .cmp(&right.artifact.blake3)
            .then(left.artifact.bytes.cmp(&right.artifact.bytes))
    });

    for pair in inputs.windows(2) {
        if pair[0].artifact == pair[1].artifact {
            return Err(format!(
                "Artefatto canonical mainline duplicato: {} {}",
                pair[0].artifact.blake3, pair[0].artifact.bytes
            ));
        }
    }

    let mut occurrence_ids = HashSet::new();
    for input in &inputs {
        for record in &input.records {
            if !occurrence_ids.insert(record.occurrence_id.clone()) {
                return Err(format!(
                    "occurrence_id presente in più artefatti canonical mainline: {}",
                    record.occurrence_id
                ));
            }
        }
    }

    Ok(inputs)
}

fn validate_record(record: &VerifiedMainlineInputV1) -> Result<(), String> {
    validate_hash("occurrence_id", &record.occurrence_id)?;
    validate_hash("source_blake3", &record.source_blake3)?;
    validate_hash("source_game_blake3", &record.source_game_blake3)?;

    if record.source_bytes == 0 || record.game_bytes == 0 {
        return Err("Identità byte occurrence vuota".into());
    }

    if let Some(id) = &record.canonical_mainline_id {
        validate_hash("canonical_mainline_id", id)?;
    }

    if record.comparable != record.canonical_mainline_id.is_some()
        || record.comparable != record.noncomparability_reasons.is_empty()
    {
        return Err("Confrontabilità proiezione Dedup incoerente".into());
    }

    let mut previous_reason = None;
    for reason in &record.noncomparability_reasons {
        let index = REASONS
            .iter()
            .position(|known| *known == reason)
            .ok_or_else(|| format!("Motivo non comparabilità sconosciuto: {reason}"))?;

        if previous_reason.is_some_and(|previous| index <= previous) {
            return Err("Ordine motivi non comparabilità non canonico".into());
        }
        previous_reason = Some(index);
    }

    for result in &record.results {
        if !matches!(result.as_str(), "1-0" | "0-1" | "1/2-1/2" | "*") {
            return Err(format!("Result proiettato non valido: {result}"));
        }
    }

    for (label, identity) in [
        (
            "occurrences_manifest",
            &record.provenance.occurrences_manifest,
        ),
        (
            "interpretation_manifest",
            &record.provenance.interpretation_manifest,
        ),
        ("source_manifest", &record.provenance.source_manifest),
        (
            "segmentation_manifest",
            &record.provenance.segmentation_manifest,
        ),
        (
            "tokenization_manifest",
            &record.provenance.tokenization_manifest,
        ),
        ("parse_manifest", &record.provenance.parse_manifest),
        ("catalog_manifest", &record.provenance.catalog_manifest),
    ] {
        validate_identity(label, identity)?;
    }

    Ok(())
}

fn validate_identity(label: &str, identity: &VerifiedArtifactIdentityV1) -> Result<(), String> {
    validate_hash(label, &identity.blake3)?;
    if identity.bytes == 0 {
        return Err(format!("Identità artefatto vuota: {label}"));
    }
    Ok(())
}

fn validate_hash(label: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("Hash BLAKE3 non valido: {label}"));
    }
    Ok(())
}

pub(super) const DEDUP_PROFILE: &str = "same_canonical_mainline_v1";
const CANONICAL_MAINLINE_PROFILE: &str = "antiskill_canonical_mainline_v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReportOccurrenceV1 {
    pub(super) mainline_artifact: VerifiedArtifactIdentityV1,
    pub(super) record: VerifiedMainlineInputV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ExactBytesGroupV1 {
    pub(super) source_game_blake3: String,
    pub(super) game_bytes: u64,
    pub(super) occurrence_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CanonicalMainlineGroupV1 {
    pub(super) group_id: String,
    pub(super) canonical_mainline_id: String,
    pub(super) occurrence_count: u64,
    pub(super) logical_source_count: u64,
    pub(super) byte_variant_count: u64,
    pub(super) same_mainline_different_bytes: bool,
    pub(super) exact_byte_groups: Vec<ExactBytesGroupV1>,
    pub(super) distinct_results: Vec<Vec<String>>,
    pub(super) result_conflict: bool,
    pub(super) members: Vec<ReportOccurrenceV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GroupSizeV1 {
    pub(super) size: u64,
    pub(super) groups: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CanonicalMainlineReportV1 {
    pub(super) input_artifacts: Vec<VerifiedArtifactIdentityV1>,
    pub(super) comparable_occurrences: u64,
    pub(super) noncomparables: Vec<ReportOccurrenceV1>,
    pub(super) groups: Vec<CanonicalMainlineGroupV1>,
    pub(super) group_size_distribution: Vec<GroupSizeV1>,
}

fn group_id(canonical_mainline_id: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"antiskill_same_canonical_mainline_v1\0");
    hasher.update(CANONICAL_MAINLINE_PROFILE.as_bytes());
    hasher.update(b"\0");
    hasher.update(canonical_mainline_id.as_bytes());
    hasher.update(b"\n");
    hasher.finalize().to_hex().to_string()
}

pub(super) fn build_report(
    inputs: &[VerifiedMainlineArtifactInputV1],
) -> Result<CanonicalMainlineReportV1, String> {
    let input_artifacts = inputs
        .iter()
        .map(|input| input.artifact.clone())
        .collect::<Vec<_>>();

    let mut comparable_occurrences = 0_u64;
    let mut by_mainline = BTreeMap::<String, Vec<ReportOccurrenceV1>>::new();
    let mut noncomparables = Vec::new();

    for input in inputs {
        for record in &input.records {
            let occurrence = ReportOccurrenceV1 {
                mainline_artifact: input.artifact.clone(),
                record: record.clone(),
            };

            if record.comparable {
                comparable_occurrences = comparable_occurrences
                    .checked_add(1)
                    .ok_or("Conteggio occurrence confrontabili oltre u64")?;

                let canonical_id = record
                    .canonical_mainline_id
                    .as_ref()
                    .ok_or("Occurrence confrontabile senza canonical_mainline_id")?
                    .clone();

                by_mainline
                    .entry(canonical_id)
                    .or_default()
                    .push(occurrence);
            } else {
                noncomparables.push(occurrence);
            }
        }
    }

    noncomparables
        .sort_by(|left, right| left.record.occurrence_id.cmp(&right.record.occurrence_id));

    let mut groups = Vec::new();

    for (canonical_mainline_id, mut members) in by_mainline {
        if members.len() < 2 {
            continue;
        }

        members.sort_by(|left, right| left.record.occurrence_id.cmp(&right.record.occurrence_id));

        let logical_sources = members
            .iter()
            .map(|member| {
                (
                    member.record.source_blake3.clone(),
                    member.record.source_bytes,
                )
            })
            .collect::<BTreeSet<_>>();

        let byte_variants = members
            .iter()
            .map(|member| {
                (
                    member.record.source_game_blake3.clone(),
                    member.record.game_bytes,
                )
            })
            .collect::<BTreeSet<_>>();

        let mut exact = BTreeMap::<(String, u64), Vec<String>>::new();
        for member in &members {
            exact
                .entry((
                    member.record.source_game_blake3.clone(),
                    member.record.game_bytes,
                ))
                .or_default()
                .push(member.record.occurrence_id.clone());
        }

        let exact_byte_groups = exact
            .into_iter()
            .filter_map(|((source_game_blake3, game_bytes), mut occurrence_ids)| {
                if occurrence_ids.len() < 2 {
                    return None;
                }
                occurrence_ids.sort();
                Some(ExactBytesGroupV1 {
                    source_game_blake3,
                    game_bytes,
                    occurrence_ids,
                })
            })
            .collect::<Vec<_>>();

        let distinct_results = members
            .iter()
            .map(|member| member.record.results.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let occurrence_count =
            u64::try_from(members.len()).map_err(|_| "Dimensione gruppo oltre u64")?;
        let logical_source_count =
            u64::try_from(logical_sources.len()).map_err(|_| "Numero sorgenti oltre u64")?;
        let byte_variant_count =
            u64::try_from(byte_variants.len()).map_err(|_| "Numero varianti byte oltre u64")?;

        groups.push(CanonicalMainlineGroupV1 {
            group_id: group_id(&canonical_mainline_id),
            canonical_mainline_id,
            occurrence_count,
            logical_source_count,
            byte_variant_count,
            same_mainline_different_bytes: byte_variant_count > 1,
            exact_byte_groups,
            result_conflict: distinct_results.len() > 1,
            distinct_results,
            members,
        });
    }

    groups.sort_by(|left, right| left.group_id.cmp(&right.group_id));

    let mut sizes = BTreeMap::<u64, u64>::new();
    for group in &groups {
        let count = sizes.entry(group.occurrence_count).or_default();
        *count = count
            .checked_add(1)
            .ok_or("Distribuzione dimensione gruppi oltre u64")?;
    }

    let group_size_distribution = sizes
        .into_iter()
        .map(|(size, groups)| GroupSizeV1 { size, groups })
        .collect();

    Ok(CanonicalMainlineReportV1 {
        input_artifacts,
        comparable_occurrences,
        noncomparables,
        groups,
        group_size_distribution,
    })
}

fn grouped_occurrences(report: &CanonicalMainlineReportV1) -> Result<u64, String> {
    report.groups.iter().try_fold(0_u64, |total, group| {
        total
            .checked_add(group.occurrence_count)
            .ok_or_else(|| "Conteggio occurrence raggruppate oltre u64".to_owned())
    })
}

fn artifact_text(identity: &VerifiedArtifactIdentityV1) -> String {
    format!("{}:{}", identity.blake3, identity.bytes)
}

fn provenance_text(provenance: &crate::inspector::VerifiedMainlineProvenanceInputV1) -> String {
    [
        ("occurrences", &provenance.occurrences_manifest),
        ("interpretation", &provenance.interpretation_manifest),
        ("source", &provenance.source_manifest),
        ("segmentation", &provenance.segmentation_manifest),
        ("tokenization", &provenance.tokenization_manifest),
        ("parse", &provenance.parse_manifest),
        ("catalog", &provenance.catalog_manifest),
    ]
    .into_iter()
    .map(|(name, identity)| format!("{name}={}", artifact_text(identity)))
    .collect::<Vec<_>>()
    .join(";")
}

pub(super) fn render_text(report: &CanonicalMainlineReportV1) -> Result<String, String> {
    use std::fmt::Write as _;

    let grouped = grouped_occurrences(report)?;
    let artifacts = report
        .input_artifacts
        .iter()
        .map(artifact_text)
        .collect::<Vec<_>>()
        .join(",");
    let distribution = report
        .group_size_distribution
        .iter()
        .map(|entry| format!("{}:{}", entry.size, entry.groups))
        .collect::<Vec<_>>()
        .join(",");

    let mut output = String::new();

    writeln!(output, "DEDUP_PROFILE\t{DEDUP_PROFILE}").expect("String write");
    writeln!(
        output,
        "INPUT_ARTIFACTS\t{}\t{}",
        report.input_artifacts.len(),
        artifacts
    )
    .expect("String write");
    writeln!(
        output,
        "COMPARABLE_OCCURRENCES\t{}",
        report.comparable_occurrences
    )
    .expect("String write");
    writeln!(
        output,
        "NONCOMPARABLE_OCCURRENCES\t{}",
        report.noncomparables.len()
    )
    .expect("String write");
    writeln!(output, "GROUPS\t{}", report.groups.len()).expect("String write");
    writeln!(output, "GROUPED_OCCURRENCES\t{grouped}").expect("String write");
    writeln!(output, "GROUP_SIZE_DISTRIBUTION\t{distribution}").expect("String write");

    for group in &report.groups {
        let distinct_results =
            serde_json::to_string(&group.distinct_results).map_err(|error| error.to_string())?;

        writeln!(
            output,
            "GROUP\tgroup_id={}\tcanonical_mainline_id={}\toccurrences={}\tlogical_sources={}\tbyte_variants={}\tsame_mainline_different_bytes={}\tdistinct_results={}\tresult_conflict={}",
            group.group_id,
            group.canonical_mainline_id,
            group.occurrence_count,
            group.logical_source_count,
            group.byte_variant_count,
            group.same_mainline_different_bytes,
            distinct_results,
            group.result_conflict,
        )
        .expect("String write");

        for exact in &group.exact_byte_groups {
            writeln!(
                output,
                "EXACT_BYTES\tgroup_id={}\tsource_game_blake3={}\tgame_bytes={}\toccurrences={}\toccurrence_ids={}",
                group.group_id,
                exact.source_game_blake3,
                exact.game_bytes,
                exact.occurrence_ids.len(),
                exact.occurrence_ids.join(","),
            )
            .expect("String write");
        }

        for member in &group.members {
            let results = if member.record.results.is_empty() {
                "-".to_owned()
            } else {
                member.record.results.join(",")
            };

            writeln!(
                output,
                "OCCURRENCE\tgroup_id={}\toccurrence_id={}\tsource_blake3={}\tsource_bytes={}\tsource_game_blake3={}\tgame_bytes={}\tresults={}\tmainline_artifact={}\tprovenance={}",
                group.group_id,
                member.record.occurrence_id,
                member.record.source_blake3,
                member.record.source_bytes,
                member.record.source_game_blake3,
                member.record.game_bytes,
                results,
                artifact_text(&member.mainline_artifact),
                provenance_text(&member.record.provenance),
            )
            .expect("String write");
        }
    }

    for member in &report.noncomparables {
        let results = if member.record.results.is_empty() {
            "-".to_owned()
        } else {
            member.record.results.join(",")
        };

        writeln!(
            output,
            "NONCOMPARABLE\toccurrence_id={}\tsource_blake3={}\tsource_bytes={}\tsource_game_blake3={}\tgame_bytes={}\treasons={}\tresults={}\tmainline_artifact={}\tprovenance={}",
            member.record.occurrence_id,
            member.record.source_blake3,
            member.record.source_bytes,
            member.record.source_game_blake3,
            member.record.game_bytes,
            member.record.noncomparability_reasons.join(","),
            results,
            artifact_text(&member.mainline_artifact),
            provenance_text(&member.record.provenance),
        )
        .expect("String write");
    }

    writeln!(output, "DEDUP_REPORT\tPASS").expect("String write");
    Ok(output)
}

fn artifact_json(identity: &VerifiedArtifactIdentityV1) -> serde_json::Value {
    serde_json::json!({
        "blake3": &identity.blake3,
        "bytes": identity.bytes,
    })
}

fn provenance_json(
    provenance: &crate::inspector::VerifiedMainlineProvenanceInputV1,
) -> serde_json::Value {
    serde_json::json!({
        "occurrences_manifest": artifact_json(&provenance.occurrences_manifest),
        "interpretation_manifest": artifact_json(&provenance.interpretation_manifest),
        "source_manifest": artifact_json(&provenance.source_manifest),
        "segmentation_manifest": artifact_json(&provenance.segmentation_manifest),
        "tokenization_manifest": artifact_json(&provenance.tokenization_manifest),
        "parse_manifest": artifact_json(&provenance.parse_manifest),
        "catalog_manifest": artifact_json(&provenance.catalog_manifest),
    })
}

fn occurrence_json(member: &ReportOccurrenceV1) -> serde_json::Value {
    serde_json::json!({
        "occurrence_id": &member.record.occurrence_id,
        "source": {
            "blake3": &member.record.source_blake3,
            "bytes": member.record.source_bytes,
        },
        "exact_game_bytes": {
            "blake3": &member.record.source_game_blake3,
            "bytes": member.record.game_bytes,
        },
        "results": &member.record.results,
        "mainline_artifact": artifact_json(&member.mainline_artifact),
        "provenance": provenance_json(&member.record.provenance),
    })
}

pub(super) fn render_jsonl(report: &CanonicalMainlineReportV1) -> Result<String, String> {
    let grouped = grouped_occurrences(report)?;
    let mut records = Vec::new();

    records.push(serde_json::json!({
        "record_type": "summary",
        "schema_version": 1,
        "dedup_profile": DEDUP_PROFILE,
        "canonical_mainline_profile": CANONICAL_MAINLINE_PROFILE,
        "input_artifacts": report.input_artifacts.iter().map(artifact_json).collect::<Vec<_>>(),
        "comparable_occurrences": report.comparable_occurrences,
        "noncomparable_occurrences": report.noncomparables.len(),
        "groups": report.groups.len(),
        "grouped_occurrences": grouped,
    }));

    for entry in &report.group_size_distribution {
        records.push(serde_json::json!({
            "record_type": "group_size",
            "size": entry.size,
            "groups": entry.groups,
        }));
    }

    for group in &report.groups {
        records.push(serde_json::json!({
            "record_type": "group",
            "group_id": &group.group_id,
            "canonical_mainline_id": &group.canonical_mainline_id,
            "occurrence_count": group.occurrence_count,
            "logical_source_count": group.logical_source_count,
            "byte_variant_count": group.byte_variant_count,
            "same_mainline_different_bytes": group.same_mainline_different_bytes,
            "exact_byte_subgroups": group.exact_byte_groups.iter().map(|exact| {
                serde_json::json!({
                    "source_game_blake3": &exact.source_game_blake3,
                    "game_bytes": exact.game_bytes,
                    "occurrence_ids": &exact.occurrence_ids,
                })
            }).collect::<Vec<_>>(),
            "distinct_results": &group.distinct_results,
            "result_conflict": group.result_conflict,
            "members": group.members.iter().map(occurrence_json).collect::<Vec<_>>(),
        }));
    }

    for member in &report.noncomparables {
        records.push(serde_json::json!({
            "record_type": "non_comparable",
            "occurrence_id": &member.record.occurrence_id,
            "reasons": &member.record.noncomparability_reasons,
            "source": {
                "blake3": &member.record.source_blake3,
                "bytes": member.record.source_bytes,
            },
            "exact_game_bytes": {
                "blake3": &member.record.source_game_blake3,
                "bytes": member.record.game_bytes,
            },
            "results": &member.record.results,
            "mainline_artifact": artifact_json(&member.mainline_artifact),
            "provenance": provenance_json(&member.record.provenance),
        }));
    }

    let mut output = records
        .into_iter()
        .map(|record| serde_json::to_string(&record).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    output.push('\n');

    Ok(output)
}

#[cfg(test)]
mod aggregation_tests {
    use super::*;
    use crate::inspector::VerifiedMainlineProvenanceInputV1;

    fn hash(character: char) -> String {
        character.to_string().repeat(64)
    }

    fn artifact(character: char, bytes: u64) -> VerifiedArtifactIdentityV1 {
        VerifiedArtifactIdentityV1 {
            blake3: hash(character),
            bytes,
        }
    }

    fn provenance() -> VerifiedMainlineProvenanceInputV1 {
        VerifiedMainlineProvenanceInputV1 {
            occurrences_manifest: artifact('1', 101),
            interpretation_manifest: artifact('2', 102),
            source_manifest: artifact('3', 103),
            segmentation_manifest: artifact('4', 104),
            tokenization_manifest: artifact('5', 105),
            parse_manifest: artifact('6', 106),
            catalog_manifest: artifact('7', 107),
        }
    }

    fn record(
        occurrence: char,
        canonical: Option<char>,
        source: char,
        source_game: char,
        game_bytes: u64,
        results: &[&str],
    ) -> VerifiedMainlineInputV1 {
        VerifiedMainlineInputV1 {
            occurrence_id: hash(occurrence),
            canonical_mainline_id: canonical.map(hash),
            comparable: canonical.is_some(),
            noncomparability_reasons: if canonical.is_some() {
                Vec::new()
            } else {
                vec!["empty_mainline".into()]
            },
            source_blake3: hash(source),
            source_bytes: 500,
            source_game_blake3: hash(source_game),
            game_bytes,
            results: results.iter().map(|value| (*value).to_owned()).collect(),
            provenance: provenance(),
        }
    }

    #[test]
    fn groups_mainlines_and_preserves_byte_and_result_distinctions() {
        let input = VerifiedMainlineArtifactInputV1 {
            artifact: artifact('8', 800),
            records: vec![
                record('1', Some('a'), '1', 'c', 20, &["1-0"]),
                record('2', Some('a'), '2', 'c', 20, &["0-1"]),
                record('3', Some('a'), '3', 'd', 21, &["1-0"]),
                record('4', Some('b'), '4', 'e', 22, &["*"]),
                record('5', None, '5', 'f', 23, &["*"]),
            ],
        };

        let report = build_report(&[input]).unwrap();

        assert_eq!(report.comparable_occurrences, 4);
        assert_eq!(report.noncomparables.len(), 1);
        assert_eq!(report.groups.len(), 1);

        let group = &report.groups[0];
        assert_eq!(group.canonical_mainline_id, hash('a'));
        assert_eq!(group.occurrence_count, 3);
        assert_eq!(group.logical_source_count, 3);
        assert_eq!(group.byte_variant_count, 2);
        assert!(group.same_mainline_different_bytes);
        assert!(group.result_conflict);
        assert_eq!(group.distinct_results.len(), 2);
        assert_eq!(group.exact_byte_groups.len(), 1);
        assert_eq!(group.exact_byte_groups[0].source_game_blake3, hash('c'));
        assert_eq!(group.exact_byte_groups[0].game_bytes, 20);
        assert_eq!(group.exact_byte_groups[0].occurrence_ids.len(), 2);
        assert_eq!(
            report.group_size_distribution,
            vec![GroupSizeV1 { size: 3, groups: 1 }]
        );
    }

    #[test]
    fn group_id_uses_the_frozen_preimage() {
        let canonical = hash('a');

        let mut expected = blake3::Hasher::new();
        expected.update(b"antiskill_same_canonical_mainline_v1\0antiskill_canonical_mainline_v1\0");
        expected.update(canonical.as_bytes());
        expected.update(b"\n");

        assert_eq!(
            group_id(&canonical),
            expected.finalize().to_hex().to_string()
        );
    }
}
