use super::analyses::{
    store_artifact, AnalysisArtifactV1, AnalysisLineV1, AnalysisOriginV1, AnalysisStatusV1,
    AnalysisSubjectScopeV1, ScorePerspectiveV1, StoredAnalysisArtifactV1, ANALYSIS_PROFILE,
    ANALYSIS_SCHEMA_VERSION,
};
use super::chess_rules::position_identity_from_fen;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;

const SOURCE_KEY: &str = "lichess-evals";
const RIGHTS: &str = "CC0-1.0";
const METHOD: &str = "lichess_stockfish_eval_unspecified_profile";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct LichessEvalRecordV1 {
    fen: String,
    evals: Vec<LichessEvalV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct LichessEvalV1 {
    knodes: u64,
    depth: u32,
    pvs: Vec<LichessPvV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct LichessPvV1 {
    #[serde(default)]
    cp: Option<i32>,
    #[serde(default)]
    mate: Option<i32>,
    line: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StoredLichessEvalRecordV1 {
    pub(super) position_id: String,
    pub(super) evaluations: usize,
    pub(super) artifacts: Vec<StoredAnalysisArtifactV1>,
}

pub(super) fn store_record(
    root: &Path,
    input: &Path,
    snapshot_id: &str,
    observed_at: Option<&str>,
) -> Result<StoredLichessEvalRecordV1, String> {
    require_text(snapshot_id, "snapshot_id")?;
    if let Some(value) = observed_at {
        require_text(value, "observed_at")?;
    }
    let record = load_record(input)?;
    let artifacts = adapt_record(&record, snapshot_id, observed_at)?;
    let position_id = artifacts
        .first()
        .ok_or("Il record Lichess non contiene valutazioni")?
        .subject_id
        .clone();
    let mut stored = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        stored.push(store_artifact(root, artifact)?);
    }
    Ok(StoredLichessEvalRecordV1 {
        position_id,
        evaluations: stored.len(),
        artifacts: stored,
    })
}

fn load_record(path: &Path) -> Result<LichessEvalRecordV1, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("File eval Lichess {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "File eval Lichess non regolare: {}",
            path.display()
        ));
    }
    let file = File::open(path)
        .map_err(|error| format!("Apertura eval Lichess {}: {error}", path.display()))?;
    let record: LichessEvalRecordV1 = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("JSON eval Lichess non valido: {error}"))?;
    validate_record(&record)?;
    Ok(record)
}

fn validate_record(record: &LichessEvalRecordV1) -> Result<(), String> {
    require_text(&record.fen, "fen")?;
    if record.evals.is_empty() {
        return Err("Il record Lichess deve contenere almeno una valutazione".into());
    }
    position_identity_from_fen(record.fen.as_bytes())
        .map_err(|failure| format!("FEN Lichess non interpretabile: {failure:?}"))?;
    for (eval_index, evaluation) in record.evals.iter().enumerate() {
        if evaluation.depth == 0 {
            return Err(format!("evals[{eval_index}].depth deve essere positivo"));
        }
        if evaluation.pvs.is_empty() {
            return Err(format!("evals[{eval_index}].pvs non può essere vuoto"));
        }
        for (pv_index, pv) in evaluation.pvs.iter().enumerate() {
            match (pv.cp, pv.mate) {
                (Some(_), None) | (None, Some(_)) => {}
                _ => {
                    return Err(format!(
                        "evals[{eval_index}].pvs[{pv_index}] deve avere esattamente uno fra cp e mate"
                    ));
                }
            }
            let moves = pv.line.split_whitespace().collect::<Vec<_>>();
            if moves.is_empty() {
                return Err(format!(
                    "evals[{eval_index}].pvs[{pv_index}].line non può essere vuota"
                ));
            }
        }
    }
    Ok(())
}

