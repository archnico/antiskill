use super::model::*;
use crate::inspector::chess_rules::{
    apply_standard_san, apply_standard_san_identity, apply_standard_san_identity_compact,
    parse_standard_fen, standard_initial_position, FenStatus, IdentityUciMove, MoveFailure,
    ParsedPosition, PositionComponents, SideToMove,
};
use crate::inspector::parsing::model::{FlatNode, NodeKind, NodeStatus, ParseTree};
use crate::inspector::tokenization::{Token, TokenKind, TokenStream};

#[derive(Clone, Copy, Debug)]
pub(super) struct NodeBytes<'a> {
    bytes: &'a [u8],
    start: u64,
    end: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NodeRangeError {
    MissingTokenStream,
    InvalidTokenRange,
    InvalidByteRange,
}

struct TreeView<'a> {
    tree: &'a ParseTree,
    children: Vec<Vec<usize>>,
}

impl<'a> TreeView<'a> {
    fn new(tree: &'a ParseTree) -> Self {
        let mut children = vec![Vec::new(); tree.nodes.len()];
        for (index, node) in tree.nodes.iter().enumerate().skip(1) {
            if let Some(parent) = node.parent_id.and_then(|id| usize::try_from(id).ok()) {
                if let Some(direct) = children.get_mut(parent) {
                    direct.push(index);
                }
            }
        }
        Self { tree, children }
    }

    fn node(&self, id: u64) -> Option<&'a FlatNode> {
        usize::try_from(id)
            .ok()
            .and_then(|index| self.tree.nodes.get(index))
    }

    fn direct_children(&self, parent: u64) -> impl Iterator<Item = &'a FlatNode> + '_ {
        usize::try_from(parent)
            .ok()
            .and_then(|index| self.children.get(index))
            .into_iter()
            .flatten()
            .filter_map(|index| self.tree.nodes.get(*index))
    }
}

#[cfg(test)]
pub(super) fn interpret_line_trees(
    trees: &[ParseTree],
    streams: &[TokenStream],
    blob: &[u8],
) -> Vec<InterpretedGame> {
    trees
        .iter()
        .enumerate()
        .map(|(index, tree)| {
            let stream = streams
                .get(index)
                .filter(|stream| stream.segment_ordinal == tree.segment_ordinal);
            interpret_game(index as u64, tree, stream, 0, blob)
        })
        .collect()
}

pub(super) fn interpret_line_tree(
    id: u64,
    tree: &ParseTree,
    stream: Option<&TokenStream>,
    source_start: u64,
    blob: &[u8],
) -> InterpretedGame {
    interpret_game(id, tree, stream, source_start, blob)
}

#[derive(Debug)]
pub(super) struct LeanMainlineIdentity {
    pub(super) comparable: bool,
    pub(super) moves: u64,
    pub(super) results: u64,
    pub(super) identity: Option<String>,
}

fn components_fen(components: &PositionComponents) -> String {
    let side = match components.side_to_move {
        SideToMove::White => "w",
        SideToMove::Black => "b",
    };
    let castling = if components.castling.is_empty() {
        "-"
    } else {
        &components.castling
    };
    let en_passant = components.en_passant.as_deref().unwrap_or("-");
    format!(
        "{} {side} {castling} {en_passant} {} {}",
        components.piece_placement, components.halfmove_clock, components.fullmove_number
    )
}

fn canonical_identity_from_uci(initial_fen: &str, moves: &[String]) -> String {
    let mut preimage = b"antiskill_canonical_mainline_v1\0standard\0".to_vec();
    preimage.extend_from_slice(initial_fen.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(moves.len().to_string().as_bytes());
    preimage.push(0);
    for uci in moves {
        preimage.extend_from_slice(uci.as_bytes());
        preimage.push(0);
    }
    preimage.push(b'\n');
    blake3::hash(&preimage).to_hex().to_string()
}

fn canonical_identity_from_compact_uci(initial_fen: &str, moves: &[IdentityUciMove]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"antiskill_canonical_mainline_v1\0standard\0");
    hasher.update(initial_fen.as_bytes());
    hasher.update(&[0]);
    let count = moves.len().to_string();
    hasher.update(count.as_bytes());
    hasher.update(&[0]);
    for uci in moves {
        hasher.update(uci.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(b"\n");
    hasher.finalize().to_hex().to_string()
}

pub(super) fn lean_mainline_identity(
    id: u64,
    tree: &ParseTree,
    stream: &TokenStream,
    blob: &[u8],
) -> LeanMainlineIdentity {
    let mut game = empty_game(id, tree.segment_ordinal);
    let view = TreeView::new(tree);
    let Some(root) = view
        .node(0)
        .filter(|node| node.kind == NodeKind::GameCandidate)
    else {
        return LeanMainlineIdentity {
            comparable: false,
            moves: 0,
            results: 0,
            identity: None,
        };
    };
    let direct_root = view.direct_children(root.id).collect::<Vec<_>>();
    let tag_sections = direct_root
        .iter()
        .filter(|node| node.kind == NodeKind::TagSection)
        .copied()
        .collect::<Vec<_>>();
    let movetexts = direct_root
        .iter()
        .filter(|node| node.kind == NodeKind::Movetext)
        .copied()
        .collect::<Vec<_>>();
    if tag_sections.len() > 1 || movetexts.len() > 1 {
        return LeanMainlineIdentity {
            comparable: false,
            moves: 0,
            results: 0,
            identity: None,
        };
    }
    if let Some(section) = tag_sections.first() {
        collect_relevant_tags(&mut game, &view, Some(stream), stream.start, blob, section);
    }
    resolve_variant(&mut game);
    let mut rules_position = resolve_initial_position(&mut game);
    let initial_fen = rules_position
        .as_ref()
        .map(|position| components_fen(&position.components));
    let mut comparable = game.variant == VariantKind::Standard
        && game.variant_status == VariantStatus::Supported
        && !matches!(
            game.initial_position_status,
            InitialPositionStatus::Invalid
                | InitialPositionStatus::Ambiguous
                | InitialPositionStatus::UnsupportedVariant
        )
        && rules_position.is_some();
    let Some(movetext) = movetexts.first() else {
        return LeanMainlineIdentity {
            comparable: false,
            moves: 0,
            results: 0,
            identity: None,
        };
    };

    let mut uci_moves = Vec::new();
    let mut moves = 0_u64;
    let mut results = 0_u64;
    let mut blocked = false;
    let mut after_result = false;
    for node in view.direct_children(movetext.id) {
        match node.kind {
            NodeKind::Trivia
            | NodeKind::EscapeLine
            | NodeKind::Comment
            | NodeKind::Nag
            | NodeKind::MoveNumber
            | NodeKind::Variation => {}
            NodeKind::Result => {
                results += 1;
                after_result = true;
            }
            NodeKind::MoveSymbolCandidate => {
                moves += 1;
                if after_result || blocked || !comparable {
                    comparable = false;
                    blocked = true;
                    continue;
                }
                let bytes = match node_bytes_at(node, Some(stream), stream.start, blob) {
                    Ok(range) if range.bytes.is_ascii() => range.bytes,
                    _ => {
                        comparable = false;
                        blocked = true;
                        continue;
                    }
                };
                let Some(position) = rules_position.as_mut() else {
                    comparable = false;
                    blocked = true;
                    continue;
                };
                match apply_standard_san_identity(position, bytes) {
                    Ok(uci) => uci_moves.push(uci),
                    Err(_) => {
                        comparable = false;
                        blocked = true;
                    }
                }
            }
            NodeKind::Unparsed
            | NodeKind::TagPair
            | NodeKind::TagSection
            | NodeKind::Movetext
            | NodeKind::GameCandidate
            | NodeKind::Delimiter
            | NodeKind::TagNameCandidate
            | NodeKind::TagValueCandidate => {
                comparable = false;
            }
        }
    }
    if moves == 0 || uci_moves.len() as u64 != moves {
        comparable = false;
    }
    let identity = if comparable {
        initial_fen
            .as_deref()
            .map(|fen| canonical_identity_from_uci(fen, &uci_moves))
    } else {
        None
    };
    LeanMainlineIdentity {
        comparable,
        moves,
        results,
        identity,
    }
}

fn direct_token_bytes<'a>(token: &Token, stream: &TokenStream, blob: &'a [u8]) -> Option<&'a [u8]> {
    let start = usize::try_from(token.start.checked_sub(stream.start)?).ok()?;
    let end = usize::try_from(token.end.checked_sub(stream.start)?).ok()?;
    blob.get(start..end)
}

