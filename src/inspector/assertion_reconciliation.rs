use super::assertion_index::{self, AssertionIndexEntryV1};
use super::source_assertions::AssertionSubjectKindV1;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReconciliationPolicyV1 {
    schema_version: u32,
    reconciliation_profile: String,
    subject_kind: AssertionSubjectKindV1,
    subject_id: String,
    predicate: String,
    as_of: String,
    source_preference: Vec<String>,
    rationale: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ReconciledAssertionV1 {
    schema_version: u32,
    reconciliation_profile: &'static str,
    subject_kind: AssertionSubjectKindV1,
    subject_id: String,
    predicate: String,
    as_of: String,
    rationale: String,
    selected_source_rank: usize,
    selected: AssertionIndexEntryV1,
    eligible_candidates: usize,
}

pub(super) fn reconcile(
    root: &Path,
    paths: &[PathBuf],
    policy_path: &Path,
) -> Result<ReconciledAssertionV1, String> {
    let policy = load_policy(policy_path)?;
    let rows = assertion_index::build(root, paths)?;
    let row = rows
        .into_iter()
        .find(|row| {
            row.subject_kind == policy.subject_kind
                && row.subject_id == policy.subject_id
                && row.predicate == policy.predicate
        })
        .ok_or("Nessuna asserzione per il claim richiesto")?;
    let mut eligible = Vec::new();
    for assertion in row.assertions {
        if valid_at(&assertion, &policy.as_of)? {
            eligible.push(assertion);
        }
    }
    if eligible.is_empty() {
        return Err("Nessuna asserzione valida alla data richiesta".into());
    }
    for (rank, source) in policy.source_preference.iter().enumerate() {
        let mut matches: Vec<_> = eligible
            .iter()
            .filter(|a| &a.source_key == source)
            .collect();
        if matches.is_empty() {
            continue;
        }
        matches.sort_by(|a, b| {
            (&b.snapshot_id, &b.observed_at, &b.blake3).cmp(&(
                &a.snapshot_id,
                &a.observed_at,
                &a.blake3,
            ))
        });
        let first = matches[0];
        if matches
            .iter()
            .skip(1)
            .any(|other| other.value != first.value)
        {
            return Err(format!("Fonte preferita {source} contiene valori concorrenti validi: riconciliazione ambigua"));
        }
        return Ok(ReconciledAssertionV1 {
            schema_version: 1,
            reconciliation_profile: "antiskill_contextual_reconciliation_v1",
            subject_kind: policy.subject_kind,
            subject_id: policy.subject_id,
            predicate: policy.predicate,
            as_of: policy.as_of,
            rationale: policy.rationale,
            selected_source_rank: rank,
            selected: first.clone(),
            eligible_candidates: eligible.len(),
        });
    }
    Err("Nessuna fonte eleggibile compare in source_preference".into())
}

pub(super) fn validate_policy(path: &Path) -> Result<(), String> {
    load_policy(path).map(|_| ())
}

fn load_policy(path: &Path) -> Result<ReconciliationPolicyV1, String> {
    let file = File::open(path)
        .map_err(|e| format!("Apertura policy riconciliazione {}: {e}", path.display()))?;
    let policy: ReconciliationPolicyV1 = serde_json::from_reader(BufReader::new(file))
        .map_err(|e| format!("JSON policy riconciliazione non valido: {e}"))?;
    if policy.schema_version != 1
        || policy.reconciliation_profile != "antiskill_contextual_reconciliation_v1"
    {
        return Err("Profilo riconciliazione non supportato".into());
    }
    for (value, field) in [
        (&policy.subject_id, "subject_id"),
        (&policy.predicate, "predicate"),
        (&policy.as_of, "as_of"),
        (&policy.rationale, "rationale"),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{field} non può essere vuoto"));
        }
    }
    validate_date(&policy.as_of)?;
    if policy.source_preference.is_empty() {
        return Err("source_preference non può essere vuoto".into());
    }
    let mut seen = BTreeSet::new();
    for source in &policy.source_preference {
        if source.trim().is_empty() || !seen.insert(source) {
            return Err("source_preference contiene valori vuoti o duplicati".into());
        }
    }
    Ok(policy)
}

fn valid_at(a: &AssertionIndexEntryV1, as_of: &str) -> Result<bool, String> {
    if let Some(valid_from) = a.valid_from.as_deref() {
        validate_assertion_date(a, "valid_from", valid_from)?;
    }
    if let Some(valid_to) = a.valid_to.as_deref() {
        validate_assertion_date(a, "valid_to", valid_to)?;
    }
    if let (Some(valid_from), Some(valid_to)) = (a.valid_from.as_deref(), a.valid_to.as_deref()) {
        if valid_from > valid_to {
            return Err(format!(
                "Intervallo temporale non valido per assertion_id {} (source {}): valid_from {} > valid_to {}",
                a.assertion_id, a.source_key, valid_from, valid_to
            ));
        }
    }
    let from_ok = a.valid_from.as_deref().map(|v| v <= as_of).unwrap_or(true);
    let to_ok = a.valid_to.as_deref().map(|v| v >= as_of).unwrap_or(true);
    Ok(from_ok && to_ok)
}

fn validate_assertion_date(
    a: &AssertionIndexEntryV1,
    field: &str,
    value: &str,
) -> Result<(), String> {
    validate_date(value).map_err(|error| {
        format!(
            "{field} non valido per assertion_id {} (source {}): {error}",
            a.assertion_id, a.source_key
        )
    })
}

fn validate_date(value: &str) -> Result<(), String> {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err(format!("Data non canonica YYYY-MM-DD: {value}"));
    }
    let y: u32 = value[0..4].parse().unwrap();
    let m: u32 = value[5..7].parse().unwrap();
    let d: u32 = value[8..10].parse().unwrap();
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let max = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if d == 0 || d > max {
        return Err(format!("Data non valida: {value}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(from: Option<&str>, to: Option<&str>) -> AssertionIndexEntryV1 {
        AssertionIndexEntryV1 {
            assertion_id: "a".into(),
            value: serde_json::json!(1),
            valid_from: from.map(str::to_owned),
            valid_to: to.map(str::to_owned),
            source_key: "fide".into(),
            snapshot_id: "s".into(),
            locator: None,
            observed_at: None,
            rights: "internal".into(),
            group_id: "g".into(),
            blake3: "h".into(),
            path: "p".into(),
        }
    }
    #[test]
    fn temporal_window_is_inclusive_and_malformed_metadata_fails_closed() {
        let a = entry(Some("2026-10-01"), Some("2026-10-31"));
        assert!(valid_at(&a, "2026-10-01").unwrap());
        assert!(valid_at(&a, "2026-10-31").unwrap());
        assert!(!valid_at(&a, "2026-11-01").unwrap());

        let bad_date = valid_at(&entry(Some("October"), None), "2026-10-11").unwrap_err();
        assert!(bad_date.contains("valid_from"));
        assert!(bad_date.contains("assertion_id a"));

        let bad_interval =
            valid_at(&entry(Some("2026-11-01"), Some("2026-10-01")), "2026-10-11").unwrap_err();
        assert!(bad_interval.contains("Intervallo temporale non valido"));
    }
    #[test]
    fn canonical_date_validation_rejects_impossible_dates() {
        assert!(validate_date("2024-02-29").is_ok());
        assert!(validate_date("2026-02-29").is_err());
        assert!(validate_date("2026-13-01").is_err());
    }
}
