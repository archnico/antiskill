use super::analyses::{
    load_verified, AnalysisArtifactV1, AnalysisOriginV1, AnalysisStatusV1, AnalysisSubjectScopeV1,
    VerifiedStoredAnalysisArtifactV1,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

pub(super) const REQUIREMENT_SCHEMA_VERSION: u32 = 1;
pub(super) const REQUIREMENT_PROFILE: &str = "antiskill_analysis_requirement_v1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum AnalysisPreferenceAxisV1 {
    Origin,
    Method,
    Completion,
    Multipv,
    Nodes,
    Depth,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct AnalysisRequirementV1 {
    pub(super) schema_version: u32,
    pub(super) requirement_profile: String,
    pub(super) subject_scope: AnalysisSubjectScopeV1,
    pub(super) subject_id: String,
    pub(super) allowed_origins: Vec<AnalysisOriginV1>,
    pub(super) origin_preference: Vec<AnalysisOriginV1>,
    pub(super) allowed_methods: Vec<String>,
    pub(super) method_preference: Vec<String>,
    pub(super) selection_priority: Vec<AnalysisPreferenceAxisV1>,
    pub(super) allowed_rights: Vec<String>,
    pub(super) required_engine_build_id: Option<String>,
    pub(super) required_search_profile_id: Option<String>,
    pub(super) accept_partial: bool,
    pub(super) min_multipv: u32,
    pub(super) min_depth: Option<u32>,
    pub(super) min_nodes: Option<u64>,
    pub(super) require_wdl: bool,
    pub(super) require_pv: bool,
}

#[derive(Clone, Debug)]
pub(super) struct AnalysisSelectionV1 {
    pub(super) candidates: usize,
    pub(super) qualified: usize,
    pub(super) selected: VerifiedStoredAnalysisArtifactV1,
}

impl AnalysisRequirementV1 {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.schema_version != REQUIREMENT_SCHEMA_VERSION {
            return Err("Versione schema requisito analisi non supportata".into());
        }
        if self.requirement_profile != REQUIREMENT_PROFILE {
            return Err("Profilo requisito analisi non supportato".into());
        }
        if !is_lower_hex_64(&self.subject_id) {
            return Err("subject_id del requisito deve essere BLAKE3 lowercase di 64 cifre".into());
        }
        validate_unique_origins(&self.allowed_origins, "allowed_origins")?;
        validate_unique_origins(&self.origin_preference, "origin_preference")?;
        if !same_origin_set(&self.allowed_origins, &self.origin_preference) {
            return Err("origin_preference deve contenere esattamente allowed_origins".into());
        }
        validate_unique_texts(&self.allowed_methods, "allowed_methods")?;
        validate_unique_texts(&self.method_preference, "method_preference")?;
        if !same_text_set(&self.allowed_methods, &self.method_preference) {
            return Err("method_preference deve contenere esattamente allowed_methods".into());
        }
        validate_selection_priority(&self.selection_priority)?;
        validate_unique_texts(&self.allowed_rights, "allowed_rights")?;
        for (value, field) in [
            (&self.required_engine_build_id, "required_engine_build_id"),
            (
                &self.required_search_profile_id,
                "required_search_profile_id",
            ),
        ] {
            if let Some(value) = value {
                require_text(value, field)?;
            }
        }
        if self.min_multipv == 0 {
            return Err("min_multipv deve essere almeno 1".into());
        }
        if self.min_depth == Some(0) {
            return Err("min_depth, se presente, deve essere positivo".into());
        }
        if self.min_nodes == Some(0) {
            return Err("min_nodes, se presente, deve essere positivo".into());
        }
        Ok(())
    }
}

pub(super) fn load_and_validate(path: &Path) -> Result<AnalysisRequirementV1, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("File requisito analisi {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "File requisito analisi non regolare: {}",
            path.display()
        ));
    }
    let file = File::open(path)
        .map_err(|error| format!("Apertura requisito analisi {}: {error}", path.display()))?;
    let requirement: AnalysisRequirementV1 = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("JSON requisito analisi non valido: {error}"))?;
    requirement.validate()?;
    Ok(requirement)
}

