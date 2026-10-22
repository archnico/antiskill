use super::annotations::{load_verified, AnnotationTargetKindV1};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(super) struct AnnotationTargetIndexRowV1 {
    pub(super) target_kind: AnnotationTargetKindV1,
    pub(super) target_id: String,
    pub(super) annotation_items: usize,
    pub(super) groups: Vec<AnnotationGroupRefV1>,
}

#[derive(Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct AnnotationGroupRefV1 {
    pub(super) group_id: String,
    pub(super) blake3: String,
    pub(super) path: String,
}

pub(super) fn build(
    root: &Path,
    paths: &[PathBuf],
) -> Result<Vec<AnnotationTargetIndexRowV1>, String> {
    if paths.is_empty() {
        return Err("Serve almeno un --annotations PATH".into());
    }
    let mut seen_artifacts = BTreeSet::new();
    let mut rows: BTreeMap<
        (String, String),
        (
            AnnotationTargetKindV1,
            usize,
            BTreeSet<AnnotationGroupRefV1>,
        ),
    > = BTreeMap::new();
    for path in paths {
        let verified = load_verified(root, path)?;
        if !seen_artifacts.insert(verified.blake3.clone()) {
            continue;
        }
        let path = verified.path.to_string_lossy().into_owned();
        let group_ref = AnnotationGroupRefV1 {
            group_id: verified.group.group_id.clone(),
            blake3: verified.blake3,
            path,
        };
        for item in verified.group.items {
            let kind = target_kind_name(item.target_kind).to_owned();
            let entry = rows
                .entry((kind, item.target_id.clone()))
                .or_insert_with(|| (item.target_kind, 0, BTreeSet::new()));
            entry.1 += 1;
            entry.2.insert(AnnotationGroupRefV1 {
                group_id: group_ref.group_id.clone(),
                blake3: group_ref.blake3.clone(),
                path: group_ref.path.clone(),
            });
        }
    }
    Ok(rows
        .into_iter()
        .map(
            |((_kind, target_id), (target_kind, annotation_items, groups))| {
                AnnotationTargetIndexRowV1 {
                    target_kind,
                    target_id,
                    annotation_items,
                    groups: groups.into_iter().collect(),
                }
            },
        )
        .collect())
}

pub(super) fn jsonl(rows: &[AnnotationTargetIndexRowV1]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    for row in rows {
        serde_json::to_writer(&mut output, row)
            .map_err(|error| format!("Serializzazione indice annotazioni: {error}"))?;
        output.push(b'\n');
    }
    Ok(output)
}

fn target_kind_name(kind: AnnotationTargetKindV1) -> &'static str {
    match kind {
        AnnotationTargetKindV1::Game => "game",
        AnnotationTargetKindV1::Move => "move",
        AnnotationTargetKindV1::Position => "position",
        AnnotationTargetKindV1::Event => "event",
        AnnotationTargetKindV1::Person => "person",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_kind_order_is_stable() {
        assert_eq!(
            target_kind_name(AnnotationTargetKindV1::Position),
            "position"
        );
    }
    #[test]
    fn empty_input_fails_closed() {
        let error = build(Path::new("."), &[]).unwrap_err();
        assert!(error.contains("almeno un"));
    }
}
