use std::fmt;
use std::str::FromStr;

const PREFIX: &str = "g1-";
const OCCURRENCE_ID_LEN: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PublicGameIdV1(String);

impl PublicGameIdV1 {
    pub(crate) fn from_occurrence_id(occurrence_id: &str) -> Result<Self, String> {
        if !valid_payload(occurrence_id) {
            return Err("ID partita non valido".into());
        }
        Ok(Self(occurrence_id.to_owned()))
    }

    pub(crate) fn occurrence_id(&self) -> &str {
        &self.0
    }
}

impl FromStr for PublicGameIdV1 {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let payload = value
            .strip_prefix(PREFIX)
            .ok_or_else(|| "ID partita non valido".to_owned())?;
        Self::from_occurrence_id(payload)
    }
}

impl fmt::Display for PublicGameIdV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{PREFIX}{}", self.0)
    }
}

fn valid_payload(value: &str) -> bool {
    value.len() == OCCURRENCE_ID_LEN
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_game_id_v1_parse_and_format_round_trip() {
        let text = format!("g1-{}", "0123456789abcdef".repeat(4));
        let parsed: PublicGameIdV1 = text.parse().unwrap();
        assert_eq!(parsed.to_string(), text);
        assert_eq!(parsed.occurrence_id(), "0123456789abcdef".repeat(4));
        assert_eq!(
            PublicGameIdV1::from_occurrence_id(parsed.occurrence_id()).unwrap(),
            parsed
        );
    }

    #[test]
    fn public_game_id_v1_rejects_every_noncanonical_shape() {
        let payload = "a".repeat(64);
        for invalid in [
            format!("x1-{payload}"),
            format!("g1-{}", "a".repeat(63)),
            format!("g1-{}", "a".repeat(65)),
            format!("g1-{}A", "a".repeat(63)),
            format!("g1-{}g", "a".repeat(63)),
            payload,
        ] {
            assert!(invalid.parse::<PublicGameIdV1>().is_err(), "{invalid}");
        }
    }
}
