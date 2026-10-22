use super::{HumanCanonicalGroup, HumanDuplicatesReport, HumanExactGroup, HumanGame};
use crate::archive::game_id::PublicGameIdV1;
use crate::archive::search::human_values;
use crate::inspector::ProductDuplicateRowV1;
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeSet, BinaryHeap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const RUN_TARGET_BYTES: usize = 16 * 1024 * 1024;
const MERGE_FANIN: usize = 64;

#[derive(Clone, Copy)]
enum SortKind {
    Exact,
    Canonical,
}

struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new(root: &Path) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "Orologio di sistema precedente a UNIX_EPOCH")?
            .as_nanos();
        let base = root.join("raw/tmp");
        fs::create_dir_all(&base)
            .map_err(|error| format!("Creazione area temporanea duplicates: {error}"))?;
        let path = base.join(format!(
            "antiskill-duplicates-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path)
            .map_err(|error| format!("Creazione temporanei duplicates: {error}"))?;
        Ok(Self { path })
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn compare_rows(
    left: &ProductDuplicateRowV1,
    right: &ProductDuplicateRowV1,
    kind: SortKind,
) -> Ordering {
    match kind {
        SortKind::Exact => left
            .source_game_blake3
            .cmp(&right.source_game_blake3)
            .then(left.game_bytes.cmp(&right.game_bytes))
            .then(left.occurrence_id.cmp(&right.occurrence_id)),
        SortKind::Canonical => left
            .canonical_mainline_id
            .as_deref()
            .unwrap_or("")
            .cmp(right.canonical_mainline_id.as_deref().unwrap_or(""))
            .then(left.occurrence_id.cmp(&right.occurrence_id)),
    }
}

fn heap_key(row: &ProductDuplicateRowV1, kind: SortKind) -> Result<String, String> {
    match kind {
        SortKind::Exact => Ok(format!(
            "{}\0{:020}\0{}",
            row.source_game_blake3, row.game_bytes, row.occurrence_id
        )),
        SortKind::Canonical => Ok(format!(
            "{}\0{}",
            row.canonical_mainline_id
                .as_deref()
                .ok_or("Occurrence confrontabile senza canonical_mainline_id")?,
            row.occurrence_id
        )),
    }
}

fn write_row(writer: &mut BufWriter<File>, row: &ProductDuplicateRowV1) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, row)
        .map_err(|error| format!("Scrittura spool duplicates: {error}"))?;
    writer
        .write_all(b"\n")
        .map_err(|error| format!("Scrittura spool duplicates: {error}"))
}

fn read_row(reader: &mut BufReader<File>) -> Result<Option<ProductDuplicateRowV1>, String> {
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .map_err(|error| format!("Lettura spool duplicates: {error}"))?;
    if bytes == 0 {
        return Ok(None);
    }
    serde_json::from_str(line.trim_end_matches(['\r', '\n']))
        .map(Some)
        .map_err(|error| format!("Record spool duplicates non valido: {error}"))
}

fn flush_run(
    rows: &mut Vec<ProductDuplicateRowV1>,
    kind: SortKind,
    directory: &Path,
    prefix: &str,
    index: usize,
) -> Result<PathBuf, String> {
    rows.sort_by(|left, right| compare_rows(left, right, kind));
    let path = directory.join(format!("{prefix}-{index:06}.jsonl"));
    let file = File::create(&path).map_err(|error| format!("Creazione run duplicates: {error}"))?;
    let mut writer = BufWriter::new(file);
    for row in rows.iter() {
        write_row(&mut writer, row)?;
    }
    writer
        .flush()
        .map_err(|error| format!("Flush run duplicates: {error}"))?;
    rows.clear();
    Ok(path)
}

fn create_runs(
    raw: &Path,
    kind: SortKind,
    directory: &Path,
    prefix: &str,
) -> Result<Vec<PathBuf>, String> {
    let file = File::open(raw).map_err(|error| format!("Apertura spool duplicates: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut rows = Vec::new();
    let mut logical_bytes = 0_usize;
    let mut runs = Vec::new();
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| format!("Lettura spool duplicates: {error}"))?;
        if bytes == 0 {
            break;
        }
        let row: ProductDuplicateRowV1 = serde_json::from_str(line.trim_end_matches(['\r', '\n']))
            .map_err(|error| format!("Record spool duplicates non valido: {error}"))?;
        if matches!(kind, SortKind::Canonical) && !row.comparable {
            continue;
        }
        if row.comparable != row.canonical_mainline_id.is_some() {
            return Err("Confrontabilità duplicate row incoerente".into());
        }
        logical_bytes = logical_bytes
            .checked_add(bytes)
            .ok_or("Dimensione chunk duplicates oltre usize")?;
        rows.push(row);
        if logical_bytes >= RUN_TARGET_BYTES {
            let index = runs.len();
            runs.push(flush_run(&mut rows, kind, directory, prefix, index)?);
            logical_bytes = 0;
        }
    }
    if !rows.is_empty() {
        let index = runs.len();
        runs.push(flush_run(&mut rows, kind, directory, prefix, index)?);
    }
    compact_runs(runs, kind, directory, prefix)
}

