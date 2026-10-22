use super::*;
use crate::inspector::{parsing, segmentation, source, tokenization};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    parse: PathBuf,
    tokenization: PathBuf,
    object: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn test_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "antiskill-interpretation-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn real_pilot_pgn() -> PathBuf {
    let source = std::env::var_os("ANTISKILL_REAL_PILOT_PGN")
        .map(PathBuf::from)
        .expect("ANTISKILL_REAL_PILOT_PGN must point to the real pilot PGN");
    assert!(
        source.is_file(),
        "ANTISKILL_REAL_PILOT_PGN does not point to a file"
    );
    source
}

fn fixture(bytes: &[u8]) -> Fixture {
    let root = test_root("fixture");
    fs::create_dir_all(&root).expect("workspace");
    let path = root.join("source.pgn");
    fs::write(&path, bytes).expect("source");
    let imported = source::import_source(&root, &path).expect("import");
    let object = root.join(&imported.manifest.object_path);
    let segmented = segmentation::segment_source(&root, &imported.manifest_path).expect("segment");
    let tokenized = tokenization::tokenize(&root, &segmented.path).expect("tokenize");
    let parsed = parsing::parse(&root, &tokenized.path).expect("parse");
    Fixture {
        root,
        parse: parsed.path,
        tokenization: tokenized.path,
        object,
    }
}

fn interpret(bytes: &[u8]) -> (Fixture, LineInterpretation) {
    let fixture = fixture(bytes);
    let result = interpret_lines(&fixture.root, &fixture.parse).expect("interpret");
    (fixture, result)
}

fn game(bytes: &[u8]) -> InterpretedGame {
    interpret(bytes).1.games.remove(0)
}

fn sans(game: &InterpretedGame, line_id: u64) -> Vec<&str> {
    game.moves
        .iter()
        .filter(|item| item.line_id == line_id)
        .filter_map(|item| item.raw_san.as_deref())
        .collect()
}

fn has(game: &InterpretedGame, code: InterpretationDiagnosticCode) -> bool {
    game.diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == code)
}

fn structurally_mutated_variation_game(
    mutate: impl FnOnce(&mut crate::inspector::parsing::model::ParseTree, u64),
) -> InterpretedGame {
    let fixture = fixture(b"[Event \"x\"]\n\n1. e4 (1. d4) *\n");
    let mut loaded =
        parsing::load_parse_manifest(&fixture.root, &fixture.parse, None).expect("load");
    let tree = &mut loaded.manifest.trees[0];
    let variation_id = tree
        .nodes
        .iter()
        .find(|node| node.kind == crate::inspector::parsing::model::NodeKind::Variation)
        .expect("variation")
        .id;
    mutate(tree, variation_id);
    let stream = &loaded.tokenization.manifest.streams[0];
    let blob = loaded
        .tokenization
        .read_source_range(stream.start, stream.end)
        .expect("lettura sorgente");
    super::lines::interpret_line_tree(
        0,
        &loaded.manifest.trees[0],
        Some(stream),
        stream.start,
        &blob,
    )
}

fn line_diagnostic_count(
    game: &InterpretedGame,
    line_id: u64,
    code: InterpretationDiagnosticCode,
) -> usize {
    game.diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.line_id == Some(line_id) && diagnostic.code == code)
        .count()
}

#[test]
fn standard_game_applies_e4_e5_nf3() {
    let game = game(b"[Event \"x\"]\n\n1. e4 e5 2. Nf3 *\n");
    assert_eq!(
        game.moves.iter().map(|m| m.status).collect::<Vec<_>>(),
        vec![MoveInterpretationStatus::Applied; 3]
    );
    assert_eq!(game.moves[2].normalized.as_ref().expect("move").uci, "g1f3");
    assert_eq!(game.moves[0].source_was_canonical_san, Some(true));
}

#[test]
fn move_mapping_preserves_cst_token_and_byte_ranges() {
    let bytes = b"[Event \"x\"]\n\n1. e4 *\n";
    let game = game(bytes);
    let item = &game.moves[0];
    assert_eq!(
        &bytes[item.start_byte as usize..item.end_byte as usize],
        b"e4"
    );
    assert_eq!(item.raw_san.as_deref(), Some("e4"));
    assert_eq!(item.end_token, item.start_token + 1);
    assert!(item.node_id > 0);
}

#[test]
fn crlf_and_mixed_whitespace_do_not_change_mainline() {
    let game = game(b"[Event \"x\"]\r\n\r\n1.\t e4  \te5\r\n");
    assert_eq!(game.moves.len(), 2);
    assert!(game
        .moves
        .iter()
        .all(|m| m.status == MoveInterpretationStatus::Applied));
}

#[test]
fn canonical_fen_with_setup_is_explicit() {
    let game = game(b"[SetUp \"1\"]\n[FEN \"7k/P7/8/8/8/8/8/7K w - - 0 1\"]\n\n1. a8=Q+ *\n");
    assert_eq!(
        game.initial_position_status,
        InitialPositionStatus::Explicit
    );
    assert_eq!(game.moves[0].status, MoveInterpretationStatus::Applied);
}

#[test]
fn noncanonical_fen_is_recovered() {
    let game = game(b"[SetUp \"1\"]\n[FEN \"7k/P7/8/8/8/8/8/7K w\"]\n\n1. a8=Q+ *\n");
    assert_eq!(
        game.initial_position_status,
        InitialPositionStatus::Recovered
    );
    assert!(has(&game, InterpretationDiagnosticCode::NoncanonicalFen));
}