fn adapt_record(
    record: &LichessEvalRecordV1,
    snapshot_id: &str,
    observed_at: Option<&str>,
) -> Result<Vec<AnalysisArtifactV1>, String> {
    validate_record(record)?;
    let identity = position_identity_from_fen(record.fen.as_bytes())
        .map_err(|failure| format!("FEN Lichess non interpretabile: {failure:?}"))?;
    let mut artifacts = Vec::with_capacity(record.evals.len());
    for (eval_index, evaluation) in record.evals.iter().enumerate() {
        let nodes = evaluation
            .knodes
            .checked_mul(1_000)
            .ok_or("knodes Lichess oltre il range u64")?;
        let mut lines = Vec::with_capacity(evaluation.pvs.len());
        for (pv_index, pv) in evaluation.pvs.iter().enumerate() {
            let multipv_rank = u32::try_from(pv_index + 1)
                .map_err(|_| "Numero di PV Lichess oltre il range u32")?;
            let raw_score = match (pv.cp, pv.mate) {
                (Some(cp), None) => format!("cp {cp}"),
                (None, Some(mate)) => format!("mate {mate}"),
                _ => return Err("Punteggio Lichess ambiguo".into()),
            };
            lines.push(AnalysisLineV1 {
                multipv_rank,
                raw_score,
                // Lichess stores these evaluation values from White's point of view.
                // Keep the raw value as authoritative and do not invent an engine profile.
                score_perspective: ScorePerspectiveV1::White,
                white_cp: pv.cp,
                side_to_move_cp: None,
                mate: pv.mate,
                wdl: None,
                pv_uci: pv.line.split_whitespace().map(str::to_owned).collect(),
                depth: Some(evaluation.depth),
                seldepth: None,
                nodes: Some(nodes),
                time_ms: None,
            });
        }
        let evaluation_number = eval_index + 1;
        let origin_identity = format!(
            "{SOURCE_KEY}:{snapshot_id}:position:{}:evaluation:{evaluation_number}",
            identity.position_id
        );
        let artifact = AnalysisArtifactV1 {
            schema_version: ANALYSIS_SCHEMA_VERSION,
            analysis_profile: ANALYSIS_PROFILE.into(),
            subject_scope: AnalysisSubjectScopeV1::Position,
            subject_id: identity.position_id.clone(),
            origin: AnalysisOriginV1::ExternalSource,
            origin_identity,
            source_key: Some(SOURCE_KEY.into()),
            snapshot_id: Some(snapshot_id.into()),
            locator: Some(format!("fen:{}#evaluation:{evaluation_number}", record.fen)),
            rights: RIGHTS.into(),
            method: METHOD.into(),
            engine_build_id: None,
            search_profile_id: None,
            status: AnalysisStatusV1::Completed,
            observed_at: observed_at.map(str::to_owned),
            lines,
        };
        artifact.validate()?;
        artifacts.push(artifact);
    }
    Ok(artifacts)
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

    fn fixture() -> LichessEvalRecordV1 {
        LichessEvalRecordV1 {
            fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -".into(),
            evals: vec![
                LichessEvalV1 {
                    knodes: 1234,
                    depth: 30,
                    pvs: vec![LichessPvV1 {
                        cp: Some(20),
                        mate: None,
                        line: "e2e4 e7e5".into(),
                    }],
                },
                LichessEvalV1 {
                    knodes: 5678,
                    depth: 28,
                    pvs: vec![
                        LichessPvV1 {
                            cp: Some(18),
                            mate: None,
                            line: "e2e4 c7c5".into(),
                        },
                        LichessPvV1 {
                            cp: Some(12),
                            mate: None,
                            line: "d2d4 d7d5".into(),
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn all_source_evaluations_are_preserved_as_distinct_artifacts() {
        let artifacts =
            adapt_record(&fixture(), "snapshot:2026-10-05", Some("2026-10-05")).unwrap();
        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0].subject_id, artifacts[1].subject_id);
        assert_eq!(artifacts[0].lines.len(), 1);
        assert_eq!(artifacts[1].lines.len(), 2);
        assert_eq!(artifacts[0].lines[0].nodes, Some(1_234_000));
        assert_eq!(artifacts[0].lines[0].depth, Some(30));
        assert_eq!(artifacts[0].source_key.as_deref(), Some("lichess-evals"));
        assert_eq!(artifacts[0].engine_build_id, None);
        assert_eq!(artifacts[0].search_profile_id, None);
    }

    #[test]
    fn four_field_lichess_fen_uses_standard_position_identity() {
        let artifacts = adapt_record(&fixture(), "snapshot:1", None).unwrap();
        assert_eq!(
            artifacts[0].subject_id,
            "8a6adc669f50706498570870a2febd962b5cc2111ddaaaea73bbdfbbd96ea2b4"
        );
    }

    #[test]
    fn ambiguous_scores_and_empty_pvs_fail_closed() {
        let mut record = fixture();
        record.evals[0].pvs[0].mate = Some(2);
        assert!(validate_record(&record)
            .unwrap_err()
            .contains("esattamente uno"));

        let mut record = fixture();
        record.evals[0].pvs.clear();
        assert!(validate_record(&record)
            .unwrap_err()
            .contains("non può essere vuoto"));
    }

    #[test]
    fn mate_zero_is_preserved_as_a_valid_lichess_game_over_score() {
        let mut record = fixture();
        record.evals.truncate(1);
        record.evals[0].pvs[0].cp = None;
        record.evals[0].pvs[0].mate = Some(0);
        let artifacts = adapt_record(&record, "snapshot:mate-zero", None).unwrap();
        assert_eq!(artifacts[0].lines[0].raw_score, "mate 0");
        assert_eq!(artifacts[0].lines[0].mate, Some(0));
        assert_eq!(
            artifacts[0].lines[0].score_perspective,
            ScorePerspectiveV1::White
        );
    }

    #[test]
    fn unknown_fields_fail_closed() {
        let value = serde_json::json!({
            "fen": "8/8/8/8/8/8/8/K6k w - -",
            "evals": [{"knodes": 1, "depth": 1, "pvs": [{"cp": 0, "line": "a1a2"}], "mystery": true}]
        });
        assert!(serde_json::from_value::<LichessEvalRecordV1>(value).is_err());
    }
}
