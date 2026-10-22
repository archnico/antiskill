mod context_registry;
mod event_candidates;
mod fsi_calendar;
mod snapshot;
mod torneionline_allin;
mod torneionline_tpol;
mod vesus;

use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

pub(crate) use torneionline_allin::AllinBridgeResolutionV1;

pub(crate) fn resolve_fide_identity_bridge(
    index: &Path,
    fide_id: &str,
) -> Result<AllinBridgeResolutionV1, String> {
    torneionline_allin::resolve_fide_bridge(index, fide_id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OfficialSourceContextV1 {
    pub context_id: String,
    pub event_code: String,
    pub event_name: String,
    pub event_start_date: String,
    pub event_end_date: String,
    pub round: u32,
    pub board: u32,
    pub white_name: String,
    pub black_name: String,
    pub white_fide_id: Option<String>,
    pub black_fide_id: Option<String>,
    pub result: String,
    pub roster_source_blake3: String,
    pub results_source_blake3: String,
    pub crosstable_source_blake3: String,
}

impl From<torneionline_tpol::TpolContextGameV1> for OfficialSourceContextV1 {
    fn from(value: torneionline_tpol::TpolContextGameV1) -> Self {
        Self {
            context_id: value.context_id,
            event_code: value.event_code,
            event_name: value.event_name,
            event_start_date: value.event_start_date,
            event_end_date: value.event_end_date,
            round: value.round,
            board: value.board,
            white_name: value.white_name,
            black_name: value.black_name,
            white_fide_id: value.white_fide_id,
            black_fide_id: value.black_fide_id,
            result: value.result,
            roster_source_blake3: value.roster_source_blake3,
            results_source_blake3: value.results_source_blake3,
            crosstable_source_blake3: value.crosstable_source_blake3,
        }
    }
}

impl From<vesus::VesusContextGameV1> for OfficialSourceContextV1 {
    fn from(value: vesus::VesusContextGameV1) -> Self {
        Self {
            context_id: value.context_id,
            event_code: value.event_code,
            event_name: value.event_name,
            event_start_date: value.event_start_date,
            event_end_date: value.event_end_date,
            round: value.round,
            board: value.board,
            white_name: value.white_name,
            black_name: value.black_name,
            white_fide_id: value.white_fide_id,
            black_fide_id: value.black_fide_id,
            result: value.result,
            roster_source_blake3: value.roster_source_blake3,
            results_source_blake3: value.results_source_blake3,
            crosstable_source_blake3: value.crosstable_source_blake3,
        }
    }
}

pub(crate) fn resolve_fide_official_contexts(
    index: &Path,
    fide_id: &str,
) -> Result<Vec<OfficialSourceContextV1>, String> {
    let profile = first_index_profile(index)?;
    match profile.as_str() {
        "antiskill_tpol_official_context_v1" => {
            torneionline_tpol::resolve_fide_contexts(index, fide_id)
                .map(|records| records.into_iter().map(Into::into).collect())
        }
        "antiskill_vesus_official_context_v1" => vesus::resolve_fide_contexts(index, fide_id)
            .map(|records| records.into_iter().map(Into::into).collect()),
        other => Err(format!("Profilo indice contestuale sconosciuto: {other}")),
    }
}

fn first_index_profile(index: &Path) -> Result<String, String> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    let metadata = std::fs::symlink_metadata(index)
        .map_err(|e| format!("Indice contestuale {}: {e}", index.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "Indice contestuale non regolare: {}",
            index.display()
        ));
    }
    let file = File::open(index).map_err(|e| format!("Apertura indice contestuale: {e}"))?;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| format!("Lettura indice contestuale: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(&line).map_err(|e| format!("Indice contestuale JSON: {e}"))?;
        return value
            .get("profile")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| "Indice contestuale senza profile".into());
    }
    Err("Indice contestuale vuoto".into())
}

