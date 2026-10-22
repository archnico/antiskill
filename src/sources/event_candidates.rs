use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

const EXAMPLE_LIMIT: usize = 10;
const MATCH_PAIR_SCHEMA_VERSION: u32 = 1;
pub(crate) const EVENT_CANDIDATE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct EventCandidateRecord {
    pub schema_version: u32,
    pub key_kind: String,
    pub source_key: String,
    pub source_event_id: String,
    pub source_title: String,
    pub normalized_title: String,
    pub starts_on: String,
    pub ends_on: Option<String>,
    pub location: Option<String>,
    pub province: Option<String>,
    pub region: Option<String>,
    pub source_url: Option<String>,
    pub event_categories: Vec<String>,
    pub classification_evidence: BTreeMap<String, Vec<String>>,
    pub canonical_match_string: String,
    pub candidate_match_key: String,
    pub snapshot_ref: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum MatchCardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct CandidateMatchPair {
    schema_version: u32,
    candidate_match_key: String,
    cardinality: MatchCardinality,
    left_key_count: u64,
    right_key_count: u64,
    left: EventCandidateRecord,
    right: EventCandidateRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MatchConfig {
    left: PathBuf,
    right: PathBuf,
}

impl MatchConfig {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut left = None;
        let mut right = None;
        let mut args = args.into_iter();

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--left" => {
                    left = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --left")?,
                    ));
                }
                "--right" => {
                    right = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --right")?,
                    ));
                }
                "--help" | "-h" => {
                    return Err(
                        "USO: antiskill sources match-candidates --left FILE --right FILE"
                            .to_owned(),
                    );
                }
                other => return Err(format!("Opzione sconosciuta: {other}")),
            }
        }

        Ok(Self {
            left: left.ok_or("Parametro obbligatorio: --left")?,
            right: right.ok_or("Parametro obbligatorio: --right")?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AuditConfig {
    input: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ValidateConfig {
    input: PathBuf,
}

impl ValidateConfig {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut input = None;
        let mut args = args.into_iter();

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--input" => {
                    input = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --input")?,
                    ));
                }
                "--help" | "-h" => {
                    return Err(
                        "USO: antiskill sources validate-events --input FILE.jsonl".to_owned()
                    );
                }
                other => return Err(format!("Opzione sconosciuta: {other}")),
            }
        }

        Ok(Self {
            input: input.ok_or("Parametro obbligatorio: --input")?,
        })
    }
}

