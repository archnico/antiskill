use super::common::{
    canonical_artifact_path, ensure_within, logical_json_bytes, path_to_manifest_string,
    prepare_directory_chain, publish_compressed_json, resolve_workspace_path,
    validate_relative_path,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

pub(super) const ANNOTATION_SCHEMA_VERSION: u32 = 1;
pub(super) const ANNOTATION_PROFILE: &str = "antiskill_annotation_group_v1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AnnotationVisibilityV1 {
    Private,
    Internal,
    Redistributable,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AnnotationTargetKindV1 {
    Game,
    Move,
    Position,
    Event,
    Person,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AnnotationKindV1 {
    Text,
    Nag,
    Variation,
    Eval,
    Clock,
    Graphic,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct AnnotationGroupV1 {
    pub(super) schema_version: u32,
    pub(super) annotation_profile: String,
    pub(super) group_id: String,
    pub(super) source_id: String,
    pub(super) snapshot_id: String,
    pub(super) attributed_to: Option<String>,
    pub(super) observed_date: Option<String>,
    pub(super) rights: String,
    pub(super) visibility: AnnotationVisibilityV1,
    pub(super) method: Option<String>,
    pub(super) engine: Option<String>,
    pub(super) items: Vec<AnnotationItemV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct AnnotationItemV1 {
    pub(super) item_id: String,
    pub(super) target_kind: AnnotationTargetKindV1,
    pub(super) target_id: String,
    pub(super) kind: AnnotationKindV1,
    pub(super) payload: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StoredAnnotationGroupV1 {
    pub(super) group_id: String,
    pub(super) items: usize,
    pub(super) blake3: String,
    pub(super) bytes: u64,
    pub(super) path: PathBuf,
    pub(super) new_artifact: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct VerifiedStoredAnnotationGroupV1 {
    pub(super) group: AnnotationGroupV1,
    pub(super) blake3: String,
    pub(super) bytes: u64,
    pub(super) path: PathBuf,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct AnnotationCoverageV1 {
    pub(super) groups: u64,
    pub(super) items: u64,
}

pub(super) fn load_and_validate(path: &Path) -> Result<AnnotationGroupV1, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("File annotazioni {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("File annotazioni non regolare: {}", path.display()));
    }
    let file = File::open(path)
        .map_err(|error| format!("Apertura annotazioni {}: {error}", path.display()))?;
    let group: AnnotationGroupV1 = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("JSON annotazioni non valido: {error}"))?;
    group.validate()?;
    Ok(group)
}

fn serialized(group: &AnnotationGroupV1) -> Result<Vec<u8>, String> {
    group.validate()?;
    serde_json::to_vec(group).map_err(|error| format!("Serializzazione annotazioni: {error}"))
}

pub(super) fn store(root: &Path, input: &Path) -> Result<StoredAnnotationGroupV1, String> {
    let group = load_and_validate(input)?;
    let bytes = serialized(&group)?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let logical_bytes = u64::try_from(bytes.len()).map_err(|_| "Gruppo annotazioni oltre u64")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let directory = prepare_directory_chain(
        root,
        &canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("annotations", "directory annotazioni"),
            ("groups", "directory gruppi annotazioni"),
        ],
    )?;
    let (target, new_artifact) = publish_compressed_json(
        &canonical_root,
        &directory,
        &hash,
        &bytes,
        "Annotation Group",
    )?;
    let name = target
        .file_name()
        .ok_or("Artefatto annotazioni senza nome")?;
    let path = PathBuf::from("inspector/annotations/groups").join(name);
    if !path.to_string_lossy().ends_with(".json.xz") {
        return Err("Artefatto annotazioni legacy non supportato".into());
    }
    let verified = load_verified(root, &path)?;
    if verified.group != group || verified.blake3 != hash || verified.bytes != logical_bytes {
        return Err("Gruppo annotazioni diverso dalla verifica post-pubblicazione".into());
    }
    let items = group.items.len();
    Ok(StoredAnnotationGroupV1 {
        group_id: group.group_id,
        items,
        blake3: hash,
        bytes: logical_bytes,
        path,
        new_artifact,
    })
}

pub(super) fn load_verified(
    root: &Path,
    path: &Path,
) -> Result<VerifiedStoredAnnotationGroupV1, String> {
    let path_value = path.to_str().ok_or("Percorso annotazioni non UTF-8")?;
    validate_relative_path(path_value, "annotazioni")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let metadata = fs::symlink_metadata(&resolved)
        .map_err(|error| format!("Artefatto annotazioni non valido: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Artefatto annotazioni non è un file regolare reale".into());
    }
    let canonical = fs::canonicalize(&resolved)
        .map_err(|error| format!("Artefatto annotazioni non valido: {error}"))?;
    ensure_within(&canonical_root, &canonical, "artefatto annotazioni")?;
    let bytes = logical_json_bytes(&canonical, "Annotation Group")?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let logical_bytes = u64::try_from(bytes.len()).map_err(|_| "Gruppo annotazioni oltre u64")?;
    let actual = canonical
        .strip_prefix(&canonical_root)
        .map_err(|_| "Artefatto annotazioni fuori dal workspace")?;
    let actual = path_to_manifest_string(actual);
    let expected = canonical_artifact_path("annotations/groups", &hash, &canonical)?;
    if actual != expected || !actual.ends_with(".json.xz") {
        return Err("Percorso artefatto annotazioni non canonico".into());
    }
    let group: AnnotationGroupV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("JSON annotazioni persistite non valido: {error}"))?;
    group.validate()?;
    let canonical_bytes = serialized(&group)?;
    if bytes != canonical_bytes {
        return Err("Serializzazione annotazioni persistite non canonica".into());
    }
    Ok(VerifiedStoredAnnotationGroupV1 {
        group,
        blake3: hash,
        bytes: logical_bytes,
        path: PathBuf::from(actual),
    })
}

pub(super) fn coverage(root: &Path) -> Result<AnnotationCoverageV1, String> {
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let directory = root.join("inspector/annotations/groups");
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AnnotationCoverageV1::default());
        }
        Err(error) => {
            return Err(format!(
                "Directory gruppi annotazioni non accessibile: {error}"
            ));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Directory gruppi annotazioni non è una directory reale".into());
    }
    let canonical_directory = fs::canonicalize(&directory)
        .map_err(|error| format!("Directory gruppi annotazioni non valida: {error}"))?;
    ensure_within(
        &canonical_root,
        &canonical_directory,
        "directory gruppi annotazioni",
    )?;

    let mut result = AnnotationCoverageV1::default();
    for entry in fs::read_dir(&canonical_directory)
        .map_err(|error| format!("Lettura directory gruppi annotazioni: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Entry gruppi annotazioni: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Tipo entry gruppi annotazioni: {error}"))?;
        if file_type.is_symlink() || !file_type.is_file() {
            return Err("Directory gruppi annotazioni contiene un'entry non regolare".into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Nome artefatto annotazioni non UTF-8")?;
        if !name.ends_with(".json.xz") {
            return Err(format!(
                "Artefatto inatteso nella directory gruppi annotazioni: {name}"
            ));
        }
        let relative = PathBuf::from("inspector/annotations/groups").join(name);
        let verified = load_verified(root, &relative)?;
        result.groups = result
            .groups
            .checked_add(1)
            .ok_or("Numero gruppi annotazioni oltre u64")?;
        result.items = result
            .items
            .checked_add(
                u64::try_from(verified.group.items.len())
                    .map_err(|_| "Numero item per gruppo oltre u64")?,
            )
            .ok_or("Numero item annotazioni oltre u64")?;
    }
    Ok(result)
}

impl AnnotationGroupV1 {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.schema_version != ANNOTATION_SCHEMA_VERSION {
            return Err("Versione schema annotazioni non supportata".into());
        }
        if self.annotation_profile != ANNOTATION_PROFILE {
            return Err("Profilo annotazioni non supportato".into());
        }
        require_text(&self.group_id, "group_id")?;
        require_text(&self.source_id, "source_id")?;
        require_text(&self.snapshot_id, "snapshot_id")?;
        require_text(&self.rights, "rights")?;
        if self.items.is_empty() {
            return Err("Un gruppo di annotazioni deve contenere almeno un item".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for item in &self.items {
            require_text(&item.item_id, "item_id")?;
            require_text(&item.target_id, "target_id")?;
            require_text(&item.payload, "payload")?;
            if !ids.insert(item.item_id.as_str()) {
                return Err(format!("item_id duplicato: {}", item.item_id));
            }
        }
        Ok(())
    }
}

fn require_text(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} non può essere vuoto"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> AnnotationGroupV1 {
        AnnotationGroupV1 {
            schema_version: ANNOTATION_SCHEMA_VERSION,
            annotation_profile: ANNOTATION_PROFILE.into(),
            group_id: "group:source:snapshot:1".into(),
            source_id: "source:book".into(),
            snapshot_id: "snapshot:abc".into(),
            attributed_to: Some("Example Author".into()),
            observed_date: Some("2026-10-11".into()),
            rights: "private_research".into(),
            visibility: AnnotationVisibilityV1::Private,
            method: Some("published commentary".into()),
            engine: None,
            items: vec![AnnotationItemV1 {
                item_id: "annotation:1".into(),
                target_kind: AnnotationTargetKindV1::Position,
                target_id: "position:abc".into(),
                kind: AnnotationKindV1::Text,
                payload: "isolated pawn".into(),
            }],
        }
    }

    #[test]
    fn valid_group_roundtrips_without_losing_attribution() {
        let group = fixture();
        group.validate().unwrap();
        let json = serde_json::to_string(&group).unwrap();
        let decoded: AnnotationGroupV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, group);
        assert_eq!(decoded.attributed_to.as_deref(), Some("Example Author"));
        assert_eq!(decoded.snapshot_id, "snapshot:abc");
    }

    #[test]
    fn source_and_snapshot_are_mandatory() {
        for field in ["source", "snapshot"] {
            let mut group = fixture();
            if field == "source" {
                group.source_id.clear();
            } else {
                group.snapshot_id.clear();
            }
            assert!(group.validate().is_err());
        }
    }

    #[test]
    fn duplicate_item_ids_are_rejected() {
        let mut group = fixture();
        group.items.push(group.items[0].clone());
        assert!(group.validate().unwrap_err().contains("duplicato"));
    }

    #[test]
    fn unknown_fields_fail_closed() {
        let mut value = serde_json::to_value(fixture()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("mystery".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<AnnotationGroupV1>(value).is_err());
    }
}