fn direct_simple_tag_pair(
    tokens: &[Token],
    stream: &TokenStream,
    blob: &[u8],
    start: usize,
) -> Option<(usize, u64, bool)> {
    if tokens.get(start)?.kind != TokenKind::LeftBracket {
        return None;
    }
    let mut cursor = start + 1;
    let mut nodes = 1_u64; // left delimiter
    while tokens
        .get(cursor)
        .is_some_and(|t| t.kind == TokenKind::Whitespace)
    {
        nodes += 1;
        cursor += 1;
    }
    let name = tokens.get(cursor)?;
    if name.kind != TokenKind::Symbol {
        return None;
    }
    nodes += 1;
    cursor += 1;
    let whitespace_start = cursor;
    while tokens
        .get(cursor)
        .is_some_and(|t| t.kind == TokenKind::Whitespace)
    {
        cursor += 1;
    }
    if cursor == whitespace_start {
        return None;
    }
    nodes += 1; // parser collapses this whitespace run into one Trivia node
    if tokens.get(cursor)?.kind != TokenKind::QuotedString {
        return None;
    }
    nodes += 1;
    cursor += 1;
    while tokens
        .get(cursor)
        .is_some_and(|t| t.kind == TokenKind::Whitespace)
    {
        nodes += 1;
        cursor += 1;
    }
    if tokens.get(cursor)?.kind != TokenKind::RightBracket {
        return None;
    }
    nodes += 1;
    cursor += 1;
    let name = direct_token_bytes(name, stream, blob)?;
    let relevant = matches!(name, b"Variant" | b"SetUp" | b"FEN");
    Some((cursor, nodes + 1, relevant)) // + TagPair container
}

pub(super) fn lean_mainline_identity_tokens_fast(
    stream: &TokenStream,
    blob: &[u8],
) -> Option<(LeanMainlineIdentity, u64, u64)> {
    if u64::try_from(blob.len()).ok()? != stream.end.checked_sub(stream.start)? {
        return None;
    }
    let tokens = &stream.tokens;
    let (mut cursor, first_nodes, relevant) = direct_simple_tag_pair(tokens, stream, blob, 0)?;
    if relevant {
        return None;
    }
    let mut tag_nodes = first_nodes;
    loop {
        let whitespace_start = cursor;
        while tokens
            .get(cursor)
            .is_some_and(|t| t.kind == TokenKind::Whitespace)
        {
            cursor += 1;
        }
        if tokens
            .get(cursor)
            .is_some_and(|t| t.kind == TokenKind::LeftBracket)
        {
            let (next, pair_nodes, relevant) =
                direct_simple_tag_pair(tokens, stream, blob, cursor)?;
            if relevant {
                return None;
            }
            if whitespace_start < cursor {
                tag_nodes += 1; // inter-pair Trivia node
            }
            tag_nodes += pair_nodes;
            cursor = next;
        } else {
            cursor = whitespace_start; // trailing whitespace belongs to movetext
            break;
        }
    }

    let mut nodes = 1_u64 + 1 + tag_nodes; // GameCandidate + TagSection
    if cursor < tokens.len() {
        nodes = nodes.checked_add(1 + u64::try_from(tokens.len() - cursor).ok()?)?;
    }

    let mut position = standard_initial_position();
    let initial_fen = components_fen(&position.components);
    let mut uci_moves: Vec<IdentityUciMove> = Vec::new();
    let mut moves = 0_u64;
    let mut results = 0_u64;
    let mut after_result = false;
    for token in &tokens[cursor..] {
        match token.kind {
            TokenKind::Whitespace
            | TokenKind::EscapeLine
            | TokenKind::BraceComment
            | TokenKind::SemicolonComment
            | TokenKind::MoveNumberCandidate
            | TokenKind::NagNumeric
            | TokenKind::NagSymbolic => {}
            TokenKind::ResultCandidate => {
                results = results.checked_add(1)?;
                after_result = true;
            }
            TokenKind::Symbol => {
                moves = moves.checked_add(1)?;
                if after_result {
                    return None;
                }
                let san = direct_token_bytes(token, stream, blob)?;
                let uci = apply_standard_san_identity_compact(&mut position, san).ok()?;
                uci_moves.push(uci);
            }
            TokenKind::LeftParenthesis
            | TokenKind::RightParenthesis
            | TokenKind::LeftBracket
            | TokenKind::RightBracket
            | TokenKind::QuotedString
            | TokenKind::UnterminatedQuotedString
            | TokenKind::UnterminatedBraceComment
            | TokenKind::Unknown => return None,
        }
    }
    let comparable = moves > 0;
    let identity =
        comparable.then(|| canonical_identity_from_compact_uci(&initial_fen, &uci_moves));
    Some((
        LeanMainlineIdentity {
            comparable,
            moves,
            results,
            identity,
        },
        nodes,
        moves,
    ))
}

