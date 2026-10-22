use super::source_assertions::{load_verified, AssertionSubjectKindV1};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, PartialEq)]
pub(super) struct AssertionIndexRowV1 {
    pub(super) subject_kind: AssertionSubjectKindV1,
    pub(super) subject_id: String,
    pub(super) predicate: String,
    pub(super) assertions: Vec<AssertionIndexEntryV1>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(super) struct AssertionIndexEntryV1 {
    pub(super) assertion_id: String,
    pub(super) value: Value,
    pub(super) valid_from: Option<String>,
    pub(super) valid_to: Option<String>,
    pub(super) source_key: String,
    pub(super) snapshot_id: String,
    pub(super) locator: Option<String>,
    pub(super) observed_at: Option<String>,
    pub(super) rights: String,
    pub(super) group_id: String,
    pub(super) blake3: String,
    pub(super) path: String,
}

pub(super) fn build(root: &Path, paths: &[PathBuf]) -> Result<Vec<AssertionIndexRowV1>, String> {
    if paths.is_empty() {
        return Err("Serve almeno un --assertions PATH".into());
    }

    let mut seen_artifacts = BTreeSet::new();
    let mut rows: BTreeMap<
        (String, String, String),
        (AssertionSubjectKindV1, Vec<AssertionIndexEntryV1>),
    > = BTreeMap::new();

    for path in paths {
        let verified = load_verified(root, path)?;
        if !seen_artifacts.insert(verified.blake3.clone()) {
            continue;
        }
        let stored_path = verified.path.to_string_lossy().into_owned();
        let group = verified.group;
        for assertion in group.assertions {
            let key = (
                subject_kind_name(assertion.subject_kind).to_owned(),
                assertion.subject_id.clone(),
                assertion.predicate.clone(),
            );
            let entry = rows
                .entry(key)
                .or_insert_with(|| (assertion.subject_kind, Vec::new()));
            entry.1.push(AssertionIndexEntryV1 {
                assertion_id: assertion.assertion_id,
                value: assertion.value,
                valid_from: assertion.valid_from,
                valid_to: assertion.valid_to,
                source_key: group.source_key.clone(),
                snapshot_id: group.snapshot_id.clone(),
                locator: group.locator.clone(),
                observed_at: group.observed_at.clone(),
                rights: group.rights.clone(),
                group_id: group.group_id.clone(),
                blake3: verified.blake3.clone(),
                path: stored_path.clone(),
            });
        }
    }

    Ok(rows
        .into_iter()
        .map(
            |((_kind, subject_id, predicate), (subject_kind, mut assertions))| {
                assertions.sort_by(|a, b| {
                    (
                        &a.source_key,
                        &a.snapshot_id,
                        &a.group_id,
                        &a.assertion_id,
                        &a.blake3,
                        &a.path,
                    )
                        .cmp(&(
                            &b.source_key,
                            &b.snapshot_id,
                            &b.group_id,
                            &b.assertion_id,
                            &b.blake3,
                            &b.path,
                        ))
                });
                AssertionIndexRowV1 {
                    subject_kind,
                    subject_id,
                    predicate,
                    assertions,
                }
            },
        )
        .collect())
}

pub(super) fn jsonl(rows: &[AssertionIndexRowV1]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    for row in rows {
        serde_json::to_writer(&mut output, row)
            .map_err(|error| format!("Serializzazione indice asserzioni: {error}"))?;
        output.push(b'\n');
    }
    Ok(output)
}

fn subject_kind_name(kind: AssertionSubjectKindV1) -> &'static str {
    match kind {
        AssertionSubjectKindV1::Person => "person",
        AssertionSubjectKindV1::Event => "event",
        AssertionSubjectKindV1::Game => "game",
        AssertionSubjectKindV1::Club => "club",
        AssertionSubjectKindV1::Federation => "federation",
        AssertionSubjectKindV1::Organization => "organization",
        AssertionSubjectKindV1::Edition => "edition",
        AssertionSubjectKindV1::Position => "position",
        AssertionSubjectKindV1::Other => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_kind_order_name_is_stable() {
        assert_eq!(subject_kind_name(AssertionSubjectKindV1::Person), "person");
        assert_eq!(
            subject_kind_name(AssertionSubjectKindV1::Position),
            "position"
        );
    }

    #[test]
    fn empty_input_fails_closed() {
        let error = build(Path::new("."), &[]).unwrap_err();
        assert!(error.contains("almeno un"));
    }
}
