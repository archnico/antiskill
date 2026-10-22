//! Internal standard-chess rules adapter.
//!
//! Types crossing this boundary are owned by Antiskill. Shakmaty remains an
//! implementation detail and none of its types are exposed by this module.

use shakmaty::{
    fen::Fen,
    san::{SanError, SanPlus},
    uci::UciMove,
    CastlingMode, Chess, Color, EnPassantMode, Move, Position, Role, Square,
};

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

static PROBE_APPLY_ENABLED: AtomicBool = AtomicBool::new(false);
static PROBE_APPLY_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static PROBE_APPLY_PARSE_NS: AtomicU64 = AtomicU64::new(0);
static PROBE_APPLY_RESOLVE_NS: AtomicU64 = AtomicU64::new(0);
static PROBE_APPLY_CANONICAL_NS: AtomicU64 = AtomicU64::new(0);
static PROBE_APPLY_NORMALIZE_NS: AtomicU64 = AtomicU64::new(0);
static PROBE_APPLY_PLAY_NS: AtomicU64 = AtomicU64::new(0);
static PROBE_APPLY_COMPONENTS_NS: AtomicU64 = AtomicU64::new(0);
static PROBE_APPLY_FINALIZE_NS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default)]
pub(in crate::inspector) struct ProbeApplySanProfile {
    pub(in crate::inspector) attempts: u64,
    pub(in crate::inspector) parse: Duration,
    pub(in crate::inspector) resolve: Duration,
    pub(in crate::inspector) canonical: Duration,
    pub(in crate::inspector) normalize: Duration,
    pub(in crate::inspector) play: Duration,
    pub(in crate::inspector) components: Duration,
    pub(in crate::inspector) finalize: Duration,
}

fn reset_counter(counter: &AtomicU64) {
    counter.store(0, Ordering::Relaxed);
}

pub(in crate::inspector) fn probe_apply_profile_start() {
    PROBE_APPLY_ENABLED.store(false, Ordering::SeqCst);
    for counter in [
        &PROBE_APPLY_ATTEMPTS,
        &PROBE_APPLY_PARSE_NS,
        &PROBE_APPLY_RESOLVE_NS,
        &PROBE_APPLY_CANONICAL_NS,
        &PROBE_APPLY_NORMALIZE_NS,
        &PROBE_APPLY_PLAY_NS,
        &PROBE_APPLY_COMPONENTS_NS,
        &PROBE_APPLY_FINALIZE_NS,
    ] {
        reset_counter(counter);
    }
    PROBE_APPLY_ENABLED.store(true, Ordering::SeqCst);
}

fn duration_from_ns(value: u64) -> Duration {
    Duration::from_nanos(value)
}

pub(in crate::inspector) fn probe_apply_profile_finish() -> ProbeApplySanProfile {
    PROBE_APPLY_ENABLED.store(false, Ordering::SeqCst);
    ProbeApplySanProfile {
        attempts: PROBE_APPLY_ATTEMPTS.load(Ordering::Relaxed),
        parse: duration_from_ns(PROBE_APPLY_PARSE_NS.load(Ordering::Relaxed)),
        resolve: duration_from_ns(PROBE_APPLY_RESOLVE_NS.load(Ordering::Relaxed)),
        canonical: duration_from_ns(PROBE_APPLY_CANONICAL_NS.load(Ordering::Relaxed)),
        normalize: duration_from_ns(PROBE_APPLY_NORMALIZE_NS.load(Ordering::Relaxed)),
        play: duration_from_ns(PROBE_APPLY_PLAY_NS.load(Ordering::Relaxed)),
        components: duration_from_ns(PROBE_APPLY_COMPONENTS_NS.load(Ordering::Relaxed)),
        finalize: duration_from_ns(PROBE_APPLY_FINALIZE_NS.load(Ordering::Relaxed)),
    }
}