#[derive(Default)]
struct ProbeMoveTiming {
    moves: u64,
    total: std::time::Duration,
    range_raw: std::time::Duration,
    position_clone: std::time::Duration,
    apply_san: std::time::Duration,
    apply_attempts: u64,
}

thread_local! {
    static PROBE_MOVE_TIMING: std::cell::RefCell<Option<ProbeMoveTiming>> = const { std::cell::RefCell::new(None) };
}

fn probe_move_enabled() -> bool {
    PROBE_MOVE_TIMING.with(|profile| profile.borrow().is_some())
}

fn probe_move_update(update: impl FnOnce(&mut ProbeMoveTiming)) {
    PROBE_MOVE_TIMING.with(|profile| {
        if let Some(profile) = profile.borrow_mut().as_mut() {
            update(profile);
        }
    });
}

#[derive(Debug)]
pub(super) struct ProbeCoreProfile {
    pub(super) game: InterpretedGame,
    pub(super) view: std::time::Duration,
    pub(super) setup: std::time::Duration,
    pub(super) movetext: std::time::Duration,
    pub(super) finalize: std::time::Duration,
    pub(super) move_total: std::time::Duration,
    pub(super) move_range_raw: std::time::Duration,
    pub(super) move_position_clone: std::time::Duration,
    pub(super) move_apply_san: std::time::Duration,
    pub(super) move_count: u64,
    pub(super) move_apply_attempts: u64,
}

pub(super) fn interpret_line_tree_profiled(
    id: u64,
    tree: &ParseTree,
    stream: Option<&TokenStream>,
    source_start: u64,
    blob: &[u8],
) -> ProbeCoreProfile {
    let started = std::time::Instant::now();
    let mut game = empty_game(id, tree.segment_ordinal);
    let view = TreeView::new(tree);
    let Some(root) = view
        .node(0)
        .filter(|node| node.kind == NodeKind::GameCandidate)
    else {
        unexpected(&mut game, None);
        return ProbeCoreProfile {
            game,
            view: started.elapsed(),
            setup: std::time::Duration::ZERO,
            movetext: std::time::Duration::ZERO,
            finalize: std::time::Duration::ZERO,
            move_total: std::time::Duration::ZERO,
            move_range_raw: std::time::Duration::ZERO,
            move_position_clone: std::time::Duration::ZERO,
            move_apply_san: std::time::Duration::ZERO,
            move_count: 0,
            move_apply_attempts: 0,
        };
    };
    let direct_root = view.direct_children(root.id).collect::<Vec<_>>();
    let tag_sections = direct_root
        .iter()
        .filter(|node| node.kind == NodeKind::TagSection)
        .copied()
        .collect::<Vec<_>>();
    let movetexts = direct_root
        .iter()
        .filter(|node| node.kind == NodeKind::Movetext)
        .copied()
        .collect::<Vec<_>>();
    if tag_sections.len() > 1 || movetexts.len() > 1 {
        unexpected(&mut game, Some(root));
        return ProbeCoreProfile {
            game,
            view: started.elapsed(),
            setup: std::time::Duration::ZERO,
            movetext: std::time::Duration::ZERO,
            finalize: std::time::Duration::ZERO,
            move_total: std::time::Duration::ZERO,
            move_range_raw: std::time::Duration::ZERO,
            move_position_clone: std::time::Duration::ZERO,
            move_apply_san: std::time::Duration::ZERO,
            move_count: 0,
            move_apply_attempts: 0,
        };
    }
    let view_time = started.elapsed();

    let started = std::time::Instant::now();
    if let Some(section) = tag_sections.first() {
        collect_relevant_tags(&mut game, &view, stream, source_start, blob, section);
    }
    resolve_variant(&mut game);
    let mut rules_position = resolve_initial_position(&mut game);
    let setup_time = started.elapsed();

    PROBE_MOVE_TIMING.with(|profile| *profile.borrow_mut() = Some(ProbeMoveTiming::default()));
    let started = std::time::Instant::now();
    if let Some(movetext) = movetexts.first() {
        interpret_movetext(
            &mut game,
            &view,
            stream,
            source_start,
            blob,
            movetext,
            &mut rules_position,
        );
    } else {
        game.lines.push(InterpretedLine {
            id: 0,
            parent_line_id: None,
            variation_node_id: None,
            depth: 0,
            status: initial_line_status(&game, rules_position.is_some()),
            start_position: rules_position.as_ref().map(|p| p.components.clone()),
            replaces_move_id: None,
            move_ids: Vec::new(),
            result_node_ids: Vec::new(),
        });
    }
    let movetext_time = started.elapsed();
    let move_profile =
        PROBE_MOVE_TIMING.with(|profile| profile.borrow_mut().take().unwrap_or_default());

    let started = std::time::Instant::now();
    game.status = final_status(&game);
    let finalize_time = started.elapsed();
    ProbeCoreProfile {
        game,
        view: view_time,
        setup: setup_time,
        movetext: movetext_time,
        finalize: finalize_time,
        move_total: move_profile.total,
        move_range_raw: move_profile.range_raw,
        move_position_clone: move_profile.position_clone,
        move_apply_san: move_profile.apply_san,
        move_count: move_profile.moves,
        move_apply_attempts: move_profile.apply_attempts,
    }
}