pub(crate) fn resolve_fide_context_registry(
    root: &Path,
    fide_id: &str,
) -> Result<Vec<OfficialSourceContextV1>, String> {
    context_registry::resolve_fide_contexts(root, fide_id)
        .map(|records| records.into_iter().map(Into::into).collect())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SourceDefinition {
    key: &'static str,
    label: &'static str,
    cadence: &'static str,
    authority: &'static str,
    access: &'static str,
}

const ITALY_SOURCES: &[SourceDefinition] = &[
    SourceDefinition {
        key: "fide-rating-list",
        label: "FIDE rating lists",
        cadence: "mensile",
        authority: "ufficiale",
        access: "download",
    },
    SourceDefinition {
        key: "fsi-calendar",
        label: "FSI calendario",
        cadence: "6 ore",
        authority: "ufficiale",
        access: "HTML/allegati",
    },
    SourceDefinition {
        key: "fsi-members",
        label: "FSI tesserati",
        cadence: "giornaliera",
        authority: "ufficiale",
        access: "CSV/HTML",
    },
    SourceDefinition {
        key: "fsi-people",
        label: "FSI persone",
        cadence: "mensile",
        authority: "ufficiale",
        access: "HTML segmentato",
    },
    SourceDefinition {
        key: "fsi-clubs",
        label: "FSI societa",
        cadence: "settimanale",
        authority: "ufficiale",
        access: "HTML",
    },
    SourceDefinition {
        key: "fsi-arbiters",
        label: "FSI arbitri",
        cadence: "settimanale",
        authority: "ufficiale",
        access: "HTML",
    },
    SourceDefinition {
        key: "fsi-instructors",
        label: "FSI istruttori",
        cadence: "settimanale",
        authority: "ufficiale",
        access: "HTML",
    },
    SourceDefinition {
        key: "fsi-documents",
        label: "FSI documenti",
        cadence: "12 ore",
        authority: "ufficiale",
        access: "HTML/file",
    },
    SourceDefinition {
        key: "fsi-game-archive",
        label: "FSI archivio partite",
        cadence: "giornaliera",
        authority: "ufficiale",
        access: "PGN/HTML",
    },
    SourceDefinition {
        key: "torneionline-allin",
        label: "Torneionline allin.csv",
        cadence: "giornaliera",
        authority: "operativa",
        access: "CSV",
    },
    SourceDefinition {
        key: "torneionline-tpol",
        label: "Torneionline T'Pol",
        cadence: "giornaliera",
        authority: "operativa",
        access: "HTML/export",
    },
    SourceDefinition {
        key: "torneionline-loto",
        label: "Torneionline LOTO",
        cadence: "settimanale",
        authority: "storica curata",
        access: "HTML/allegati",
    },
    SourceDefinition {
        key: "vesus-linked-events",
        label: "Vesus eventi collegati",
        cadence: "12 ore",
        authority: "organizzatore",
        access: "mirato",
    },
];

fn print_help() {
    println!(
        "\
antiskill sources

Catalogo, stato e acquisizione delle fonti esterne.

USO:
    antiskill sources status italy [--root DIR] [--format human|json]
    antiskill sources audit-events --input FILE.jsonl
    antiskill sources validate-events --input FILE.jsonl
    antiskill sources match-candidates --left FILE --right FILE
    antiskill sources crosswalk context-registry --help
    antiskill sources registry torneionline-tpol --help
    antiskill sources stress fsi-calendar --help
    antiskill sources candidates fsi-calendar --help
    antiskill sources sync fsi-calendar --root DIR
    antiskill sources index fsi-calendar --help
    antiskill sources index torneionline-allin --help
    antiskill sources index torneionline-tpol --help
    antiskill sources index vesus-linked-events --help
    antiskill sources query fsi-calendar --help
    antiskill sources query torneionline-allin --help
    antiskill sources query torneionline-tpol --help
    antiskill sources query vesus-linked-events --help
    antiskill sources query context-registry --help
    antiskill sources diff fsi-calendar --help
    antiskill sources sync torneionline-allin --root DIR
    antiskill sources sync torneionline-tpol --root DIR --event-code CODE
    antiskill sources sync vesus-linked-events --root DIR --tournament-key KEY
    antiskill sources help

Per i dettagli del primo sincronizzatore:
    antiskill sources sync torneionline-allin --help
"
    );
}

fn print_italy_status() {
    println!("KEY\tFONTE\tCADENZA\tAUTORITA\tACCESSO\tSTATO");

    for source in ITALY_SOURCES {
        println!(
            "{}\t{}\t{}\t{}\t{}\tnon configurata",
            source.key, source.label, source.cadence, source.authority, source.access
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LocalSourceState {
    NonConfigurata,
    Fresh,
    Stale,
    Invalid,
}

#[derive(Debug, Serialize)]
struct LocalSourceStatus {
    source_key: &'static str,
    cadence: &'static str,
    state: LocalSourceState,
    #[serde(skip_serializing_if = "Option::is_none")]
    fetched_at_unix_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    age_ms: Option<u128>,
}

#[derive(Serialize)]
struct ItalyStatusReport {
    schema_version: u32,
    sources: Vec<LocalSourceStatus>,
}

#[derive(Clone, Copy)]
enum StatusFormat {
    Human,
    Json,
}

fn cadence_ms(cadence: &str) -> u128 {
    const HOUR: u128 = 60 * 60 * 1_000;
    const DAY: u128 = 24 * HOUR;
    match cadence {
        "6 ore" => 6 * HOUR,
        "12 ore" => 12 * HOUR,
        "giornaliera" => DAY,
        "settimanale" => 7 * DAY,
        "mensile" => 31 * DAY,
        _ => unreachable!("cadenza del catalogo non riconosciuta"),
    }
}

fn italy_status_at(root: &Path, now_unix_ms: u128) -> Vec<LocalSourceStatus> {
    ITALY_SOURCES
        .iter()
        .map(|source| {
            let reference = root.join("raw/refs").join(format!("{}.json", source.key));
            if matches!(
                std::fs::symlink_metadata(&reference),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            ) {
                return LocalSourceStatus {
                    source_key: source.key,
                    cadence: source.cadence,
                    state: LocalSourceState::NonConfigurata,
                    fetched_at_unix_ms: None,
                    age_ms: None,
                };
            }

            match snapshot::load_verified_latest_snapshot(root, source.key) {
                Ok(snapshot) if snapshot.fetched_at_unix_ms <= now_unix_ms => {
                    let age_ms = now_unix_ms - snapshot.fetched_at_unix_ms;
                    LocalSourceStatus {
                        source_key: source.key,
                        cadence: source.cadence,
                        state: if age_ms <= cadence_ms(source.cadence) {
                            LocalSourceState::Fresh
                        } else {
                            LocalSourceState::Stale
                        },
                        fetched_at_unix_ms: Some(snapshot.fetched_at_unix_ms),
                        age_ms: Some(age_ms),
                    }
                }
                Ok(_) | Err(_) => LocalSourceStatus {
                    source_key: source.key,
                    cadence: source.cadence,
                    state: LocalSourceState::Invalid,
                    fetched_at_unix_ms: None,
                    age_ms: None,
                },
            }
        })
        .collect()
}

fn print_local_italy_status(root: &Path, format: StatusFormat) -> Result<(), String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "Orologio di sistema precedente a UNIX_EPOCH".to_owned())?
        .as_millis();
    let sources = italy_status_at(root, now);
    match format {
        StatusFormat::Human => {
            println!("KEY\tCADENZA\tSTATO\tFETCHED_AT_UNIX_MS\tAGE_MS");
            for status in sources {
                let fetched = status
                    .fetched_at_unix_ms
                    .map_or_else(|| "-".to_owned(), |value| value.to_string());
                let age = status
                    .age_ms
                    .map_or_else(|| "-".to_owned(), |value| value.to_string());
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    status.source_key,
                    status.cadence,
                    match status.state {
                        LocalSourceState::NonConfigurata => "non_configurata",
                        LocalSourceState::Fresh => "fresh",
                        LocalSourceState::Stale => "stale",
                        LocalSourceState::Invalid => "invalid",
                    },
                    fetched,
                    age
                );
            }
        }
        StatusFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&ItalyStatusReport {
                schema_version: 1,
                sources,
            })
            .map_err(|error| format!("Serializzazione stato fonti: {error}"))?
        ),
    }
    Ok(())
}