pub(super) fn select(
    root: &Path,
    requirement: &AnalysisRequirementV1,
    paths: &[PathBuf],
) -> Result<AnalysisSelectionV1, String> {
    requirement.validate()?;
    if paths.is_empty() {
        return Err("La selezione richiede almeno un --analysis PATH".into());
    }
    for (index, path) in paths.iter().enumerate() {
        if paths[..index].iter().any(|previous| previous == path) {
            return Err(format!("Candidato analisi duplicato: {}", path.display()));
        }
    }
    let mut verified = Vec::with_capacity(paths.len());
    for path in paths {
        verified.push(load_verified(root, path)?);
    }
    select_verified(requirement, verified)
}

fn select_verified(
    requirement: &AnalysisRequirementV1,
    candidates: Vec<VerifiedStoredAnalysisArtifactV1>,
) -> Result<AnalysisSelectionV1, String> {
    let count = candidates.len();
    let mut qualified = candidates
        .into_iter()
        .filter(|candidate| qualifies(requirement, &candidate.artifact))
        .collect::<Vec<_>>();
    let qualified_count = qualified.len();
    if qualified.is_empty() {
        return Err("Nessuna analisi soddisfa il requisito dichiarato".into());
    }
    qualified.sort_by(|left, right| compare_candidates(requirement, left, right));
    Ok(AnalysisSelectionV1 {
        candidates: count,
        qualified: qualified_count,
        selected: qualified.remove(0),
    })
}

fn qualifies(requirement: &AnalysisRequirementV1, artifact: &AnalysisArtifactV1) -> bool {
    if artifact.subject_scope != requirement.subject_scope
        || artifact.subject_id != requirement.subject_id
        || !requirement.allowed_origins.contains(&artifact.origin)
        || !requirement.allowed_methods.contains(&artifact.method)
        || !requirement.allowed_rights.contains(&artifact.rights)
    {
        return false;
    }
    if !requirement.accept_partial && artifact.status != AnalysisStatusV1::Completed {
        return false;
    }
    if let Some(required) = &requirement.required_engine_build_id {
        if artifact.engine_build_id.as_ref() != Some(required) {
            return false;
        }
    }
    if let Some(required) = &requirement.required_search_profile_id {
        if artifact.search_profile_id.as_ref() != Some(required) {
            return false;
        }
    }
    let required_lines = match lines_for_required_ranks(artifact, requirement.min_multipv) {
        Some(lines) => lines,
        None => return false,
    };
    for line in required_lines {
        if let Some(min_depth) = requirement.min_depth {
            if line.depth.is_none_or(|depth| depth < min_depth) {
                return false;
            }
        }
        if let Some(min_nodes) = requirement.min_nodes {
            if line.nodes.is_none_or(|nodes| nodes < min_nodes) {
                return false;
            }
        }
        if requirement.require_wdl && line.wdl.is_none() {
            return false;
        }
        if requirement.require_pv && line.pv_uci.is_empty() {
            return false;
        }
    }
    true
}