fn interpret_game(
    id: u64,
    tree: &ParseTree,
    stream: Option<&TokenStream>,
    source_start: u64,
    blob: &[u8],
) -> InterpretedGame {
    let mut game = empty_game(id, tree.segment_ordinal);
    let view = TreeView::new(tree);
    let Some(root) = view
        .node(0)
        .filter(|node| node.kind == NodeKind::GameCandidate)
    else {
        unexpected(&mut game, None);
        return game;
    };
    let direct_root = view.direct_children(root.id).collect::<Vec<_>>();
    let tag_sections = direct_root
        .iter()
        .filter(|node| node.kind == NodeKind::TagSection)
        .copied()
        .collect::<Vec<_>>();
    let movetexts = direct_root
        .iter()
        .filter(|node| node.kind == NodeKind::Movetext)
        .copied()
        .collect::<Vec<_>>();
    if tag_sections.len() > 1 || movetexts.len() > 1 {
        unexpected(&mut game, Some(root));
        return game;
    }
    if let Some(section) = tag_sections.first() {
        collect_relevant_tags(&mut game, &view, stream, source_start, blob, section);
    }
    resolve_variant(&mut game);
    let mut rules_position = resolve_initial_position(&mut game);

    if let Some(movetext) = movetexts.first() {
        interpret_movetext(
            &mut game,
            &view,
            stream,
            source_start,
            blob,
            movetext,
            &mut rules_position,
        );
    } else {
        game.lines.push(InterpretedLine {
            id: 0,
            parent_line_id: None,
            variation_node_id: None,
            depth: 0,
            status: initial_line_status(&game, rules_position.is_some()),
            start_position: rules_position.as_ref().map(|p| p.components.clone()),
            replaces_move_id: None,
            move_ids: Vec::new(),
            result_node_ids: Vec::new(),
        });
    }
    game.status = final_status(&game);
    game
}