fn run_italy_status<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let mut root = None;
    let mut format = StatusFormat::Human;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                if root.is_some() {
                    return Err("Parametro duplicato: --root".into());
                }
                root = Some(args.next().ok_or("Manca il valore di --root")?);
            }
            "--format" => {
                format = match args.next().as_deref() {
                    Some("human") => StatusFormat::Human,
                    Some("json") => StatusFormat::Json,
                    Some(value) => return Err(format!("Formato sconosciuto: {value}")),
                    None => return Err("Manca il valore di --format".into()),
                };
            }
            _ => return Err(format!("Argomento sconosciuto: {arg}")),
        }
    }
    match root {
        None => {
            print_italy_status();
            Ok(())
        }
        Some(root) => print_local_italy_status(Path::new(&root), format),
    }
}

pub fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();

    match args.next().as_deref() {
        None | Some("help" | "--help" | "-h") => {
            print_help();
            ExitCode::SUCCESS
        }

        Some("status") => match args.next().as_deref() {
            Some("italy") => match run_italy_status(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources status italy: {error}");
                    ExitCode::from(2)
                }
            },

            Some(scope) => {
                eprintln!("Ambito sources sconosciuto: {scope}");
                ExitCode::from(2)
            }

            None => {
                eprintln!("Manca l'ambito. Usa: antiskill sources status italy");
                ExitCode::from(2)
            }
        },

        Some("audit-events") => match event_candidates::run_audit(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("ERRORE sources audit-events: {error}");
                ExitCode::from(1)
            }
        },

        Some("validate-events") => match event_candidates::run_validate(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("ERRORE sources validate-events: {error}");
                ExitCode::from(1)
            }
        },

        Some("match-candidates") => match event_candidates::run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("ERRORE sources match-candidates: {error}");
                ExitCode::from(1)
            }
        },

        Some("crosswalk") => {
            match args.next().as_deref() {
                Some("context-registry") => match context_registry::run_crosswalk(args) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(error) => {
                        eprintln!("ERRORE sources crosswalk context-registry: {error}");
                        ExitCode::from(1)
                    }
                },
                Some(source) => {
                    eprintln!("Crosswalk sconosciuto: {source}");
                    ExitCode::from(2)
                }
                None => {
                    eprintln!("Manca il crosswalk. Usa: antiskill sources crosswalk context-registry --help");
                    ExitCode::from(2)
                }
            }
        }

        Some("stress") => match args.next().as_deref() {
            Some("fsi-calendar") => match fsi_calendar::run_stress(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources stress fsi-calendar: {error}");
                    ExitCode::from(1)
                }
            },

            Some(source) => {
                eprintln!("Sorgente stress sconosciuta: {source}");
                ExitCode::from(2)
            }

            None => {
                eprintln!("Manca la sorgente. Usa: antiskill sources stress fsi-calendar --help");
                ExitCode::from(2)
            }
        },

        Some("candidates") => match args.next().as_deref() {
            Some("fsi-calendar") => match fsi_calendar::run_candidates(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources candidates fsi-calendar: {error}");
                    ExitCode::from(1)
                }
            },

            Some(source) => {
                eprintln!("Sorgente candidates sconosciuta: {source}");
                ExitCode::from(2)
            }

            None => {
                eprintln!(
                    "Manca la sorgente. Usa: antiskill sources candidates fsi-calendar --help"
                );
                ExitCode::from(2)
            }
        },

        Some("registry") => {
            match args.next().as_deref() {
                Some("torneionline-tpol") => match context_registry::run_build(args) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(error) => {
                        eprintln!("ERRORE sources registry torneionline-tpol: {error}");
                        ExitCode::from(1)
                    }
                },
                Some(source) => {
                    eprintln!("Sorgente registry sconosciuta: {source}");
                    ExitCode::from(2)
                }
                None => {
                    eprintln!("Manca la sorgente. Usa: antiskill sources registry torneionline-tpol --help");
                    ExitCode::from(2)
                }
            }
        }

        Some("index") => match args.next().as_deref() {
            Some("fsi-calendar") => match fsi_calendar::run_index(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources index fsi-calendar: {error}");
                    ExitCode::from(1)
                }
            },

            Some("torneionline-allin") => match torneionline_allin::run_index(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources index torneionline-allin: {error}");
                    ExitCode::from(1)
                }
            },

            Some("torneionline-tpol") => match torneionline_tpol::run_index(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources index torneionline-tpol: {error}");
                    ExitCode::from(1)
                }
            },

            Some("vesus-linked-events") => match vesus::run_index(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources index vesus-linked-events: {error}");
                    ExitCode::from(1)
                }
            },

            Some(source) => {
                eprintln!("Sorgente index sconosciuta: {source}");
                ExitCode::from(2)
            }

            None => {
                eprintln!("Manca la sorgente. Usa: antiskill sources index fsi-calendar --help");
                ExitCode::from(2)
            }
        },

        Some("query") => match args.next().as_deref() {
            Some("fsi-calendar") => match fsi_calendar::run_query(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources query fsi-calendar: {error}");
                    ExitCode::from(1)
                }
            },

            Some("torneionline-allin") => match torneionline_allin::run_query(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources query torneionline-allin: {error}");
                    ExitCode::from(1)
                }
            },

            Some("torneionline-tpol") => match torneionline_tpol::run_query(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources query torneionline-tpol: {error}");
                    ExitCode::from(1)
                }
            },

            Some("vesus-linked-events") => match vesus::run_query(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources query vesus-linked-events: {error}");
                    ExitCode::from(1)
                }
            },

            Some("context-registry") => match context_registry::run_query(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources query context-registry: {error}");
                    ExitCode::from(1)
                }
            },

            Some(source) => {
                eprintln!("Sorgente query sconosciuta: {source}");
                ExitCode::from(2)
            }

            None => {
                eprintln!("Manca la sorgente. Usa: antiskill sources query fsi-calendar --help");
                ExitCode::from(2)
            }
        },

        Some("diff") => match args.next().as_deref() {
            Some("fsi-calendar") => match fsi_calendar::run_diff(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources diff fsi-calendar: {error}");
                    ExitCode::from(1)
                }
            },

            Some(source) => {
                eprintln!("Sorgente diff sconosciuta: {source}");
                ExitCode::from(2)
            }

            None => {
                eprintln!("Manca la sorgente. Usa: antiskill sources diff fsi-calendar --help");
                ExitCode::from(2)
            }
        },

        Some("sync") => match args.next().as_deref() {
            Some("fsi-calendar") => match fsi_calendar::run(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources sync fsi-calendar: {error}");
                    ExitCode::from(1)
                }
            },

            Some("torneionline-allin") => match torneionline_allin::run(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources sync torneionline-allin: {error}");
                    ExitCode::from(1)
                }
            },

            Some("torneionline-tpol") => match torneionline_tpol::run_sync(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources sync torneionline-tpol: {error}");
                    ExitCode::from(1)
                }
            },

            Some("vesus-linked-events") => match vesus::run_sync(args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ERRORE sources sync vesus-linked-events: {error}");
                    ExitCode::from(1)
                }
            },

            Some(source) => {
                eprintln!("Sorgente sync sconosciuta: {source}");
                ExitCode::from(2)
            }

            None => {
                eprintln!("Manca la sorgente. Usa: antiskill sources sync fsi-calendar --help");
                ExitCode::from(2)
            }
        },

        Some(command) => {
            eprintln!("Sottocomando sources sconosciuto: {command}\n");
            print_help();
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_NONCE: AtomicU64 = AtomicU64::new(0);

    fn test_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "antiskill-source-status-{}-{}",
            std::process::id(),
            TEST_NONCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn store_calendar_snapshot(root: &Path, fetched_at_unix_ms: u128) {
        snapshot::store_snapshot(
            root,
            snapshot::SnapshotMetadata {
                source_key: "fsi-calendar".into(),
                source_url: "https://example.invalid/calendar".into(),
                fetched_at_unix_ms,
                http_status: 200,
                content_type: Some("text/html".into()),
                content_length_header: None,
                etag: None,
                last_modified: None,
                content_disposition: None,
                rights_class: "test".into(),
                retrieved_by: "antiskill-test".into(),
            },
            Cursor::new(b"synthetic calendar"),
        )
        .expect("creazione snapshot sintetico");
    }

    fn calendar_status(statuses: &[LocalSourceStatus]) -> &LocalSourceStatus {
        statuses
            .iter()
            .find(|status| status.source_key == "fsi-calendar")
            .expect("stato calendario")
    }

    #[test]
    fn source_keys_are_unique() {
        let mut keys = HashSet::new();

        for source in ITALY_SOURCES {
            assert!(keys.insert(source.key), "chiave duplicata: {}", source.key);
        }
    }

    #[test]
    fn core_italian_sources_are_present() {
        let keys: HashSet<_> = ITALY_SOURCES.iter().map(|source| source.key).collect();

        for required in [
            "fide-rating-list",
            "fsi-calendar",
            "fsi-members",
            "torneionline-allin",
            "torneionline-tpol",
            "torneionline-loto",
            "fsi-game-archive",
        ] {
            assert!(
                keys.contains(required),
                "fonte obbligatoria assente: {required}"
            );
        }
    }

    #[test]
    fn local_status_reports_missing_snapshot_as_not_configured() {
        let root = test_root();
        let statuses = italy_status_at(&root, 1_000_000);

        assert_eq!(
            calendar_status(&statuses).state,
            LocalSourceState::NonConfigurata
        );
        assert!(!root.exists());
    }

    #[test]
    fn local_status_uses_source_cadence_for_fresh_and_stale() {
        let root = test_root();
        let fetched = 1_000_000;
        store_calendar_snapshot(&root, fetched);

        let fresh = italy_status_at(&root, fetched + cadence_ms("6 ore"));
        assert_eq!(calendar_status(&fresh).state, LocalSourceState::Fresh);
        assert_eq!(calendar_status(&fresh).age_ms, Some(cadence_ms("6 ore")));

        let stale = italy_status_at(&root, fetched + cadence_ms("6 ore") + 1);
        assert_eq!(calendar_status(&stale).state, LocalSourceState::Stale);
        fs::remove_dir_all(root).expect("pulizia test");
    }

    #[test]
    fn local_status_fails_closed_for_corrupt_snapshot_object() {
        let root = test_root();
        store_calendar_snapshot(&root, 1_000_000);
        let verified = snapshot::load_verified_latest_snapshot(&root, "fsi-calendar")
            .expect("snapshot inizialmente valido");
        fs::write(verified.object_path, b"corrupt").expect("corruzione oggetto sintetico");

        let statuses = italy_status_at(&root, 1_000_001);
        assert_eq!(calendar_status(&statuses).state, LocalSourceState::Invalid);
        assert_eq!(calendar_status(&statuses).fetched_at_unix_ms, None);
        fs::remove_dir_all(root).expect("pulizia test");
    }

    #[test]
    fn local_status_json_has_a_stable_schema_version() {
        let report = ItalyStatusReport {
            schema_version: 1,
            sources: italy_status_at(&test_root(), 1_000_000),
        };
        let value = serde_json::to_value(report).expect("serializzazione report");

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["sources"][0]["state"], "non_configurata");
        assert!(value["sources"][0].get("age_ms").is_none());
    }
}
