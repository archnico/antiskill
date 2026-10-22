use crate::{inspector, sources};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    fide_id: String,
    format: OutputFormat,
    candidates: bool,
    export: Option<PathBuf>,
    federation_index: Option<PathBuf>,
    context_index: Option<PathBuf>,
    context_registry: Option<PathBuf>,
}

pub(crate) fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if matches!(args.as_slice(), [argument] if matches!(argument.as_str(), "help" | "--help" | "-h"))
    {
        print_help();
        return ExitCode::SUCCESS;
    }
    let result = parse_args(args).and_then(|options| {
        let root = PathBuf::from(".");
        let collection = inspector::resolve_active_collection_path(&root)?;
        let context_records = match (
            options.context_index.as_deref(),
            options.context_registry.as_deref(),
        ) {
            (Some(index), None) => Some(sources::resolve_fide_official_contexts(
                index,
                &options.fide_id,
            )?),
            (None, Some(registry)) => Some(sources::resolve_fide_context_registry(
                registry,
                &options.fide_id,
            )?),
            (None, None) => None,
            (Some(_), Some(_)) => unreachable!("parse_args rejects dual context sources"),
        };
        if let Some(contexts) = context_records {
            let canonical_requested_fide = canonical_numeric_id(&options.fide_id, "FIDE")?;
            let contexts = contexts
                .into_iter()
                .map(|context| {
                    let (target_side, target_fide_id, opponent_fide_id, opponent_name) =
                        if context.white_fide_id.as_deref()
                            == Some(canonical_requested_fide.as_str())
                        {
                            (
                                inspector::PlayerSideV1::White,
                                context.white_fide_id.clone().expect("target FIDE verified"),
                                context
                                    .black_fide_id
                                    .clone()
                                    .ok_or("Contesto T'Pol senza FIDE ID dell'avversario")?,
                                context.black_name.clone(),
                            )
                        } else if context.black_fide_id.as_deref()
                            == Some(canonical_requested_fide.as_str())
                        {
                            (
                                inspector::PlayerSideV1::Black,
                                context.black_fide_id.clone().expect("target FIDE verified"),
                                context
                                    .white_fide_id
                                    .clone()
                                    .ok_or("Contesto T'Pol senza FIDE ID dell'avversario")?,
                                context.white_name.clone(),
                            )
                        } else {
                            return Err("Contesto T'Pol non riferito al FIDE ID richiesto".into());
                        };
                    Ok(inspector::OfficialContextInputV3 {
                        context_id: context.context_id,
                        event_code: context.event_code,
                        event_name: context.event_name,
                        event_start_date: context.event_start_date,
                        event_end_date: context.event_end_date,
                        round: context.round,
                        board: context.board,
                        target_side,
                        target_fide_id,
                        opponent_fide_id,
                        opponent_name,
                        result: context.result,
                        roster_source_blake3: context.roster_source_blake3,
                        results_source_blake3: context.results_source_blake3,
                        crosstable_source_blake3: context.crosstable_source_blake3,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let bridge_input = options
                .federation_index
                .as_deref()
                .map(|index| load_federation_bridge(index, &options.fide_id))
                .transpose()?;
            let view = inspector::query_player_identity_v3(
                &root,
                &collection,
                &options.fide_id,
                bridge_input,
                contexts,
            )?;
            if let Some(path) = options.export.as_deref() {
                export_view_v3(&root, &view, path, options.candidates)?;
            }
            match options.format {
                OutputFormat::Human => Ok(format_human_v3(&view, options.export.as_deref())),
                OutputFormat::Json => format_json_v3(&view),
            }
        } else if let Some(index) = options.federation_index.as_deref() {
            let bridge_input = load_federation_bridge(index, &options.fide_id)?;
            let view = inspector::query_player_identity_v2(
                &root,
                &collection,
                &options.fide_id,
                bridge_input,
            )?;
            if let Some(path) = options.export.as_deref() {
                export_view_v2(&root, &view, path, options.candidates)?;
            }
            match options.format {
                OutputFormat::Human => Ok(format_human_v2(&view, options.export.as_deref())),
                OutputFormat::Json => format_json_v2(&view),
            }
        } else {
            let view = inspector::query_player_identity(
                &root,
                &collection,
                &options.fide_id,
                options.candidates,
            )?;
            if let Some(path) = options.export.as_deref() {
                export_view(&root, &view, path, options.candidates)?;
            }
            match options.format {
                OutputFormat::Human => Ok(format_human(&view, options.export.as_deref())),
                OutputFormat::Json => format_json(&view),
            }
        }
    });
    match result {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERRORE players: {error}");
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    println!(
        "\\
Interroga l'identità giocatore derivata da identificatori FIDE espliciti.

USO:
    antiskill players --fide-id ID [--format human|json] [--candidates]
    antiskill players --fide-id ID --federation-index FILE [--format human|json] [--candidates]
    antiskill players --fide-id ID --context-index FILE [--federation-index FILE] [--format human|json] [--candidates]
    antiskill players --fide-id ID --context-registry DIR [--federation-index FILE] [--format human|json] [--candidates]
    antiskill players --fide-id ID [--federation-index FILE] [--context-index FILE|--context-registry DIR] [--candidates] --export FILE
"
    );
}

fn parse_args(args: Vec<String>) -> Result<Options, String> {
    let mut fide_id = None;
    let mut format = OutputFormat::Human;
    let mut candidates = false;
    let mut export = None;
    let mut federation_index = None;
    let mut context_index = None;
    let mut context_registry = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--fide-id" => {
                index += 1;
                let value = args.get(index).ok_or("Manca il valore di --fide-id")?;
                if fide_id.replace(value.clone()).is_some() {
                    return Err("--fide-id specificato più volte".into());
                }
            }
            "--format" => {
                index += 1;
                format = match args.get(index).map(String::as_str) {
                    Some("human") => OutputFormat::Human,
                    Some("json") => OutputFormat::Json,
                    Some(value) => return Err(format!("Formato players non supportato: {value}")),
                    None => return Err("Manca il valore di --format".into()),
                };
            }
            "--candidates" => candidates = true,
            "--export" => {
                index += 1;
                let value = args.get(index).ok_or("Manca il valore di --export")?;
                if export.replace(PathBuf::from(value)).is_some() {
                    return Err("--export specificato più volte".into());
                }
            }
            "--federation-index" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("Manca il valore di --federation-index")?;
                if federation_index.replace(PathBuf::from(value)).is_some() {
                    return Err("--federation-index specificato più volte".into());
                }
            }
            "--context-index" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("Manca il valore di --context-index")?;
                if context_index.replace(PathBuf::from(value)).is_some() {
                    return Err("--context-index specificato più volte".into());
                }
            }
            "--context-registry" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("Manca il valore di --context-registry")?;
                if context_registry.replace(PathBuf::from(value)).is_some() {
                    return Err("--context-registry specificato più volte".into());
                }
            }
            argument => return Err(format!("Argomento players inatteso: {argument}")),
        }
        index += 1;
    }
    if context_index.is_some() && context_registry.is_some() {
        return Err("Usare uno solo tra --context-index e --context-registry".into());
    }
    Ok(Options {
        fide_id: fide_id.ok_or("Serve --fide-id ID")?,
        format,
        candidates,
        export,
        federation_index,
        context_index,
        context_registry,
    })
}