fn empty_game(id: u64, segment_ordinal: u64) -> InterpretedGame {
    InterpretedGame {
        id,
        parse_tree_index: id,
        segment_ordinal,
        status: GameInterpretationStatus::Complete,
        variant: VariantKind::Standard,
        variant_status: VariantStatus::Supported,
        initial_position_status: InitialPositionStatus::Default,
        initial_position: None,
        relevant_tags: Vec::new(),
        moves: Vec::new(),
        lines: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn collect_relevant_tags(
    game: &mut InterpretedGame,
    view: &TreeView<'_>,
    stream: Option<&TokenStream>,
    source_start: u64,
    blob: &[u8],
    section: &FlatNode,
) {
    for pair in view
        .direct_children(section.id)
        .filter(|node| node.kind == NodeKind::TagPair)
    {
        let children = view.direct_children(pair.id).collect::<Vec<_>>();
        let names = children
            .iter()
            .filter(|node| node.kind == NodeKind::TagNameCandidate)
            .copied()
            .collect::<Vec<_>>();
        let values = children
            .iter()
            .filter(|node| node.kind == NodeKind::TagValueCandidate)
            .copied()
            .collect::<Vec<_>>();
        if names.len() != 1 || values.len() > 1 {
            unexpected(game, Some(pair));
            continue;
        }
        let name_range = match node_bytes_at(names[0], stream, source_start, blob) {
            Ok(range) => range,
            Err(error) => {
                invalid_range(game, names[0], error, None);
                continue;
            }
        };
        let name = match decode_tag_name(name_range.bytes) {
            Ok(name) => Some(name),
            Err(TagDecodeStatus::InvalidEncoding) => {
                diagnostic(
                    game,
                    InterpretationDiagnosticCode::InvalidTagEncoding,
                    None,
                    Some(names[0]),
                    None,
                    Some(name_range),
                );
                continue;
            }
            Err(_) => unreachable!("tag names only distinguish encoding"),
        };
        if !matches!(name.as_deref(), Some("Variant" | "SetUp" | "FEN")) {
            continue;
        }
        let pair_range = match node_bytes_at(pair, stream, source_start, blob) {
            Ok(range) => range,
            Err(error) => {
                invalid_range(game, pair, error, None);
                continue;
            }
        };
        let (value_node_id, value, decode_status) = if let Some(value_node) = values.first() {
            match node_bytes_at(value_node, stream, source_start, blob) {
                Ok(range) => match decode_quoted_tag(range.bytes) {
                    Ok(value) => (Some(value_node.id), Some(value), TagDecodeStatus::Decoded),
                    Err(TagDecodeStatus::InvalidEncoding) => {
                        diagnostic(
                            game,
                            InterpretationDiagnosticCode::InvalidTagEncoding,
                            None,
                            Some(value_node),
                            None,
                            Some(range),
                        );
                        (Some(value_node.id), None, TagDecodeStatus::InvalidEncoding)
                    }
                    Err(TagDecodeStatus::InvalidEscape) => {
                        diagnostic(
                            game,
                            InterpretationDiagnosticCode::InvalidTagEscape,
                            None,
                            Some(value_node),
                            None,
                            Some(range),
                        );
                        (Some(value_node.id), None, TagDecodeStatus::InvalidEscape)
                    }
                    Err(TagDecodeStatus::MissingValue) => {
                        diagnostic(
                            game,
                            InterpretationDiagnosticCode::UnexpectedCstStructure,
                            None,
                            Some(value_node),
                            None,
                            Some(range),
                        );
                        (Some(value_node.id), None, TagDecodeStatus::MissingValue)
                    }
                    Err(TagDecodeStatus::Decoded) => unreachable!("decoded is not an error"),
                },
                Err(error) => {
                    invalid_range(game, value_node, error, None);
                    (Some(value_node.id), None, TagDecodeStatus::MissingValue)
                }
            }
        } else {
            (None, None, TagDecodeStatus::MissingValue)
        };
        game.relevant_tags.push(RelevantTag {
            tag_pair_node_id: pair.id,
            name_node_id: names[0].id,
            value_node_id,
            name,
            value,
            decode_status,
            start_token: pair.start_token,
            end_token: pair.end_token,
            start_byte: pair_range.start,
            end_byte: pair_range.end,
        });
    }
}

pub(super) fn decode_tag_name(bytes: &[u8]) -> Result<String, TagDecodeStatus> {
    if !bytes.is_ascii() {
        return Err(TagDecodeStatus::InvalidEncoding);
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| TagDecodeStatus::InvalidEncoding)
}

pub(super) fn decode_quoted_tag(bytes: &[u8]) -> Result<String, TagDecodeStatus> {
    if !bytes.is_ascii() {
        return Err(TagDecodeStatus::InvalidEncoding);
    }
    if bytes.len() < 2 || bytes.first() != Some(&b'"') || bytes.last() != Some(&b'"') {
        return Err(TagDecodeStatus::MissingValue);
    }
    let content_end = bytes.len() - 1;
    let mut out = Vec::with_capacity(content_end - 1);
    let mut index = 1;
    while index < content_end {
        match bytes[index] {
            b'\\' => {
                let escaped_index = index + 1;
                if escaped_index >= content_end {
                    return Err(TagDecodeStatus::MissingValue);
                }
                let escaped = bytes[escaped_index];
                if !matches!(escaped, b'\\' | b'"') {
                    return Err(TagDecodeStatus::InvalidEscape);
                }
                out.push(escaped);
                index += 2;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| TagDecodeStatus::InvalidEncoding)
}

enum ValueResolution<'a> {
    None,
    Unique(&'a str),
    Conflicting,
    Undecodable,
}

fn resolve_tag_values<'a>(tags: impl Iterator<Item = &'a RelevantTag>) -> ValueResolution<'a> {
    let mut unique = None;
    let mut seen = false;
    for tag in tags {
        seen = true;
        let Some(value) = tag.value.as_deref() else {
            return ValueResolution::Undecodable;
        };
        match unique {
            None => unique = Some(value),
            Some(existing) if existing == value => {}
            Some(_) => return ValueResolution::Conflicting,
        }
    }
    if seen {
        ValueResolution::Unique(unique.expect("decoded occurrence has a value"))
    } else {
        ValueResolution::None
    }
}

fn resolve_variant(game: &mut InterpretedGame) {
    let value = match resolve_tag_values(
        game.relevant_tags
            .iter()
            .filter(|tag| tag.name.as_deref() == Some("Variant")),
    ) {
        ValueResolution::None => return,
        ValueResolution::Unique(value) => value,
        ValueResolution::Conflicting | ValueResolution::Undecodable => {
            game.variant = VariantKind::Unknown;
            game.variant_status = VariantStatus::Ambiguous;
            simple_diagnostic(game, InterpretationDiagnosticCode::ConflictingVariantTags);
            return;
        }
    };
    match value {
        "Standard" => {
            game.variant = VariantKind::Standard;
            game.variant_status = VariantStatus::Supported;
        }
        "Chess960" | "FischerRandom" => {
            game.variant = VariantKind::Chess960;
            game.variant_status = VariantStatus::Unsupported;
        }
        _ => {
            game.variant = VariantKind::Other;
            game.variant_status = VariantStatus::Unsupported;
        }
    }
    if game.variant_status == VariantStatus::Unsupported {
        simple_diagnostic(game, InterpretationDiagnosticCode::UnsupportedVariant);
    }
}

fn resolve_initial_position(game: &mut InterpretedGame) -> Option<ParsedPosition> {
    if game.variant_status == VariantStatus::Unsupported {
        game.initial_position_status = InitialPositionStatus::UnsupportedVariant;
        return None;
    }
    if game.variant_status == VariantStatus::Ambiguous {
        game.initial_position_status = InitialPositionStatus::Ambiguous;
        return None;
    }
    let setup_one = match resolve_tag_values(
        game.relevant_tags
            .iter()
            .filter(|tag| tag.name.as_deref() == Some("SetUp")),
    ) {
        ValueResolution::None => false,
        ValueResolution::Unique("1") => true,
        ValueResolution::Unique(_) | ValueResolution::Undecodable => {
            game.initial_position_status = InitialPositionStatus::Invalid;
            simple_diagnostic(game, InterpretationDiagnosticCode::InvalidSetupTag);
            return None;
        }
        ValueResolution::Conflicting => {
            game.initial_position_status = InitialPositionStatus::Ambiguous;
            simple_diagnostic(game, InterpretationDiagnosticCode::InvalidSetupTag);
            simple_diagnostic(game, InterpretationDiagnosticCode::AmbiguousInitialPosition);
            return None;
        }
    };
    let fen = match resolve_tag_values(
        game.relevant_tags
            .iter()
            .filter(|tag| tag.name.as_deref() == Some("FEN")),
    ) {
        ValueResolution::None => None,
        ValueResolution::Unique(value) => Some(value.to_owned()),
        ValueResolution::Undecodable => {
            game.initial_position_status = InitialPositionStatus::Invalid;
            simple_diagnostic(game, InterpretationDiagnosticCode::InvalidFen);
            return None;
        }
        ValueResolution::Conflicting => {
            game.initial_position_status = InitialPositionStatus::Ambiguous;
            simple_diagnostic(game, InterpretationDiagnosticCode::ConflictingFenTags);
            simple_diagnostic(game, InterpretationDiagnosticCode::AmbiguousInitialPosition);
            return None;
        }
    };
    let Some(fen) = fen else {
        if setup_one {
            game.initial_position_status = InitialPositionStatus::Invalid;
            simple_diagnostic(game, InterpretationDiagnosticCode::SetupWithoutFen);
            return None;
        }
        let position = standard_initial_position();
        game.initial_position = Some(position.components.clone());
        game.initial_position_status = InitialPositionStatus::Default;
        return Some(position);
    };
    let mut status_without_setup = false;
    if !setup_one {
        status_without_setup = true;
        simple_diagnostic(game, InterpretationDiagnosticCode::FenWithoutSetup);
    }
    match parse_standard_fen(fen.as_bytes()) {
        Ok(position) => {
            game.initial_position = Some(position.components.clone());
            if position.fen_status == FenStatus::Recovered {
                simple_diagnostic(game, InterpretationDiagnosticCode::NoncanonicalFen);
            }
            game.initial_position_status =
                if status_without_setup || position.fen_status == FenStatus::Recovered {
                    InitialPositionStatus::Recovered
                } else {
                    InitialPositionStatus::Explicit
                };
            Some(position)
        }
        Err(_) => {
            game.initial_position_status = InitialPositionStatus::Invalid;
            simple_diagnostic(game, InterpretationDiagnosticCode::InvalidFen);
            None
        }
    }
}

#[derive(Clone)]
struct LineRuntime {
    current_position: Option<ParsedPosition>,
    line_start_position: Option<ParsedPosition>,
    blocked: bool,
    after_result: bool,
    pending_move_number_node_ids: Vec<u64>,
    last_move_id: Option<u64>,
    last_move_previous_position: Option<ParsedPosition>,
}

fn interpret_movetext(
    game: &mut InterpretedGame,
    view: &TreeView<'_>,
    stream: Option<&TokenStream>,
    source_start: u64,
    blob: &[u8],
    movetext: &FlatNode,
    position: &mut Option<ParsedPosition>,
) {
    let start = position.clone();
    game.lines.push(InterpretedLine {
        id: 0,
        parent_line_id: None,
        variation_node_id: None,
        depth: 0,
        status: initial_line_status(game, start.is_some()),
        start_position: start.as_ref().map(|p| p.components.clone()),
        replaces_move_id: None,
        move_ids: Vec::new(),
        result_node_ids: Vec::new(),
    });
    let mut runtime = LineRuntime {
        current_position: position.take(),
        line_start_position: start,
        blocked: false,
        after_result: false,
        pending_move_number_node_ids: Vec::new(),
        last_move_id: None,
        last_move_previous_position: None,
    };
    interpret_line(
        game,
        view,
        stream,
        source_start,
        blob,
        movetext,
        0,
        &mut runtime,
    );
    *position = runtime.current_position;
}

fn initial_line_status(game: &InterpretedGame, has_start: bool) -> LineStatus {
    if game.variant_status == VariantStatus::Unsupported {
        LineStatus::UnsupportedVariant
    } else if has_start {
        LineStatus::Complete
    } else {
        LineStatus::UnknownStartPosition
    }
}

#[allow(clippy::too_many_arguments)]
fn interpret_line(
    game: &mut InterpretedGame,
    view: &TreeView<'_>,
    stream: Option<&TokenStream>,
    source_start: u64,
    blob: &[u8],
    movetext: &FlatNode,
    line_id: u64,
    runtime: &mut LineRuntime,
) {
    for node in view.direct_children(movetext.id) {
        match node.kind {
            NodeKind::Trivia | NodeKind::EscapeLine | NodeKind::Comment | NodeKind::Nag => {}
            NodeKind::MoveNumber => runtime.pending_move_number_node_ids.push(node.id),
            NodeKind::Variation => interpret_variation(
                game,
                view,
                stream,
                source_start,
                blob,
                node,
                line_id,
                runtime,
            ),
            NodeKind::Result => {
                game.lines[line_id as usize].result_node_ids.push(node.id);
                runtime.after_result = true;
                runtime.last_move_id = None;
                runtime.last_move_previous_position = None;
                runtime.pending_move_number_node_ids.clear();
            }
            NodeKind::MoveSymbolCandidate => {
                interpret_move(game, stream, source_start, blob, node, line_id, runtime)
            }
            NodeKind::Unparsed
            | NodeKind::TagPair
            | NodeKind::TagSection
            | NodeKind::Movetext
            | NodeKind::GameCandidate
            | NodeKind::Delimiter
            | NodeKind::TagNameCandidate
            | NodeKind::TagValueCandidate => {
                line_diagnostic(
                    game,
                    InterpretationDiagnosticCode::UnexpectedCstStructure,
                    line_id,
                    Some(node),
                );
                mark_line(game, line_id, LineStatus::UnexpectedStructure);
                runtime.pending_move_number_node_ids.clear();
            }
        }
    }
}

fn interpret_move(
    game: &mut InterpretedGame,
    stream: Option<&TokenStream>,
    source_start: u64,
    blob: &[u8],
    node: &FlatNode,
    line_id: u64,
    runtime: &mut LineRuntime,
) {
    let profile_enabled = probe_move_enabled();
    let profile_total_started = profile_enabled.then(std::time::Instant::now);
    if profile_enabled {
        probe_move_update(|profile| profile.moves += 1);
    }

    let move_id = game.moves.len() as u64;
    let ordinal_in_line = game.lines[line_id as usize].move_ids.len() as u64;
    let profile_started = profile_enabled.then(std::time::Instant::now);
    let range = node_bytes_at(node, stream, source_start, blob);
    let (start_byte, end_byte, bytes) = range
        .map(|r| (r.start, r.end, Some(r.bytes)))
        .unwrap_or((0, 0, None));
    let raw_san = bytes
        .filter(|b| b.is_ascii())
        .map(|b| String::from_utf8(b.to_vec()).expect("ASCII"));
    if let Some(started) = profile_started {
        probe_move_update(|profile| profile.range_raw += started.elapsed());
    }
    let profile_started = profile_enabled.then(std::time::Instant::now);
    let previous_runtime = runtime.current_position.clone();
    if let Some(started) = profile_started {
        probe_move_update(|profile| profile.position_clone += started.elapsed());
    }
    let mut item = InterpretedMove {
        id: move_id,
        line_id,
        ordinal_in_line,
        node_id: node.id,
        move_number_node_ids: std::mem::take(&mut runtime.pending_move_number_node_ids),
        start_token: node.start_token,
        end_token: node.end_token,
        start_byte,
        end_byte,
        raw_san,
        status: MoveInterpretationStatus::UnknownInitialPosition,
        previous_position: previous_runtime.as_ref().map(|p| p.components.clone()),
        resulting_position: None,
        normalized: None,
        source_was_canonical_san: None,
    };
    if let Err(error) = range {
        invalid_range(game, node, error, Some(line_id));
        item.status = MoveInterpretationStatus::InvalidEncoding;
        runtime.blocked = true;
    } else if runtime.after_result {
        item.status = MoveInterpretationStatus::AfterResult;
        item.previous_position = None;
        move_diagnostic(
            game,
            InterpretationDiagnosticCode::MoveAfterResult,
            node,
            move_id,
            line_id,
            range.ok(),
        );
    } else if game.variant_status == VariantStatus::Unsupported {
        item.status = MoveInterpretationStatus::UnsupportedVariant;
    } else if runtime.current_position.is_none() {
        item.status = MoveInterpretationStatus::UnknownInitialPosition;
    } else if runtime.blocked {
        item.status = MoveInterpretationStatus::BlockedByPriorError;
        item.previous_position = None;
        move_diagnostic(
            game,
            InterpretationDiagnosticCode::BlockedByPriorError,
            node,
            move_id,
            line_id,
            range.ok(),
        );
    } else if let Some(bytes) = bytes {
        if !bytes.is_ascii() {
            item.status = MoveInterpretationStatus::InvalidEncoding;
            move_diagnostic(
                game,
                InterpretationDiagnosticCode::InvalidSanEncoding,
                node,
                move_id,
                line_id,
                range.ok(),
            );
            runtime.blocked = true;
        } else {
            if let Some(position) = runtime.current_position.as_mut() {
                let profile_started = profile_enabled.then(std::time::Instant::now);
                if profile_enabled {
                    probe_move_update(|profile| profile.apply_attempts += 1);
                }
                let applied_result = apply_standard_san(position, bytes);
                if let Some(started) = profile_started {
                    probe_move_update(|profile| profile.apply_san += started.elapsed());
                }
                match applied_result {
                    Ok(applied) => {
                        item.status = MoveInterpretationStatus::Applied;
                        item.resulting_position = Some(applied.resulting_position);
                        item.source_was_canonical_san = Some(applied.source_was_canonical_san);
                        item.normalized = Some(applied.normalized);
                    }
                    Err(error) => {
                        let (status, code) = move_error(error);
                        item.status = status;
                        move_diagnostic(game, code, node, move_id, line_id, range.ok());
                        runtime.blocked = true;
                    }
                }
            }
        }
    }
    if item.status != MoveInterpretationStatus::Applied {
        mark_line(
            game,
            line_id,
            if runtime.current_position.is_some() {
                LineStatus::Partial
            } else {
                LineStatus::UnknownStartPosition
            },
        );
    }
    runtime.last_move_id = Some(move_id);
    runtime.last_move_previous_position = if item.status
        == MoveInterpretationStatus::BlockedByPriorError
        || item.status == MoveInterpretationStatus::AfterResult
    {
        None
    } else {
        previous_runtime
    };
    game.lines[line_id as usize].move_ids.push(move_id);
    game.moves.push(item);
    if let Some(started) = profile_total_started {
        probe_move_update(|profile| profile.total += started.elapsed());
    }
}

#[allow(clippy::too_many_arguments)]
fn interpret_variation(
    game: &mut InterpretedGame,
    view: &TreeView<'_>,
    stream: Option<&TokenStream>,
    source_start: u64,
    blob: &[u8],
    variation: &FlatNode,
    parent_line_id: u64,
    parent: &LineRuntime,
) {
    let depth = game.lines[parent_line_id as usize].depth.saturating_add(1);
    let line_id = game.lines.len() as u64;
    let (replaces_move_id, start, anchor_code) = if parent.after_result {
        (
            None,
            None,
            Some(InterpretationDiagnosticCode::VariationAfterResult),
        )
    } else if let Some(move_id) = parent.last_move_id {
        let start = parent.last_move_previous_position.clone();
        let code = start
            .is_none()
            .then_some(InterpretationDiagnosticCode::UnknownVariationStart);
        (Some(move_id), start, code)
    } else {
        (
            None,
            parent.line_start_position.clone(),
            Some(InterpretationDiagnosticCode::VariationWithoutPrecedingMove),
        )
    };
    let status = initial_line_status(game, start.is_some());
    game.lines.push(InterpretedLine {
        id: line_id,
        parent_line_id: Some(parent_line_id),
        variation_node_id: Some(variation.id),
        depth,
        status,
        start_position: start.as_ref().map(|p| p.components.clone()),
        replaces_move_id,
        move_ids: Vec::new(),
        result_node_ids: Vec::new(),
    });
    if let Some(code) = anchor_code {
        line_diagnostic(game, code, line_id, Some(variation));
    }
    if usize::try_from(depth).map_or(true, |d| {
        d > crate::inspector::parsing::model::MAX_VARIATION_DEPTH
    }) {
        mark_line(game, line_id, LineStatus::UnexpectedStructure);
        line_diagnostic(
            game,
            InterpretationDiagnosticCode::VariationDepthExceeded,
            line_id,
            Some(variation),
        );
        return;
    }
    let mut movetexts = Vec::new();
    let mut unexpected_child = false;
    for child in view.direct_children(variation.id) {
        match child.kind {
            NodeKind::Movetext => movetexts.push(child),
            NodeKind::Delimiter | NodeKind::Trivia => {}
            _ => unexpected_child = true,
        }
    }
    // Parse v1 represents the exact token sequence `()` as a recovered
    // Variation containing only its delimiters. Preserve that already
    // verified CST spelling as the one valid empty-RAV case.
    if movetexts.is_empty() && !unexpected_child && variation.status == NodeStatus::Recovered {
        mark_line(game, line_id, LineStatus::Partial);
        line_diagnostic(
            game,
            InterpretationDiagnosticCode::EmptyVariation,
            line_id,
            Some(variation),
        );
        return;
    }
    if movetexts.len() != 1 {
        mark_line(game, line_id, LineStatus::UnexpectedStructure);
        line_diagnostic(
            game,
            InterpretationDiagnosticCode::UnexpectedVariationStructure,
            line_id,
            Some(variation),
        );
        return;
    }
    if unexpected_child {
        mark_line(game, line_id, LineStatus::UnexpectedStructure);
        line_diagnostic(
            game,
            InterpretationDiagnosticCode::UnexpectedVariationStructure,
            line_id,
            Some(variation),
        );
    }
    let movetext = movetexts[0];
    let mut runtime = LineRuntime {
        current_position: start.clone(),
        line_start_position: start,
        blocked: false,
        after_result: false,
        pending_move_number_node_ids: Vec::new(),
        last_move_id: None,
        last_move_previous_position: None,
    };
    let line_count_before = game.lines.len();
    interpret_line(
        game,
        view,
        stream,
        source_start,
        blob,
        movetext,
        line_id,
        &mut runtime,
    );
    if game.lines[line_id as usize].move_ids.is_empty()
        && game.lines[line_id as usize].result_node_ids.is_empty()
        && game.lines.len() == line_count_before
    {
        mark_line(game, line_id, LineStatus::Partial);
        line_diagnostic(
            game,
            InterpretationDiagnosticCode::EmptyVariation,
            line_id,
            Some(variation),
        );
    }
}

fn mark_line(game: &mut InterpretedGame, line_id: u64, status: LineStatus) {
    let line = &mut game.lines[line_id as usize];
    if line.status != LineStatus::UnexpectedStructure
        && (status == LineStatus::UnexpectedStructure || line.status == LineStatus::Complete)
    {
        line.status = status;
    }
}

fn move_error(error: MoveFailure) -> (MoveInterpretationStatus, InterpretationDiagnosticCode) {
    match error {
        MoveFailure::InvalidEncoding => (
            MoveInterpretationStatus::InvalidEncoding,
            InterpretationDiagnosticCode::InvalidSanEncoding,
        ),
        MoveFailure::InvalidSan => (
            MoveInterpretationStatus::InvalidSan,
            InterpretationDiagnosticCode::InvalidSan,
        ),
        MoveFailure::AmbiguousSan => (
            MoveInterpretationStatus::AmbiguousSan,
            InterpretationDiagnosticCode::AmbiguousSan,
        ),
        MoveFailure::IllegalMove => (
            MoveInterpretationStatus::IllegalMove,
            InterpretationDiagnosticCode::IllegalMove,
        ),
    }
}

fn final_status(game: &InterpretedGame) -> GameInterpretationStatus {
    if game.variant_status == VariantStatus::Unsupported {
        return GameInterpretationStatus::UnsupportedVariant;
    }
    if game.initial_position_status == InitialPositionStatus::Ambiguous {
        return GameInterpretationStatus::AmbiguousInitialPosition;
    }
    if game.initial_position_status == InitialPositionStatus::Invalid {
        return GameInterpretationStatus::InvalidInitialPosition;
    }
    if game
        .lines
        .iter()
        .any(|line| line.status == LineStatus::UnexpectedStructure)
        || game.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code,
                InterpretationDiagnosticCode::UnexpectedCstStructure
                    | InterpretationDiagnosticCode::UnexpectedVariationStructure
                    | InterpretationDiagnosticCode::VariationDepthExceeded
            )
        })
    {
        return GameInterpretationStatus::UnexpectedStructure;
    }
    if game.lines.iter().any(|line| {
        matches!(
            line.status,
            LineStatus::Partial | LineStatus::UnknownStartPosition
        )
    }) {
        GameInterpretationStatus::Partial
    } else {
        GameInterpretationStatus::Complete
    }
}