fn probe_add(counter: &AtomicU64, started: Option<Instant>) {
    if let Some(started) = started {
        let nanos = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        counter.fetch_add(nanos, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SideToMove {
    White,
    Black,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PositionComponents {
    pub(super) piece_placement: String,
    pub(super) side_to_move: SideToMove,
    pub(super) castling: String,
    pub(super) en_passant: Option<String>,
    pub(super) halfmove_clock: u32,
    pub(super) fullmove_number: u32,
}

pub(super) const POSITION_ID_PROFILE: &str = "antiskill_position_standard_v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PositionIdentityV1 {
    pub(super) profile: &'static str,
    pub(super) position_id: String,
    pub(super) position_key: String,
    pub(super) fen_status: FenStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FenStatus {
    Canonical,
    Recovered,
}

#[derive(Clone, Debug)]
pub(super) struct ParsedPosition {
    pub(super) components: PositionComponents,
    pub(super) fen_status: FenStatus,
    position: Chess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FenFailure {
    InvalidEncoding,
    InvalidFen,
    IllegalPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MoveKind {
    Normal,
    CastleKingside,
    CastleQueenside,
    EnPassant,
    Promotion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NormalizedMove {
    pub(super) uci: String,
    pub(super) from: String,
    pub(super) to: String,
    pub(super) promotion: Option<String>,
    pub(super) kind: MoveKind,
    pub(super) canonical_san: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MoveFailure {
    InvalidEncoding,
    InvalidSan,
    AmbiguousSan,
    IllegalMove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AppliedMove {
    pub(super) normalized: NormalizedMove,
    pub(super) resulting_position: PositionComponents,
    pub(super) source_was_canonical_san: bool,
}

pub(super) fn standard_initial_position() -> ParsedPosition {
    parsed_position(Chess::default(), FenStatus::Canonical)
}

pub(super) fn parse_standard_fen(bytes: &[u8]) -> Result<ParsedPosition, FenFailure> {
    if !bytes.is_ascii() {
        return Err(FenFailure::InvalidEncoding);
    }

    let fen = Fen::from_ascii(bytes).map_err(|_| FenFailure::InvalidFen)?;
    let position: Chess = fen
        .into_position(CastlingMode::Standard)
        .map_err(|_| FenFailure::IllegalPosition)?;
    let canonical = Fen::from_position(&position, EnPassantMode::Legal).to_string();
    let status = if is_canonical_fen_form(bytes, &canonical) {
        FenStatus::Canonical
    } else {
        FenStatus::Recovered
    };
    Ok(parsed_position(position, status))
}

pub(super) fn apply_standard_san(
    position: &mut ParsedPosition,
    san_bytes: &[u8],
) -> Result<AppliedMove, MoveFailure> {
    if !san_bytes.is_ascii() {
        return Err(MoveFailure::InvalidEncoding);
    }
    let profiling = PROBE_APPLY_ENABLED.load(Ordering::Relaxed);
    if profiling {
        PROBE_APPLY_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    }

    let started = profiling.then(Instant::now);
    let rules_san = explicit_numeric_castling_alias(san_bytes).unwrap_or(san_bytes);
    let parsed_san = SanPlus::from_ascii(rules_san).map_err(|_| MoveFailure::InvalidSan);
    probe_add(&PROBE_APPLY_PARSE_NS, started);
    let source_san = parsed_san?;

    let started = profiling.then(Instant::now);
    let resolved = source_san
        .san
        .to_move(&position.position)
        .map_err(|error| match error {
            SanError::AmbiguousSan => MoveFailure::AmbiguousSan,
            SanError::IllegalSan => MoveFailure::IllegalMove,
        });
    probe_add(&PROBE_APPLY_RESOLVE_NS, started);
    let played = resolved?;

    let started = profiling.then(Instant::now);
    let canonical_san = SanPlus::from_move(position.position.clone(), played).to_string();
    probe_add(&PROBE_APPLY_CANONICAL_NS, started);

    let started = profiling.then(Instant::now);
    let normalized = normalize_move(&position.position, played, canonical_san);
    probe_add(&PROBE_APPLY_NORMALIZE_NS, started);

    let started = profiling.then(Instant::now);
    let mut resulting_rules_position = position.position.clone();
    resulting_rules_position.play_unchecked(played);
    probe_add(&PROBE_APPLY_PLAY_NS, started);

    let started = profiling.then(Instant::now);
    let resulting_position = position_components(&resulting_rules_position);
    probe_add(&PROBE_APPLY_COMPONENTS_NS, started);

    let started = profiling.then(Instant::now);
    let applied = AppliedMove {
        source_was_canonical_san: san_bytes == normalized.canonical_san.as_bytes(),
        normalized,
        resulting_position: resulting_position.clone(),
    };
    position.position = resulting_rules_position;
    position.components = resulting_position;
    position.fen_status = FenStatus::Canonical;
    probe_add(&PROBE_APPLY_FINALIZE_NS, started);
    Ok(applied)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct IdentityUciMove {
    bytes: [u8; 5],
    len: u8,
}

impl IdentityUciMove {
    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

fn identity_uci_move(played: Move) -> IdentityUciMove {
    match UciMove::from_move(played, CastlingMode::Standard) {
        UciMove::Normal {
            from,
            to,
            promotion,
        } => {
            let mut bytes = [0_u8; 5];
            bytes[0] = from.file().char() as u8;
            bytes[1] = from.rank().char() as u8;
            bytes[2] = to.file().char() as u8;
            bytes[3] = to.rank().char() as u8;
            let len = if let Some(role) = promotion {
                bytes[4] = role.char() as u8;
                5
            } else {
                4
            };
            IdentityUciMove { bytes, len }
        }
        UciMove::Put { .. } | UciMove::Null => {
            unreachable!("standard Chess SAN cannot produce drops or null moves")
        }
    }
}

pub(super) fn apply_standard_san_identity_compact(
    position: &mut ParsedPosition,
    san_bytes: &[u8],
) -> Result<IdentityUciMove, MoveFailure> {
    if !san_bytes.is_ascii() {
        return Err(MoveFailure::InvalidEncoding);
    }
    let rules_san = explicit_numeric_castling_alias(san_bytes).unwrap_or(san_bytes);
    let source_san = SanPlus::from_ascii(rules_san).map_err(|_| MoveFailure::InvalidSan)?;
    let played = source_san
        .san
        .to_move(&position.position)
        .map_err(|error| match error {
            SanError::AmbiguousSan => MoveFailure::AmbiguousSan,
            SanError::IllegalSan => MoveFailure::IllegalMove,
        })?;
    let uci = identity_uci_move(played);
    position.position.play_unchecked(played);
    Ok(uci)
}

pub(super) fn apply_standard_san_identity(
    position: &mut ParsedPosition,
    san_bytes: &[u8],
) -> Result<String, MoveFailure> {
    if !san_bytes.is_ascii() {
        return Err(MoveFailure::InvalidEncoding);
    }
    let rules_san = explicit_numeric_castling_alias(san_bytes).unwrap_or(san_bytes);
    let source_san = SanPlus::from_ascii(rules_san).map_err(|_| MoveFailure::InvalidSan)?;
    let played = source_san
        .san
        .to_move(&position.position)
        .map_err(|error| match error {
            SanError::AmbiguousSan => MoveFailure::AmbiguousSan,
            SanError::IllegalSan => MoveFailure::IllegalMove,
        })?;
    let uci = UciMove::from_move(played, CastlingMode::Standard).to_string();
    position.position.play_unchecked(played);
    Ok(uci)
}

fn explicit_numeric_castling_alias(san_bytes: &[u8]) -> Option<&'static [u8]> {
    match san_bytes {
        b"0-0" => Some(b"O-O"),
        b"0-0+" => Some(b"O-O+"),
        b"0-0#" => Some(b"O-O#"),
        b"0-0-0" => Some(b"O-O-O"),
        b"0-0-0+" => Some(b"O-O-O+"),
        b"0-0-0#" => Some(b"O-O-O#"),
        _ => None,
    }
}

pub(super) fn position_identity_from_fen(bytes: &[u8]) -> Result<PositionIdentityV1, FenFailure> {
    let parsed = parse_standard_fen(bytes)?;
    Ok(position_identity_from_components(
        &parsed.components,
        parsed.fen_status,
    ))
}

fn position_identity_from_components(
    components: &PositionComponents,
    fen_status: FenStatus,
) -> PositionIdentityV1 {
    let side = match components.side_to_move {
        SideToMove::White => "w",
        SideToMove::Black => "b",
    };
    let castling = if components.castling.is_empty() {
        "-"
    } else {
        components.castling.as_str()
    };
    let en_passant = components.en_passant.as_deref().unwrap_or("-");
    let position_key = format!(
        "{} {side} {castling} {en_passant}",
        components.piece_placement
    );
    let mut hasher = blake3::Hasher::new();
    hasher.update(POSITION_ID_PROFILE.as_bytes());
    hasher.update(b"\0standard\0");
    hasher.update(position_key.as_bytes());
    PositionIdentityV1 {
        profile: POSITION_ID_PROFILE,
        position_id: hasher.finalize().to_hex().to_string(),
        position_key,
        fen_status,
    }
}

fn parsed_position(position: Chess, fen_status: FenStatus) -> ParsedPosition {
    ParsedPosition {
        components: position_components(&position),
        fen_status,
        position,
    }
}

fn is_canonical_fen_form(bytes: &[u8], canonical: &str) -> bool {
    let fields = bytes.split(|byte| *byte == b' ').collect::<Vec<_>>();
    fields.len() == 6
        && fields.iter().all(|field| !field.is_empty())
        && fields[5].iter().all(|byte| byte.is_ascii_digit())
        && fields[5].iter().fold(0_u64, |value, byte| {
            value
                .saturating_mul(10)
                .saturating_add(u64::from(byte - b'0'))
        }) >= 1
        && bytes == canonical.as_bytes()
}

fn position_components(position: &Chess) -> PositionComponents {
    if std::env::var_os("ANTISKILL_FAST_COMPONENTS").is_some() {
        let direct = position_components_direct(position);
        if std::env::var_os("ANTISKILL_VERIFY_FAST_COMPONENTS").is_some() {
            let legacy = position_components_via_fen(position);
            assert_eq!(
                direct, legacy,
                "fast PositionComponents diverged from FEN oracle"
            );
        }
        direct
    } else {
        position_components_via_fen(position)
    }
}

fn position_components_direct(position: &Chess) -> PositionComponents {
    let rights = position.castles().castling_rights();
    let mut castling = String::with_capacity(4);
    for (square, symbol) in [
        (Square::H1, 'K'),
        (Square::A1, 'Q'),
        (Square::H8, 'k'),
        (Square::A8, 'q'),
    ] {
        if rights.contains(square) {
            castling.push(symbol);
        }
    }
    PositionComponents {
        piece_placement: position.board().board_fen().to_string(),
        side_to_move: match position.turn() {
            Color::White => SideToMove::White,
            Color::Black => SideToMove::Black,
        },
        castling,
        en_passant: position
            .ep_square(EnPassantMode::Legal)
            .map(|square| square.to_string()),
        halfmove_clock: position.halfmoves(),
        fullmove_number: position.fullmoves().get(),
    }
}

fn position_components_via_fen(position: &Chess) -> PositionComponents {
    let canonical = Fen::from_position(position, EnPassantMode::Legal).to_string();
    let mut fields = canonical.split(' ');
    let piece_placement = fields.next().expect("canonical FEN has board").to_owned();
    let side_to_move = match fields.next().expect("canonical FEN has turn") {
        "w" => SideToMove::White,
        "b" => SideToMove::Black,
        _ => unreachable!("Shakmaty emitted an unknown side"),
    };
    let castling_field = fields.next().expect("canonical FEN has castling");
    let en_passant_field = fields.next().expect("canonical FEN has en passant");
    let halfmove_clock = fields
        .next()
        .expect("canonical FEN has halfmove clock")
        .parse()
        .expect("Shakmaty emitted a numeric halfmove clock");
    let fullmove_number = fields
        .next()
        .expect("canonical FEN has fullmove number")
        .parse()
        .expect("Shakmaty emitted a numeric fullmove number");
    debug_assert!(fields.next().is_none());
    PositionComponents {
        piece_placement,
        side_to_move,
        castling: if castling_field == "-" {
            String::new()
        } else {
            castling_field.to_owned()
        },
        en_passant: (en_passant_field != "-").then(|| en_passant_field.to_owned()),
        halfmove_clock,
        fullmove_number,
    }
}

fn normalize_move(position: &Chess, played: Move, canonical_san: String) -> NormalizedMove {
    let uci = UciMove::from_move(played, CastlingMode::Standard);
    let (from, to, promotion) = match uci {
        UciMove::Normal {
            from,
            to,
            promotion,
        } => (from.to_string(), to.to_string(), promotion.map(role_name)),
        UciMove::Put { .. } | UciMove::Null => {
            unreachable!("standard Chess SAN cannot produce drops or null moves")
        }
    };
    let kind = match played {
        Move::Castle { king, rook } if king.file() < rook.file() => MoveKind::CastleKingside,
        Move::Castle { .. } => MoveKind::CastleQueenside,
        Move::EnPassant { .. } => MoveKind::EnPassant,
        Move::Normal {
            promotion: Some(_), ..
        } => MoveKind::Promotion,
        Move::Normal { .. } => MoveKind::Normal,
        Move::Put { .. } => unreachable!("standard Chess has no drops"),
    };
    debug_assert_eq!(position.castles().mode(), CastlingMode::Standard);
    NormalizedMove {
        uci: uci.to_string(),
        from,
        to,
        promotion,
        kind,
        canonical_san,
    }
}

fn role_name(role: Role) -> String {
    role.char().to_ascii_lowercase().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INITIAL_FEN: &[u8] = b"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    fn parsed(fen: &[u8]) -> ParsedPosition {
        parse_standard_fen(fen).expect("legal test FEN")
    }

    #[test]
    fn initial_position_id_is_a_stable_known_vector() {
        let identity =
            position_identity_from_fen(b"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
                .unwrap();
        assert_eq!(
            identity.position_id,
            "8a6adc669f50706498570870a2febd962b5cc2111ddaaaea73bbdfbbd96ea2b4"
        );
    }

    #[test]
    fn position_id_ignores_halfmove_and_fullmove_counters() {
        let a = position_identity_from_fen(b"7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        let b = position_identity_from_fen(b"7k/8/8/8/8/8/8/K7 w - - 99 42").unwrap();
        assert_eq!(a.position_key, b.position_key);
        assert_eq!(a.position_id, b.position_id);
    }

    #[test]
    fn position_id_normalizes_non_capturable_en_passant_target() {
        let with_textual_ep = position_identity_from_fen(
            b"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        )
        .unwrap();
        let without_ep = position_identity_from_fen(
            b"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        )
        .unwrap();
        assert_eq!(with_textual_ep.position_key, without_ep.position_key);
        assert_eq!(with_textual_ep.position_id, without_ep.position_id);
        assert!(with_textual_ep.position_key.ends_with(" -"));
    }

    #[test]
    fn position_id_preserves_legally_available_en_passant() {
        let with_ep = position_identity_from_fen(
            b"rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3",
        )
        .unwrap();
        let without_ep = position_identity_from_fen(
            b"rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq - 0 3",
        )
        .unwrap();
        assert_ne!(with_ep.position_key, without_ep.position_key);
        assert_ne!(with_ep.position_id, without_ep.position_id);
        assert!(with_ep.position_key.ends_with(" d6"));
    }

    #[test]
    fn position_id_distinguishes_castling_rights() {
        let with_rights =
            position_identity_from_fen(b"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
        let without_rights =
            position_identity_from_fen(b"r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1").unwrap();
        assert_ne!(with_rights.position_id, without_rights.position_id);
    }

    #[test]
    fn recovered_and_canonical_fen_share_position_id() {
        let canonical = position_identity_from_fen(b"7k/P7/8/8/8/8/8/7K w - - 0 1").unwrap();
        let recovered = position_identity_from_fen(b"7k/P7/8/8/8/8/8/7K w").unwrap();
        assert_eq!(canonical.position_key, recovered.position_key);
        assert_eq!(canonical.position_id, recovered.position_id);
        assert_eq!(canonical.fen_status, FenStatus::Canonical);
        assert_eq!(recovered.fen_status, FenStatus::Recovered);
    }

    fn apply(position: &mut ParsedPosition, san: &[u8]) -> AppliedMove {
        apply_standard_san(position, san).expect("legal test SAN")
    }

    #[test]
    fn standard_initial_position_has_canonical_components() {
        let position = standard_initial_position();
        assert_eq!(position.fen_status, FenStatus::Canonical);
        assert_eq!(position.components, parsed(INITIAL_FEN).components);
    }

    #[test]
    fn canonical_initial_fen_is_classified_canonical() {
        assert_eq!(parsed(INITIAL_FEN).fen_status, FenStatus::Canonical);
    }

    #[test]
    fn fen_with_missing_fields_is_recovered_by_shakmaty() {
        let position = parsed(b"8/8/8/8/8/8/K7/7k w");
        assert_eq!(position.fen_status, FenStatus::Recovered);
        assert_eq!(position.components.halfmove_clock, 0);
        assert_eq!(position.components.fullmove_number, 1);
    }

    #[test]
    fn fullmove_zero_is_recovered_as_one() {
        let position = parsed(b"8/8/8/8/8/8/K7/7k w - - 0 0");
        assert_eq!(position.fen_status, FenStatus::Recovered);
        assert_eq!(position.components.fullmove_number, 1);
    }

    #[test]
    fn syntactically_invalid_fen_is_rejected() {
        assert_eq!(
            parse_standard_fen(b"not a fen").unwrap_err(),
            FenFailure::InvalidFen
        );
    }

    #[test]
    fn illegal_fen_position_is_rejected() {
        assert_eq!(
            parse_standard_fen(b"8/8/8/8/8/8/8/8 w - - 0 1").unwrap_err(),
            FenFailure::IllegalPosition
        );
    }

    #[test]
    fn non_ascii_fen_is_rejected_as_invalid_encoding() {
        assert_eq!(
            parse_standard_fen(b"8/8/8/8/8/8/K7/7k w - - 0 1\xff").unwrap_err(),
            FenFailure::InvalidEncoding
        );
    }

    #[test]
    fn e4_is_applied_from_the_initial_position() {
        let mut position = standard_initial_position();
        let applied = apply(&mut position, b"e4");
        assert_eq!(applied.normalized.uci, "e2e4");
        assert_eq!(applied.normalized.canonical_san, "e4");
    }

    #[test]
    fn legal_sequence_reaches_nf3() {
        let mut position = standard_initial_position();
        apply(&mut position, b"e4");
        apply(&mut position, b"e5");
        let nf3 = apply(&mut position, b"Nf3");
        assert_eq!(nf3.normalized.uci, "g1f3");
    }

    #[test]
    fn capture_is_normalized() {
        let mut position = standard_initial_position();
        apply(&mut position, b"e4");
        apply(&mut position, b"d5");
        let capture = apply(&mut position, b"exd5");
        assert_eq!(capture.normalized.uci, "e4d5");
        assert_eq!(capture.normalized.kind, MoveKind::Normal);
    }

    #[test]
    fn check_suffix_is_canonicalized() {
        let mut position = parsed(b"7k/8/8/8/8/8/8/R6K w - - 0 1");
        let check = apply(&mut position, b"Ra8+");
        assert_eq!(check.normalized.canonical_san, "Ra8+");
    }

    #[test]
    fn checkmate_suffix_is_canonicalized() {
        let mut position = parsed(b"7k/5Q2/6K1/8/8/8/8/8 w - - 0 1");
        let mate = apply(&mut position, b"Qg7#");
        assert_eq!(mate.normalized.canonical_san, "Qg7#");
    }

    #[test]
    fn kingside_castling_is_recognized() {
        let mut position = parsed(b"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        let castle = apply(&mut position, b"O-O");
        assert_eq!(castle.normalized.kind, MoveKind::CastleKingside);
        assert_eq!(castle.normalized.uci, "e1g1");
    }

    #[test]
    fn numeric_kingside_castling_alias_is_applied_but_not_canonical() {
        let mut position = parsed(b"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        let castle = apply(&mut position, b"0-0");
        assert_eq!(castle.normalized.kind, MoveKind::CastleKingside);
        assert_eq!(castle.normalized.uci, "e1g1");
        assert_eq!(castle.normalized.canonical_san, "O-O");
        assert!(!castle.source_was_canonical_san);
    }

    #[test]
    fn canonical_kingside_castling_remains_canonical() {
        let mut position = parsed(b"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        let castle = apply(&mut position, b"O-O");
        assert!(castle.source_was_canonical_san);
    }

    #[test]
    fn numeric_castling_alias_is_illegal_transactionally_from_initial_position() {
        let mut position = standard_initial_position();
        let before = position.components.clone();
        assert_eq!(
            apply_standard_san(&mut position, b"0-0"),
            Err(MoveFailure::IllegalMove)
        );
        assert_eq!(position.components, before);
    }

    #[test]
    fn numeric_castling_alias_with_check_suffix_is_applied() {
        let mut position = parsed(b"5k2/8/8/8/8/8/8/4K2R w K - 0 1");
        let castle = apply(&mut position, b"0-0+");
        assert_eq!(castle.normalized.kind, MoveKind::CastleKingside);
        assert_eq!(castle.normalized.canonical_san, "O-O+");
        assert!(!castle.source_was_canonical_san);
    }

    #[test]
    fn non_enumerated_numeric_castling_spellings_remain_invalid_san() {
        for san in [b"00-0".as_slice(), b"0--0", b"0-0x"] {
            let mut position = parsed(b"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
            assert_eq!(
                apply_standard_san(&mut position, san),
                Err(MoveFailure::InvalidSan),
                "unexpectedly accepted {}",
                String::from_utf8_lossy(san)
            );
        }
    }

    #[test]
    fn illegal_castling_is_rejected() {
        let mut position = standard_initial_position();
        assert_eq!(
            apply_standard_san(&mut position, b"O-O"),
            Err(MoveFailure::IllegalMove)
        );
    }

    #[test]
    fn promotion_is_recognized() {
        let mut position = parsed(b"7k/P7/8/8/8/8/8/7K w - - 0 1");
        let promotion = apply(&mut position, b"a8=Q+");
        assert_eq!(promotion.normalized.kind, MoveKind::Promotion);
        assert_eq!(promotion.normalized.promotion.as_deref(), Some("q"));
    }

    #[test]
    fn en_passant_is_recognized() {
        let mut position = parsed(b"7k/8/8/3pP3/8/8/8/7K w - d6 0 2");
        let capture = apply(&mut position, b"exd6");
        assert_eq!(capture.normalized.kind, MoveKind::EnPassant);
        assert_eq!(capture.normalized.uci, "e5d6");
    }

    #[test]
    fn en_passant_component_requires_a_legal_capture() {
        let unavailable = parsed(b"7k/8/8/8/4P3/8/8/7K b - e3 0 1");
        assert_eq!(unavailable.components.en_passant, None);
        assert_eq!(unavailable.fen_status, FenStatus::Recovered);
        let available = parsed(b"7k/8/8/3pP3/8/8/8/7K w - d6 0 2");
        assert_eq!(available.components.en_passant.as_deref(), Some("d6"));
    }

    #[test]
    fn non_ascii_san_is_rejected_as_invalid_encoding() {
        let mut position = standard_initial_position();
        assert_eq!(
            apply_standard_san(&mut position, b"e4\xff"),
            Err(MoveFailure::InvalidEncoding)
        );
    }

    #[test]
    fn syntactically_invalid_san_is_rejected() {
        let mut position = standard_initial_position();
        assert_eq!(
            apply_standard_san(&mut position, b"not-a-move"),
            Err(MoveFailure::InvalidSan)
        );
    }

    #[test]
    fn ambiguous_san_uses_two_legal_same_kind_pieces() {
        let mut position = parsed(b"4k3/8/8/8/8/2N1N3/8/4K3 w - - 0 1");
        assert_eq!(
            apply_standard_san(&mut position, b"Nd5"),
            Err(MoveFailure::AmbiguousSan)
        );
    }

    #[test]
    fn syntactically_valid_but_illegal_san_is_rejected() {
        let mut position = standard_initial_position();
        assert_eq!(
            apply_standard_san(&mut position, b"e5"),
            Err(MoveFailure::IllegalMove)
        );
    }

    #[test]
    fn incorrect_check_suffix_is_reported_as_noncanonical() {
        let mut position = standard_initial_position();
        let applied = apply(&mut position, b"e4+");
        assert_eq!(applied.normalized.canonical_san, "e4");
        assert!(!applied.source_was_canonical_san);
    }

    #[test]
    fn normalized_move_exposes_from_to_promotion_and_kind() {
        let mut position = parsed(b"7k/P7/8/8/8/8/8/7K w - - 0 1");
        let promotion = apply(&mut position, b"a8=Q+");
        assert_eq!(promotion.normalized.from, "a7");
        assert_eq!(promotion.normalized.to, "a8");
        assert_eq!(promotion.normalized.promotion.as_deref(), Some("q"));
        assert_eq!(promotion.normalized.kind, MoveKind::Promotion);
    }

    #[test]
    fn resulting_position_components_are_exact() {
        let mut position = standard_initial_position();
        let applied = apply(&mut position, b"e4");
        assert_eq!(
            applied.resulting_position,
            PositionComponents {
                piece_placement: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR".to_owned(),
                side_to_move: SideToMove::Black,
                castling: "KQkq".to_owned(),
                en_passant: None,
                halfmove_clock: 0,
                fullmove_number: 1,
            }
        );
    }

    #[test]
    fn failed_move_is_transactional() {
        let mut position = standard_initial_position();
        let before = position.components.clone();
        assert_eq!(
            apply_standard_san(&mut position, b"e5"),
            Err(MoveFailure::IllegalMove)
        );
        assert_eq!(position.components, before);
        assert_eq!(apply(&mut position, b"e4").normalized.uci, "e2e4");
    }

    #[test]
    fn identical_runs_are_deterministic() {
        let run = || {
            let mut position = standard_initial_position();
            apply(&mut position, b"e4")
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn numeric_queenside_castling_alias_is_applied_but_not_canonical() {
        let mut position = parsed(b"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        let castle = apply(&mut position, b"0-0-0");
        assert_eq!(castle.normalized.kind, MoveKind::CastleQueenside);
        assert_eq!(castle.normalized.uci, "e1c1");
        assert_eq!(castle.normalized.canonical_san, "O-O-O");
        assert!(!castle.source_was_canonical_san);
    }
}
