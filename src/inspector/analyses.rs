use super::common::{
    canonical_artifact_path, ensure_within, logical_json_bytes, path_to_manifest_string,
    prepare_directory_chain, publish_compressed_json, resolve_workspace_path,
    validate_relative_path,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

pub(super) const ANALYSIS_SCHEMA_VERSION: u32 = 1;
pub(super) const ANALYSIS_PROFILE: &str = "antiskill_analysis_artifact_v1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AnalysisSubjectScopeV1 {
    Position,
    RuleState,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AnalysisOriginV1 {
    ExternalSource,
    LocalRun,
    ContributedRun,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AnalysisStatusV1 {
    Completed,
    Partial,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ScorePerspectiveV1 {
    SideToMove,
    White,
    Black,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct AnalysisArtifactV1 {
    pub(super) schema_version: u32,
    pub(super) analysis_profile: String,
    pub(super) subject_scope: AnalysisSubjectScopeV1,
    pub(super) subject_id: String,
    pub(super) origin: AnalysisOriginV1,
    pub(super) origin_identity: String,
    pub(super) source_key: Option<String>,
    pub(super) snapshot_id: Option<String>,
    pub(super) locator: Option<String>,
    pub(super) rights: String,
    pub(super) method: String,
    pub(super) engine_build_id: Option<String>,
    pub(super) search_profile_id: Option<String>,
    pub(super) status: AnalysisStatusV1,
    pub(super) observed_at: Option<String>,
    pub(super) lines: Vec<AnalysisLineV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct AnalysisLineV1 {
    pub(super) multipv_rank: u32,
    pub(super) raw_score: String,
    pub(super) score_perspective: ScorePerspectiveV1,
    pub(super) white_cp: Option<i32>,
    pub(super) side_to_move_cp: Option<i32>,
    pub(super) mate: Option<i32>,
    pub(super) wdl: Option<WdlV1>,
    pub(super) pv_uci: Vec<String>,
    pub(super) depth: Option<u32>,
    pub(super) seldepth: Option<u32>,
    pub(super) nodes: Option<u64>,
    pub(super) time_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct WdlV1 {
    pub(super) win: u32,
    pub(super) draw: u32,
    pub(super) loss: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StoredAnalysisArtifactV1 {
    pub(super) subject_id: String,
    pub(super) lines: usize,
    pub(super) blake3: String,
    pub(super) bytes: u64,
    pub(super) path: PathBuf,
    pub(super) new_artifact: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct VerifiedStoredAnalysisArtifactV1 {
    pub(super) artifact: AnalysisArtifactV1,
    pub(super) blake3: String,
    pub(super) bytes: u64,
    pub(super) path: PathBuf,
}

impl AnalysisArtifactV1 {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.schema_version != ANALYSIS_SCHEMA_VERSION {
            return Err("Versione schema analisi non supportata".into());
        }
        if self.analysis_profile != ANALYSIS_PROFILE {
            return Err("Profilo analisi non supportato".into());
        }
        require_text(&self.subject_id, "subject_id")?;
        if !is_lower_hex_64(&self.subject_id) {
            return Err(
                "subject_id deve essere un identificatore BLAKE3 lowercase di 64 cifre".into(),
            );
        }
        require_text(&self.origin_identity, "origin_identity")?;
        require_text(&self.rights, "rights")?;
        require_text(&self.method, "method")?;
        if self.origin == AnalysisOriginV1::ExternalSource {
            require_optional_text(&self.source_key, "source_key")?;
            require_optional_text(&self.snapshot_id, "snapshot_id")?;
        }
        for (value, field) in [
            (&self.source_key, "source_key"),
            (&self.snapshot_id, "snapshot_id"),
            (&self.locator, "locator"),
            (&self.engine_build_id, "engine_build_id"),
            (&self.search_profile_id, "search_profile_id"),
            (&self.observed_at, "observed_at"),
        ] {
            if let Some(value) = value {
                require_text(value, field)?;
            }
        }
        if self.lines.is_empty() {
            return Err("Un artefatto analisi deve contenere almeno una riga".into());
        }
        let mut ranks = BTreeSet::new();
        for line in &self.lines {
            if line.multipv_rank == 0 {
                return Err("multipv_rank deve partire da 1".into());
            }
            if !ranks.insert(line.multipv_rank) {
                return Err(format!("multipv_rank duplicato: {}", line.multipv_rank));
            }
            require_text(&line.raw_score, "raw_score")?;
            for mv in &line.pv_uci {
                require_text(mv, "pv_uci")?;
            }
            if let Some(wdl) = &line.wdl {
                if wdl.win == 0 && wdl.draw == 0 && wdl.loss == 0 {
                    return Err("WDL tutto zero non è informativo".into());
                }
            }
        }
        Ok(())
    }
}

pub(super) fn load_and_validate(path: &Path) -> Result<AnalysisArtifactV1, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("File analisi {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("File analisi non regolare: {}", path.display()));
    }
    let file = File::open(path)
        .map_err(|error| format!("Apertura analisi {}: {error}", path.display()))?;
    let artifact: AnalysisArtifactV1 = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("JSON analisi non valido: {error}"))?;
    artifact.validate()?;
    Ok(artifact)
}

fn serialized(artifact: &AnalysisArtifactV1) -> Result<Vec<u8>, String> {
    artifact.validate()?;
    serde_json::to_vec(artifact).map_err(|error| format!("Serializzazione analisi: {error}"))
}

pub(super) fn store(root: &Path, input: &Path) -> Result<StoredAnalysisArtifactV1, String> {
    let artifact = load_and_validate(input)?;
    store_artifact(root, artifact)
}

pub(super) fn store_artifact(
    root: &Path,
    artifact: AnalysisArtifactV1,
) -> Result<StoredAnalysisArtifactV1, String> {
    let bytes = serialized(&artifact)?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let logical_bytes = u64::try_from(bytes.len()).map_err(|_| "Artefatto analisi oltre u64")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let directory = prepare_directory_chain(
        root,
        &canonical_root,
        &[
            ("inspector", "directory inspector"),
            ("analyses", "directory analisi"),
            ("artifacts", "directory artefatti analisi"),
        ],
    )?;
    let (target, new_artifact) = publish_compressed_json(
        &canonical_root,
        &directory,
        &hash,
        &bytes,
        "Analysis Artifact",
    )?;
    let name = target.file_name().ok_or("Artefatto analisi senza nome")?;
    let path = PathBuf::from("inspector/analyses/artifacts").join(name);
    let verified = load_verified(root, &path)?;
    if verified.artifact != artifact || verified.blake3 != hash || verified.bytes != logical_bytes {
        return Err("Artefatto analisi diverso dalla verifica post-pubblicazione".into());
    }
    Ok(StoredAnalysisArtifactV1 {
        subject_id: artifact.subject_id,
        lines: artifact.lines.len(),
        blake3: hash,
        bytes: logical_bytes,
        path,
        new_artifact,
    })
}

pub(super) fn load_verified(
    root: &Path,
    path: &Path,
) -> Result<VerifiedStoredAnalysisArtifactV1, String> {
    let path_value = path.to_str().ok_or("Percorso analisi non UTF-8")?;
    validate_relative_path(path_value, "analisi")?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| format!("Workspace non valido: {error}"))?;
    let resolved = resolve_workspace_path(root, path)?;
    let metadata = fs::symlink_metadata(&resolved)
        .map_err(|error| format!("Artefatto analisi non valido: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Artefatto analisi non è un file regolare reale".into());
    }
    let canonical = fs::canonicalize(&resolved)
        .map_err(|error| format!("Artefatto analisi non valido: {error}"))?;
    ensure_within(&canonical_root, &canonical, "artefatto analisi")?;
    let bytes = logical_json_bytes(&canonical, "Analysis Artifact")?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let logical_bytes = u64::try_from(bytes.len()).map_err(|_| "Artefatto analisi oltre u64")?;
    let actual = canonical
        .strip_prefix(&canonical_root)
        .map_err(|_| "Artefatto analisi fuori dal workspace")?;
    let actual = path_to_manifest_string(actual);
    let expected = canonical_artifact_path("analyses/artifacts", &hash, &canonical)?;
    if actual != expected || !actual.ends_with(".json.xz") {
        return Err("Percorso artefatto analisi non canonico".into());
    }
    let artifact: AnalysisArtifactV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("JSON analisi persistita non valido: {error}"))?;
    artifact.validate()?;
    let canonical_bytes = serialized(&artifact)?;
    if bytes != canonical_bytes {
        return Err("Serializzazione analisi persistita non canonica".into());
    }
    Ok(VerifiedStoredAnalysisArtifactV1 {
        artifact,
        blake3: hash,
        bytes: logical_bytes,
        path: PathBuf::from(actual),
    })
}

fn require_optional_text(value: &Option<String>, field: &str) -> Result<(), String> {
    match value {
        Some(value) => require_text(value, field),
        None => Err(format!(
            "{field} è obbligatorio per analisi external_source"
        )),
    }
}

fn require_text(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} non può essere vuoto"))
    } else {
        Ok(())
    }
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> AnalysisArtifactV1 {
        AnalysisArtifactV1 {
            schema_version: ANALYSIS_SCHEMA_VERSION,
            analysis_profile: ANALYSIS_PROFILE.into(),
            subject_scope: AnalysisSubjectScopeV1::Position,
            subject_id: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
            origin: AnalysisOriginV1::ExternalSource,
            origin_identity: "lichess-evals:snapshot:record:1".into(),
            source_key: Some("lichess-evals".into()),
            snapshot_id: Some("snapshot:2026-10".into()),
            locator: Some("line:1".into()),
            rights: "cc0".into(),
            method: "stockfish".into(),
            engine_build_id: None,
            search_profile_id: None,
            status: AnalysisStatusV1::Completed,
            observed_at: Some("2026-10-11".into()),
            lines: vec![AnalysisLineV1 {
                multipv_rank: 1,
                raw_score: "cp 34".into(),
                score_perspective: ScorePerspectiveV1::SideToMove,
                white_cp: Some(34),
                side_to_move_cp: Some(34),
                mate: None,
                wdl: None,
                pv_uci: vec!["e2e4".into(), "e7e5".into()],
                depth: Some(24),
                seldepth: None,
                nodes: Some(1_000_000),
                time_ms: None,
            }],
        }
    }

    #[test]
    fn valid_artifact_roundtrips() {
        let artifact = fixture();
        artifact.validate().unwrap();
        let json = serde_json::to_vec(&artifact).unwrap();
        let decoded: AnalysisArtifactV1 = serde_json::from_slice(&json).unwrap();
        assert_eq!(decoded, artifact);
    }

    #[test]
    fn external_analysis_requires_source_and_snapshot() {
        for field in ["source", "snapshot"] {
            let mut artifact = fixture();
            if field == "source" {
                artifact.source_key = None;
            } else {
                artifact.snapshot_id = None;
            }
            assert!(artifact.validate().is_err());
        }
    }

    #[test]
    fn invalid_subject_and_duplicate_multipv_fail_closed() {
        let mut artifact = fixture();
        artifact.subject_id = "position:abc".into();
        assert!(artifact.validate().is_err());

        let mut artifact = fixture();
        artifact.lines.push(artifact.lines[0].clone());
        assert!(artifact.validate().unwrap_err().contains("duplicato"));
    }

    #[test]
    fn unknown_fields_fail_closed() {
        let mut value = serde_json::to_value(fixture()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("mystery".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<AnalysisArtifactV1>(value).is_err());
    }
}