#[cfg(test)]
pub(super) fn node_bytes<'a>(
    node: &FlatNode,
    stream: Option<&'a TokenStream>,
    blob: &'a [u8],
) -> Result<NodeBytes<'a>, NodeRangeError> {
    node_bytes_at(node, stream, 0, blob)
}

fn node_bytes_at<'a>(
    node: &FlatNode,
    stream: Option<&'a TokenStream>,
    source_start: u64,
    blob: &'a [u8],
) -> Result<NodeBytes<'a>, NodeRangeError> {
    let stream = stream.ok_or(NodeRangeError::MissingTokenStream)?;
    if node.start_token >= node.end_token || node.end_token > stream.tokens.len() as u64 {
        return Err(NodeRangeError::InvalidTokenRange);
    }
    let start = usize::try_from(node.start_token).map_err(|_| NodeRangeError::InvalidTokenRange)?;
    let end = usize::try_from(node.end_token).map_err(|_| NodeRangeError::InvalidTokenRange)?;
    let first = stream
        .tokens
        .get(start)
        .ok_or(NodeRangeError::InvalidTokenRange)?;
    let last_index = end
        .checked_sub(1)
        .ok_or(NodeRangeError::InvalidTokenRange)?;
    let last = stream
        .tokens
        .get(last_index)
        .ok_or(NodeRangeError::InvalidTokenRange)?;
    let blob_bytes = u64::try_from(blob.len()).map_err(|_| NodeRangeError::InvalidByteRange)?;
    let blob_end = source_start
        .checked_add(blob_bytes)
        .ok_or(NodeRangeError::InvalidByteRange)?;
    if first.start >= last.end
        || first.start < stream.start
        || last.end > stream.end
        || first.start < source_start
        || last.end > blob_end
    {
        return Err(NodeRangeError::InvalidByteRange);
    }
    let byte_start = usize::try_from(first.start - source_start)
        .map_err(|_| NodeRangeError::InvalidByteRange)?;
    let byte_end =
        usize::try_from(last.end - source_start).map_err(|_| NodeRangeError::InvalidByteRange)?;
    let bytes = blob
        .get(byte_start..byte_end)
        .ok_or(NodeRangeError::InvalidByteRange)?;
    Ok(NodeBytes {
        bytes,
        start: first.start,
        end: last.end,
    })
}

