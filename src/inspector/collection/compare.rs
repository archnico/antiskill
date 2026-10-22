use super::{manifest::CollectionManifest, scan};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceIdentity {
    blake3: String,
    bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotComparison {
    pub(crate) unchanged: u64,
    pub(crate) modified: Vec<String>,
    pub(crate) added: Vec<String>,
    pub(crate) removed: Vec<String>,
}

pub(super) fn active_snapshot(root: &Path, input: &Path) -> Result<SnapshotComparison, String> {
    let active = super::resolve_active_collection(root)?;
    let common = super::load_common(root, &active)?;
    super::load_verified_summary(root, &active)?;
    let historical = historical_projection(&common.manifest)?;
    let current = current_projection(input)?;
    compare(&historical, &current)
}

fn historical_projection(
    manifest: &CollectionManifest,
) -> Result<BTreeMap<String, SourceIdentity>, String> {
    let mut projection = BTreeMap::new();
    match manifest {
        CollectionManifest::V1(manifest) => {
            for source in &manifest.sources {
                insert_observations(
                    &mut projection,
                    &source.source_blake3,
                    source.source_bytes,
                    source
                        .observations
                        .iter()
                        .map(|observation| observation.input_relative_path.as_str()),
                )?;
            }
        }
        CollectionManifest::V2(manifest) => {
            for source in &manifest.sources {
                insert_observations(
                    &mut projection,
                    &source.source_blake3,
                    source.source_bytes,
                    source
                        .observations
                        .iter()
                        .map(|observation| observation.input_relative_path.as_str()),
                )?;
            }
        }
        CollectionManifest::V3(manifest) => {
            for source in &manifest.sources {
                insert_observations(
                    &mut projection,
                    &source.source_blake3,
                    source.source_bytes,
                    source
                        .observations
                        .iter()
                        .map(|observation| observation.input_relative_path.as_str()),
                )?;
            }
        }
    }
    Ok(projection)
}

fn insert_observations<'a>(
    projection: &mut BTreeMap<String, SourceIdentity>,
    blake3: &str,
    bytes: u64,
    paths: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    let identity = SourceIdentity {
        blake3: blake3.to_owned(),
        bytes,
    };
    for path in paths {
        if projection
            .insert(path.to_owned(), identity.clone())
            .is_some_and(|existing| existing != identity)
        {
            return Err(format!(
                "Snapshot attivo ambiguo: il percorso {path} ha identità sorgente confliggenti"
            ));
        }
    }
    Ok(())
}

fn current_projection(input: &Path) -> Result<BTreeMap<String, SourceIdentity>, String> {
    let scanned = scan::directory(input)?;
    if !scanned.failures.is_empty() {
        let details = scanned
            .failures
            .iter()
            .map(|failure| format!("{}: {}", failure.relative, failure.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("Scansione PGN non completata: {details}"));
    }
    let mut projection = BTreeMap::new();
    for candidate in scanned.candidates {
        let data = fs::read(&candidate.path).map_err(|error| {
            format!("Impossibile leggere il PGN {}: {error}", candidate.relative)
        })?;
        let identity = SourceIdentity {
            blake3: blake3::hash(&data).to_hex().to_string(),
            bytes: u64::try_from(data.len()).map_err(|_| "File PGN oltre u64")?,
        };
        if projection
            .insert(candidate.relative.clone(), identity.clone())
            .is_some_and(|existing| existing != identity)
        {
            return Err(format!(
                "Scansione PGN ambigua: il percorso {} ha identità confliggenti",
                candidate.relative
            ));
        }
    }
    Ok(projection)
}

fn compare(
    historical: &BTreeMap<String, SourceIdentity>,
    current: &BTreeMap<String, SourceIdentity>,
) -> Result<SnapshotComparison, String> {
    let paths = historical
        .keys()
        .chain(current.keys())
        .collect::<BTreeSet<_>>();
    let mut result = SnapshotComparison {
        unchanged: 0,
        modified: Vec::new(),
        added: Vec::new(),
        removed: Vec::new(),
    };
    for path in paths {
        match (historical.get(path), current.get(path)) {
            (Some(before), Some(after)) if before == after => {
                result.unchanged = result
                    .unchanged
                    .checked_add(1)
                    .ok_or("Conteggio file invariati oltre u64")?;
            }
            (Some(_), Some(_)) => result.modified.push(path.clone()),
            (None, Some(_)) => result.added.push(path.clone()),
            (Some(_), None) => result.removed.push(path.clone()),
            (None, None) => return Err("Proiezione confronto incoerente".into()),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(value: &str, bytes: u64) -> SourceIdentity {
        SourceIdentity {
            blake3: value.repeat(64),
            bytes,
        }
    }

    #[test]
    fn comparison_is_path_keyed_sorted_and_does_not_infer_renames() {
        let historical = BTreeMap::from([
            ("a.pgn".into(), identity("a", 1)),
            ("same.pgn".into(), identity("s", 2)),
            ("z.pgn".into(), identity("z", 3)),
        ]);
        let current = BTreeMap::from([
            ("b.pgn".into(), identity("a", 1)),
            ("same.pgn".into(), identity("s", 2)),
            ("z.pgn".into(), identity("x", 3)),
        ]);
        assert_eq!(
            compare(&historical, &current).unwrap(),
            SnapshotComparison {
                unchanged: 1,
                modified: vec!["z.pgn".into()],
                added: vec!["b.pgn".into()],
                removed: vec!["a.pgn".into()],
            }
        );
    }

    #[test]
    fn conflicting_projection_fails_closed_but_identical_repeat_is_allowed() {
        let mut projection = BTreeMap::new();
        insert_observations(&mut projection, &"a".repeat(64), 1, ["x.pgn"].into_iter()).unwrap();
        insert_observations(&mut projection, &"a".repeat(64), 1, ["x.pgn"].into_iter()).unwrap();
        assert!(
            insert_observations(&mut projection, &"b".repeat(64), 1, ["x.pgn"].into_iter())
                .unwrap_err()
                .contains("confliggenti")
        );
    }
}