#[test]
fn fen_without_setup_is_recovered_with_diagnostic() {
    let game = game(b"[FEN \"7k/P7/8/8/8/8/8/7K w - - 0 1\"]\n\n1. a8=Q+ *\n");
    assert_eq!(
        game.initial_position_status,
        InitialPositionStatus::Recovered
    );
    assert!(has(&game, InterpretationDiagnosticCode::FenWithoutSetup));
}

#[test]
fn setup_without_fen_is_invalid() {
    let game = game(b"[SetUp \"1\"]\n\n1. e4 *\n");
    assert_eq!(
        game.status,
        GameInterpretationStatus::InvalidInitialPosition
    );
    assert!(has(&game, InterpretationDiagnosticCode::SetupWithoutFen));
}

#[test]
fn invalid_fen_is_diagnostic() {
    let game = game(b"[SetUp \"1\"]\n[FEN \"invalid\"]\n\n1. e4 *\n");
    assert!(has(&game, InterpretationDiagnosticCode::InvalidFen));
}

#[test]
fn identical_duplicate_fens_have_one_decision() {
    let game = game(b"[SetUp \"1\"]\n[FEN \"7k/P7/8/8/8/8/8/7K w - - 0 1\"]\n[FEN \"7k/P7/8/8/8/8/8/7K w - - 0 1\"]\n\n1. a8=Q+ *\n");
    assert_eq!(
        game.initial_position_status,
        InitialPositionStatus::Explicit
    );
    assert_eq!(
        game.relevant_tags
            .iter()
            .filter(|t| t.name.as_deref() == Some("FEN"))
            .count(),
        2
    );
}

#[test]
fn conflicting_fens_are_ambiguous() {
    let game = game(b"[SetUp \"1\"]\n[FEN \"7k/P7/8/8/8/8/8/7K w - - 0 1\"]\n[FEN \"7k/8/P7/8/8/8/8/7K w - - 0 1\"]\n\n1. a8=Q+ *\n");
    assert_eq!(
        game.status,
        GameInterpretationStatus::AmbiguousInitialPosition
    );
    assert!(has(&game, InterpretationDiagnosticCode::ConflictingFenTags));
}

#[test]
fn explicit_standard_variant_is_supported() {
    let game = game(b"[Variant \"Standard\"]\n\n1. e4 *\n");
    assert_eq!(
        (game.variant, game.variant_status),
        (VariantKind::Standard, VariantStatus::Supported)
    );
}

#[test]
fn chess960_variant_is_not_applied() {
    let game = game(b"[Variant \"Chess960\"]\n\n1. e4 *\n");
    assert_eq!(game.status, GameInterpretationStatus::UnsupportedVariant);
    assert_eq!(
        game.moves[0].status,
        MoveInterpretationStatus::UnsupportedVariant
    );
}

#[test]
fn unknown_variant_is_never_standard() {
    let game = game(b"[Variant \"Atomic\"]\n\n1. e4 *\n");
    assert_eq!(
        (game.variant, game.variant_status),
        (VariantKind::Other, VariantStatus::Unsupported)
    );
}

#[test]
fn conflicting_variants_are_ambiguous() {
    let game = game(b"[Variant \"Standard\"]\n[Variant \"Chess960\"]\n\n1. e4 *\n");
    assert_eq!(game.variant_status, VariantStatus::Ambiguous);
    assert!(has(
        &game,
        InterpretationDiagnosticCode::ConflictingVariantTags
    ));
}

#[test]
fn simple_rav_is_interpreted_without_changing_mainline() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 d5) e5 *\n");
    assert_eq!(game.lines.len(), 2);
    assert_eq!(
        game.moves
            .iter()
            .filter(|m| m.line_id == 0)
            .filter_map(|m| m.raw_san.as_deref())
            .collect::<Vec<_>>(),
        vec!["e4", "e5"]
    );
    assert_eq!(game.lines[1].replaces_move_id, Some(0));
}

#[test]
fn multiple_direct_ravs_create_distinct_lines() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4) (1. c4) e5 *\n");
    assert_eq!(game.lines.len(), 3);
    assert_eq!(game.lines[1].replaces_move_id, Some(0));
    assert_eq!(game.lines[2].replaces_move_id, Some(0));
}

#[test]
fn mainline_has_fixed_identity() {
    let game = game(b"[Event \"x\"]\n\n1. e4 *\n");
    let line = &game.lines[0];
    assert_eq!(
        (
            line.id,
            line.parent_line_id,
            line.variation_node_id,
            line.depth,
            line.replaces_move_id
        ),
        (0, None, None, 0, None)
    );
}

#[test]
fn rav_replaces_e4_from_initial_position() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 d5) e5 *\n");
    assert_eq!(game.lines[1].replaces_move_id, Some(0));
    assert_eq!(
        game.lines[1].start_position,
        game.moves[0].previous_position
    );
    assert_eq!(sans(&game, 1), vec!["d4", "d5"]);
}

#[test]
fn parent_and_rav_runtime_are_independent() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 d5) e5 *\n");
    assert_eq!(sans(&game, 0), vec!["e4", "e5"]);
    assert!(game
        .moves
        .iter()
        .all(|item| item.status == MoveInterpretationStatus::Applied));
}

#[test]
fn nested_rav_has_parent_and_depth() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 (1. c4 c5) d5) e5 *\n");
    assert_eq!(game.lines.len(), 3);
    assert_eq!(
        (game.lines[1].parent_line_id, game.lines[1].depth),
        (Some(0), 1)
    );
    assert_eq!(
        (game.lines[2].parent_line_id, game.lines[2].depth),
        (Some(1), 2)
    );
    assert!(game.lines[1].variation_node_id.is_some());
    assert!(game.lines[2].variation_node_id.is_some());
}