fn canonical_numeric_id(raw: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{label} ID non valido: {raw}"));
    }
    let numeric = value
        .parse::<u64>()
        .map_err(|_| format!("{label} ID non valido: {raw}"))?;
    if numeric == 0 {
        return Err(format!("{label} ID non valido: {raw}"));
    }
    Ok(numeric.to_string())
}

fn load_federation_bridge(
    index: &Path,
    fide_id: &str,
) -> Result<inspector::FederationBridgeInputV2, String> {
    let bridge = sources::resolve_fide_identity_bridge(index, fide_id)?;
    Ok(inspector::FederationBridgeInputV2 {
        fsi_id: bridge.fsi_id.clone(),
        fide_id: bridge.fide_id.clone(),
        observations: bridge
            .observations
            .into_iter()
            .map(|observation| inspector::FederationBridgeObservationV2 {
                source_profile: observation.profile,
                source_blake3: observation.source_blake3,
                source_bytes: observation.source_bytes,
                source_line: observation.source_line,
                raw_name: observation.raw_name,
                name_signature: observation.name_signature,
                signature_unique: observation.signature_unique,
                fsi_id: observation.fsi_id,
                fide_id: observation.fide_id,
                birth_year: observation.birth_year,
                birth_date: observation.birth_date,
                federation: observation.federation,
            })
            .collect(),
    })
}

