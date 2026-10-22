use super::common::{
    canonical_artifact_path, ensure_within, logical_json_bytes, path_to_manifest_string,
    prepare_directory_chain, publish_compressed_json, resolve_workspace_path,
    validate_relative_path,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

pub(super) const ASSERTION_SCHEMA_VERSION: u32 = 1;
pub(super) const ASSERTION_PROFILE: &str = "antiskill_source_assertion_group_v1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AssertionSubjectKindV1 {
    Person,
    Event,
    Game,
    Club,
    Federation,
    Organization,
    Edition,
    Position,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceAssertionGroupV1 {
    pub(super) schema_version: u32,
    pub(super) assertion_profile: String,
    pub(super) group_id: String,
    pub(super) source_key: String,
    pub(super) snapshot_id: String,
    pub(super) locator: Option<String>,
    pub(super) observed_at: Option<String>,
    pub(super) rights: String,
    pub(super) assertions: Vec<SourceAssertionV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceAssertionV1 {
    pub(super) assertion_id: String,
    pub(super) subject_kind: AssertionSubjectKindV1,
    pub(super) subject_id: String,
    pub(super) predicate: String,
    pub(super) value: Value,
    pub(super) valid_from: Option<String>,
    pub(super) valid_to: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StoredSourceAssertionGroupV1 {
    pub(super) group_id: String,
    pub(super) assertions: usize,
    pub(super) blake3: String,
    pub(super) bytes: u64,
    pub(super) path: PathBuf,
    pub(super) new_artifact: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct VerifiedStoredSourceAssertionGroupV1 {
    pub(super) group: SourceAssertionGroupV1,
    pub(super) blake3: String,
    pub(super) bytes: u64,
    pub(super) path: PathBuf,
}

impl SourceAssertionGroupV1 {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.schema_version != ASSERTION_SCHEMA_VERSION {
            return Err("Versione schema asserzioni non supportata".into());
        }
        if self.assertion_profile != ASSERTION_PROFILE {
            return Err("Profilo asserzioni non supportato".into());
        }
        require_text(&self.group_id, "group_id")?;
        require_text(&self.source_key, "source_key")?;
        require_text(&self.snapshot_id, "snapshot_id")?;
        require_text(&self.rights, "rights")?;
        for (value, field) in [
            (&self.locator, "locator"),
            (&self.observed_at, "observed_at"),
        ] {
            if let Some(value) = value {
                require_text(value, field)?;
            }
        }
        if self.assertions.is_empty() {
            return Err("Un gruppo di asserzioni deve contenere almeno un item".into());
        }
        let mut ids = BTreeSet::new();
        for assertion in &self.assertions {
            require_text(&assertion.assertion_id, "assertion_id")?;
            if !ids.insert(assertion.assertion_id.as_str()) {
                return Err(format!(
                    "assertion_id duplicato nel gruppo: {}",
                    assertion.assertion_id
                ));
            }
            require_text(&assertion.subject_id, "subject_id")?;
            validate_predicate(&assertion.predicate)?;
            if assertion.value.is_null() {
                return Err(format!(
                    "value nullo non ammesso per {}",
                    assertion.assertion_id
                ));
            }
            for (value, field) in [
                (&assertion.valid_from, "valid_from"),
                (&assertion.valid_to, "valid_to"),
            ] {
                if let Some(value) = value {
                    require_text(value, field)?;
                }
            }
        }
        Ok(())
    }
}

pub(super) fn load_and_validate(path: &Path) -> Result<SourceAssertionGroupV1, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("File asserzioni {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("File asserzioni non regolare: {}", path.display()));
    }
    let file = File::open(path)
        .map_err(|error| format!("Apertura asserzioni {}: {error}", path.display()))?;
    let group: SourceAssertionGroupV1 = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("JSON asserzioni non valido: {error}"))?;
    group.validate()?;
    Ok(group)
}

fn serialized(group: &SourceAssertionGroupV1) -> Result<Vec<u8>, String> {
    group.validate()?;
    serde_json::to_vec(group).map_err(|error| format!("Serializzazione asserzioni: {error}"))
}

pub(super) fn store(root: &Path, input: &Path) -> Result<StoredSourceAssertionGroupV1, String> {
    let group = load_and_validate(input)?;
    let bytes = serialized(&group)?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let logical_bytes = u64::try_from(bytes.len()).map_err(|_| "Gruppo asserzioni oltre u64")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let directory = prepare_directory_chain(
        root,
        &canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("assertions", "directory asserzioni"),
            ("groups", "directory gruppi asserzioni"),
        ],
    )?;
    let (target, new_artifact) = publish_compressed_json(
        &canonical_root,
        &directory,
        &hash,
        &bytes,
        "Source Assertion Group",
    )?;
    let name = target
        .file_name()
        .ok_or("Artefatto asserzioni senza nome")?;
    let path = PathBuf::from("inspector/assertions/groups").join(name);
    let verified = load_verified(root, &path)?;
    if verified.group != group || verified.blake3 != hash || verified.bytes != logical_bytes {
        return Err("Gruppo asserzioni diverso dalla verifica post-pubblicazione".into());
    }
    Ok(StoredSourceAssertionGroupV1 {
        group_id: group.group_id,
        assertions: group.assertions.len(),
        blake3: hash,
        bytes: logical_bytes,
        path,
        new_artifact,
    })
}