#[test]
fn line_and_move_ids_follow_depth_first_source_order() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 (1. c4) d5) e5 *\n");
    assert_eq!(
        game.lines.iter().map(|line| line.id).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(
        game.moves
            .iter()
            .map(|item| (item.id, item.line_id))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 1), (2, 2), (3, 1), (4, 0)]
    );
}

#[test]
fn line_ids_alone_follow_depth_first_order() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 (1. c4)) e5 *\n");
    assert_eq!(
        game.lines.iter().map(|l| l.id).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn move_ids_alone_follow_depth_first_order() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 (1. c4) d5) e5 *\n");
    assert_eq!(
        game.moves
            .iter()
            .map(|m| m.raw_san.as_deref().unwrap())
            .collect::<Vec<_>>(),
        vec!["e4", "d4", "c4", "d5", "e5"]
    );
}

#[test]
fn rav_metadata_identifies_parent_node_and_depth() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4) *\n");
    assert_eq!(game.lines[1].parent_line_id, Some(0));
    assert!(game.lines[1].variation_node_id.is_some());
    assert_eq!(game.lines[1].depth, 1);
}

#[test]
fn rav_after_applied_move_uses_pre_move_position() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4) *\n");
    assert_eq!(
        game.lines[1].start_position,
        game.moves[0].previous_position
    );
}

#[test]
fn move_ids_and_ordinals_are_local_to_lines() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 d5) e5 *\n");
    assert_eq!(
        game.moves
            .iter()
            .map(|m| (m.line_id, m.ordinal_in_line))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (1, 1), (0, 1)]
    );
    assert_eq!(game.lines[0].move_ids, vec![0, 3]);
    assert_eq!(game.lines[1].move_ids, vec![1, 2]);
}

#[test]
fn rav_after_invalid_san_uses_failed_moves_previous_position() {
    let game = game(b"[Event \"x\"]\n\n1. e4 e5 2. Qz9 (2. Nf3 Nc6) *\n");
    assert_eq!(game.moves[2].status, MoveInterpretationStatus::InvalidSan);
    assert_eq!(game.lines[1].replaces_move_id, Some(2));
    assert_eq!(
        game.lines[1].start_position,
        game.moves[2].previous_position
    );
    assert!(game.moves[3..]
        .iter()
        .all(|m| m.status == MoveInterpretationStatus::Applied));
}

#[test]
fn blocked_mainline_does_not_block_failed_move_rav() {
    let game = game(b"[Event \"x\"]\n\n1. e4 e5 2. Qz9 (2. Nf3 Nc6) Bb5 *\n");
    assert_eq!(
        game.moves
            .iter()
            .find(|m| m.raw_san.as_deref() == Some("Bb5"))
            .unwrap()
            .status,
        MoveInterpretationStatus::BlockedByPriorError
    );
    assert!(game
        .moves
        .iter()
        .filter(|m| m.line_id == 1)
        .all(|m| m.status == MoveInterpretationStatus::Applied));
}

#[test]
fn rav_after_ambiguous_san_remains_interpretable() {
    let game =
        game(b"[SetUp \"1\"]\n[FEN \"4k3/8/8/8/8/2N1N3/8/4K3 w - - 0 1\"]\n\n1. Nd5 (1. Nc4) *\n");
    assert_eq!(game.moves[0].status, MoveInterpretationStatus::AmbiguousSan);
    assert_eq!(game.moves[1].status, MoveInterpretationStatus::Applied);
}

#[test]
fn rav_after_illegal_move_remains_interpretable() {
    let game = game(b"[Event \"x\"]\n\n1. e5 (1. e4) *\n");
    assert_eq!(game.moves[0].status, MoveInterpretationStatus::IllegalMove);
    assert_eq!(game.moves[1].status, MoveInterpretationStatus::Applied);
}

#[test]
fn rav_after_blocked_move_has_unknown_start() {
    let game = game(b"[Event \"x\"]\n\n1. Qz9 e5 (1... Nc6) *\n");
    assert_eq!(
        game.moves[1].status,
        MoveInterpretationStatus::BlockedByPriorError
    );
    assert_eq!(game.lines[1].replaces_move_id, Some(1));
    assert!(game.lines[1].start_position.is_none());
    assert_eq!(game.lines[1].status, LineStatus::UnknownStartPosition);
    assert_eq!(
        game.moves[2].status,
        MoveInterpretationStatus::UnknownInitialPosition
    );
    assert!(game.moves[2].previous_position.is_none());
    assert!(has(
        &game,
        InterpretationDiagnosticCode::UnknownVariationStart
    ));
}

#[test]
fn rav_after_blocked_move_never_uses_last_known_position() {
    let game = game(b"[Event \"x\"]\n\n1. e4 Qz9 Nc6 (2. Nf3) *\n");
    assert!(game.lines[1].start_position.is_none());
    assert_ne!(
        game.moves[0].resulting_position,
        game.lines[1].start_position
    );
}

#[test]
fn rav_before_first_move_uses_parent_start() {
    let game = game(b"[Event \"x\"]\n\n(1. d4) 1. e4 *\n");
    assert_eq!(game.lines[1].replaces_move_id, None);
    assert_eq!(game.lines[1].start_position, game.lines[0].start_position);
    assert_eq!(game.moves[0].status, MoveInterpretationStatus::Applied);
    assert!(has(
        &game,
        InterpretationDiagnosticCode::VariationWithoutPrecedingMove
    ));
}

#[test]
fn rav_before_first_move_uses_fen_start() {
    let game = game(b"[SetUp \"1\"]\n[FEN \"7k/P7/8/8/8/8/8/7K w - - 0 1\"]\n\n(1. a8=Q+) *\n");
    assert_eq!(game.lines[1].start_position, game.initial_position);
    assert_eq!(game.moves[0].status, MoveInterpretationStatus::Applied);
}