fn compare_candidates(
    requirement: &AnalysisRequirementV1,
    left: &VerifiedStoredAnalysisArtifactV1,
    right: &VerifiedStoredAnalysisArtifactV1,
) -> Ordering {
    for axis in &requirement.selection_priority {
        let ordering = compare_axis(*axis, requirement, &left.artifact, &right.artifact);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.blake3.cmp(&right.blake3)
}

fn compare_axis(
    axis: AnalysisPreferenceAxisV1,
    requirement: &AnalysisRequirementV1,
    left: &AnalysisArtifactV1,
    right: &AnalysisArtifactV1,
) -> Ordering {
    match axis {
        AnalysisPreferenceAxisV1::Origin => {
            preference_rank(&requirement.origin_preference, &left.origin).cmp(&preference_rank(
                &requirement.origin_preference,
                &right.origin,
            ))
        }
        AnalysisPreferenceAxisV1::Method => {
            preference_rank(&requirement.method_preference, &left.method).cmp(&preference_rank(
                &requirement.method_preference,
                &right.method,
            ))
        }
        AnalysisPreferenceAxisV1::Completion => {
            let left_completed = left.status == AnalysisStatusV1::Completed;
            let right_completed = right.status == AnalysisStatusV1::Completed;
            right_completed.cmp(&left_completed)
        }
        AnalysisPreferenceAxisV1::Multipv => {
            contiguous_multipv(right).cmp(&contiguous_multipv(left))
        }
        AnalysisPreferenceAxisV1::Nodes => minimum_nodes(right).cmp(&minimum_nodes(left)),
        AnalysisPreferenceAxisV1::Depth => minimum_depth(right).cmp(&minimum_depth(left)),
    }
}

fn preference_rank<T: PartialEq>(values: &[T], value: &T) -> usize {
    values
        .iter()
        .position(|candidate| candidate == value)
        .unwrap_or(usize::MAX)
}

fn lines_for_required_ranks(
    artifact: &AnalysisArtifactV1,
    min_multipv: u32,
) -> Option<Vec<&super::analyses::AnalysisLineV1>> {
    (1..=min_multipv)
        .map(|rank| artifact.lines.iter().find(|line| line.multipv_rank == rank))
        .collect()
}

fn contiguous_multipv(artifact: &AnalysisArtifactV1) -> u32 {
    let mut rank = 1u32;
    while artifact.lines.iter().any(|line| line.multipv_rank == rank) {
        match rank.checked_add(1) {
            Some(next) => rank = next,
            None => return u32::MAX,
        }
    }
    rank - 1
}

fn minimum_nodes(artifact: &AnalysisArtifactV1) -> u64 {
    let coverage = contiguous_multipv(artifact);
    if coverage == 0 {
        return 0;
    }
    (1..=coverage)
        .map(|rank| {
            artifact
                .lines
                .iter()
                .find(|line| line.multipv_rank == rank)
                .and_then(|line| line.nodes)
        })
        .collect::<Option<Vec<_>>>()
        .and_then(|values| values.into_iter().min())
        .unwrap_or(0)
}

fn minimum_depth(artifact: &AnalysisArtifactV1) -> u32 {
    let coverage = contiguous_multipv(artifact);
    if coverage == 0 {
        return 0;
    }
    (1..=coverage)
        .map(|rank| {
            artifact
                .lines
                .iter()
                .find(|line| line.multipv_rank == rank)
                .and_then(|line| line.depth)
        })
        .collect::<Option<Vec<_>>>()
        .and_then(|values| values.into_iter().min())
        .unwrap_or(0)
}

pub(super) fn origin_name(origin: AnalysisOriginV1) -> &'static str {
    match origin {
        AnalysisOriginV1::ExternalSource => "external_source",
        AnalysisOriginV1::LocalRun => "local_run",
        AnalysisOriginV1::ContributedRun => "contributed_run",
    }
}

fn validate_selection_priority(values: &[AnalysisPreferenceAxisV1]) -> Result<(), String> {
    if values.is_empty() {
        return Err("selection_priority non può essere vuoto".into());
    }
    for (index, value) in values.iter().enumerate() {
        if values[..index].contains(value) {
            return Err("selection_priority contiene un asse duplicato".into());
        }
    }
    Ok(())
}

fn validate_unique_origins(values: &[AnalysisOriginV1], field: &str) -> Result<(), String> {
    if values.is_empty() {
        return Err(format!("{field} non può essere vuoto"));
    }
    for (index, value) in values.iter().enumerate() {
        if values[..index].contains(value) {
            return Err(format!(
                "{field} contiene un valore duplicato: {}",
                origin_name(*value)
            ));
        }
    }
    Ok(())
}

fn same_origin_set(left: &[AnalysisOriginV1], right: &[AnalysisOriginV1]) -> bool {
    left.len() == right.len() && left.iter().all(|value| right.contains(value))
}

fn validate_unique_texts(values: &[String], field: &str) -> Result<(), String> {
    if values.is_empty() {
        return Err(format!("{field} non può essere vuoto"));
    }
    for (index, value) in values.iter().enumerate() {
        require_text(value, field)?;
        if values[..index].contains(value) {
            return Err(format!("{field} contiene un valore duplicato: {value}"));
        }
    }
    Ok(())
}

fn same_text_set(left: &[String], right: &[String]) -> bool {
    left.len() == right.len() && left.iter().all(|value| right.contains(value))
}