pub(super) fn load_verified(
    root: &Path,
    path: &Path,
) -> Result<VerifiedStoredSourceAssertionGroupV1, String> {
    let path_value = path.to_str().ok_or("Percorso asserzioni non UTF-8")?;
    validate_relative_path(path_value, "asserzioni")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let metadata = fs::symlink_metadata(&resolved)
        .map_err(|error| format!("Artefatto asserzioni non valido: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Artefatto asserzioni non è un file regolare reale".into());
    }
    let canonical = fs::canonicalize(&resolved)
        .map_err(|error| format!("Artefatto asserzioni non valido: {error}"))?;
    ensure_within(&canonical_root, &canonical, "artefatto asserzioni")?;
    let bytes = logical_json_bytes(&canonical, "Source Assertion Group")?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let logical_bytes = u64::try_from(bytes.len()).map_err(|_| "Gruppo asserzioni oltre u64")?;
    let actual = canonical
        .strip_prefix(&canonical_root)
        .map_err(|_| "Artefatto asserzioni fuori dal workspace")?;
    let actual = path_to_manifest_string(actual);
    let expected = canonical_artifact_path("assertions/groups", &hash, &canonical)?;
    if actual != expected || !actual.ends_with(".json.xz") {
        return Err("Percorso artefatto asserzioni non canonico".into());
    }
    let group: SourceAssertionGroupV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("JSON asserzioni persistite non valido: {error}"))?;
    group.validate()?;
    let canonical_bytes = serialized(&group)?;
    if bytes != canonical_bytes {
        return Err("Serializzazione asserzioni persistite non canonica".into());
    }
    Ok(VerifiedStoredSourceAssertionGroupV1 {
        group,
        blake3: hash,
        bytes: logical_bytes,
        path: PathBuf::from(actual),
    })
}

fn validate_predicate(value: &str) -> Result<(), String> {
    require_text(value, "predicate")?;
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'.' | b'_' | b'-' | b':')
    }) {
        return Err("predicate deve essere namespaced ASCII lowercase senza spazi".into());
    }
    Ok(())
}

fn require_text(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} non può essere vuoto"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SourceAssertionGroupV1 {
        SourceAssertionGroupV1 {
            schema_version: 1,
            assertion_profile: ASSERTION_PROFILE.into(),
            group_id: "fide-ratings:2026-10:123456".into(),
            source_key: "fide-rating-lists".into(),
            snapshot_id: "snapshot:2026-10".into(),
            locator: Some("player:123456".into()),
            observed_at: Some("2026-10-11".into()),
            rights: "open".into(),
            assertions: vec![SourceAssertionV1 {
                assertion_id: "rating:standard".into(),
                subject_kind: AssertionSubjectKindV1::Person,
                subject_id: "fide:123456".into(),
                predicate: "fide.rating.standard".into(),
                value: serde_json::json!(2412),
                valid_from: Some("2026-10-01".into()),
                valid_to: None,
            }],
        }
    }

    #[test]
    fn valid_group_accepts_structured_source_fact() {
        sample().validate().unwrap();
    }

    #[test]
    fn duplicate_assertion_ids_fail_closed() {
        let mut group = sample();
        group.assertions.push(group.assertions[0].clone());
        assert!(group.validate().unwrap_err().contains("duplicato"));
    }

    #[test]
    fn null_values_and_unscoped_predicates_fail_closed() {
        let mut group = sample();
        group.assertions[0].value = Value::Null;
        assert!(group.validate().unwrap_err().contains("nullo"));
        group = sample();
        group.assertions[0].predicate = "FIDE Rating".into();
        assert!(group.validate().unwrap_err().contains("predicate"));
    }
}