#[test]
fn comments_and_nags_preserve_variation_anchor() {
    let game = game(b"[Event \"x\"]\n\n1. e4 {x} $1 (1. d4) e5 *\n");
    assert_eq!(game.lines[1].replaces_move_id, Some(0));
}

#[test]
fn move_numbers_are_isolated_between_parent_and_rav() {
    let game = game(b"[Event \"x\"]\n\n1. (1. d4) e4 (1... c5) e5 *\n");
    assert_eq!(
        game.moves
            .iter()
            .find(|m| m.line_id == 0)
            .unwrap()
            .move_number_node_ids
            .len(),
        1
    );
    assert_eq!(
        game.moves
            .iter()
            .find(|m| m.line_id == 1)
            .unwrap()
            .move_number_node_ids
            .len(),
        1
    );
    assert_eq!(
        game.moves
            .iter()
            .find(|m| m.line_id == 2)
            .unwrap()
            .move_number_node_ids
            .len(),
        1
    );
}

#[test]
fn parent_move_numbers_are_not_passed_to_rav() {
    let game = game(b"[Event \"x\"]\n\n1. (d4) e4 *\n");
    assert!(game
        .moves
        .iter()
        .find(|m| m.line_id == 1)
        .unwrap()
        .move_number_node_ids
        .is_empty());
    assert_eq!(
        game.moves
            .iter()
            .find(|m| m.line_id == 0)
            .unwrap()
            .move_number_node_ids
            .len(),
        1
    );
}

#[test]
fn rav_move_numbers_do_not_consume_parent_numbers() {
    let game = game(b"[Event \"x\"]\n\n1. (1. d4) e4 *\n");
    assert_eq!(
        game.moves
            .iter()
            .find(|m| m.line_id == 0)
            .unwrap()
            .move_number_node_ids
            .len(),
        1
    );
}

#[test]
fn result_inside_rav_does_not_close_parent() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 1-0 d5) e5 *\n");
    assert_eq!(
        game.moves.last().unwrap().status,
        MoveInterpretationStatus::Applied
    );
    assert_eq!(
        game.moves
            .iter()
            .find(|m| m.raw_san.as_deref() == Some("d5"))
            .unwrap()
            .status,
        MoveInterpretationStatus::AfterResult
    );
    assert_eq!(game.lines[1].result_node_ids.len(), 1);
}

#[test]
fn rav_does_not_change_parent_result_state() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 *) e5 *\n");
    assert_eq!(game.lines[0].result_node_ids.len(), 1);
    assert_eq!(game.lines[1].result_node_ids.len(), 1);
    assert_eq!(
        game.moves.last().unwrap().status,
        MoveInterpretationStatus::Applied
    );
}

#[test]
fn rav_after_result_is_unknown_and_local() {
    let game = game(b"[Event \"x\"]\n\n1. e4 1-0 (1... e5) Nf3\n");
    assert_eq!(game.lines[1].status, LineStatus::UnknownStartPosition);
    assert!(game.lines[1].start_position.is_none());
    assert_eq!(
        game.moves.iter().find(|m| m.line_id == 1).unwrap().status,
        MoveInterpretationStatus::UnknownInitialPosition
    );
    assert_eq!(
        game.moves
            .iter()
            .find(|m| m.line_id == 0 && m.raw_san.as_deref() == Some("Nf3"))
            .unwrap()
            .status,
        MoveInterpretationStatus::AfterResult
    );
    assert!(has(
        &game,
        InterpretationDiagnosticCode::VariationAfterResult
    ));
}

#[test]
fn moves_after_result_are_local_to_the_closed_line() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 1-0 d5) e5 *\n");
    assert_eq!(
        game.moves
            .iter()
            .find(|m| m.raw_san.as_deref() == Some("d5"))
            .unwrap()
            .status,
        MoveInterpretationStatus::AfterResult
    );
    assert_eq!(
        game.moves
            .iter()
            .find(|m| m.raw_san.as_deref() == Some("e5"))
            .unwrap()
            .status,
        MoveInterpretationStatus::Applied
    );
}

#[test]
fn empty_variation_is_preserved_as_partial_line() {
    let game = game(b"[Event \"x\"]\n\n1. e4 () e5 *\n");
    assert_eq!(game.lines.len(), 2);
    assert_eq!(game.lines[1].status, LineStatus::Partial);
    assert!(has(&game, InterpretationDiagnosticCode::EmptyVariation));
    assert!(!has(
        &game,
        InterpretationDiagnosticCode::UnexpectedVariationStructure
    ));
}

#[test]
fn variation_without_movetext_is_unexpected_not_empty() {
    let game = structurally_mutated_variation_game(|tree, variation_id| {
        tree.nodes
            .iter_mut()
            .find(|node| {
                node.parent_id == Some(variation_id)
                    && node.kind == crate::inspector::parsing::model::NodeKind::Movetext
            })
            .expect("movetext")
            .kind = crate::inspector::parsing::model::NodeKind::Trivia;
    });
    assert_eq!(game.lines[1].status, LineStatus::UnexpectedStructure);
    assert_eq!(
        line_diagnostic_count(
            &game,
            1,
            InterpretationDiagnosticCode::UnexpectedVariationStructure
        ),
        1
    );
    assert!(!has(&game, InterpretationDiagnosticCode::EmptyVariation));
    assert!(game.lines[1].move_ids.is_empty());
}