fn require_text(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} non può contenere stringhe vuote"))
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
    use crate::inspector::analyses::{AnalysisLineV1, ScorePerspectiveV1};

    const SUBJECT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn requirement() -> AnalysisRequirementV1 {
        AnalysisRequirementV1 {
            schema_version: REQUIREMENT_SCHEMA_VERSION,
            requirement_profile: REQUIREMENT_PROFILE.into(),
            subject_scope: AnalysisSubjectScopeV1::Position,
            subject_id: SUBJECT.into(),
            allowed_origins: vec![AnalysisOriginV1::ExternalSource, AnalysisOriginV1::LocalRun],
            origin_preference: vec![AnalysisOriginV1::LocalRun, AnalysisOriginV1::ExternalSource],
            allowed_methods: vec!["standardized_stockfish".into(), "lichess_external".into()],
            method_preference: vec!["standardized_stockfish".into(), "lichess_external".into()],
            selection_priority: vec![
                AnalysisPreferenceAxisV1::Origin,
                AnalysisPreferenceAxisV1::Method,
                AnalysisPreferenceAxisV1::Completion,
                AnalysisPreferenceAxisV1::Multipv,
                AnalysisPreferenceAxisV1::Nodes,
                AnalysisPreferenceAxisV1::Depth,
            ],
            allowed_rights: vec!["private_research".into(), "CC0-1.0".into()],
            required_engine_build_id: None,
            required_search_profile_id: None,
            accept_partial: false,
            min_multipv: 1,
            min_depth: Some(20),
            min_nodes: Some(1_000),
            require_wdl: false,
            require_pv: true,
        }
    }

    fn artifact(
        origin: AnalysisOriginV1,
        method: &str,
        rights: &str,
        status: AnalysisStatusV1,
        depth: u32,
        nodes: u64,
        multipv: u32,
    ) -> AnalysisArtifactV1 {
        AnalysisArtifactV1 {
            schema_version: 1,
            analysis_profile: "antiskill_analysis_artifact_v1".into(),
            subject_scope: AnalysisSubjectScopeV1::Position,
            subject_id: SUBJECT.into(),
            origin,
            origin_identity: format!("origin:{method}:{depth}:{nodes}:{multipv}"),
            source_key: (origin == AnalysisOriginV1::ExternalSource).then(|| "source".into()),
            snapshot_id: (origin == AnalysisOriginV1::ExternalSource).then(|| "snapshot".into()),
            locator: None,
            rights: rights.into(),
            method: method.into(),
            engine_build_id: None,
            search_profile_id: None,
            status,
            observed_at: None,
            lines: (1..=multipv)
                .map(|rank| AnalysisLineV1 {
                    multipv_rank: rank,
                    raw_score: "cp 10".into(),
                    score_perspective: ScorePerspectiveV1::White,
                    white_cp: Some(10),
                    side_to_move_cp: None,
                    mate: None,
                    wdl: None,
                    pv_uci: vec!["e2e4".into()],
                    depth: Some(depth),
                    seldepth: None,
                    nodes: Some(nodes),
                    time_ms: None,
                })
                .collect(),
        }
    }

    fn verified(hash: char, artifact: AnalysisArtifactV1) -> VerifiedStoredAnalysisArtifactV1 {
        VerifiedStoredAnalysisArtifactV1 {
            artifact,
            blake3: hash.to_string().repeat(64),
            bytes: 1,
            path: PathBuf::from(format!(
                "inspector/analyses/artifacts/{}.json.xz",
                hash.to_string().repeat(64)
            )),
        }
    }

    #[test]
    fn policy_orders_are_explicit_and_must_match_allowed_sets() {
        let mut req = requirement();
        req.method_preference.pop();
        assert!(req.validate().unwrap_err().contains("method_preference"));
        let mut req = requirement();
        req.origin_preference = vec![AnalysisOriginV1::LocalRun, AnalysisOriginV1::LocalRun];
        assert!(req.validate().unwrap_err().contains("duplicato"));
        let mut req = requirement();
        req.selection_priority.clear();
        assert!(req.validate().unwrap_err().contains("selection_priority"));
        let mut req = requirement();
        req.selection_priority = vec![
            AnalysisPreferenceAxisV1::Origin,
            AnalysisPreferenceAxisV1::Origin,
        ];
        assert!(req.validate().unwrap_err().contains("duplicato"));
    }

    #[test]
    fn explicit_origin_and_method_preference_beat_raw_depth() {
        let req = requirement();
        let local = verified(
            'b',
            artifact(
                AnalysisOriginV1::LocalRun,
                "standardized_stockfish",
                "private_research",
                AnalysisStatusV1::Completed,
                22,
                2_000,
                1,
            ),
        );
        let external = verified(
            'a',
            artifact(
                AnalysisOriginV1::ExternalSource,
                "lichess_external",
                "CC0-1.0",
                AnalysisStatusV1::Completed,
                40,
                50_000_000,
                5,
            ),
        );
        let selected = select_verified(&req, vec![external, local]).unwrap();
        assert_eq!(
            selected.selected.artifact.origin,
            AnalysisOriginV1::LocalRun
        );
        assert_eq!(selected.qualified, 2);
    }

    #[test]
    fn selection_axis_order_is_declared_by_the_requirement() {
        let mut req = requirement();
        req.allowed_methods = vec!["preferred_method".into(), "other_method".into()];
        req.method_preference = req.allowed_methods.clone();
        req.selection_priority = vec![
            AnalysisPreferenceAxisV1::Method,
            AnalysisPreferenceAxisV1::Origin,
            AnalysisPreferenceAxisV1::Completion,
            AnalysisPreferenceAxisV1::Multipv,
            AnalysisPreferenceAxisV1::Nodes,
            AnalysisPreferenceAxisV1::Depth,
        ];
        let local = verified(
            'b',
            artifact(
                AnalysisOriginV1::LocalRun,
                "other_method",
                "private_research",
                AnalysisStatusV1::Completed,
                40,
                50_000_000,
                5,
            ),
        );
        let external = verified(
            'a',
            artifact(
                AnalysisOriginV1::ExternalSource,
                "preferred_method",
                "CC0-1.0",
                AnalysisStatusV1::Completed,
                22,
                2_000,
                1,
            ),
        );
        let selected = select_verified(&req, vec![local, external]).unwrap();
        assert_eq!(
            selected.selected.artifact.origin,
            AnalysisOriginV1::ExternalSource
        );
        assert_eq!(selected.selected.artifact.method, "preferred_method");
    }

    #[test]
    fn omitted_axes_do_not_influence_selection() {
        let mut req = requirement();
        req.selection_priority = vec![AnalysisPreferenceAxisV1::Method];
        req.allowed_methods = vec!["same_method".into()];
        req.method_preference = req.allowed_methods.clone();
        let shallow_local = verified(
            'a',
            artifact(
                AnalysisOriginV1::LocalRun,
                "same_method",
                "private_research",
                AnalysisStatusV1::Completed,
                20,
                1_000,
                1,
            ),
        );
        let deep_external = verified(
            'b',
            artifact(
                AnalysisOriginV1::ExternalSource,
                "same_method",
                "CC0-1.0",
                AnalysisStatusV1::Completed,
                50,
                50_000_000,
                5,
            ),
        );
        let selected = select_verified(&req, vec![deep_external, shallow_local]).unwrap();
        assert_eq!(selected.selected.blake3, "a".repeat(64));
    }

    #[test]
    fn minimum_multipv_requires_contiguous_required_ranks() {
        let mut req = requirement();
        req.min_multipv = 2;
        let mut candidate = artifact(
            AnalysisOriginV1::ExternalSource,
            "lichess_external",
            "CC0-1.0",
            AnalysisStatusV1::Completed,
            30,
            10_000,
            2,
        );
        candidate.lines[1].multipv_rank = 3;
        assert!(!qualifies(&req, &candidate));
    }

    #[test]
    fn missing_quality_metadata_ranks_below_complete_observations() {
        let req = requirement();
        let mut missing = artifact(
            AnalysisOriginV1::LocalRun,
            "standardized_stockfish",
            "private_research",
            AnalysisStatusV1::Completed,
            40,
            50_000,
            1,
        );
        missing.lines[0].nodes = None;
        let observed = artifact(
            AnalysisOriginV1::LocalRun,
            "standardized_stockfish",
            "private_research",
            AnalysisStatusV1::Completed,
            22,
            2_000,
            1,
        );
        let selected =
            select_verified(&req, vec![verified('a', missing), verified('b', observed)]).unwrap();
        assert_eq!(selected.selected.blake3, "b".repeat(64));
    }

    #[test]
    fn requirements_do_not_silently_accept_insufficient_quality() {
        let mut req = requirement();
        req.allowed_origins = vec![AnalysisOriginV1::ExternalSource];
        req.origin_preference = req.allowed_origins.clone();
        req.allowed_methods = vec!["lichess_external".into()];
        req.method_preference = req.allowed_methods.clone();
        req.allowed_rights = vec!["CC0-1.0".into()];
        req.min_depth = Some(35);
        let weak = verified(
            'a',
            artifact(
                AnalysisOriginV1::ExternalSource,
                "lichess_external",
                "CC0-1.0",
                AnalysisStatusV1::Completed,
                30,
                10_000_000,
                1,
            ),
        );
        assert!(select_verified(&req, vec![weak])
            .unwrap_err()
            .contains("Nessuna analisi"));
    }
}