fn format_human(view: &inspector::PlayerIdentityViewV1, exported: Option<&Path>) -> String {
    let mut output = format!(
        "Identità giocatore v1\n\nFIDE ID: {}\nPlayer entity: {}\nOccurrence authoritative: {}\nAlias osservati: {}\nConflitti ID: {}\n",
        view.fide_id,
        view.player_entity_id,
        view.claims.len(),
        view.aliases.len(),
        view.conflicts.len(),
    );
    if view.candidates_scanned {
        output.push_str(&format!(
            "Candidati name-only non collegati: {}\n",
            view.candidates.len()
        ));
    } else {
        output.push_str("Candidati name-only: non scansionati (usa --candidates)\n");
    }
    output.push_str("\nAlias:\n");
    for alias in &view.aliases {
        output.push_str(&format!("- {alias}\n"));
    }
    output.push_str(&format!(
        "\nCollection BLAKE3: {}\nOccurrence sorgente scansionate: {}\n",
        view.collection_blake3, view.source_occurrences
    ));
    if let Some(path) = exported {
        output.push_str(&format!("Export PGN: {}\n", path.display()));
    }
    output
}

fn format_json(view: &inspector::PlayerIdentityViewV1) -> Result<String, String> {
    let mut output = serde_json::to_string_pretty(view)
        .map_err(|error| format!("JSON players non serializzabile: {error}"))?;
    output.push('\n');
    Ok(output)
}

fn export_view(
    root: &Path,
    view: &inspector::PlayerIdentityViewV1,
    path: &Path,
    include_candidates: bool,
) -> Result<(), String> {
    let mut ids = view
        .claims
        .iter()
        .map(|claim| claim.occurrence_id.clone())
        .collect::<BTreeSet<_>>();
    if include_candidates {
        ids.extend(
            view.candidates
                .iter()
                .map(|candidate| candidate.occurrence_id.clone()),
        );
    }
    export_ids(root, &ids, path)
}

fn format_human_v2(view: &inspector::PlayerIdentityViewV2, exported: Option<&Path>) -> String {
    let fsi_id = view
        .external_ids
        .iter()
        .find(|id| id.namespace == "fsi")
        .map(|id| id.value.as_str())
        .unwrap_or("-");
    let mut output = format!(
        "Identità giocatore v2\n\nFIDE ID: {}\nFSI ID: {}\nPlayer entity: {}\nOccurrence direct_fide_id: {}\nOccurrence verified_alias: {}\nOccurrence verificate totali: {}\nCandidati irrisolti: {}\nConflitti ID: {}\n",
        view.fide_id,
        fsi_id,
        view.player_entity_id,
        view.direct_count,
        view.promoted_count,
        view.verified_occurrences.len(),
        view.unresolved_candidates.len(),
        view.conflicts.len(),
    );
    output.push_str("\nAlias verificati:\n");
    for alias in &view.aliases {
        output.push_str(&format!("- {alias}\n"));
    }
    output.push_str(&format!(
        "\nBridge observations: {}\nCollection BLAKE3: {}\nOccurrence sorgente scansionate: {}\n",
        view.bridge_evidence.len(),
        view.collection_blake3,
        view.source_occurrences
    ));
    if !view.blocked_signatures.is_empty() {
        output.push_str(&format!(
            "Firme bloccate per conflitto: {}\n",
            view.blocked_signatures.join(", ")
        ));
    }
    if let Some(path) = exported {
        output.push_str(&format!("Export PGN: {}\n", path.display()));
    }
    output
}

fn format_json_v2(view: &inspector::PlayerIdentityViewV2) -> Result<String, String> {
    let mut output = serde_json::to_string_pretty(view)
        .map_err(|error| format!("JSON players v2 non serializzabile: {error}"))?;
    output.push('\n');
    Ok(output)
}