#[test]
fn variation_with_two_movetexts_is_not_interpreted() {
    let game = structurally_mutated_variation_game(|tree, variation_id| {
        let mut duplicate = tree
            .nodes
            .iter()
            .find(|node| {
                node.parent_id == Some(variation_id)
                    && node.kind == crate::inspector::parsing::model::NodeKind::Movetext
            })
            .expect("movetext")
            .clone();
        duplicate.id = tree.nodes.len() as u64;
        duplicate.child_ordinal = Some(u64::MAX);
        tree.nodes.push(duplicate);
    });
    assert_eq!(game.lines[1].status, LineStatus::UnexpectedStructure);
    assert_eq!(
        line_diagnostic_count(
            &game,
            1,
            InterpretationDiagnosticCode::UnexpectedVariationStructure
        ),
        1
    );
    assert!(game.lines[1].move_ids.is_empty());
    assert!(!has(&game, InterpretationDiagnosticCode::EmptyVariation));
}

#[test]
fn variation_with_one_movetext_and_unexpected_child_recovers_movetext_only() {
    let game = structurally_mutated_variation_game(|tree, variation_id| {
        let mut unexpected = tree
            .nodes
            .iter()
            .find(|node| {
                node.kind == crate::inspector::parsing::model::NodeKind::MoveSymbolCandidate
                    && node.parent_id != Some(variation_id)
            })
            .expect("move")
            .clone();
        unexpected.id = tree.nodes.len() as u64;
        unexpected.parent_id = Some(variation_id);
        unexpected.child_ordinal = Some(u64::MAX);
        tree.nodes.push(unexpected);
    });
    assert_eq!(game.lines[1].status, LineStatus::UnexpectedStructure);
    assert_eq!(
        line_diagnostic_count(
            &game,
            1,
            InterpretationDiagnosticCode::UnexpectedVariationStructure
        ),
        1
    );
    assert_eq!(sans(&game, 1), vec!["d4"]);
    assert_eq!(game.lines[1].move_ids.len(), 1);
}

#[test]
fn move_diagnostics_identify_their_line() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. Qz9) *\n");
    let failed = game.moves.iter().find(|m| m.line_id == 1).unwrap();
    let diagnostic = game
        .diagnostics
        .iter()
        .find(|d| d.move_id == Some(failed.id))
        .unwrap();
    assert_eq!(diagnostic.line_id, Some(1));
}

#[test]
fn numeric_castling_inside_rav_uses_rules_adapter() {
    let game = game(
        b"[SetUp \"1\"]\n[FEN \"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1\"]\n\n1. O-O-O (1. 0-0) *\n",
    );
    let castle = game.moves.iter().find(|m| m.line_id == 1).unwrap();
    assert_eq!(castle.normalized.as_ref().unwrap().canonical_san, "O-O");
    assert_eq!(castle.source_was_canonical_san, Some(false));
}

#[test]
fn rav_uses_explicit_fen_position() {
    let game =
        game(b"[SetUp \"1\"]\n[FEN \"7k/P7/8/8/8/8/8/7K w - - 0 1\"]\n\n1. a8=Q+ (1. a8=R+) *\n");
    assert_eq!(game.lines[1].start_position, game.initial_position);
    assert!(game
        .moves
        .iter()
        .all(|m| m.status == MoveInterpretationStatus::Applied));
}

#[test]
fn unsupported_variant_preserves_lines_without_standard_moves() {
    let game = game(b"[Variant \"Chess960\"]\n\n1. e4 (1. d4) *\n");
    assert!(game
        .lines
        .iter()
        .all(|l| l.status == LineStatus::UnsupportedVariant));
    assert!(game
        .moves
        .iter()
        .all(|m| m.status == MoveInterpretationStatus::UnsupportedVariant));
}

#[test]
fn every_move_belongs_to_exactly_one_consistent_line() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 (1. c4) d5) e5 *\n");
    let mut seen = vec![0_u8; game.moves.len()];
    for line in &game.lines {
        for &id in &line.move_ids {
            seen[id as usize] += 1;
            assert_eq!(game.moves[id as usize].line_id, line.id);
        }
    }
    assert!(seen.into_iter().all(|count| count == 1));
}

#[test]
fn every_line_move_id_refers_to_an_existing_move() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 (1. c4) d5) e5 *\n");
    assert!(game
        .lines
        .iter()
        .flat_map(|l| &l.move_ids)
        .all(|id| game.moves.get(*id as usize).is_some()));
}

#[test]
fn nested_and_incomplete_variations_do_not_panic() {
    let game = game(b"[Event \"x\"]\n\n1. e4 (1. d4 (1. c4\n");
    assert!(game.lines.len() >= 2);
}

#[test]
fn maximum_parser_variation_depth_is_interpreted() {
    let mut pgn = b"[Event \"x\"]\n\n1. e4 ".to_vec();
    pgn.extend(std::iter::repeat_n(
        b'(',
        crate::inspector::parsing::model::MAX_VARIATION_DEPTH,
    ));
    pgn.extend_from_slice(b"1. d4");
    pgn.extend(std::iter::repeat_n(
        b')',
        crate::inspector::parsing::model::MAX_VARIATION_DEPTH,
    ));
    let game = game(&pgn);
    assert_eq!(
        game.lines.len(),
        crate::inspector::parsing::model::MAX_VARIATION_DEPTH + 1
    );
    assert_eq!(
        game.lines.last().unwrap().depth as usize,
        crate::inspector::parsing::model::MAX_VARIATION_DEPTH
    );
}

#[test]
fn comments_and_nags_do_not_change_mainline() {
    let game = game(b"[Event \"x\"]\n\n1. e4 $1 {note} e5 *\n");
    assert_eq!(game.moves.len(), 2);
    assert!(game
        .moves
        .iter()
        .all(|m| m.status == MoveInterpretationStatus::Applied));
}