fn merge_rows<F>(paths: &[PathBuf], kind: SortKind, mut on_row: F) -> Result<(), String>
where
    F: FnMut(ProductDuplicateRowV1) -> Result<(), String>,
{
    let mut readers = paths
        .iter()
        .map(|path| {
            File::open(path)
                .map(BufReader::new)
                .map_err(|error| format!("Apertura run duplicates: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut current = Vec::with_capacity(readers.len());
    let mut heap = BinaryHeap::<Reverse<(String, usize)>>::new();
    for (index, reader) in readers.iter_mut().enumerate() {
        let row = read_row(reader)?;
        if let Some(value) = &row {
            heap.push(Reverse((heap_key(value, kind)?, index)));
        }
        current.push(row);
    }
    while let Some(Reverse((_key, index))) = heap.pop() {
        let row = current[index]
            .take()
            .ok_or("Merge duplicates senza record corrente")?;
        on_row(row)?;
        let next = read_row(&mut readers[index])?;
        if let Some(value) = &next {
            heap.push(Reverse((heap_key(value, kind)?, index)));
        }
        current[index] = next;
    }
    Ok(())
}

fn compact_runs(
    mut runs: Vec<PathBuf>,
    kind: SortKind,
    directory: &Path,
    prefix: &str,
) -> Result<Vec<PathBuf>, String> {
    let mut round = 0_usize;
    while runs.len() > MERGE_FANIN {
        let mut next = Vec::new();
        for (batch_index, batch) in runs.chunks(MERGE_FANIN).enumerate() {
            let path = directory.join(format!("{prefix}-merge-{round:03}-{batch_index:06}.jsonl"));
            let file = File::create(&path)
                .map_err(|error| format!("Creazione merge duplicates: {error}"))?;
            let mut writer = BufWriter::new(file);
            merge_rows(batch, kind, |row| write_row(&mut writer, &row))?;
            writer
                .flush()
                .map_err(|error| format!("Flush merge duplicates: {error}"))?;
            next.push(path);
        }
        for path in &runs {
            fs::remove_file(path).map_err(|error| format!("Pulizia run duplicates: {error}"))?;
        }
        runs = next;
        round = round.checked_add(1).ok_or("Round merge oltre usize")?;
    }
    Ok(runs)
}

fn human_game(row: &ProductDuplicateRowV1) -> Result<HumanGame, String> {
    Ok(HumanGame {
        id: PublicGameIdV1::from_occurrence_id(&row.occurrence_id)?,
        date: human_values(&row.date),
        white: human_values(&row.white),
        black: human_values(&row.black),
        result: human_values(&row.result),
        event: human_values(&row.event),
    })
}

fn finish_exact_group(
    rows: &mut Vec<ProductDuplicateRowV1>,
    groups: &mut Vec<HumanExactGroup>,
) -> Result<(), String> {
    if rows.len() < 2 {
        rows.clear();
        return Ok(());
    }
    for pair in rows.windows(2) {
        if pair[0].occurrence_id == pair[1].occurrence_id {
            return Err("occurrence_id duplicato nella proiezione exact duplicates".into());
        }
    }
    let mut members = rows.iter().map(human_game).collect::<Result<Vec<_>, _>>()?;
    members.sort_by(|left, right| left.id.cmp(&right.id));
    groups.push(HumanExactGroup { members });
    rows.clear();
    Ok(())
}

fn exact_groups(runs: &[PathBuf]) -> Result<Vec<HumanExactGroup>, String> {
    let mut groups = Vec::new();
    let mut key: Option<(String, u64)> = None;
    let mut rows = Vec::new();
    merge_rows(runs, SortKind::Exact, |row| {
        let next = (row.source_game_blake3.clone(), row.game_bytes);
        if key.as_ref().is_some_and(|current| *current != next) {
            finish_exact_group(&mut rows, &mut groups)?;
        }
        key = Some(next);
        rows.push(row);
        Ok(())
    })?;
    finish_exact_group(&mut rows, &mut groups)?;
    Ok(groups)
}

fn canonical_group_id(canonical_mainline_id: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"antiskill_same_canonical_mainline_v1\0");
    hasher.update(b"antiskill_canonical_mainline_v1");
    hasher.update(b"\0");
    hasher.update(canonical_mainline_id.as_bytes());
    hasher.update(b"\n");
    hasher.finalize().to_hex().to_string()
}

fn finish_canonical_group(
    canonical_id: Option<&str>,
    rows: &mut Vec<ProductDuplicateRowV1>,
    groups: &mut Vec<(String, HumanCanonicalGroup)>,
) -> Result<(), String> {
    if rows.len() < 2 {
        rows.clear();
        return Ok(());
    }
    let byte_variants = rows
        .iter()
        .map(|row| (row.source_game_blake3.clone(), row.game_bytes))
        .collect::<BTreeSet<_>>();
    let byte_variant_count =
        u64::try_from(byte_variants.len()).map_err(|_| "Numero varianti byte oltre u64")?;
    if byte_variant_count <= 1 {
        rows.clear();
        return Ok(());
    }
    let result_conflict = rows
        .iter()
        .map(|row| row.mainline_results.clone())
        .collect::<BTreeSet<_>>()
        .len()
        > 1;
    let mut members = rows.iter().map(human_game).collect::<Result<Vec<_>, _>>()?;
    members.sort_by(|left, right| left.id.cmp(&right.id));
    let canonical_id = canonical_id.ok_or("Gruppo canonical senza identità")?;
    groups.push((
        canonical_group_id(canonical_id),
        HumanCanonicalGroup {
            members,
            byte_variant_count,
            result_conflict,
        },
    ));
    rows.clear();
    Ok(())
}

fn canonical_groups(runs: &[PathBuf]) -> Result<Vec<HumanCanonicalGroup>, String> {
    let mut groups = Vec::new();
    let mut key: Option<String> = None;
    let mut rows = Vec::new();
    merge_rows(runs, SortKind::Canonical, |row| {
        let next = row
            .canonical_mainline_id
            .clone()
            .ok_or("Occurrence confrontabile senza canonical_mainline_id")?;
        if key.as_ref().is_some_and(|current| *current != next) {
            finish_canonical_group(key.as_deref(), &mut rows, &mut groups)?;
        }
        key = Some(next);
        rows.push(row);
        Ok(())
    })?;
    finish_canonical_group(key.as_deref(), &mut rows, &mut groups)?;
    groups.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(groups.into_iter().map(|(_, group)| group).collect())
}

pub(super) fn build(root: &Path, collection: &Path) -> Result<HumanDuplicatesReport, String> {
    let temporary = TempWorkspace::new(root)?;
    let raw = temporary.path.join("verified.jsonl");
    let file =
        File::create(&raw).map_err(|error| format!("Creazione spool duplicates: {error}"))?;
    let mut writer = BufWriter::new(file);
    let mut noncomparable_occurrences = 0_u64;
    let verified = crate::inspector::for_each_verified_duplicate_row(root, collection, |row| {
        if !row.comparable {
            noncomparable_occurrences = noncomparable_occurrences
                .checked_add(1)
                .ok_or("Numero occurrence non confrontabili oltre u64")?;
        }
        write_row(&mut writer, &row)
    })?;
    writer
        .flush()
        .map_err(|error| format!("Flush spool duplicates: {error}"))?;
    if verified == 0 && noncomparable_occurrences != 0 {
        return Err("Conteggio duplicates incoerente".into());
    }

    let exact_runs = create_runs(&raw, SortKind::Exact, &temporary.path, "exact")?;
    let exact_groups = exact_groups(&exact_runs)?;
    for path in &exact_runs {
        fs::remove_file(path).map_err(|error| format!("Pulizia run exact: {error}"))?;
    }

    let canonical_runs = create_runs(&raw, SortKind::Canonical, &temporary.path, "canonical")?;
    let canonical_groups = canonical_groups(&canonical_runs)?;

    Ok(HumanDuplicatesReport {
        exact_groups,
        canonical_groups,
        noncomparable_occurrences,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        id: char,
        exact: &str,
        canonical: Option<&str>,
        comparable: bool,
    ) -> ProductDuplicateRowV1 {
        ProductDuplicateRowV1 {
            occurrence_id: id.to_string().repeat(64),
            source_game_blake3: exact.to_string().repeat(64),
            game_bytes: 10,
            canonical_mainline_id: canonical.map(|value| value.to_string().repeat(64)),
            comparable,
            mainline_results: vec!["1-0".into()],
            date: vec!["2026.01.01".into()],
            white: vec!["White".into()],
            black: vec!["Black".into()],
            result: vec!["1-0".into()],
            event: vec!["Event".into()],
        }
    }

    #[test]
    fn canonical_group_id_matches_frozen_profile_formula() {
        let id = "a".repeat(64);
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"antiskill_same_canonical_mainline_v1\0");
        hasher.update(b"antiskill_canonical_mainline_v1");
        hasher.update(b"\0");
        hasher.update(id.as_bytes());
        hasher.update(b"\n");
        assert_eq!(
            canonical_group_id(&id),
            hasher.finalize().to_hex().to_string()
        );
    }

    #[test]
    fn temporary_workspace_is_local_to_workspace_and_cleanup_is_complete() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "antiskill-duplicates-temp-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("raw/tmp")).unwrap();
        let path;
        {
            let temporary = TempWorkspace::new(&root).unwrap();
            path = temporary.path.clone();
            assert!(path.starts_with(root.join("raw/tmp")));
            assert!(path.is_dir());
        }
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sort_orders_match_frozen_exact_and_canonical_keys() {
        let a = row('a', "b", Some("c"), true);
        let b = row('b', "a", Some("b"), true);
        assert_eq!(compare_rows(&a, &b, SortKind::Exact), Ordering::Greater);
        assert_eq!(compare_rows(&a, &b, SortKind::Canonical), Ordering::Greater);
    }
}
