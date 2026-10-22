use super::{collection, PlayerSideV1, ProductPlayerSideEvidenceV1};
use crate::identity::{fold_identity_latin, player_name_signature as name_signature};
use serde::{Serialize, Serializer};
use std::collections::BTreeSet;
use std::path::Path;

const PROFILE: &str = "antiskill_player_identity_view_v1";
const ENTITY_PROFILE: &str = "antiskill_player_entity_v1";
const CANDIDATE_POLICY: &str = "normalized_name_signature_candidate_v1";

fn serialize_public_occurrence_id<S>(value: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let public = crate::archive::game_id::PublicGameIdV1::from_occurrence_id(value)
        .map_err(serde::ser::Error::custom)?;
    serializer.serialize_str(&public.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PlayerIdentityClaimV1 {
    #[serde(serialize_with = "serialize_public_occurrence_id")]
    pub(crate) occurrence_id: String,
    pub(crate) side: PlayerSideV1,
    pub(crate) names: Vec<String>,
    pub(crate) raw_fide_ids: Vec<String>,
    pub(crate) source_blake3: String,
    pub(crate) source_bytes: u64,
    pub(crate) source_start: u64,
    pub(crate) source_end: u64,
    pub(crate) ordinal: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PlayerIdentityConflictV1 {
    #[serde(serialize_with = "serialize_public_occurrence_id")]
    pub(crate) occurrence_id: String,
    pub(crate) side: PlayerSideV1,
    pub(crate) names: Vec<String>,
    pub(crate) raw_fide_ids: Vec<String>,
    pub(crate) normalized_fide_ids: Vec<String>,
    pub(crate) invalid_fide_id_value: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PlayerIdentityCandidateV1 {
    #[serde(serialize_with = "serialize_public_occurrence_id")]
    pub(crate) occurrence_id: String,
    pub(crate) side: PlayerSideV1,
    pub(crate) names: Vec<String>,
    pub(crate) candidate_policy: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PlayerIdentityViewV1 {
    pub(crate) profile: &'static str,
    pub(crate) entity_profile: &'static str,
    pub(crate) player_entity_id: String,
    pub(crate) fide_id: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) claims: Vec<PlayerIdentityClaimV1>,
    pub(crate) conflicts: Vec<PlayerIdentityConflictV1>,
    pub(crate) candidates: Vec<PlayerIdentityCandidateV1>,
    pub(crate) candidates_scanned: bool,
    pub(crate) collection_blake3: String,
    pub(crate) collection_bytes: u64,
    pub(crate) source_occurrences: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct FederationBridgeObservationV2 {
    pub(crate) source_profile: String,
    pub(crate) source_blake3: String,
    pub(crate) source_bytes: u64,
    pub(crate) source_line: u64,
    pub(crate) raw_name: String,
    pub(crate) name_signature: String,
    pub(crate) signature_unique: bool,
    pub(crate) fsi_id: String,
    pub(crate) fide_id: String,
    pub(crate) birth_year: Option<u16>,
    pub(crate) birth_date: Option<String>,
    pub(crate) federation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FederationBridgeInputV2 {
    pub(crate) fsi_id: String,
    pub(crate) fide_id: String,
    pub(crate) observations: Vec<FederationBridgeObservationV2>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ExternalPlayerIdV2 {
    pub(crate) namespace: &'static str,
    pub(crate) value: String,
    pub(crate) evidence_class: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct VerifiedPlayerOccurrenceV2 {
    #[serde(serialize_with = "serialize_public_occurrence_id")]
    pub(crate) occurrence_id: String,
    pub(crate) side: PlayerSideV1,
    pub(crate) names: Vec<String>,
    pub(crate) evidence_class: &'static str,
    pub(crate) matched_name_signature: Option<String>,
    pub(crate) raw_fide_ids: Vec<String>,
    pub(crate) source_blake3: String,
    pub(crate) source_bytes: u64,
    pub(crate) source_start: u64,
    pub(crate) source_end: u64,
    pub(crate) ordinal: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OfficialContextInputV3 {
    pub(crate) context_id: String,
    pub(crate) event_code: String,
    pub(crate) event_name: String,
    pub(crate) event_start_date: String,
    pub(crate) event_end_date: String,
    pub(crate) round: u32,
    pub(crate) board: u32,
    pub(crate) target_side: PlayerSideV1,
    pub(crate) target_fide_id: String,
    pub(crate) opponent_fide_id: String,
    pub(crate) opponent_name: String,
    pub(crate) result: String,
    pub(crate) roster_source_blake3: String,
    pub(crate) results_source_blake3: String,
    pub(crate) crosstable_source_blake3: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct OfficialContextEvidenceV3 {
    #[serde(serialize_with = "serialize_public_occurrence_id")]
    pub(crate) occurrence_id: String,
    pub(crate) side: PlayerSideV1,
    pub(crate) context_id: String,
    pub(crate) event_code: String,
    pub(crate) event_name: String,
    pub(crate) round: u32,
    pub(crate) board: u32,
    pub(crate) opponent_fide_id: String,
    pub(crate) opponent_name: String,
    pub(crate) result: String,
    pub(crate) roster_source_blake3: String,
    pub(crate) results_source_blake3: String,
    pub(crate) crosstable_source_blake3: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct OfficialContextCandidateV3 {
    #[serde(serialize_with = "serialize_public_occurrence_id")]
    pub(crate) occurrence_id: String,
    pub(crate) side: PlayerSideV1,
    pub(crate) matching_context_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PlayerIdentityViewV2 {
    pub(crate) profile: &'static str,
    pub(crate) entity_profile: &'static str,
    pub(crate) player_entity_id: String,
    pub(crate) fide_id: String,
    pub(crate) external_ids: Vec<ExternalPlayerIdV2>,
    pub(crate) aliases: Vec<String>,
    pub(crate) verified_occurrences: Vec<VerifiedPlayerOccurrenceV2>,
    pub(crate) direct_count: usize,
    pub(crate) promoted_count: usize,
    pub(crate) conflicts: Vec<PlayerIdentityConflictV1>,
    pub(crate) unresolved_candidates: Vec<PlayerIdentityCandidateV1>,
    pub(crate) bridge_evidence: Vec<FederationBridgeObservationV2>,
    pub(crate) blocked_signatures: Vec<String>,
    pub(crate) collection_blake3: String,
    pub(crate) collection_bytes: u64,
    pub(crate) source_occurrences: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PlayerIdentityViewV3 {
    pub(crate) profile: &'static str,
    pub(crate) entity_profile: &'static str,
    pub(crate) player_entity_id: String,
    pub(crate) fide_id: String,
    pub(crate) external_ids: Vec<ExternalPlayerIdV2>,
    pub(crate) aliases: Vec<String>,
    pub(crate) verified_occurrences: Vec<VerifiedPlayerOccurrenceV2>,
    pub(crate) direct_count: usize,
    pub(crate) promoted_count: usize,
    pub(crate) context_count: usize,
    pub(crate) conflicts: Vec<PlayerIdentityConflictV1>,
    pub(crate) unresolved_candidates: Vec<PlayerIdentityCandidateV1>,
    pub(crate) unresolved_context_candidates: Vec<OfficialContextCandidateV3>,
    pub(crate) bridge_evidence: Vec<FederationBridgeObservationV2>,
    pub(crate) blocked_signatures: Vec<String>,
    pub(crate) context_evidence: Vec<OfficialContextEvidenceV3>,
    pub(crate) collection_blake3: String,
    pub(crate) collection_bytes: u64,
    pub(crate) source_occurrences: u64,
}

fn canonical_fide_id(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let numeric = value.parse::<u64>().ok()?;
    (numeric > 0).then(|| numeric.to_string())
}

fn exact_values(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalized_ids(values: &[String]) -> (Vec<String>, bool) {
    let mut ids = BTreeSet::new();
    let mut invalid = false;
    for value in values {
        match canonical_fide_id(value) {
            Some(id) => {
                ids.insert(id);
            }
            None => invalid = true,
        }
    }
    (ids.into_iter().collect(), invalid)
}

fn entity_id(fide_id: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(ENTITY_PROFILE.as_bytes());
    hasher.update(b"\0fide\0");
    hasher.update(fide_id.as_bytes());
    hasher.update(b"\n");
    format!("p1-{}", hasher.finalize().to_hex())
}

fn side_rank(side: PlayerSideV1) -> u8 {
    match side {
        PlayerSideV1::White => 0,
        PlayerSideV1::Black => 1,
    }
}

fn claim_from(evidence: ProductPlayerSideEvidenceV1) -> PlayerIdentityClaimV1 {
    PlayerIdentityClaimV1 {
        occurrence_id: evidence.occurrence_id,
        side: evidence.side,
        names: exact_values(&evidence.names),
        raw_fide_ids: exact_values(&evidence.fide_id_values),
        source_blake3: evidence.source_blake3,
        source_bytes: evidence.source_bytes,
        source_start: evidence.source_start,
        source_end: evidence.source_end,
        ordinal: evidence.ordinal,
    }
}

pub(crate) fn query(
    root: &Path,
    collection_path: &Path,
    fide_id: &str,
    include_candidates: bool,
) -> Result<PlayerIdentityViewV1, String> {
    let fide_id =
        canonical_fide_id(fide_id).ok_or_else(|| format!("FIDE ID non valido: {fide_id}"))?;
    let mut claims = Vec::new();
    let mut conflicts = Vec::new();
    let scan =
        collection::for_each_verified_player_side_evidence(root, collection_path, |evidence| {
            if evidence.fide_id_values.is_empty() {
                return Ok(());
            }
            let (ids, invalid) = normalized_ids(&evidence.fide_id_values);
            if invalid || ids.len() != 1 {
                if ids.iter().any(|id| id == &fide_id) {
                    conflicts.push(PlayerIdentityConflictV1 {
                        occurrence_id: evidence.occurrence_id,
                        side: evidence.side,
                        names: exact_values(&evidence.names),
                        raw_fide_ids: exact_values(&evidence.fide_id_values),
                        normalized_fide_ids: ids,
                        invalid_fide_id_value: invalid,
                    });
                }
                return Ok(());
            }
            if ids[0] == fide_id {
                claims.push(claim_from(evidence));
            }
            Ok(())
        })?;

    claims.sort_by(|left, right| {
        (&left.occurrence_id, side_rank(left.side))
            .cmp(&(&right.occurrence_id, side_rank(right.side)))
    });
    conflicts.sort_by(|left, right| {
        (&left.occurrence_id, side_rank(left.side))
            .cmp(&(&right.occurrence_id, side_rank(right.side)))
    });

    let aliases = claims
        .iter()
        .flat_map(|claim| claim.names.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let signatures = aliases
        .iter()
        .filter_map(|alias| name_signature(alias))
        .collect::<BTreeSet<_>>();
    let mut candidates = Vec::new();
    if include_candidates && !signatures.is_empty() {
        let second = collection::for_each_verified_player_side_evidence(
            root,
            collection_path,
            |evidence| {
                if !evidence.fide_id_values.is_empty() {
                    return Ok(());
                }
                let names = exact_values(&evidence.names);
                if names
                    .iter()
                    .filter_map(|name| name_signature(name))
                    .any(|signature| signatures.contains(&signature))
                {
                    candidates.push(PlayerIdentityCandidateV1 {
                        occurrence_id: evidence.occurrence_id,
                        side: evidence.side,
                        names,
                        candidate_policy: CANDIDATE_POLICY,
                    });
                }
                Ok(())
            },
        )?;
        if second.collection_blake3 != scan.collection_blake3
            || second.collection_bytes != scan.collection_bytes
            || second.source_occurrences != scan.source_occurrences
        {
            return Err("Collection cambiata durante la scansione candidati giocatore".into());
        }
        candidates.sort_by(|left, right| {
            (&left.occurrence_id, side_rank(left.side))
                .cmp(&(&right.occurrence_id, side_rank(right.side)))
        });
    }

    Ok(PlayerIdentityViewV1 {
        profile: PROFILE,
        entity_profile: ENTITY_PROFILE,
        player_entity_id: entity_id(&fide_id),
        fide_id,
        aliases,
        claims,
        conflicts,
        candidates,
        candidates_scanned: include_candidates,
        collection_blake3: scan.collection_blake3,
        collection_bytes: scan.collection_bytes,
        source_occurrences: scan.source_occurrences,
    })
}

const PROFILE_V2: &str = "antiskill_player_identity_view_v2";

fn verified_from_evidence(
    evidence: ProductPlayerSideEvidenceV1,
    evidence_class: &'static str,
    matched_name_signature: Option<String>,
) -> VerifiedPlayerOccurrenceV2 {
    VerifiedPlayerOccurrenceV2 {
        occurrence_id: evidence.occurrence_id,
        side: evidence.side,
        names: exact_values(&evidence.names),
        evidence_class,
        matched_name_signature,
        raw_fide_ids: exact_values(&evidence.fide_id_values),
        source_blake3: evidence.source_blake3,
        source_bytes: evidence.source_bytes,
        source_start: evidence.source_start,
        source_end: evidence.source_end,
        ordinal: evidence.ordinal,
    }
}

pub(crate) fn query_with_federation_bridge(
    root: &Path,
    collection_path: &Path,
    fide_id: &str,
    bridge: FederationBridgeInputV2,
) -> Result<PlayerIdentityViewV2, String> {
    let fide_id =
        canonical_fide_id(fide_id).ok_or_else(|| format!("FIDE ID non valido: {fide_id}"))?;
    if bridge.fide_id != fide_id
        || bridge.fsi_id.trim().is_empty()
        || bridge.observations.is_empty()
    {
        return Err("Bridge federativo incoerente con la query FIDE".into());
    }
    let mut bridge_signatures = BTreeSet::new();
    let mut bridge_aliases = BTreeSet::new();
    let mut bridge_signature_unique = BTreeSet::new();
    for observation in &bridge.observations {
        if observation.fide_id != fide_id || observation.fsi_id != bridge.fsi_id {
            return Err("Osservazioni bridge federativo contraddittorie".into());
        }
        let calculated = name_signature(&observation.raw_name)
            .ok_or("Nome bridge federativo non identificabile")?;
        if calculated != observation.name_signature {
            return Err("Firma nome bridge federativo non verificabile".into());
        }
        bridge_aliases.insert(observation.raw_name.clone());
        bridge_signatures.insert(calculated.clone());
        if observation.signature_unique {
            bridge_signature_unique.insert(calculated);
        }
    }

    let mut direct = Vec::new();
    let mut conflicts = Vec::new();
    let scan =
        collection::for_each_verified_player_side_evidence(root, collection_path, |evidence| {
            if evidence.fide_id_values.is_empty() {
                return Ok(());
            }
            let (ids, invalid) = normalized_ids(&evidence.fide_id_values);
            if invalid || ids.len() != 1 {
                if ids.iter().any(|id| id == &fide_id) {
                    conflicts.push(PlayerIdentityConflictV1 {
                        occurrence_id: evidence.occurrence_id,
                        side: evidence.side,
                        names: exact_values(&evidence.names),
                        raw_fide_ids: exact_values(&evidence.fide_id_values),
                        normalized_fide_ids: ids,
                        invalid_fide_id_value: invalid,
                    });
                }
                return Ok(());
            }
            if ids[0] == fide_id {
                direct.push(verified_from_evidence(evidence, "direct_fide_id", None));
            }
            Ok(())
        })?;

    let mut blocked_signatures = BTreeSet::new();
    let mut name_only = Vec::<(ProductPlayerSideEvidenceV1, String)>::new();
    let second =
        collection::for_each_verified_player_side_evidence(root, collection_path, |evidence| {
            let matching_signature = evidence
                .names
                .iter()
                .filter_map(|name| name_signature(name))
                .find(|signature| bridge_signatures.contains(signature));
            let Some(signature) = matching_signature else {
                return Ok(());
            };
            if evidence.fide_id_values.is_empty() {
                name_only.push((evidence, signature));
                return Ok(());
            }
            let (ids, invalid) = normalized_ids(&evidence.fide_id_values);
            if invalid || ids.len() != 1 || ids[0] != fide_id {
                blocked_signatures.insert(signature);
            }
            Ok(())
        })?;
    if second.collection_blake3 != scan.collection_blake3
        || second.collection_bytes != scan.collection_bytes
        || second.source_occurrences != scan.source_occurrences
    {
        return Err("Collection cambiata durante la riconciliazione federativa".into());
    }

    let safe_signatures = bridge_signature_unique
        .difference(&blocked_signatures)
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut promoted = Vec::new();
    let mut unresolved = Vec::new();
    for (evidence, signature) in name_only {
        if safe_signatures.contains(&signature) {
            promoted.push(verified_from_evidence(
                evidence,
                "verified_alias",
                Some(signature),
            ));
        } else {
            unresolved.push(PlayerIdentityCandidateV1 {
                occurrence_id: evidence.occurrence_id,
                side: evidence.side,
                names: exact_values(&evidence.names),
                candidate_policy: CANDIDATE_POLICY,
            });
        }
    }
    direct.sort_by(|left, right| {
        (&left.occurrence_id, side_rank(left.side))
            .cmp(&(&right.occurrence_id, side_rank(right.side)))
    });
    promoted.sort_by(|left, right| {
        (&left.occurrence_id, side_rank(left.side))
            .cmp(&(&right.occurrence_id, side_rank(right.side)))
    });
    unresolved.sort_by(|left, right| {
        (&left.occurrence_id, side_rank(left.side))
            .cmp(&(&right.occurrence_id, side_rank(right.side)))
    });
    conflicts.sort_by(|left, right| {
        (&left.occurrence_id, side_rank(left.side))
            .cmp(&(&right.occurrence_id, side_rank(right.side)))
    });

    let direct_count = direct.len();
    let promoted_count = promoted.len();
    let mut verified_occurrences = direct;
    verified_occurrences.extend(promoted);
    verified_occurrences.sort_by(|left, right| {
        (&left.occurrence_id, side_rank(left.side))
            .cmp(&(&right.occurrence_id, side_rank(right.side)))
    });
    let aliases = verified_occurrences
        .iter()
        .flat_map(|occurrence| occurrence.names.iter().cloned())
        .chain(bridge_aliases)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    Ok(PlayerIdentityViewV2 {
        profile: PROFILE_V2,
        entity_profile: ENTITY_PROFILE,
        player_entity_id: entity_id(&fide_id),
        fide_id: fide_id.clone(),
        external_ids: vec![
            ExternalPlayerIdV2 {
                namespace: "fide",
                value: fide_id,
                evidence_class: "direct_fide_id",
            },
            ExternalPlayerIdV2 {
                namespace: "fsi",
                value: bridge.fsi_id,
                evidence_class: "fsi_fide_bridge",
            },
        ],
        aliases,
        verified_occurrences,
        direct_count,
        promoted_count,
        conflicts,
        unresolved_candidates: unresolved,
        bridge_evidence: bridge.observations,
        blocked_signatures: blocked_signatures.into_iter().collect(),
        collection_blake3: scan.collection_blake3,
        collection_bytes: scan.collection_bytes,
        source_occurrences: scan.source_occurrences,
    })
}

const PROFILE_V3: &str = "antiskill_player_identity_view_v3";

fn context_text_signature(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut separator = false;
    for character in raw.chars() {
        let folded = fold_identity_latin(character);
        if folded.is_alphanumeric() {
            if separator && !out.is_empty() {
                out.push(' ');
            }
            separator = false;
            out.push(folded);
        } else {
            separator = true;
        }
    }
    out
}

fn canonical_context_date(raw: &str) -> Option<String> {
    let value = raw.trim().replace('-', ".");
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'.' || bytes[7] != b'.' {
        return None;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 7) {
            continue;
        }
        if !byte.is_ascii_digit() {
            return None;
        }
    }
    let year = value[0..4].parse::<u16>().ok()?;
    let month = value[5..7].parse::<u8>().ok()?;
    let day = value[8..10].parse::<u8>().ok()?;
    if year == 0 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(value)
}

fn singleton_value(values: &[String]) -> Option<&str> {
    let mut unique = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    let first = unique.next()?;
    if unique.all(|value| value == first) {
        Some(first)
    } else {
        None
    }
}

fn canonical_round(raw: &str) -> Option<u32> {
    let value = raw.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let round = value.parse::<u32>().ok()?;
    (round > 0).then_some(round)
}

fn valid_blake3(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_context_input(context: &OfficialContextInputV3, fide_id: &str) -> Result<(), String> {
    if !(context.context_id.starts_with("tc1-") || context.context_id.starts_with("vc1-"))
        || context.event_code.trim().is_empty()
        || context.event_name.trim().is_empty()
        || context.round == 0
        || context.board == 0
        || !matches!(context.result.as_str(), "1-0" | "0-1" | "1/2-1/2")
    {
        return Err("Contesto ufficiale con coordinate/profilo non validi".into());
    }
    let target = canonical_fide_id(&context.target_fide_id)
        .ok_or("FIDE ID target non valido nel contesto ufficiale")?;
    if target != fide_id {
        return Err("Contesto ufficiale riferito a un FIDE ID diverso".into());
    }
    canonical_fide_id(&context.opponent_fide_id)
        .ok_or("FIDE ID avversario non valido nel contesto ufficiale")?;
    let start = canonical_context_date(&context.event_start_date)
        .ok_or("Data iniziale contesto ufficiale non valida")?;
    let end = canonical_context_date(&context.event_end_date)
        .ok_or("Data finale contesto ufficiale non valida")?;
    if start > end {
        return Err("Intervallo date contesto ufficiale invertito".into());
    }
    if !valid_blake3(&context.roster_source_blake3)
        || !valid_blake3(&context.results_source_blake3)
        || !valid_blake3(&context.crosstable_source_blake3)
    {
        return Err("Provenienza BLAKE3 contesto ufficiale non valida".into());
    }
    Ok(())
}

fn occurrence_matches_context(
    occurrence: &crate::inspector::ProductContextOccurrenceV1,
    context: &OfficialContextInputV3,
) -> bool {
    let event = singleton_value(&occurrence.event)
        .map(context_text_signature)
        .filter(|value| !value.is_empty());
    if event.as_deref() != Some(context_text_signature(&context.event_name).as_str()) {
        return false;
    }
    let Some(date) = singleton_value(&occurrence.date).and_then(canonical_context_date) else {
        return false;
    };
    let Some(start) = canonical_context_date(&context.event_start_date) else {
        return false;
    };
    let Some(end) = canonical_context_date(&context.event_end_date) else {
        return false;
    };
    if date < start || date > end {
        return false;
    }
    if singleton_value(&occurrence.round).and_then(canonical_round) != Some(context.round) {
        return false;
    }
    if singleton_value(&occurrence.result) != Some(context.result.as_str()) {
        return false;
    }
    let (target_ids, opponent_ids) = match context.target_side {
        PlayerSideV1::White => (
            &occurrence.white_fide_id_values,
            &occurrence.black_fide_id_values,
        ),
        PlayerSideV1::Black => (
            &occurrence.black_fide_id_values,
            &occurrence.white_fide_id_values,
        ),
    };
    if !target_ids.is_empty() {
        return false;
    }
    let (opponent, invalid) = normalized_ids(opponent_ids);
    !invalid && opponent.len() == 1 && opponent[0] == context.opponent_fide_id
}

fn base_v3(
    root: &Path,
    collection_path: &Path,
    fide_id: &str,
    bridge: Option<FederationBridgeInputV2>,
) -> Result<PlayerIdentityViewV3, String> {
    if let Some(bridge) = bridge {
        let view = query_with_federation_bridge(root, collection_path, fide_id, bridge)?;
        return Ok(PlayerIdentityViewV3 {
            profile: PROFILE_V3,
            entity_profile: view.entity_profile,
            player_entity_id: view.player_entity_id,
            fide_id: view.fide_id,
            external_ids: view.external_ids,
            aliases: view.aliases,
            verified_occurrences: view.verified_occurrences,
            direct_count: view.direct_count,
            promoted_count: view.promoted_count,
            context_count: 0,
            conflicts: view.conflicts,
            unresolved_candidates: view.unresolved_candidates,
            unresolved_context_candidates: Vec::new(),
            bridge_evidence: view.bridge_evidence,
            blocked_signatures: view.blocked_signatures,
            context_evidence: Vec::new(),
            collection_blake3: view.collection_blake3,
            collection_bytes: view.collection_bytes,
            source_occurrences: view.source_occurrences,
        });
    }
    let view = query(root, collection_path, fide_id, true)?;
    let verified_occurrences = view
        .claims
        .into_iter()
        .map(|claim| VerifiedPlayerOccurrenceV2 {
            occurrence_id: claim.occurrence_id,
            side: claim.side,
            names: claim.names,
            evidence_class: "direct_fide_id",
            matched_name_signature: None,
            raw_fide_ids: claim.raw_fide_ids,
            source_blake3: claim.source_blake3,
            source_bytes: claim.source_bytes,
            source_start: claim.source_start,
            source_end: claim.source_end,
            ordinal: claim.ordinal,
        })
        .collect::<Vec<_>>();
    Ok(PlayerIdentityViewV3 {
        profile: PROFILE_V3,
        entity_profile: view.entity_profile,
        player_entity_id: view.player_entity_id,
        fide_id: view.fide_id.clone(),
        external_ids: vec![ExternalPlayerIdV2 {
            namespace: "fide",
            value: view.fide_id,
            evidence_class: "direct_fide_id",
        }],
        aliases: view.aliases,
        direct_count: verified_occurrences.len(),
        promoted_count: 0,
        context_count: 0,
        verified_occurrences,
        conflicts: view.conflicts,
        unresolved_candidates: view.candidates,
        unresolved_context_candidates: Vec::new(),
        bridge_evidence: Vec::new(),
        blocked_signatures: Vec::new(),
        context_evidence: Vec::new(),
        collection_blake3: view.collection_blake3,
        collection_bytes: view.collection_bytes,
        source_occurrences: view.source_occurrences,
    })
}

pub(crate) fn query_with_official_context(
    root: &Path,
    collection_path: &Path,
    fide_id: &str,
    bridge: Option<FederationBridgeInputV2>,
    contexts: Vec<OfficialContextInputV3>,
) -> Result<PlayerIdentityViewV3, String> {
    let fide_id =
        canonical_fide_id(fide_id).ok_or_else(|| format!("FIDE ID non valido: {fide_id}"))?;
    if contexts.is_empty() {
        return Err("Nessun contesto ufficiale per il FIDE ID richiesto".into());
    }
    let mut context_ids = BTreeSet::new();
    for context in &contexts {
        validate_context_input(context, &fide_id)?;
        if !context_ids.insert(context.context_id.clone()) {
            return Err("Context ID ufficiale duplicato".into());
        }
    }
    let mut view = base_v3(root, collection_path, &fide_id, bridge)?;
    let already_verified = view
        .verified_occurrences
        .iter()
        .map(|row| (row.occurrence_id.clone(), side_rank(row.side)))
        .collect::<BTreeSet<_>>();

    let mut occurrences = std::collections::BTreeMap::new();
    let mut occurrence_matches = std::collections::BTreeMap::<(String, u8), Vec<usize>>::new();
    let mut context_matches = vec![Vec::<(String, u8)>::new(); contexts.len()];
    let scan =
        collection::for_each_verified_context_occurrence(root, collection_path, |occurrence| {
            for (index, context) in contexts.iter().enumerate() {
                let key = (
                    occurrence.occurrence_id.clone(),
                    side_rank(context.target_side),
                );
                if already_verified.contains(&key)
                    || !occurrence_matches_context(&occurrence, context)
                {
                    continue;
                }
                occurrence_matches
                    .entry(key.clone())
                    .or_default()
                    .push(index);
                context_matches[index].push(key.clone());
                occurrences.entry(key).or_insert_with(|| occurrence.clone());
            }
            Ok(())
        })?;
    if scan.collection_blake3 != view.collection_blake3
        || scan.collection_bytes != view.collection_bytes
        || scan.source_occurrences != view.source_occurrences
    {
        return Err("Collection cambiata durante il join contestuale".into());
    }

    let mut promoted_keys = BTreeSet::new();
    let mut context_evidence = Vec::new();
    for (key, matching) in &occurrence_matches {
        if matching.len() != 1 {
            continue;
        }
        let context_index = matching[0];
        if context_matches[context_index].len() != 1 {
            continue;
        }
        let occurrence = occurrences.get(key).ok_or("Occurrence context mancante")?;
        let context = &contexts[context_index];
        let names = match context.target_side {
            PlayerSideV1::White => exact_values(&occurrence.white_names),
            PlayerSideV1::Black => exact_values(&occurrence.black_names),
        };
        let raw_fide_ids = match context.target_side {
            PlayerSideV1::White => exact_values(&occurrence.white_fide_id_values),
            PlayerSideV1::Black => exact_values(&occurrence.black_fide_id_values),
        };
        view.verified_occurrences.push(VerifiedPlayerOccurrenceV2 {
            occurrence_id: occurrence.occurrence_id.clone(),
            side: context.target_side,
            names,
            evidence_class: "official_context_join",
            matched_name_signature: None,
            raw_fide_ids,
            source_blake3: occurrence.source_blake3.clone(),
            source_bytes: occurrence.source_bytes,
            source_start: occurrence.source_start,
            source_end: occurrence.source_end,
            ordinal: occurrence.ordinal,
        });
        context_evidence.push(OfficialContextEvidenceV3 {
            occurrence_id: occurrence.occurrence_id.clone(),
            side: context.target_side,
            context_id: context.context_id.clone(),
            event_code: context.event_code.clone(),
            event_name: context.event_name.clone(),
            round: context.round,
            board: context.board,
            opponent_fide_id: context.opponent_fide_id.clone(),
            opponent_name: context.opponent_name.clone(),
            result: context.result.clone(),
            roster_source_blake3: context.roster_source_blake3.clone(),
            results_source_blake3: context.results_source_blake3.clone(),
            crosstable_source_blake3: context.crosstable_source_blake3.clone(),
        });
        promoted_keys.insert(key.clone());
    }

    let mut unresolved_context_candidates = Vec::new();
    for (key, matching) in occurrence_matches {
        if promoted_keys.contains(&key) {
            continue;
        }
        let side = if key.1 == side_rank(PlayerSideV1::White) {
            PlayerSideV1::White
        } else {
            PlayerSideV1::Black
        };
        let mut matching_context_ids = matching
            .into_iter()
            .map(|index| contexts[index].context_id.clone())
            .collect::<Vec<_>>();
        matching_context_ids.sort();
        matching_context_ids.dedup();
        unresolved_context_candidates.push(OfficialContextCandidateV3 {
            occurrence_id: key.0,
            side,
            matching_context_ids,
        });
    }
    unresolved_context_candidates.sort_by(|left, right| {
        (&left.occurrence_id, side_rank(left.side))
            .cmp(&(&right.occurrence_id, side_rank(right.side)))
    });
    context_evidence.sort_by(|left, right| {
        (&left.occurrence_id, side_rank(left.side), &left.context_id).cmp(&(
            &right.occurrence_id,
            side_rank(right.side),
            &right.context_id,
        ))
    });
    view.verified_occurrences.sort_by(|left, right| {
        (&left.occurrence_id, side_rank(left.side))
            .cmp(&(&right.occurrence_id, side_rank(right.side)))
    });
    view.context_count = context_evidence.len();
    view.context_evidence = context_evidence;
    view.unresolved_context_candidates = unresolved_context_candidates;
    view.aliases = view
        .verified_occurrences
        .iter()
        .flat_map(|occurrence| occurrence.names.iter().cloned())
        .chain(view.aliases)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fide_ids_are_canonical_decimal_and_fail_closed() {
        assert_eq!(canonical_fide_id("900000001"), Some("900000001".into()));
        assert_eq!(canonical_fide_id("00900000001"), Some("900000001".into()));
        assert_eq!(canonical_fide_id(" 900000001 "), Some("900000001".into()));
        assert_eq!(canonical_fide_id("0"), None);
        assert_eq!(canonical_fide_id("28A35580"), None);
    }

    #[test]
    fn candidate_signature_handles_accents_punctuation_and_order() {
        let expected = name_signature("Esempio, Auròra").unwrap();
        assert_eq!(name_signature("Aurora' Esempio").unwrap(), expected);
        assert_eq!(name_signature("Esempio, Aurora`").unwrap(), expected);
        assert_ne!(name_signature("Esempio, Bruno").unwrap(), expected);
    }

    #[test]
    fn player_entity_id_is_domain_separated_and_deterministic() {
        assert_eq!(entity_id("900000001"), entity_id("900000001"));
        assert!(entity_id("900000001").starts_with("p1-"));
        assert_ne!(entity_id("900000001"), entity_id("900000003"));
    }
}