#[test]
fn invalid_san_blocks_following_moves() {
    let game = game(b"[Event \"x\"]\n\n1. nope e5 Nf3 *\n");
    assert_eq!(game.moves[0].status, MoveInterpretationStatus::InvalidSan);
    assert!(game.moves[1..]
        .iter()
        .all(|m| m.status == MoveInterpretationStatus::BlockedByPriorError));
    assert!(game
        .moves
        .iter()
        .all(|m| m.source_was_canonical_san.is_none()));
}

#[test]
fn blocked_move_has_no_attributed_position_or_normalization() {
    let game = game(b"[Event \"x\"]\n\n1. e4 e5 2. Qz9 Nc6 *\n");
    let failed = &game.moves[2];
    let blocked = &game.moves[3];
    assert_eq!(failed.status, MoveInterpretationStatus::InvalidSan);
    assert!(failed.previous_position.is_some());
    assert!(failed.resulting_position.is_none());
    assert_eq!(
        blocked.status,
        MoveInterpretationStatus::BlockedByPriorError
    );
    assert!(blocked.previous_position.is_none());
    assert!(blocked.resulting_position.is_none());
    assert!(blocked.normalized.is_none());
    assert!(blocked.source_was_canonical_san.is_none());
}

#[test]
fn ambiguous_san_is_reported() {
    let game = game(b"[SetUp \"1\"]\n[FEN \"4k3/8/8/8/8/2N1N3/8/4K3 w - - 0 1\"]\n\n1. Nd5 *\n");
    assert_eq!(game.moves[0].status, MoveInterpretationStatus::AmbiguousSan);
}

#[test]
fn illegal_move_is_reported() {
    let game = game(b"[Event \"x\"]\n\n1. e5 *\n");
    assert_eq!(game.moves[0].status, MoveInterpretationStatus::IllegalMove);
}

#[test]
fn non_ascii_move_bytes_are_invalid_encoding() {
    use crate::inspector::parsing::model::{FlatNode, NodeKind, NodeStatus, ParseTree};
    use crate::inspector::tokenization::{Token, TokenKind, TokenStream};
    let blob = b"e\xff";
    let stream = TokenStream {
        segment_ordinal: 0,
        start: 0,
        end: 2,
        tokens: vec![Token {
            ordinal: 0,
            start: 0,
            end: 2,
            kind: TokenKind::Symbol,
        }],
    };
    let tree = ParseTree {
        segment_ordinal: 0,
        token_count: 1,
        diagnostics: vec![],
        nodes: vec![
            FlatNode {
                id: 0,
                parent_id: None,
                child_ordinal: None,
                kind: NodeKind::GameCandidate,
                status: NodeStatus::Valid,
                start_token: 0,
                end_token: 1,
            },
            FlatNode {
                id: 1,
                parent_id: Some(0),
                child_ordinal: Some(0),
                kind: NodeKind::Movetext,
                status: NodeStatus::Valid,
                start_token: 0,
                end_token: 1,
            },
            FlatNode {
                id: 2,
                parent_id: Some(1),
                child_ordinal: Some(0),
                kind: NodeKind::MoveSymbolCandidate,
                status: NodeStatus::Valid,
                start_token: 0,
                end_token: 1,
            },
        ],
    };
    let game = super::lines::interpret_line_trees(&[tree], &[stream], blob).remove(0);
    assert_eq!(
        game.moves[0].status,
        MoveInterpretationStatus::InvalidEncoding
    );
}

#[test]
fn move_after_result_is_recorded_not_applied() {
    let game = game(b"[Event \"x\"]\n\n1. e4 1-0 e5\n");
    assert_eq!(game.moves[1].status, MoveInterpretationStatus::AfterResult);
    assert!(has(&game, InterpretationDiagnosticCode::MoveAfterResult));
}

#[test]
fn move_numbers_are_attached_to_next_move() {
    let game = game(b"[Event \"x\"]\n\n1. e4 1... e5 2. Nf3 *\n");
    assert_eq!(
        game.moves
            .iter()
            .map(|m| m.move_number_node_ids.len())
            .collect::<Vec<_>>(),
        vec![1, 1, 1]
    );
}

#[test]
fn numeric_castling_is_delegated_to_chess_rules() {
    let game = game(b"[SetUp \"1\"]\n[FEN \"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1\"]\n\n1. 0-0 *\n");
    let normalized = game.moves[0].normalized.as_ref().expect("applied");
    assert_eq!(normalized.canonical_san, "O-O");
    assert_eq!(normalized.uci, "e1g1");
    assert_eq!(game.moves[0].source_was_canonical_san, Some(false));
}

#[test]
fn canonical_castling_preserves_source_canonicality() {
    let game = game(b"[SetUp \"1\"]\n[FEN \"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1\"]\n\n1. O-O *\n");
    assert_eq!(game.moves[0].source_was_canonical_san, Some(true));
}

#[test]
fn node_range_errors_distinguish_tokens_from_bytes() {
    use super::lines::{node_bytes, range_diagnostic_code, NodeRangeError};
    use crate::inspector::parsing::model::{FlatNode, NodeKind, NodeStatus};
    use crate::inspector::tokenization::{Token, TokenKind, TokenStream};
    let node = FlatNode {
        id: 0,
        parent_id: None,
        child_ordinal: None,
        kind: NodeKind::MoveSymbolCandidate,
        status: NodeStatus::Valid,
        start_token: 0,
        end_token: 1,
    };
    assert_eq!(
        node_bytes(&node, None, b"e4").unwrap_err(),
        NodeRangeError::MissingTokenStream
    );
    let bad_tokens = FlatNode {
        end_token: 2,
        ..node.clone()
    };
    let stream = TokenStream {
        segment_ordinal: 0,
        start: 0,
        end: 2,
        tokens: vec![Token {
            ordinal: 0,
            start: 0,
            end: 2,
            kind: TokenKind::Symbol,
        }],
    };
    assert_eq!(
        node_bytes(&bad_tokens, Some(&stream), b"e4").unwrap_err(),
        NodeRangeError::InvalidTokenRange
    );
    let bad_bytes = TokenStream {
        segment_ordinal: 0,
        start: 0,
        end: 3,
        tokens: vec![Token {
            ordinal: 0,
            start: 0,
            end: 3,
            kind: TokenKind::Symbol,
        }],
    };
    assert_eq!(
        node_bytes(&node, Some(&bad_bytes), b"e4").unwrap_err(),
        NodeRangeError::InvalidByteRange
    );
    assert_eq!(
        range_diagnostic_code(NodeRangeError::InvalidTokenRange),
        InterpretationDiagnosticCode::InvalidNodeTokenRange
    );
    assert_eq!(
        range_diagnostic_code(NodeRangeError::InvalidByteRange),
        InterpretationDiagnosticCode::InvalidNodeByteRange
    );
}