fn export_view_v2(
    root: &Path,
    view: &inspector::PlayerIdentityViewV2,
    path: &Path,
    include_candidates: bool,
) -> Result<(), String> {
    let mut ids = view
        .verified_occurrences
        .iter()
        .map(|occurrence| occurrence.occurrence_id.clone())
        .collect::<BTreeSet<_>>();
    if include_candidates {
        ids.extend(
            view.unresolved_candidates
                .iter()
                .map(|candidate| candidate.occurrence_id.clone()),
        );
    }
    export_ids(root, &ids, path)
}

fn format_human_v3(view: &inspector::PlayerIdentityViewV3, exported: Option<&Path>) -> String {
    let fsi_id = view
        .external_ids
        .iter()
        .find(|id| id.namespace == "fsi")
        .map(|id| id.value.as_str())
        .unwrap_or("-");
    let mut output = format!(
        "Identità giocatore v3\n\nFIDE ID: {}\nFSI ID: {}\nPlayer entity: {}\nOccurrence direct_fide_id: {}\nOccurrence verified_alias: {}\nOccurrence official_context_join: {}\nOccurrence verificate totali: {}\nCandidati name-only irrisolti: {}\nCandidati contestuali irrisolti: {}\nConflitti ID: {}\n",
        view.fide_id,
        fsi_id,
        view.player_entity_id,
        view.direct_count,
        view.promoted_count,
        view.context_count,
        view.verified_occurrences.len(),
        view.unresolved_candidates.len(),
        view.unresolved_context_candidates.len(),
        view.conflicts.len(),
    );
    output.push_str("\nAlias documentati:\n");
    for alias in &view.aliases {
        output.push_str(&format!("- {alias}\n"));
    }
    output.push_str(&format!(
        "\nContext evidence: {}\nBridge observations: {}\nCollection BLAKE3: {}\nOccurrence sorgente scansionate: {}\n",
        view.context_evidence.len(),
        view.bridge_evidence.len(),
        view.collection_blake3,
        view.source_occurrences,
    ));
    if let Some(path) = exported {
        output.push_str(&format!("Export PGN: {}\n", path.display()));
    }
    output
}

fn format_json_v3(view: &inspector::PlayerIdentityViewV3) -> Result<String, String> {
    let mut output = serde_json::to_string_pretty(view)
        .map_err(|error| format!("JSON players v3 non serializzabile: {error}"))?;
    output.push('\n');
    Ok(output)
}

fn export_view_v3(
    root: &Path,
    view: &inspector::PlayerIdentityViewV3,
    path: &Path,
    include_candidates: bool,
) -> Result<(), String> {
    let mut ids = view
        .verified_occurrences
        .iter()
        .map(|occurrence| occurrence.occurrence_id.clone())
        .collect::<BTreeSet<_>>();
    if include_candidates {
        ids.extend(
            view.unresolved_candidates
                .iter()
                .map(|candidate| candidate.occurrence_id.clone()),
        );
        ids.extend(
            view.unresolved_context_candidates
                .iter()
                .map(|candidate| candidate.occurrence_id.clone()),
        );
    }
    export_ids(root, &ids, path)
}

fn export_ids(root: &Path, ids: &BTreeSet<String>, path: &Path) -> Result<(), String> {
    let games = inspector::resolve_active_public_games(root, ids)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Creazione export players {}: {error}", path.display()))?;
    let written = (|| -> Result<(), String> {
        for (index, game) in games.iter().enumerate() {
            if index > 0 {
                file.write_all(b"\n\n")
                    .map_err(|error| format!("Separatore export players: {error}"))?;
            }
            file.write_all(&game.bytes)
                .map_err(|error| format!("Scrittura export players: {error}"))?;
        }
        file.flush()
            .map_err(|error| format!("Flush export players: {error}"))
    })();
    if let Err(error) = written {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_args_are_narrow_and_order_independent() {
        let parsed = parse_args(vec![
            "--candidates".into(),
            "--fide-id".into(),
            "900000001".into(),
            "--format".into(),
            "json".into(),
        ])
        .unwrap();
        assert_eq!(parsed.fide_id, "900000001");
        assert_eq!(parsed.format, OutputFormat::Json);
        assert!(parsed.candidates);
        assert!(parse_args(vec![]).is_err());
        assert!(parse_args(vec!["--fide-id".into()]).is_err());
        assert!(parse_args(vec!["--fide-id".into(), "1".into(), "extra".into()]).is_err());
    }
}