impl AuditConfig {
    fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut input = None;
        let mut args = args.into_iter();

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--input" => {
                    input = Some(PathBuf::from(
                        args.next().ok_or("Manca il valore di --input")?,
                    ));
                }
                "--help" | "-h" => {
                    return Err("USO: antiskill sources audit-events --input FILE.jsonl".to_owned());
                }
                other => return Err(format!("Opzione sconosciuta: {other}")),
            }
        }

        Ok(Self {
            input: input.ok_or("Parametro obbligatorio: --input")?,
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
struct FieldCoverage {
    title: u64,
    start_date: u64,
    end_date: u64,
    location: u64,
    region: u64,
    source_url: u64,
    event_categories: u64,
    classification_evidence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct RecordExample {
    source_key: String,
    source_event_id: String,
    source_title: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct DuplicateIdExample {
    source_key: String,
    source_event_id: String,
    records: u64,
    titles: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct DuplicateKeyExample {
    candidate_match_key: String,
    records: u64,
    sources: Vec<String>,
    source_event_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct CollisionAuditExample {
    candidate_match_key: String,
    collision_kind: String,
    source_counts: BTreeMap<String, u64>,
    source_event_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
struct EventAuditReport {
    total_records: u64,
    missing_source_event_ids: u64,
    duplicate_source_event_ids: u64,
    records_with_duplicate_source_event_id: u64,
    missing_candidate_match_keys: u64,
    duplicate_candidate_match_keys: u64,
    records_with_duplicate_candidate_match_key: u64,
    coverage: FieldCoverage,
    category_counts: BTreeMap<String, u64>,
    one_to_many_collisions: u64,
    many_to_many_collisions: u64,
    missing_id_examples: Vec<RecordExample>,
    duplicate_id_examples: Vec<DuplicateIdExample>,
    missing_key_examples: Vec<RecordExample>,
    duplicate_key_examples: Vec<DuplicateKeyExample>,
    collision_examples: Vec<CollisionAuditExample>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ValidationExample {
    line: u64,
    source_key: Option<String>,
    source_event_id: Option<String>,
    errors: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
struct EventValidationReport {
    supported_schema_version: u32,
    total_records: u64,
    invalid_json_records: u64,
    unsupported_schema_versions: BTreeMap<u32, u64>,
    missing_required_fields: BTreeMap<String, u64>,
    invalid_source_event_ids: u64,
    invalid_candidate_match_keys: u64,
    invalid_start_dates: u64,
    invalid_end_dates: u64,
    incoherent_date_ranges: u64,
    invalid_source_urls: u64,
    invalid_event_categories: u64,
    invalid_classification_evidence: u64,
    duplicate_source_event_ids: u64,
    duplicate_candidate_match_keys: u64,
    deterministically_ordered: bool,
    validation_errors: u64,
    examples: Vec<ValidationExample>,
}

pub(crate) fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let config = MatchConfig::from_args(args)?;
    let left = fs::read(&config.left)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.left.display()))?;
    let right = fs::read(&config.right)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.right.display()))?;
    let output = match_candidate_jsonl(&left, &right)?;

    let stdout = io::stdout();
    let mut writer = stdout.lock();
    writer
        .write_all(&output)
        .map_err(|error| format!("Impossibile scrivere le coppie candidate: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("Impossibile completare le coppie candidate: {error}"))
}

pub(crate) fn run_audit<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let config = AuditConfig::from_args(args)?;
    let input = fs::read(&config.input)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.input.display()))?;
    let output = audit_event_jsonl(&input)?;

    let stdout = io::stdout();
    let mut writer = stdout.lock();
    writer
        .write_all(&output)
        .map_err(|error| format!("Impossibile scrivere il rapporto audit: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("Impossibile completare il rapporto audit: {error}"))
}

pub(crate) fn run_validate<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    let config = ValidateConfig::from_args(args)?;
    let input = fs::read(&config.input)
        .map_err(|error| format!("Impossibile leggere {}: {error}", config.input.display()))?;
    let output = validate_event_jsonl(&input)?;

    let stdout = io::stdout();
    let mut writer = stdout.lock();
    writer
        .write_all(&output)
        .map_err(|error| format!("Impossibile scrivere il rapporto validazione: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("Impossibile completare il rapporto validazione: {error}"))
}

pub(crate) fn validate_event_jsonl(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut report = EventValidationReport {
        supported_schema_version: EVENT_CANDIDATE_SCHEMA_VERSION,
        deterministically_ordered: true,
        ..EventValidationReport::default()
    };
    let mut parsed_records = Vec::<EventCandidateRecord>::new();

    for (line_index, line) in input.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        report.total_records += 1;
        let line_number = (line_index + 1) as u64;
        let value: serde_json::Value = match serde_json::from_slice(line) {
            Ok(value) => value,
            Err(_) => {
                report.invalid_json_records += 1;
                report.validation_errors += 1;
                push_validation_example(
                    &mut report,
                    ValidationExample {
                        line: line_number,
                        source_key: None,
                        source_event_id: None,
                        errors: vec!["invalid_json".to_owned()],
                    },
                );
                continue;
            }
        };

        let mut errors = Vec::new();
        validate_schema_version(&value, &mut report, &mut errors);
        validate_required_fields(&value, &mut report, &mut errors);
        validate_identifier_and_key(&value, &mut report, &mut errors);
        validate_dates(&value, &mut report, &mut errors);
        validate_url(&value, &mut report, &mut errors);
        validate_classification(&value, &mut report, &mut errors);

        match serde_json::from_value::<EventCandidateRecord>(value.clone()) {
            Ok(record) => parsed_records.push(record),
            Err(_) => errors.push("invalid_record_shape".to_owned()),
        }
        errors.sort();
        errors.dedup();
        report.validation_errors += errors.len() as u64;
        if !errors.is_empty() {
            push_validation_example(
                &mut report,
                ValidationExample {
                    line: line_number,
                    source_key: json_string(&value, "source_key"),
                    source_event_id: json_string(&value, "source_event_id"),
                    errors,
                },
            );
        }
    }

    report.deterministically_ordered = parsed_records
        .windows(2)
        .all(|records| compare_records(&records[0], &records[1]).is_le());
    let mut ids = BTreeMap::<(String, String), u64>::new();
    let mut keys = BTreeMap::<String, u64>::new();
    for record in &parsed_records {
        if !record.source_event_id.is_empty() {
            *ids.entry((record.source_key.clone(), record.source_event_id.clone()))
                .or_default() += 1;
        }
        if !record.candidate_match_key.is_empty() {
            *keys.entry(record.candidate_match_key.clone()).or_default() += 1;
        }
    }
    report.duplicate_source_event_ids = ids.values().filter(|count| **count > 1).count() as u64;
    report.duplicate_candidate_match_keys =
        keys.values().filter(|count| **count > 1).count() as u64;
    if !report.deterministically_ordered {
        report.validation_errors += 1;
    }
    report.validation_errors +=
        report.duplicate_source_event_ids + report.duplicate_candidate_match_keys;

    let mut output = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("Errore serializzando il rapporto validazione: {error}"))?;
    output.push(b'\n');
    Ok(output)
}

fn validate_schema_version(
    value: &serde_json::Value,
    report: &mut EventValidationReport,
    errors: &mut Vec<String>,
) {
    match value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
    {
        Some(version) if version == u64::from(EVENT_CANDIDATE_SCHEMA_VERSION) => {}
        Some(version) if version <= u64::from(u32::MAX) => {
            *report
                .unsupported_schema_versions
                .entry(version as u32)
                .or_default() += 1;
            errors.push("unsupported_schema_version".to_owned());
        }
        _ => errors.push("invalid_schema_version".to_owned()),
    }
}

fn validate_required_fields(
    value: &serde_json::Value,
    report: &mut EventValidationReport,
    errors: &mut Vec<String>,
) {
    for field in [
        "key_kind",
        "source_key",
        "source_event_id",
        "source_title",
        "normalized_title",
        "starts_on",
        "canonical_match_string",
        "candidate_match_key",
        "event_categories",
        "classification_evidence",
        "snapshot_ref",
    ] {
        let missing = match field {
            "event_categories" => value
                .get(field)
                .and_then(serde_json::Value::as_array)
                .is_none_or(Vec::is_empty),
            "classification_evidence" => value
                .get(field)
                .and_then(serde_json::Value::as_object)
                .is_none(),
            _ => json_string(value, field)
                .as_deref()
                .is_none_or(str::is_empty),
        };
        if missing {
            *report
                .missing_required_fields
                .entry(field.to_owned())
                .or_default() += 1;
            errors.push(format!("missing_{field}"));
        }
    }
}

fn validate_classification(
    value: &serde_json::Value,
    report: &mut EventValidationReport,
    errors: &mut Vec<String>,
) {
    let categories = value
        .get("event_categories")
        .and_then(serde_json::Value::as_array);
    let evidence = value
        .get("classification_evidence")
        .and_then(serde_json::Value::as_object);
    let valid_categories = categories.is_some_and(|categories| {
        let values: Option<Vec<&str>> = categories.iter().map(serde_json::Value::as_str).collect();
        values.is_some_and(|values| {
            !values.is_empty()
                && values.iter().all(|category| !category.is_empty())
                && values.windows(2).all(|pair| pair[0] < pair[1])
        })
    });
    if !valid_categories {
        report.invalid_event_categories += 1;
        errors.push("invalid_event_categories".to_owned());
    }

    let valid_evidence = match (categories, evidence) {
        (Some(categories), Some(evidence)) => {
            let category_values: Option<Vec<&str>> =
                categories.iter().map(serde_json::Value::as_str).collect();
            category_values.is_some_and(|categories| {
                let evidence_keys: Vec<&str> = evidence.keys().map(String::as_str).collect();
                evidence_keys == categories
                    && categories.iter().all(|category| {
                        evidence.get(*category).is_some_and(|values| {
                            values.as_array().is_some_and(|values| {
                                let strings: Option<Vec<&str>> =
                                    values.iter().map(serde_json::Value::as_str).collect();
                                strings.is_some_and(|strings| {
                                    if *category == "unclassified" {
                                        strings.is_empty()
                                    } else {
                                        !strings.is_empty()
                                            && strings.iter().all(|value| !value.is_empty())
                                            && strings.windows(2).all(|pair| pair[0] < pair[1])
                                    }
                                })
                            })
                        })
                    })
            })
        }
        _ => false,
    };
    if !valid_evidence {
        report.invalid_classification_evidence += 1;
        errors.push("invalid_classification_evidence".to_owned());
    }
}

fn validate_identifier_and_key(
    value: &serde_json::Value,
    report: &mut EventValidationReport,
    errors: &mut Vec<String>,
) {
    if json_string(value, "source_event_id")
        .as_deref()
        .is_some_and(|identifier| identifier.trim() != identifier || identifier.is_empty())
    {
        report.invalid_source_event_ids += 1;
        errors.push("invalid_source_event_id".to_owned());
    }
    if json_string(value, "candidate_match_key")
        .as_deref()
        .is_some_and(|key| !valid_candidate_key(key))
    {
        report.invalid_candidate_match_keys += 1;
        errors.push("invalid_candidate_match_key".to_owned());
    }
}

fn validate_dates(
    value: &serde_json::Value,
    report: &mut EventValidationReport,
    errors: &mut Vec<String>,
) {
    let start = json_string(value, "starts_on");
    let parsed_start = start.as_deref().and_then(parse_calendar_date);
    if start.as_deref().is_some_and(|date| !date.is_empty()) && parsed_start.is_none() {
        report.invalid_start_dates += 1;
        errors.push("invalid_start_date".to_owned());
    }

    let end = json_string(value, "ends_on");
    let parsed_end = end.as_deref().and_then(parse_calendar_date);
    if end.as_deref().is_some_and(|date| !date.is_empty()) && parsed_end.is_none() {
        report.invalid_end_dates += 1;
        errors.push("invalid_end_date".to_owned());
    }
    if let (Some(start), Some(end)) = (parsed_start, parsed_end) {
        if end < start {
            report.incoherent_date_ranges += 1;
            errors.push("incoherent_date_range".to_owned());
        }
    }
}

fn validate_url(
    value: &serde_json::Value,
    report: &mut EventValidationReport,
    errors: &mut Vec<String>,
) {
    if let Some(url) = json_string(value, "source_url") {
        if !url.is_empty() && !valid_http_url(&url) {
            report.invalid_source_urls += 1;
            errors.push("invalid_source_url".to_owned());
        }
    }
}

fn json_string(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn valid_candidate_key(key: &str) -> bool {
    key.strip_prefix("candidate-v1:blake3:")
        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

pub(crate) fn parse_calendar_date(value: &str) -> Option<(u32, u32, u32)> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[2] != b'-'
        || bytes[5] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| !matches!(index, 2 | 5) && !byte.is_ascii_digit())
    {
        return None;
    }
    let mut parts = value.split('-');
    let day = parts.next()?.parse::<u32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let year = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || value.len() != 10 || year == 0 || !(1..=12).contains(&month) {
        return None;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (day >= 1 && day <= days).then_some((year, month, day))
}

fn valid_http_url(value: &str) -> bool {
    value.parse::<ureq::http::Uri>().ok().is_some_and(|uri| {
        matches!(uri.scheme_str(), Some("http" | "https")) && uri.host().is_some()
    })
}

fn push_validation_example(report: &mut EventValidationReport, example: ValidationExample) {
    if report.examples.len() < EXAMPLE_LIMIT {
        report.examples.push(example);
    }
}

fn audit_event_jsonl(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut records = read_jsonl(input, "input")?;
    records.sort_by(compare_audit_records);
    let mut report = EventAuditReport {
        total_records: records.len() as u64,
        ..EventAuditReport::default()
    };
    let mut ids = BTreeMap::<(String, String), Vec<&EventCandidateRecord>>::new();
    let mut keys = BTreeMap::<String, Vec<&EventCandidateRecord>>::new();

    for record in &records {
        update_coverage(&mut report.coverage, record);
        for category in &record.event_categories {
            *report.category_counts.entry(category.clone()).or_default() += 1;
        }

        if record.source_event_id.is_empty() {
            report.missing_source_event_ids += 1;
            if report.missing_id_examples.len() < EXAMPLE_LIMIT {
                report.missing_id_examples.push(record_example(record));
            }
        } else {
            ids.entry((record.source_key.clone(), record.source_event_id.clone()))
                .or_default()
                .push(record);
        }

        if record.candidate_match_key.is_empty() {
            report.missing_candidate_match_keys += 1;
            if report.missing_key_examples.len() < EXAMPLE_LIMIT {
                report.missing_key_examples.push(record_example(record));
            }
        } else {
            keys.entry(record.candidate_match_key.clone())
                .or_default()
                .push(record);
        }
    }

    for ((source_key, source_event_id), duplicate_records) in
        ids.into_iter().filter(|(_, records)| records.len() > 1)
    {
        report.duplicate_source_event_ids += 1;
        report.records_with_duplicate_source_event_id += duplicate_records.len() as u64;
        if report.duplicate_id_examples.len() < EXAMPLE_LIMIT {
            report.duplicate_id_examples.push(DuplicateIdExample {
                source_key,
                source_event_id,
                records: duplicate_records.len() as u64,
                titles: sorted_unique(
                    duplicate_records
                        .iter()
                        .map(|record| record.source_title.clone()),
                ),
            });
        }
    }

    for (candidate_match_key, duplicate_records) in
        keys.into_iter().filter(|(_, records)| records.len() > 1)
    {
        report.duplicate_candidate_match_keys += 1;
        report.records_with_duplicate_candidate_match_key += duplicate_records.len() as u64;
        let source_counts = source_counts(&duplicate_records);
        let many_sources = source_counts.values().filter(|count| **count > 1).count();
        let has_single_source = source_counts.values().any(|count| *count == 1);
        let collision_kind = if many_sources >= 2 {
            report.many_to_many_collisions += 1;
            Some("many_to_many")
        } else if many_sources == 1 && has_single_source {
            report.one_to_many_collisions += 1;
            Some("one_to_many")
        } else {
            None
        };

        if report.duplicate_key_examples.len() < EXAMPLE_LIMIT {
            report.duplicate_key_examples.push(DuplicateKeyExample {
                candidate_match_key: candidate_match_key.clone(),
                records: duplicate_records.len() as u64,
                sources: sorted_unique(
                    duplicate_records
                        .iter()
                        .map(|record| record.source_key.clone()),
                ),
                source_event_ids: sorted_unique(
                    duplicate_records
                        .iter()
                        .map(|record| record.source_event_id.clone()),
                ),
            });
        }
        if let Some(collision_kind) = collision_kind {
            if report.collision_examples.len() < EXAMPLE_LIMIT {
                report.collision_examples.push(CollisionAuditExample {
                    candidate_match_key,
                    collision_kind: collision_kind.to_owned(),
                    source_counts,
                    source_event_ids: sorted_unique(
                        duplicate_records
                            .iter()
                            .map(|record| record.source_event_id.clone()),
                    ),
                });
            }
        }
    }

    let mut output = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("Errore serializzando il rapporto audit: {error}"))?;
    output.push(b'\n');
    Ok(output)
}

fn update_coverage(coverage: &mut FieldCoverage, record: &EventCandidateRecord) {
    coverage.title += u64::from(!record.source_title.is_empty());
    coverage.start_date += u64::from(!record.starts_on.is_empty());
    coverage.end_date += u64::from(
        record
            .ends_on
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
    );
    coverage.location += u64::from(
        record
            .location
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
    );
    coverage.region += u64::from(
        record
            .region
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
    );
    coverage.source_url += u64::from(
        record
            .source_url
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
    );
    coverage.event_categories += u64::from(!record.event_categories.is_empty());
    coverage.classification_evidence += u64::from(!record.classification_evidence.is_empty());
}

fn record_example(record: &EventCandidateRecord) -> RecordExample {
    RecordExample {
        source_key: record.source_key.clone(),
        source_event_id: record.source_event_id.clone(),
        source_title: record.source_title.clone(),
    }
}

fn source_counts(records: &[&EventCandidateRecord]) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for record in records {
        *counts.entry(record.source_key.clone()).or_default() += 1;
    }
    counts
}

fn sorted_unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn match_candidate_jsonl(left: &[u8], right: &[u8]) -> Result<Vec<u8>, String> {
    let left = group_by_key(read_jsonl(left, "left")?);
    let right = group_by_key(read_jsonl(right, "right")?);
    let mut output = Vec::new();

    for (key, left_records) in left {
        let Some(right_records) = right.get(&key) else {
            continue;
        };
        let cardinality = cardinality(left_records.len(), right_records.len());

        for left_record in &left_records {
            for right_record in right_records {
                let pair = CandidateMatchPair {
                    schema_version: MATCH_PAIR_SCHEMA_VERSION,
                    candidate_match_key: key.clone(),
                    cardinality,
                    left_key_count: left_records.len() as u64,
                    right_key_count: right_records.len() as u64,
                    left: left_record.clone(),
                    right: right_record.clone(),
                };
                serde_json::to_writer(&mut output, &pair)
                    .map_err(|error| format!("Errore serializzando coppia candidate: {error}"))?;
                output.push(b'\n');
            }
        }
    }

    Ok(output)
}

fn read_jsonl(input: &[u8], side: &str) -> Result<Vec<EventCandidateRecord>, String> {
    input
        .split(|byte| *byte == b'\n')
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(index, line)| {
            let value: serde_json::Value = serde_json::from_slice(line).map_err(|error| {
                format!("File {side} non valido alla riga {}: {error}", index + 1)
            })?;
            let version = value
                .get("schema_version")
                .and_then(serde_json::Value::as_u64);
            if version != Some(u64::from(EVENT_CANDIDATE_SCHEMA_VERSION)) {
                return Err(format!(
                    "Versione EventCandidateRecord non supportata nel file {side} alla riga {}: \
                     trovata {}, attesa V{EVENT_CANDIDATE_SCHEMA_VERSION}",
                    index + 1,
                    version
                        .map(|version| format!("V{version}"))
                        .unwrap_or_else(|| "mancante".to_owned())
                ));
            }
            serde_json::from_value(value)
                .map_err(|error| format!("File {side} non valido alla riga {}: {error}", index + 1))
        })
        .collect()
}

fn group_by_key(records: Vec<EventCandidateRecord>) -> BTreeMap<String, Vec<EventCandidateRecord>> {
    let mut groups = BTreeMap::new();
    for record in records {
        groups
            .entry(record.candidate_match_key.clone())
            .or_insert_with(Vec::new)
            .push(record);
    }
    for records in groups.values_mut() {
        records.sort_by(compare_records);
    }
    groups
}

fn compare_records(
    left: &EventCandidateRecord,
    right: &EventCandidateRecord,
) -> std::cmp::Ordering {
    left.source_key
        .cmp(&right.source_key)
        .then_with(|| left.source_event_id.cmp(&right.source_event_id))
        .then_with(|| left.source_title.cmp(&right.source_title))
        .then_with(|| left.starts_on.cmp(&right.starts_on))
        .then_with(|| left.ends_on.cmp(&right.ends_on))
        .then_with(|| left.source_url.cmp(&right.source_url))
}

fn compare_audit_records(
    left: &EventCandidateRecord,
    right: &EventCandidateRecord,
) -> std::cmp::Ordering {
    left.candidate_match_key
        .cmp(&right.candidate_match_key)
        .then_with(|| compare_records(left, right))
}

fn cardinality(left: usize, right: usize) -> MatchCardinality {
    match (left > 1, right > 1) {
        (false, false) => MatchCardinality::OneToOne,
        (false, true) => MatchCardinality::OneToMany,
        (true, false) => MatchCardinality::ManyToOne,
        (true, true) => MatchCardinality::ManyToMany,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(source: &str, id: &str, key: &str) -> EventCandidateRecord {
        EventCandidateRecord {
            schema_version: EVENT_CANDIDATE_SCHEMA_VERSION,
            key_kind: "candidate-only".to_owned(),
            source_key: source.to_owned(),
            source_event_id: id.to_owned(),
            source_title: format!("Evento {id}"),
            normalized_title: format!("evento {id}"),
            starts_on: "01-09-2026".to_owned(),
            ends_on: None,
            location: Some("Roma".to_owned()),
            province: Some("Roma".to_owned()),
            region: Some("Lazio".to_owned()),
            source_url: Some(format!("https://example.invalid/{source}/{id}")),
            event_categories: vec!["unclassified".to_owned()],
            classification_evidence: BTreeMap::from([("unclassified".to_owned(), Vec::new())]),
            canonical_match_string: format!("test={key}"),
            candidate_match_key: key.to_owned(),
            snapshot_ref: format!("{source}-snapshot"),
        }
    }

    fn jsonl(records: &[EventCandidateRecord]) -> Vec<u8> {
        let mut output = Vec::new();
        for record in records {
            serde_json::to_writer(&mut output, record).expect("record sintetico");
            output.push(b'\n');
        }
        output
    }

    fn validated_candidate(source: &str, id: &str, label: &str) -> EventCandidateRecord {
        let mut record = candidate(source, id, "");
        record.canonical_match_string = format!("title={label} | start=01-09-2026");
        record.candidate_match_key = format!(
            "candidate-v1:blake3:{}",
            blake3::hash(record.canonical_match_string.as_bytes()).to_hex()
        );
        record
    }

    fn output_records(output: &[u8]) -> Vec<serde_json::Value> {
        output
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("coppia JSON"))
            .collect()
    }

    #[test]
    fn exact_key_join_is_deterministic_and_preserves_records() {
        let left = jsonl(&[
            candidate("fsi", "2", "key-b"),
            candidate("fsi", "1", "key-a"),
        ]);
        let right = jsonl(&[
            candidate("vesus", "20", "key-b"),
            candidate("vesus", "10", "key-a"),
        ]);

        let first = match_candidate_jsonl(&left, &right).expect("primo confronto");
        let second = match_candidate_jsonl(&left, &right).expect("secondo confronto");
        let records = output_records(&first);

        assert_eq!(first, second);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["candidate_match_key"], "key-a");
        assert_eq!(records[0]["cardinality"], "one_to_one");
        assert_eq!(records[0]["left"]["source_event_id"], "1");
        assert_eq!(records[0]["right"]["source_event_id"], "10");
        assert_eq!(
            records[0]["right"]["source_url"],
            "https://example.invalid/vesus/10"
        );
    }

    #[test]
    fn reports_one_to_many_and_many_to_many_on_every_pair() {
        let left = jsonl(&[
            candidate("fsi", "1", "one-many"),
            candidate("fsi", "3", "many-many"),
            candidate("fsi", "2", "many-many"),
        ]);
        let right = jsonl(&[
            candidate("vesus", "11", "one-many"),
            candidate("vesus", "10", "one-many"),
            candidate("vesus", "21", "many-many"),
            candidate("vesus", "20", "many-many"),
        ]);
        let records =
            output_records(&match_candidate_jsonl(&left, &right).expect("confronto multiplo"));

        assert_eq!(records.len(), 6);
        assert_eq!(records[0]["cardinality"], "many_to_many");
        assert_eq!(records[0]["left_key_count"], 2);
        assert_eq!(records[0]["right_key_count"], 2);
        assert_eq!(records[4]["cardinality"], "one_to_many");
        assert_eq!(records[4]["left_key_count"], 1);
        assert_eq!(records[4]["right_key_count"], 2);
    }

    #[test]
    fn similar_titles_with_different_keys_do_not_match() {
        let left = jsonl(&[candidate("fsi", "1", "exact-a")]);
        let right = jsonl(&[candidate("vesus", "2", "exact-b")]);

        assert!(match_candidate_jsonl(&left, &right)
            .expect("confronto senza match")
            .is_empty());
    }

    #[test]
    fn audit_reports_coverage_duplicates_and_cross_source_collisions() {
        let mut missing_id = candidate("fsi", "", "missing-id-key");
        missing_id.source_title = "Senza ID".to_owned();

        let mut missing_key = candidate("fsi", "4", "");
        missing_key.source_title.clear();
        missing_key.normalized_title.clear();
        missing_key.starts_on.clear();
        missing_key.location = None;
        missing_key.region = None;
        missing_key.source_url = None;

        let records = [
            candidate("fsi", "1", "one-many"),
            candidate("vesus", "10", "one-many"),
            candidate("vesus", "11", "one-many"),
            candidate("fsi", "2", "many-many"),
            candidate("fsi", "3", "many-many"),
            candidate("vesus", "20", "many-many"),
            candidate("vesus", "21", "many-many"),
            candidate("fsi", "1", "other-key"),
            missing_id,
            missing_key,
        ];

        let first = audit_event_jsonl(&jsonl(&records)).expect("primo audit");
        let second = audit_event_jsonl(&jsonl(&records)).expect("secondo audit");
        let report: serde_json::Value = serde_json::from_slice(&first).expect("rapporto JSON");

        assert_eq!(first, second);
        assert_eq!(report["total_records"], 10);
        assert_eq!(report["missing_source_event_ids"], 1);
        assert_eq!(report["duplicate_source_event_ids"], 1);
        assert_eq!(report["records_with_duplicate_source_event_id"], 2);
        assert_eq!(report["missing_candidate_match_keys"], 1);
        assert_eq!(report["duplicate_candidate_match_keys"], 2);
        assert_eq!(report["records_with_duplicate_candidate_match_key"], 7);
        assert_eq!(report["coverage"]["title"], 9);
        assert_eq!(report["coverage"]["start_date"], 9);
        assert_eq!(report["coverage"]["end_date"], 0);
        assert_eq!(report["coverage"]["location"], 9);
        assert_eq!(report["coverage"]["region"], 9);
        assert_eq!(report["coverage"]["source_url"], 9);
        assert_eq!(report["coverage"]["event_categories"], 10);
        assert_eq!(report["coverage"]["classification_evidence"], 10);
        assert_eq!(report["category_counts"]["unclassified"], 10);
        assert_eq!(report["one_to_many_collisions"], 1);
        assert_eq!(report["many_to_many_collisions"], 1);
        assert_eq!(
            report["collision_examples"][0]["candidate_match_key"],
            "many-many"
        );
        assert_eq!(
            report["collision_examples"][1]["candidate_match_key"],
            "one-many"
        );
    }

    #[test]
    fn audit_examples_are_limited_and_independent_of_input_order() {
        let forward: Vec<_> = (0..15)
            .map(|index| candidate("fsi", "", &format!("key-{index:02}")))
            .collect();
        let reverse: Vec<_> = forward.iter().cloned().rev().collect();

        let first = audit_event_jsonl(&jsonl(&forward)).expect("audit ordinato");
        let second = audit_event_jsonl(&jsonl(&reverse)).expect("audit inverso");
        let report: serde_json::Value = serde_json::from_slice(&first).expect("rapporto JSON");

        assert_eq!(first, second);
        assert_eq!(
            report["missing_id_examples"]
                .as_array()
                .expect("esempi array")
                .len(),
            EXAMPLE_LIMIT
        );
    }

    #[test]
    fn validation_accepts_supported_well_formed_ordered_records() {
        let records = [
            validated_candidate("fsi", "1", "evento uno"),
            validated_candidate("fsi", "2", "evento due"),
        ];

        let first = validate_event_jsonl(&jsonl(&records)).expect("prima validazione");
        let second = validate_event_jsonl(&jsonl(&records)).expect("seconda validazione");
        let report: serde_json::Value = serde_json::from_slice(&first).expect("rapporto JSON");

        assert_eq!(first, second);
        assert_eq!(report["supported_schema_version"], 2);
        assert_eq!(report["total_records"], 2);
        assert_eq!(report["validation_errors"], 0);
        assert_eq!(report["deterministically_ordered"], true);
    }

    #[test]
    fn validation_reports_format_date_url_duplicates_and_order() {
        let mut first = validated_candidate("fsi", "2", "evento due");
        first.schema_version = 1;
        first.starts_on = "31-02-2026".to_owned();
        first.ends_on = Some("01-01-2026".to_owned());
        first.source_url = Some("ftp://example.invalid/event".to_owned());
        first.candidate_match_key = "not-a-key".to_owned();

        let mut second = validated_candidate("fsi", "2", "evento uno");
        second.source_title.clear();
        second.normalized_title.clear();
        second.candidate_match_key = first.candidate_match_key.clone();

        let report: serde_json::Value = serde_json::from_slice(
            &validate_event_jsonl(&jsonl(&[first, second])).expect("validazione errori"),
        )
        .expect("rapporto JSON");

        assert_eq!(report["unsupported_schema_versions"]["1"], 1);
        assert_eq!(report["missing_required_fields"]["source_title"], 1);
        assert_eq!(report["missing_required_fields"]["normalized_title"], 1);
        assert_eq!(report["invalid_candidate_match_keys"], 2);
        assert_eq!(report["invalid_start_dates"], 1);
        assert_eq!(report["invalid_source_urls"], 1);
        assert_eq!(report["duplicate_source_event_ids"], 1);
        assert_eq!(report["duplicate_candidate_match_keys"], 1);
        assert_eq!(report["deterministically_ordered"], false);
        assert!(report["validation_errors"].as_u64().expect("numero errori") >= 8);
    }

    #[test]
    fn validation_rejects_invalid_end_date_and_incoherent_range() {
        let mut invalid_end = validated_candidate("fsi", "1", "fine invalida");
        invalid_end.ends_on = Some("31-11-2026".to_owned());
        let mut reversed = validated_candidate("fsi", "2", "intervallo inverso");
        reversed.starts_on = "02-09-2026".to_owned();
        reversed.ends_on = Some("01-09-2026".to_owned());

        let report: serde_json::Value = serde_json::from_slice(
            &validate_event_jsonl(&jsonl(&[invalid_end, reversed])).expect("validazione date"),
        )
        .expect("rapporto JSON");

        assert_eq!(report["invalid_end_dates"], 1);
        assert_eq!(report["incoherent_date_ranges"], 1);
    }

    #[test]
    fn validation_checks_ordered_categories_and_matching_evidence() {
        let mut record = validated_candidate("fsi", "1", "classificazioni");
        record.event_categories = vec!["rapid".to_owned(), "blitz".to_owned()];
        record.classification_evidence =
            BTreeMap::from([("rapid".to_owned(), vec!["Rapid".to_owned()])]);

        let report: serde_json::Value = serde_json::from_slice(
            &validate_event_jsonl(&jsonl(&[record])).expect("validazione classificazioni"),
        )
        .expect("rapporto JSON");

        assert_eq!(report["invalid_event_categories"], 1);
        assert_eq!(report["invalid_classification_evidence"], 1);
    }

    #[test]
    fn generic_consumers_explicitly_reject_v1_records() {
        let mut record = validated_candidate("fsi", "1", "versione precedente");
        record.schema_version = 1;
        let error = read_jsonl(&jsonl(&[record]), "test")
            .expect_err("EventCandidateRecord V1 deve essere rifiutato");

        assert!(error.contains("trovata V1"));
        assert!(error.contains("attesa V2"));
    }
}
