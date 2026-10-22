mod canonical_mainline;

use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReportFormat {
    Text,
    Jsonl,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductCanonicalGroup {
    pub(crate) occurrence_ids: Vec<String>,
    pub(crate) byte_variant_count: u64,
    pub(crate) result_conflict: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductCanonicalReport {
    pub(crate) groups: Vec<ProductCanonicalGroup>,
    pub(crate) noncomparable_occurrences: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct CanonicalMainlineArgs {
    root: PathBuf,
    mainlines: Vec<PathBuf>,
    collection: Option<PathBuf>,
    format: ReportFormat,
}

pub(crate) fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();

    match args.next().as_deref() {
        None | Some("help" | "--help" | "-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some("canonical-mainline") => match parse_canonical_mainline_args(args) {
            Ok(parsed) => {
                let inputs = if let Some(collection) = &parsed.collection {
                    crate::inspector::load_verified_mainlines_for_dedup_from_collection(
                        &parsed.root,
                        collection,
                    )
                    .and_then(canonical_mainline::validate_verified_inputs)
                } else {
                    canonical_mainline::load_verified_inputs(&parsed.root, &parsed.mainlines)
                };
                match inputs {
                    Ok(inputs) => match canonical_mainline::build_report(&inputs) {
                        Ok(report) => {
                            let rendered = match parsed.format {
                                ReportFormat::Text => canonical_mainline::render_text(&report),
                                ReportFormat::Jsonl => canonical_mainline::render_jsonl(&report),
                            };

                            match rendered {
                                Ok(output) => {
                                    print!("{output}");
                                    ExitCode::SUCCESS
                                }
                                Err(error) => {
                                    eprintln!("{error}");
                                    ExitCode::from(2)
                                }
                            }
                        }
                        Err(error) => {
                            eprintln!("{error}");
                            ExitCode::from(2)
                        }
                    },
                    Err(error) => {
                        eprintln!("{error}");
                        ExitCode::from(2)
                    }
                }
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(2)
            }
        },
        Some(command) => {
            eprintln!("Comando dedup sconosciuto: {command}\n");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn parse_canonical_mainline_args<I>(args: I) -> Result<CanonicalMainlineArgs, String>
where
    I: IntoIterator<Item = String>,
{
    let mut root = None;
    let mut mainlines = Vec::new();
    let mut collection = None;
    let mut format = None;
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(&mut args, "--root")?));
            }
            "--mainlines" => {
                mainlines.push(PathBuf::from(next_value(&mut args, "--mainlines")?));
            }
            "--collection" if collection.is_none() => {
                collection = Some(PathBuf::from(next_value(&mut args, "--collection")?));
            }
            "--format" if format.is_none() => {
                format = Some(match next_value(&mut args, "--format")?.as_str() {
                    "text" => ReportFormat::Text,
                    "jsonl" => ReportFormat::Jsonl,
                    value => return Err(format!("Formato dedup sconosciuto: {value}")),
                });
            }
            "--root" | "--collection" | "--format" => {
                return Err(format!("Opzione duplicata: {argument}"));
            }
            _ => return Err(format!("Argomento inatteso: {argument}")),
        }
    }

    if collection.is_some() && !mainlines.is_empty() {
        return Err("--collection e --mainlines sono mutuamente esclusivi".into());
    }
    if collection.is_none() && mainlines.is_empty() {
        return Err("Manca --collection COLLECTION o almeno un --mainlines MAINLINES".into());
    }

    Ok(CanonicalMainlineArgs {
        root: root.ok_or("Manca --root WORKSPACE")?,
        mainlines,
        collection,
        format: format.unwrap_or(ReportFormat::Text),
    })
}

fn next_value<I>(args: &mut I, option: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    let value = args
        .next()
        .ok_or_else(|| format!("Manca valore per {option}"))?;

    if value.starts_with("--") {
        return Err(format!("Manca valore per {option}"));
    }

    Ok(value)
}

fn print_help() {
    println!(
        "\
antiskill dedup

Deduplicazione verificabile delle occorrenze scacchistiche.

USO:
    antiskill dedup canonical-mainline \\
      --root WORKSPACE \\
      --mainlines MAINLINES \\
      [--mainlines MAINLINES ...] \\
      [--format text|jsonl]

oppure:
    antiskill dedup canonical-mainline \\
      --root WORKSPACE \\
      --collection COLLECTION \\
      [--format text|jsonl]

COMANDI:
    canonical-mainline    Gruppi con la stessa mainline canonica
    help                  Mostra questo messaggio
"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(values: &[&str]) -> Result<CanonicalMainlineArgs, String> {
        parse_canonical_mainline_args(values.iter().map(|value| (*value).to_string()))
    }

    #[test]
    fn accepts_multiple_mainlines_and_defaults_to_text() {
        let parsed = parse(&[
            "--root",
            "workspace",
            "--mainlines",
            "a.json.xz",
            "--mainlines",
            "b.json.xz",
        ])
        .unwrap();

        assert_eq!(parsed.root, PathBuf::from("workspace"));
        assert_eq!(
            parsed.mainlines,
            [PathBuf::from("a.json.xz"), PathBuf::from("b.json.xz")]
        );
        assert_eq!(parsed.format, ReportFormat::Text);
        assert_eq!(parsed.collection, None);
    }

    #[test]
    fn accepts_collection_as_alternative_input() {
        let parsed = parse(&["--root", "workspace", "--collection", "collection.json.xz"]).unwrap();
        assert!(parsed.mainlines.is_empty());
        assert_eq!(parsed.collection, Some(PathBuf::from("collection.json.xz")));
    }

    #[test]
    fn accepts_jsonl_format() {
        assert_eq!(
            parse(&[
                "--root",
                "workspace",
                "--mainlines",
                "a.json.xz",
                "--format",
                "jsonl",
            ])
            .unwrap()
            .format,
            ReportFormat::Jsonl
        );
    }

    #[test]
    fn requires_root_and_at_least_one_mainlines() {
        assert_eq!(
            parse(&["--mainlines", "a.json.xz"]).unwrap_err(),
            "Manca --root WORKSPACE"
        );
        assert_eq!(
            parse(&["--root", "workspace"]).unwrap_err(),
            "Manca --collection COLLECTION o almeno un --mainlines MAINLINES"
        );
    }

    #[test]
    fn rejects_duplicate_singleton_options_and_unknown_format() {
        assert_eq!(
            parse(&["--root", "a", "--root", "b", "--mainlines", "x.json.xz",]).unwrap_err(),
            "Opzione duplicata: --root"
        );
        assert_eq!(
            parse(&[
                "--root",
                "a",
                "--mainlines",
                "x.json.xz",
                "--format",
                "yaml",
            ])
            .unwrap_err(),
            "Formato dedup sconosciuto: yaml"
        );
    }

    #[test]
    fn rejects_missing_values_and_unexpected_arguments() {
        assert_eq!(
            parse(&["--root", "--mainlines", "x.json.xz"]).unwrap_err(),
            "Manca valore per --root"
        );
        assert_eq!(
            parse(&[
                "--root",
                "workspace",
                "--mainlines",
                "x.json.xz",
                "--collection",
                "c.json.xz",
            ])
            .unwrap_err(),
            "--collection e --mainlines sono mutuamente esclusivi"
        );
    }
}
