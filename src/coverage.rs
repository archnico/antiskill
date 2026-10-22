use crate::inspector;
use serde::Serialize;
use std::path::Path;
use std::process::ExitCode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct CoverageReportV1 {
    schema_version: u32,
    dimensions: Vec<CoverageDimensionV1>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct CoverageDimensionV1 {
    dimension: &'static str,
    status: &'static str,
    observed: Option<u64>,
    expected: Option<u64>,
    ratio: Option<String>,
    note: &'static str,
}

pub(crate) fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if matches!(args.as_slice(), [a] if matches!(a.as_str(), "help"|"--help"|"-h")) {
        print_help();
        return ExitCode::SUCCESS;
    }
    let result = parse_args(&args)
        .and_then(|format| build(Path::new(".")).and_then(|report| render(&report, format)));
    match result {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERRORE coverage: {error}");
            ExitCode::from(1)
        }
    }
}

fn parse_args(args: &[String]) -> Result<OutputFormat, String> {
    match args {
        [] => Ok(OutputFormat::Human),
        [flag, value] if flag == "--format" => match value.as_str() {
            "human" => Ok(OutputFormat::Human),
            "json" => Ok(OutputFormat::Json),
            _ => Err(format!("Formato coverage non supportato: {value}")),
        },
        _ => Err("Uso: antiskill coverage [--format human|json]".into()),
    }
}

fn build(root: &Path) -> Result<CoverageReportV1, String> {
    let diagnosis = inspector::diagnose_workspace(root)?;
    let (sources, games) = match diagnosis {
        inspector::WorkspaceDiagnosis::InitializedNoSnapshot => (0, 0),
        inspector::WorkspaceDiagnosis::Active { sources, games } => (sources, games),
    };
    let annotations = inspector::annotation_coverage(root)?;
    let mut dimensions = Vec::with_capacity(8);
    dimensions.push(observed("sources", sources, "Acquired source artifacts in the active verified snapshot; expected universe not yet declared."));
    dimensions.push(observed(
        "games",
        games,
        "Game occurrences in the active verified snapshot; expected universe not yet declared.",
    ));
    for (dimension, note) in [
        (
            "identities",
            "Identity coverage requires a declared target population.",
        ),
        (
            "metadata",
            "Metadata coverage requires field-level expectations.",
        ),
        (
            "positions",
            "Position coverage awaits a canonical position index denominator.",
        ),
        (
            "analysis",
            "Analysis coverage awaits reusable-analysis requirements and a position denominator.",
        ),
        (
            "knowledge",
            "Historical and authority coverage requires explicit expected series or records.",
        ),
    ] {
        dimensions.push(unavailable(dimension, note));
    }
    dimensions.insert(
        6,
        observed(
            "annotations",
            annotations.items,
            "Attributed annotation items in verified content-addressed groups; expected annotation universe not yet declared.",
        ),
    );
    Ok(CoverageReportV1 {
        schema_version: 1,
        dimensions,
    })
}

fn observed(dimension: &'static str, value: u64, note: &'static str) -> CoverageDimensionV1 {
    CoverageDimensionV1 {
        dimension,
        status: "observed_only",
        observed: Some(value),
        expected: None,
        ratio: None,
        note,
    }
}
fn unavailable(dimension: &'static str, note: &'static str) -> CoverageDimensionV1 {
    CoverageDimensionV1 {
        dimension,
        status: "not_measurable_yet",
        observed: None,
        expected: None,
        ratio: None,
        note,
    }
}
fn render(report: &CoverageReportV1, format: OutputFormat) -> Result<String, String> {
    match format {
        OutputFormat::Json => serde_json::to_string_pretty(report)
            .map(|mut s| {
                s.push('\n');
                s
            })
            .map_err(|e| e.to_string()),
        OutputFormat::Human => {
            let mut out = String::from("Coverage vector v1\n");
            for d in &report.dimensions {
                let value = d
                    .observed
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".into());
                out.push_str(&format!(
                    "- {}: {} (observed={})\n",
                    d.dimension, d.status, value
                ));
            }
            out.push_str("No percentage is reported without an explicit expected denominator.\n");
            Ok(out)
        }
    }
}
fn print_help() {
    println!("Measure the Antiskill coverage vector without inventing missing denominators.\n\nUSAGE:\n    antiskill coverage [--format human|json]\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_dimensions_never_fake_a_ratio() {
        let d = unavailable("analysis", "x");
        assert_eq!(d.status, "not_measurable_yet");
        assert_eq!(d.ratio, None);
        assert_eq!(d.expected, None);
    }
    #[test]
    fn observed_dimensions_do_not_imply_completeness() {
        let d = observed("games", 42, "x");
        assert_eq!(d.observed, Some(42));
        assert_eq!(d.expected, None);
        assert_eq!(d.ratio, None);
    }
    #[test]
    fn json_schema_is_versioned() {
        let report = CoverageReportV1 {
            schema_version: 1,
            dimensions: vec![observed("games", 2, "x")],
        };
        let json = render(&report, OutputFormat::Json).unwrap();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"observed_only\""));
    }
}