fn diagnostic(
    game: &mut InterpretedGame,
    code: InterpretationDiagnosticCode,
    line_id: Option<u64>,
    node: Option<&FlatNode>,
    move_id: Option<u64>,
    range: Option<NodeBytes<'_>>,
) {
    game.diagnostics.push(InterpretationDiagnostic {
        code,
        line_id,
        node_id: node.map(|n| n.id),
        move_id,
        start_token: node.map(|n| n.start_token),
        end_token: node.map(|n| n.end_token),
        start_byte: range.map(|r| r.start),
        end_byte: range.map(|r| r.end),
    });
}
fn simple_diagnostic(game: &mut InterpretedGame, code: InterpretationDiagnosticCode) {
    diagnostic(game, code, None, None, None, None);
}
fn move_diagnostic(
    game: &mut InterpretedGame,
    code: InterpretationDiagnosticCode,
    node: &FlatNode,
    move_id: u64,
    line_id: u64,
    range: Option<NodeBytes<'_>>,
) {
    diagnostic(game, code, Some(line_id), Some(node), Some(move_id), range);
}
fn line_diagnostic(
    game: &mut InterpretedGame,
    code: InterpretationDiagnosticCode,
    line_id: u64,
    node: Option<&FlatNode>,
) {
    diagnostic(game, code, Some(line_id), node, None, None);
}
fn unexpected(game: &mut InterpretedGame, node: Option<&FlatNode>) {
    diagnostic(
        game,
        InterpretationDiagnosticCode::UnexpectedCstStructure,
        None,
        node,
        None,
        None,
    );
    game.status = GameInterpretationStatus::UnexpectedStructure;
}
fn invalid_range(
    game: &mut InterpretedGame,
    node: &FlatNode,
    error: NodeRangeError,
    line_id: Option<u64>,
) {
    let code = range_diagnostic_code(error);
    diagnostic(game, code, line_id, Some(node), None, None);
}

pub(super) fn range_diagnostic_code(error: NodeRangeError) -> InterpretationDiagnosticCode {
    match error {
        NodeRangeError::MissingTokenStream | NodeRangeError::InvalidTokenRange => {
            InterpretationDiagnosticCode::InvalidNodeTokenRange
        }
        NodeRangeError::InvalidByteRange => InterpretationDiagnosticCode::InvalidNodeByteRange,
    }
}
