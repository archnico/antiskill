use super::{load_verified_catalog, manifest::*};
use std::path::PathBuf;

pub(in crate::inspector) struct Query {
    pub root: PathBuf,
    pub catalog: PathBuf,
    pub filters: QueryFilters,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::inspector) struct QueryFilters {
    pub player: Option<String>,
    pub event: Option<String>,
    pub date: Option<CivilDate>,
    pub date_from: Option<CivilDate>,
    pub date_to: Option<CivilDate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::inspector) struct CivilDate {
    year: u16,
    month: u8,
    day: u8,
}

impl CivilDate {
    pub(in crate::inspector) fn canonical_string(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl QueryFilters {
    pub(in crate::inspector) fn canonicalized(&self) -> Self {
        Self {
            player: self.player.as_ref().map(|value| value.to_lowercase()),
            event: self.event.as_ref().map(|value| value.to_lowercase()),
            date: self.date,
            date_from: self.date_from,
            date_to: self.date_to,
        }
    }
}

pub(in crate::inspector) fn parse_cli_date(value: &str) -> Result<CivilDate, String> {
    parse_date(value, b'-').ok_or_else(|| format!("Data CLI non valida: {value}"))
}

fn parse_pgn_date(value: &str) -> Option<CivilDate> {
    parse_date(value, b'.')
}

fn parse_date(value: &str, separator: u8) -> Option<CivilDate> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes.get(4) != Some(&separator) || bytes.get(7) != Some(&separator) {
        return None;
    }
    let year = parse_digits(bytes.get(0..4)?)?;
    let month = parse_digits(bytes.get(5..7)?)?;
    let day = parse_digits(bytes.get(8..10)?)?;
    let year = u16::try_from(year).ok()?;
    let month = u8::try_from(month).ok()?;
    let day = u8::try_from(day).ok()?;
    if month == 0 || month > 12 || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(CivilDate { year, month, day })
}
fn parse_digits(bytes: &[u8]) -> Option<u32> {
    bytes.iter().try_fold(0_u32, |n, b| {
        b.is_ascii_digit().then(|| n * 10 + u32::from(*b - b'0'))
    })
}
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}
fn is_leap(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

pub(in crate::inspector) fn validate_filters(filters: &QueryFilters) -> Result<(), String> {
    if filters.player.as_ref().is_some_and(|v| v.trim().is_empty()) {
        return Err("Query player vuota".into());
    }
    if filters.event.as_ref().is_some_and(|v| v.trim().is_empty()) {
        return Err("Query event vuota".into());
    }
    if filters.date.is_some() && (filters.date_from.is_some() || filters.date_to.is_some()) {
        return Err("--date non può essere combinata con un range".into());
    }
    if filters
        .date_from
        .zip(filters.date_to)
        .is_some_and(|(from, to)| from > to)
    {
        return Err("date-from successiva a date-to".into());
    }
    Ok(())
}

pub(super) fn run(q: &Query) -> Result<String, String> {
    validate_filters(&q.filters)?;
    let loaded = load_verified_catalog(&q.root, &q.catalog)?;
    let games = loaded
        .manifest
        .games
        .iter()
        .filter(|g| matches_game(g, &q.filters))
        .collect::<Vec<_>>();
    let mut out = format!("MATCHES\t{}\n", games.len());
    for game in games {
        let values = |name: &str| decoded_values(game, name);
        let white = serde_json::to_string(&values("White")).map_err(|e| e.to_string())?;
        let black = serde_json::to_string(&values("Black")).map_err(|e| e.to_string())?;
        let event = serde_json::to_string(&values("Event")).map_err(|e| e.to_string())?;
        let date = serde_json::to_string(&values("Date")).map_err(|e| e.to_string())?;
        let result = serde_json::to_string(&values("Result")).map_err(|e| e.to_string())?;
        out.push_str(&format!(
            "GAME\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            game.id,
            game.segment_ordinal,
            game.source_start,
            game.source_end,
            white,
            black,
            event,
            date,
            result
        ));
    }
    Ok(out)
}

fn decoded_values<'a>(game: &'a CatalogGameRecordV1, name: &str) -> Vec<&'a str> {
    game.tags
        .iter()
        .filter(|t| {
            t.decode_status == TagDecodeStatusV1::Decoded
                && t.name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .filter_map(|t| t.value.as_deref())
        .collect()
}
fn any_substring(game: &CatalogGameRecordV1, names: &[&str], needle: &str) -> bool {
    let needle = needle.to_lowercase();
    game.tags.iter().any(|t| {
        t.decode_status == TagDecodeStatusV1::Decoded
            && t.name
                .as_deref()
                .is_some_and(|n| names.iter().any(|wanted| n.eq_ignore_ascii_case(wanted)))
            && t.value
                .as_deref()
                .is_some_and(|v| v.to_lowercase().contains(&needle))
    })
}
fn dates(game: &CatalogGameRecordV1) -> impl Iterator<Item = CivilDate> + '_ {
    decoded_values(game, "Date")
        .into_iter()
        .filter_map(parse_pgn_date)
}
pub(in crate::inspector) fn matches_game(
    game: &CatalogGameRecordV1,
    filters: &QueryFilters,
) -> bool {
    let date_matches = if let Some(wanted) = filters.date {
        dates(game).any(|date| date == wanted)
    } else if filters.date_from.is_some() || filters.date_to.is_some() {
        dates(game).any(|date| {
            filters.date_from.is_none_or(|from| date >= from)
                && filters.date_to.is_none_or(|to| date <= to)
        })
    } else {
        true
    };
    filters
        .player
        .as_deref()
        .is_none_or(|v| any_substring(game, &["White", "Black"], v))
        && filters
            .event
            .as_deref()
            .is_none_or(|v| any_substring(game, &["Event"], v))
        && date_matches
}

pub(in crate::inspector) fn matches_projection(
    white: &[String],
    black: &[String],
    event: &[String],
    dates: &[String],
    filters: &QueryFilters,
) -> bool {
    let contains = |values: &[String], needle: &str| {
        let needle = needle.to_lowercase();
        values
            .iter()
            .any(|value| value.to_lowercase().contains(&needle))
    };
    let date_matches = if let Some(wanted) = filters.date {
        dates
            .iter()
            .filter_map(|value| parse_pgn_date(value))
            .any(|date| date == wanted)
    } else if filters.date_from.is_some() || filters.date_to.is_some() {
        dates
            .iter()
            .filter_map(|value| parse_pgn_date(value))
            .any(|date| {
                filters.date_from.is_none_or(|from| date >= from)
                    && filters.date_to.is_none_or(|to| date <= to)
            })
    } else {
        true
    };
    filters
        .player
        .as_deref()
        .is_none_or(|needle| contains(white, needle) || contains(black, needle))
        && filters
            .event
            .as_deref()
            .is_none_or(|needle| contains(event, needle))
        && date_matches
}