#[test]
fn quoted_tag_decoder_accepts_only_explicit_escapes() {
    use super::lines::decode_quoted_tag;
    assert_eq!(decode_quoted_tag(br#""A\"B""#), Ok("A\"B".to_owned()));
    assert_eq!(decode_quoted_tag(br#""A\\B""#), Ok("A\\B".to_owned()));
    assert_eq!(
        decode_quoted_tag(br#""A\nB""#),
        Err(TagDecodeStatus::InvalidEscape)
    );
}

#[test]
fn quoted_tag_decoder_rejects_missing_closing_quote() {
    use super::lines::decode_quoted_tag;
    assert_eq!(
        decode_quoted_tag(b"\"abc"),
        Err(TagDecodeStatus::MissingValue)
    );
    assert_eq!(
        decode_quoted_tag(b"\"abc\\\""),
        Err(TagDecodeStatus::MissingValue)
    );
}

#[test]
fn non_ascii_tag_name_is_invalid_encoding() {
    use super::lines::decode_tag_name;
    use crate::inspector::parsing::model::{FlatNode, NodeKind, NodeStatus, ParseTree};
    use crate::inspector::tokenization::{Token, TokenKind, TokenStream};
    assert_eq!(
        decode_tag_name(b"F\xffN"),
        Err(TagDecodeStatus::InvalidEncoding)
    );
    let blob = b"[\xff \"x\"]";
    let kinds = [
        TokenKind::LeftBracket,
        TokenKind::Symbol,
        TokenKind::Whitespace,
        TokenKind::QuotedString,
        TokenKind::RightBracket,
    ];
    let bounds = [(0, 1), (1, 2), (2, 3), (3, 6), (6, 7)];
    let stream = TokenStream {
        segment_ordinal: 0,
        start: 0,
        end: 7,
        tokens: kinds
            .into_iter()
            .zip(bounds)
            .enumerate()
            .map(|(ordinal, (kind, (start, end)))| Token {
                ordinal: ordinal as u64,
                start,
                end,
                kind,
            })
            .collect(),
    };
    let leaf = |id, child_ordinal, kind, start_token, end_token| FlatNode {
        id,
        parent_id: Some(2),
        child_ordinal: Some(child_ordinal),
        kind,
        status: NodeStatus::Valid,
        start_token,
        end_token,
    };
    let tree = ParseTree {
        segment_ordinal: 0,
        token_count: 5,
        diagnostics: vec![],
        nodes: vec![
            FlatNode {
                id: 0,
                parent_id: None,
                child_ordinal: None,
                kind: NodeKind::GameCandidate,
                status: NodeStatus::Valid,
                start_token: 0,
                end_token: 5,
            },
            FlatNode {
                id: 1,
                parent_id: Some(0),
                child_ordinal: Some(0),
                kind: NodeKind::TagSection,
                status: NodeStatus::Valid,
                start_token: 0,
                end_token: 5,
            },
            FlatNode {
                id: 2,
                parent_id: Some(1),
                child_ordinal: Some(0),
                kind: NodeKind::TagPair,
                status: NodeStatus::Valid,
                start_token: 0,
                end_token: 5,
            },
            leaf(3, 0, NodeKind::Delimiter, 0, 1),
            leaf(4, 1, NodeKind::TagNameCandidate, 1, 2),
            leaf(5, 2, NodeKind::Trivia, 2, 3),
            leaf(6, 3, NodeKind::TagValueCandidate, 3, 4),
            leaf(7, 4, NodeKind::Delimiter, 4, 5),
        ],
    };
    let game = super::lines::interpret_line_trees(&[tree], &[stream], blob).remove(0);
    assert!(has(&game, InterpretationDiagnosticCode::InvalidTagEncoding));
}

#[test]
fn game_without_moves_can_be_complete() {
    let game = game(b"[Event \"x\"]\n\n*\n");
    assert!(game.moves.is_empty());
    assert_eq!(game.status, GameInterpretationStatus::Complete);
}

#[test]
fn corrupted_node_token_range_is_rejected() {
    let fixture = fixture(b"[Event \"x\"]\n\n1. e4 *\n");
    let path = fixture.root.join(&fixture.parse);
    let mut json: serde_json::Value = serde_json::from_slice(
        &crate::inspector::common::logical_json_bytes(&path, "parse test").expect("read"),
    )
    .expect("json");
    json["trees"][0]["nodes"][1]["end_token"] = serde_json::Value::from(999_u64);
    fs::write(path, serde_json::to_vec(&json).expect("json")).expect("write");
    assert!(interpret_lines(&fixture.root, &fixture.parse).is_err());
}

#[test]
fn altered_parse_is_rejected() {
    let fixture = fixture(b"[Event \"x\"]\n\n1. e4 *\n");
    let path = fixture.root.join(&fixture.parse);
    let mut json: serde_json::Value = serde_json::from_slice(
        &crate::inspector::common::logical_json_bytes(&path, "parse test").expect("read"),
    )
    .expect("json");
    json["trees"][0]["nodes"][0]["status"] = serde_json::Value::String("recovered".to_owned());
    fs::write(path, serde_json::to_vec(&json).expect("json")).expect("write");
    assert!(interpret_lines(&fixture.root, &fixture.parse).is_err());
}

#[test]
fn altered_tokenization_is_rejected() {
    let fixture = fixture(b"[Event \"x\"]\n\n1. e4 *\n");
    let path = fixture.root.join(&fixture.tokenization);
    let mut bytes = fs::read(&path).expect("read");
    bytes.push(b' ');
    fs::write(path, bytes).expect("write");
    assert!(interpret_lines(&fixture.root, &fixture.parse).is_err());
}

#[test]
fn altered_source_blob_is_rejected() {
    let fixture = fixture(b"[Event \"x\"]\n\n1. e4 *\n");
    fs::write(&fixture.object, b"altered").expect("alter");
    assert!(interpret_lines(&fixture.root, &fixture.parse).is_err());
}

#[cfg(unix)]
#[test]
fn traversal_and_external_symlink_are_rejected() {
    use std::os::unix::fs::symlink;
    let fixture = fixture(b"[Event \"x\"]\n\n1. e4 *\n");
    assert!(interpret_lines(&fixture.root, Path::new("../outside")).is_err());
    let outside = fixture.root.with_extension("outside.json");
    fs::write(
        &outside,
        fs::read(fixture.root.join(&fixture.parse)).expect("read"),
    )
    .expect("outside");
    let link = fixture.root.join("parse-link.json");
    symlink(&outside, &link).expect("link");
    assert!(interpret_lines(&fixture.root, Path::new("parse-link.json")).is_err());
    fs::remove_file(outside).expect("cleanup");
}

#[test]
fn identical_interpretations_are_deterministic() {
    let fixture = fixture(b"[Event \"x\"]\n\n1. e4 (1. d4 (1. c4) d5) e5 *\n");
    assert_eq!(
        interpret_lines(&fixture.root, &fixture.parse).expect("first"),
        interpret_lines(&fixture.root, &fixture.parse).expect("second")
    );
}

#[test]
#[ignore = "real pilot requires the local PGN"]
fn manual_real_pilot_line_interpretation() {
    let source_path = real_pilot_pgn();
    let root = test_root("real");
    fs::create_dir_all(&root).expect("workspace");
    let imported = source::import_source(&root, &source_path).expect("import");
    assert_eq!(
        imported.manifest.blake3,
        "913d6ee7cf8a6dece754dfd70b5471b531466b90880593a8c010a659d2f3e7fb"
    );
    assert_eq!(imported.manifest.bytes, 40990);
    let segmented = segmentation::segment_source(&root, &imported.manifest_path).expect("segment");
    let tokenized = tokenization::tokenize(&root, &segmented.path).expect("tokenize");
    let parsed = parsing::parse(&root, &tokenized.path).expect("parse");
    let result = interpret_lines(&root, &parsed.path).expect("interpret");
    let complete = result
        .games
        .iter()
        .filter(|g| g.status == GameInterpretationStatus::Complete)
        .count();
    let partial = result
        .games
        .iter()
        .filter(|g| {
            matches!(
                g.status,
                GameInterpretationStatus::Partial | GameInterpretationStatus::UnexpectedStructure
            )
        })
        .count();
    let unsupported = result
        .games
        .iter()
        .filter(|g| g.status == GameInterpretationStatus::UnsupportedVariant)
        .count();
    let applied = result
        .games
        .iter()
        .flat_map(|g| &g.moves)
        .filter(|m| m.status == MoveInterpretationStatus::Applied)
        .count();
    let failed = result
        .games
        .iter()
        .flat_map(|g| &g.moves)
        .filter(|m| {
            matches!(
                m.status,
                MoveInterpretationStatus::InvalidEncoding
                    | MoveInterpretationStatus::InvalidSan
                    | MoveInterpretationStatus::AmbiguousSan
                    | MoveInterpretationStatus::IllegalMove
            )
        })
        .count();
    let blocked = result
        .games
        .iter()
        .flat_map(|g| &g.moves)
        .filter(|m| m.status == MoveInterpretationStatus::BlockedByPriorError)
        .count();
    let lines: usize = result.games.iter().map(|g| g.lines.len()).sum();
    let variation_lines = lines.saturating_sub(result.games.len());
    let complete_lines = result
        .games
        .iter()
        .flat_map(|g| &g.lines)
        .filter(|l| l.status == LineStatus::Complete)
        .count();
    let partial_lines = result
        .games
        .iter()
        .flat_map(|g| &g.lines)
        .filter(|l| l.status == LineStatus::Partial)
        .count();
    let unknown_start_lines = result
        .games
        .iter()
        .flat_map(|g| &g.lines)
        .filter(|l| l.status == LineStatus::UnknownStartPosition)
        .count();
    let diagnostics: usize = result.games.iter().map(|g| g.diagnostics.len()).sum();
    assert!(!result.games.is_empty());
    assert!(result.games.iter().all(|game| !game.lines.is_empty()));
    assert!(applied > 0);
    println!("games={} lines={} variation_lines={} complete_games={} partial_games={} complete_lines={} partial_lines={} unknown_start_lines={} applied_moves={} failed_moves={} blocked_moves={} diagnostics={}", result.games.len(), lines, variation_lines, complete, partial + unsupported, complete_lines, partial_lines, unknown_start_lines, applied, failed, blocked, diagnostics);
    fs::remove_dir_all(root).expect("cleanup");
}
