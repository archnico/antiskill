use super::model::*;
use super::parser::parse_stream;
use super::validation::{is_container, validate_parse_manifest, validate_tree};
use super::*;
use crate::inspector::segmentation::{load_segmentation_manifest, segment_source};
use crate::inspector::source::import_source;
use crate::inspector::tokenization::{tokenize, Token, TokenKind, TokenStream};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    source_manifest: PathBuf,
    segmentation: PathBuf,
    tokenization: PathBuf,
    parse: PathBuf,
}

fn test_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("orologio precedente a UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!("antiskill-parsing-{}-{nonce}", std::process::id()))
}

fn make_fixture(bytes: &[u8]) -> Fixture {
    let root = test_root();
    fs::create_dir_all(&root).expect("workspace");
    let source = root.join("source.pgn");
    fs::write(&source, bytes).expect("sorgente");
    let imported = import_source(&root, &source).expect("import");
    let segmented = segment_source(&root, &imported.manifest_path).expect("segment");
    let tokenized = tokenize(&root, &segmented.path).expect("tokenize");
    let parsed = parse(&root, &tokenized.path).expect("parse");
    Fixture {
        root,
        source_manifest: imported.manifest_path,
        segmentation: segmented.path,
        tokenization: tokenized.path,
        parse: parsed.path,
    }
}

fn stream(kinds: &[TokenKind]) -> TokenStream {
    TokenStream {
        segment_ordinal: 3,
        start: 100,
        end: 100 + kinds.len() as u64,
        tokens: kinds
            .iter()
            .enumerate()
            .map(|(ordinal, kind)| Token {
                ordinal: ordinal as u64,
                start: 100 + ordinal as u64,
                end: 101 + ordinal as u64,
                kind: *kind,
            })
            .collect(),
    }
}

fn tree(kinds: &[TokenKind]) -> ParseTree {
    parse_stream(&stream(kinds)).expect("parse stream")
}

fn nodes(tree: &ParseTree, kind: NodeKind) -> Vec<&FlatNode> {
    tree.nodes.iter().filter(|node| node.kind == kind).collect()
}

fn has_diagnostic(tree: &ParseTree, code: DiagnosticCode) -> bool {
    tree.diagnostics.iter().any(|item| item.code == code)
}

fn assert_exact_coverage(tree: &ParseTree) {
    validate_tree(tree).expect("albero valido");
    let leaves = tree
        .nodes
        .iter()
        .filter(|node| {
            !tree
                .nodes
                .iter()
                .any(|other| other.parent_id == Some(node.id))
        })
        .collect::<Vec<_>>();
    let mut cursor = 0;
    for leaf in leaves {
        assert_eq!(leaf.start_token, cursor);
        cursor = leaf.end_token;
    }
    assert_eq!(cursor, tree.token_count);
}

fn maximum_depths(tree: &ParseTree) -> (usize, usize) {
    let mut depths = vec![0; tree.nodes.len()];
    let mut variation_depths = vec![0; tree.nodes.len()];
    for node in tree.nodes.iter().skip(1) {
        let parent = node.parent_id.expect("parent") as usize;
        depths[node.id as usize] = depths[parent] + 1;
        variation_depths[node.id as usize] =
            variation_depths[parent] + usize::from(node.kind == NodeKind::Variation);
    }
    (
        depths.into_iter().max().unwrap_or(0),
        variation_depths.into_iter().max().unwrap_or(0),
    )
}

fn read_parse(root: &Path, path: &Path) -> ParseManifest {
    serde_json::from_slice(
        &crate::inspector::common::logical_json_bytes(&root.join(path), "test parse")
            .expect("lettura parse"),
    )
    .expect("deserializzazione parse")
}

fn write_parse(root: &Path, path: &Path, manifest: &ParseManifest) {
    crate::inspector::common::replace_logical_json(
        &root.join(path),
        &serde_json::to_vec_pretty(manifest).expect("serializzazione parse"),
    );
}

#[test]
fn streaming_parse_is_byte_identical_to_legacy_v1() {
    let cases: &[&[u8]] = &[
        b"[Event \"One\"]\n\n1. e4 *\n",
        b"[Event \"One\"]\n\n1. e4 *\n\n[Event \"Two\"]\n\n1. d4 d5 *\n",
        b"[Event \"Rav\"]\n\n1. e4 (1. d4 (1... d5) 2. c4) e5 *\n",
        b"[Event \"Recovery\"]\n\n1. e4 () [Broken \"x\"] @ *\n",
        b"[Event \"Incomplete\"]\n\n1. e4 (1... e5 {open\n",
    ];
    for bytes in cases {
        let fixture = make_fixture(bytes);
        let actual = crate::inspector::common::logical_json_bytes(
            &fixture.root.join(&fixture.parse),
            "parse streaming test",
        )
        .expect("parse streaming");
        let manifest: ParseManifest = serde_json::from_slice(&actual).expect("manifest parse v1");
        let mut legacy = serde_json::to_vec_pretty(&manifest).expect("serializzazione legacy");
        legacy.push(b'\n');
        assert_eq!(actual, legacy);
        assert_eq!(blake3::hash(&actual), blake3::hash(&legacy));
        fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    }
}

#[test]
fn streaming_tokenization_preserves_order_and_does_not_load_source_blob() {
    let fixture =
        make_fixture(b"[Event \"One\"]\n\n1. e4 *\n\n[Event \"Two\"]\n\n1. d4 (1... d5) *\n");
    let loaded = crate::inspector::tokenization::load_streaming_tokenization(
        &fixture.root,
        &fixture.tokenization,
        None,
    )
    .expect("tokenizzazione streaming");
    let mut ordinals = Vec::new();
    let count = loaded
        .for_each_stream(|stream| {
            ordinals.push(stream.segment_ordinal);
            Ok(())
        })
        .expect("stream verificati");
    assert_eq!(count, 2);
    assert_eq!(ordinals.len(), 2);
    assert!(ordinals[0] < ordinals[1]);
    fs::remove_dir_all(fixture.root).expect("pulizia workspace");
}

#[test]
fn streaming_parse_rejects_truncated_and_mismatched_tokenization() {
    let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
    let tokenization_path = fixture.root.join(&fixture.tokenization);
    let logical = crate::inspector::common::logical_json_bytes(&tokenization_path, "test")
        .expect("tokenizzazione");
    let before = fs::read_dir(fixture.root.join("inspector/parses"))
        .expect("directory parse")
        .count();
    crate::inspector::common::replace_logical_json(
        &tokenization_path,
        &logical[..logical.len() / 2],
    );
    assert!(parse(&fixture.root, &fixture.tokenization).is_err());
    assert_eq!(
        fs::read_dir(fixture.root.join("inspector/parses"))
            .expect("directory parse")
            .count(),
        before
    );

    crate::inspector::common::replace_logical_json(&tokenization_path, &logical);
    let identity = crate::inspector::tokenization::load_streaming_tokenization(
        &fixture.root,
        &fixture.tokenization,
        Some((&"0".repeat(64), logical.len() as u64)),
    );
    assert!(identity.is_err());
    fs::remove_dir_all(fixture.root).expect("pulizia workspace");
}

#[test]
fn streaming_parse_rejects_stream_segmentation_and_source_mismatches() {
    let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
    let tokenization_path = fixture.root.join(&fixture.tokenization);
    let logical = crate::inspector::common::logical_json_bytes(&tokenization_path, "test")
        .expect("tokenizzazione");
    let mut value: serde_json::Value =
        serde_json::from_slice(&logical).expect("tokenizzazione JSON");
    value["streams"][0]["segment_ordinal"] = serde_json::Value::from(99);
    let mut changed = serde_json::to_vec_pretty(&value).expect("tokenizzazione modificata");
    changed.push(b'\n');
    crate::inspector::common::replace_logical_json(&tokenization_path, &changed);
    assert!(parse(&fixture.root, &fixture.tokenization).is_err());

    crate::inspector::common::replace_logical_json(&tokenization_path, &logical);
    let segmentation = load_segmentation_manifest(&fixture.root, &fixture.segmentation, None)
        .expect("segmentazione");
    fs::write(&segmentation.object_path, b"source changed").expect("corrompi sorgente");
    assert!(parse(&fixture.root, &fixture.tokenization).is_err());
    fs::remove_dir_all(fixture.root).expect("pulizia workspace");
}

#[test]
fn streaming_parse_rejects_skipped_and_duplicated_streams() {
    for mutation in ["skip", "duplicate"] {
        let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n\n[Event \"Two\"]\n\n1. d4 *\n");
        let tokenization_path = fixture.root.join(&fixture.tokenization);
        let logical = crate::inspector::common::logical_json_bytes(&tokenization_path, "test")
            .expect("tokenizzazione");
        let mut value: serde_json::Value =
            serde_json::from_slice(&logical).expect("tokenizzazione JSON");
        let streams = value["streams"].as_array_mut().expect("stream array");
        if mutation == "skip" {
            streams.remove(0);
        } else {
            streams.push(streams[0].clone());
        }
        let mut changed = serde_json::to_vec_pretty(&value).expect("tokenizzazione modificata");
        changed.push(b'\n');
        crate::inspector::common::replace_logical_json(&tokenization_path, &changed);
        assert!(
            parse(&fixture.root, &fixture.tokenization).is_err(),
            "{mutation}"
        );
        fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    }
}

#[cfg(unix)]
#[test]
fn streaming_parse_rejects_external_tokenization_symlink() {
    use std::os::unix::fs::symlink;

    let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
    let outside = test_root();
    fs::create_dir_all(&outside).expect("directory esterna");
    let external = outside.join("tokenization.json.xz");
    fs::copy(fixture.root.join(&fixture.tokenization), &external).expect("copia tokenizzazione");
    let link = fixture.root.join("tokenization-link.json.xz");
    symlink(&external, &link).expect("symlink tokenizzazione");
    assert!(parse(&fixture.root, &link).is_err());
    fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    fs::remove_dir_all(outside).expect("pulizia esterna");
}

#[test]
fn parses_valid_tag_pairs_and_deterministic_movetext_boundary() {
    let kinds = [
        TokenKind::LeftBracket,
        TokenKind::Symbol,
        TokenKind::Whitespace,
        TokenKind::QuotedString,
        TokenKind::RightBracket,
        TokenKind::Whitespace,
        TokenKind::LeftBracket,
        TokenKind::Whitespace,
        TokenKind::Symbol,
        TokenKind::Whitespace,
        TokenKind::QuotedString,
        TokenKind::Whitespace,
        TokenKind::RightBracket,
        TokenKind::Whitespace,
        TokenKind::MoveNumberCandidate,
        TokenKind::Whitespace,
        TokenKind::Symbol,
    ];
    let parsed = tree(&kinds);
    assert_eq!(nodes(&parsed, NodeKind::TagPair).len(), 2);
    let tag_section = nodes(&parsed, NodeKind::TagSection)[0];
    let movetext = nodes(&parsed, NodeKind::Movetext)[0];
    assert_eq!(tag_section.end_token, 13);
    assert_eq!(movetext.start_token, 13);
    assert_eq!(nodes(&parsed, NodeKind::TagNameCandidate).len(), 2);
    assert_eq!(nodes(&parsed, NodeKind::TagValueCandidate).len(), 2);
    validate_tree(&parsed).expect("albero valido");
}

#[test]
fn parses_prudent_incomplete_and_malformed_tags() {
    let unterminated_value = tree(&[
        TokenKind::LeftBracket,
        TokenKind::Symbol,
        TokenKind::Whitespace,
        TokenKind::UnterminatedQuotedString,
    ]);
    assert!(has_diagnostic(
        &unterminated_value,
        DiagnosticCode::UnterminatedTagValue
    ));
    assert_eq!(unterminated_value.nodes[0].status, NodeStatus::Incomplete);

    let missing_bracket = tree(&[
        TokenKind::LeftBracket,
        TokenKind::Symbol,
        TokenKind::Whitespace,
        TokenKind::QuotedString,
        TokenKind::Whitespace,
    ]);
    assert!(has_diagnostic(
        &missing_bracket,
        DiagnosticCode::UnterminatedTagPair
    ));

    let malformed = tree(&[
        TokenKind::LeftBracket,
        TokenKind::Symbol,
        TokenKind::QuotedString,
        TokenKind::RightBracket,
        TokenKind::Whitespace,
        TokenKind::Symbol,
    ]);
    assert!(has_diagnostic(&malformed, DiagnosticCode::MalformedTagPair));
    assert_eq!(nodes(&malformed, NodeKind::Unparsed)[0].start_token, 0);
    assert_eq!(nodes(&malformed, NodeKind::Unparsed)[0].end_token, 4);
}

#[test]
fn parses_movetext_without_treating_result_as_terminator() {
    let parsed = tree(&[
        TokenKind::MoveNumberCandidate,
        TokenKind::Whitespace,
        TokenKind::Symbol,
        TokenKind::NagNumeric,
        TokenKind::Whitespace,
        TokenKind::BraceComment,
        TokenKind::Whitespace,
        TokenKind::SemicolonComment,
        TokenKind::Whitespace,
        TokenKind::ResultCandidate,
        TokenKind::Whitespace,
        TokenKind::Symbol,
        TokenKind::EscapeLine,
    ]);
    assert_eq!(nodes(&parsed, NodeKind::MoveNumber).len(), 1);
    assert_eq!(nodes(&parsed, NodeKind::MoveSymbolCandidate).len(), 2);
    assert_eq!(nodes(&parsed, NodeKind::Nag).len(), 1);
    assert_eq!(nodes(&parsed, NodeKind::Comment).len(), 2);
    assert_eq!(nodes(&parsed, NodeKind::Result).len(), 1);
    assert_eq!(nodes(&parsed, NodeKind::EscapeLine).len(), 1);
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn recovers_unterminated_comment_and_stray_parenthesis() {
    let parsed = tree(&[
        TokenKind::Symbol,
        TokenKind::RightParenthesis,
        TokenKind::Whitespace,
        TokenKind::UnterminatedBraceComment,
    ]);
    assert!(has_diagnostic(
        &parsed,
        DiagnosticCode::StrayRightParenthesis
    ));
    assert!(has_diagnostic(&parsed, DiagnosticCode::UnterminatedComment));
    assert_eq!(parsed.nodes[0].status, NodeStatus::Incomplete);
}

#[test]
fn parses_simple_nested_empty_and_unterminated_variations() {
    let simple = tree(&[
        TokenKind::LeftParenthesis,
        TokenKind::Symbol,
        TokenKind::RightParenthesis,
    ]);
    assert_eq!(nodes(&simple, NodeKind::Variation).len(), 1);
    assert_eq!(
        nodes(&simple, NodeKind::Variation)[0].status,
        NodeStatus::Valid
    );

    let nested = tree(&[
        TokenKind::LeftParenthesis,
        TokenKind::Symbol,
        TokenKind::LeftParenthesis,
        TokenKind::Symbol,
        TokenKind::RightParenthesis,
        TokenKind::RightParenthesis,
    ]);
    assert_eq!(nodes(&nested, NodeKind::Variation).len(), 2);

    for kinds in [
        vec![TokenKind::LeftParenthesis, TokenKind::RightParenthesis],
        vec![
            TokenKind::LeftParenthesis,
            TokenKind::Whitespace,
            TokenKind::RightParenthesis,
        ],
    ] {
        let empty = tree(&kinds);
        assert!(has_diagnostic(&empty, DiagnosticCode::EmptyVariation));
        assert_eq!(
            nodes(&empty, NodeKind::Variation)[0].status,
            NodeStatus::Recovered
        );
    }

    let unterminated = tree(&[
        TokenKind::LeftParenthesis,
        TokenKind::Symbol,
        TokenKind::Whitespace,
    ]);
    assert!(has_diagnostic(
        &unterminated,
        DiagnosticCode::UnterminatedVariation
    ));
    assert_eq!(
        nodes(&unterminated, NodeKind::Variation)[0].status,
        NodeStatus::Incomplete
    );
}

#[test]
fn bracket_led_recovery_never_crosses_the_current_variation() {
    let malformed = tree(&[
        TokenKind::LeftParenthesis,
        TokenKind::LeftBracket,
        TokenKind::Symbol,
        TokenKind::QuotedString,
        TokenKind::RightParenthesis,
        TokenKind::RightBracket,
    ]);
    let variation = nodes(&malformed, NodeKind::Variation)[0];
    assert_eq!((variation.start_token, variation.end_token), (0, 5));
    let internal_unparsed = nodes(&malformed, NodeKind::Unparsed)
        .into_iter()
        .find(|node| node.start_token == 1)
        .expect("recovery interna");
    assert!(internal_unparsed.end_token <= 4);
    assert!(has_diagnostic(&malformed, DiagnosticCode::MalformedTagPair));
    assert_exact_coverage(&malformed);

    let crossing_complete = tree(&[
        TokenKind::LeftParenthesis,
        TokenKind::LeftBracket,
        TokenKind::Symbol,
        TokenKind::Whitespace,
        TokenKind::QuotedString,
        TokenKind::RightParenthesis,
        TokenKind::RightBracket,
    ]);
    let variation = nodes(&crossing_complete, NodeKind::Variation)[0];
    assert_eq!((variation.start_token, variation.end_token), (0, 6));
    let internal_unparsed = nodes(&crossing_complete, NodeKind::Unparsed)
        .into_iter()
        .find(|node| node.start_token == 1)
        .expect("tag confinata");
    assert_eq!(internal_unparsed.end_token, 5);
    assert!(has_diagnostic(
        &crossing_complete,
        DiagnosticCode::TagPairOutsideTagSection
    ));
    assert_exact_coverage(&crossing_complete);

    let nested = tree(&[
        TokenKind::LeftParenthesis,
        TokenKind::LeftParenthesis,
        TokenKind::LeftBracket,
        TokenKind::Unknown,
        TokenKind::RightParenthesis,
        TokenKind::RightBracket,
        TokenKind::RightParenthesis,
    ]);
    let variations = nodes(&nested, NodeKind::Variation);
    assert_eq!(variations.len(), 2);
    assert_eq!((variations[1].start_token, variations[1].end_token), (1, 5));
    let internal_unparsed = nodes(&nested, NodeKind::Unparsed)
        .into_iter()
        .find(|node| node.start_token == 2)
        .expect("recovery annidata");
    assert!(internal_unparsed.end_token <= 4);
    assert_exact_coverage(&nested);
}

#[test]
fn pathological_variation_depth_is_bounded_and_lossless() {
    for close_count in [20_000, 0] {
        let mut kinds = vec![TokenKind::LeftParenthesis; 20_000];
        kinds.extend(std::iter::repeat_n(
            TokenKind::RightParenthesis,
            close_count,
        ));
        let parsed = tree(&kinds);
        assert_exact_coverage(&parsed);
        let (arena_depth, variation_depth) = maximum_depths(&parsed);
        assert!(arena_depth <= MAX_ARENA_DEPTH);
        assert!(variation_depth <= MAX_VARIATION_DEPTH);
        assert!(has_diagnostic(
            &parsed,
            DiagnosticCode::UnexpectedTokenRange
        ));
        let reparsed = tree(&kinds);
        assert_eq!(parsed, reparsed);
    }
}

#[test]
fn every_prefix_around_the_depth_limit_is_bounded_and_valid() {
    let sample_depth = MAX_VARIATION_DEPTH + 4;
    let mut kinds = vec![TokenKind::LeftParenthesis; sample_depth];
    kinds.extend(std::iter::repeat_n(
        TokenKind::RightParenthesis,
        sample_depth,
    ));
    for end in 1..=kinds.len() {
        let parsed = tree(&kinds[..end]);
        assert_exact_coverage(&parsed);
        let (arena_depth, variation_depth) = maximum_depths(&parsed);
        assert!(arena_depth <= MAX_ARENA_DEPTH);
        assert!(variation_depth <= MAX_VARIATION_DEPTH);
    }
}

#[test]
fn validation_rejects_an_artificial_arena_beyond_the_depth_limit() {
    let chain_length = MAX_ARENA_DEPTH + 2;
    let mut deep = ParseTree {
        segment_ordinal: 3,
        token_count: 1,
        nodes: Vec::with_capacity(chain_length + 1),
        diagnostics: Vec::new(),
    };
    deep.nodes.push(FlatNode {
        id: 0,
        parent_id: None,
        child_ordinal: None,
        kind: NodeKind::GameCandidate,
        status: NodeStatus::Valid,
        start_token: 0,
        end_token: 1,
    });
    for id in 1..chain_length {
        deep.nodes.push(FlatNode {
            id: id as u64,
            parent_id: Some(id as u64 - 1),
            child_ordinal: Some(0),
            kind: NodeKind::Movetext,
            status: NodeStatus::Valid,
            start_token: 0,
            end_token: 1,
        });
    }
    deep.nodes.push(FlatNode {
        id: chain_length as u64,
        parent_id: Some(chain_length as u64 - 1),
        child_ordinal: Some(0),
        kind: NodeKind::Trivia,
        status: NodeStatus::Valid,
        start_token: 0,
        end_token: 1,
    });
    assert!(validate_tree(&deep).is_err());
}

#[test]
fn recovers_tag_pair_outside_section_and_maximal_unexpected_range() {
    let outside = tree(&[
        TokenKind::Symbol,
        TokenKind::Whitespace,
        TokenKind::LeftBracket,
        TokenKind::Symbol,
        TokenKind::Whitespace,
        TokenKind::QuotedString,
        TokenKind::RightBracket,
    ]);
    assert!(has_diagnostic(
        &outside,
        DiagnosticCode::TagPairOutsideTagSection
    ));
    let unparsed = nodes(&outside, NodeKind::Unparsed)[0];
    assert_eq!((unparsed.start_token, unparsed.end_token), (2, 7));

    let unexpected = tree(&[
        TokenKind::Unknown,
        TokenKind::QuotedString,
        TokenKind::RightBracket,
        TokenKind::Whitespace,
        TokenKind::Symbol,
    ]);
    let unparsed = nodes(&unexpected, NodeKind::Unparsed)[0];
    assert_eq!((unparsed.start_token, unparsed.end_token), (0, 3));
    assert!(has_diagnostic(
        &unexpected,
        DiagnosticCode::UnexpectedTokenRange
    ));
}

#[test]
fn every_tree_is_lossless_and_preorder_partitioned() {
    let pathological = [
        TokenKind::LeftBracket,
        TokenKind::Unknown,
        TokenKind::RightBracket,
        TokenKind::Whitespace,
        TokenKind::LeftParenthesis,
        TokenKind::Whitespace,
        TokenKind::LeftParenthesis,
        TokenKind::Unknown,
        TokenKind::RightParenthesis,
        TokenKind::UnterminatedBraceComment,
    ];
    let parsed = tree(&pathological);
    validate_tree(&parsed).expect("partizione valida");
    let leaves = parsed
        .nodes
        .iter()
        .filter(|node| {
            !parsed
                .nodes
                .iter()
                .any(|other| other.parent_id == Some(node.id))
        })
        .collect::<Vec<_>>();
    let mut cursor = 0;
    for leaf in leaves {
        assert_eq!(leaf.start_token, cursor);
        cursor = leaf.end_token;
    }
    assert_eq!(cursor, pathological.len() as u64);
}

#[test]
fn every_prefix_and_arbitrary_token_sequence_parses_without_panic() {
    let input = [
        TokenKind::LeftBracket,
        TokenKind::Symbol,
        TokenKind::Whitespace,
        TokenKind::QuotedString,
        TokenKind::LeftParenthesis,
        TokenKind::Unknown,
        TokenKind::UnterminatedQuotedString,
        TokenKind::RightParenthesis,
    ];
    for end in 1..=input.len() {
        let parsed = tree(&input[..end]);
        validate_tree(&parsed).expect("prefisso rappresentato");
    }
    for seed in 0..128_u64 {
        let kinds = (0..1 + seed as usize % 31)
            .map(|index| ALL_TOKEN_KINDS[(seed as usize * 17 + index * 11) % ALL_TOKEN_KINDS.len()])
            .collect::<Vec<_>>();
        let parsed = tree(&kinds);
        validate_tree(&parsed).expect("input arbitrario rappresentato");
    }
}

const ALL_TOKEN_KINDS: [TokenKind; 17] = [
    TokenKind::Whitespace,
    TokenKind::LeftBracket,
    TokenKind::RightBracket,
    TokenKind::LeftParenthesis,
    TokenKind::RightParenthesis,
    TokenKind::QuotedString,
    TokenKind::UnterminatedQuotedString,
    TokenKind::BraceComment,
    TokenKind::UnterminatedBraceComment,
    TokenKind::SemicolonComment,
    TokenKind::EscapeLine,
    TokenKind::MoveNumberCandidate,
    TokenKind::ResultCandidate,
    TokenKind::NagNumeric,
    TokenKind::NagSymbolic,
    TokenKind::Symbol,
    TokenKind::Unknown,
];

#[test]
fn parse_is_repeatable_and_validates_the_flat_arena() {
    let fixture = make_fixture(b"[Event \"One\"]\r\n[Site \"X\"]\n\n1. e4$1 ( ) {x} @\n");
    let first = read_parse(&fixture.root, &fixture.parse);
    let second_created = parse(&fixture.root, &fixture.tokenization).expect("seconda parse");
    let second = read_parse(&fixture.root, &second_created.path);
    assert_ne!(fixture.parse, second_created.path);
    assert_eq!(first.trees, second.trees);
    for tree in &first.trees {
        validate_tree(tree).expect("arena valida");
    }
    verify_parse(&fixture.root, &fixture.parse).expect("verify parse");
    fs::remove_dir_all(fixture.root).expect("pulizia workspace");
}

#[test]
fn validation_rejects_arena_identity_parent_and_partition_corruptions() {
    let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 (e5) * @\n");
    let original = read_parse(&fixture.root, &fixture.parse);
    let tree = &original.trees[0];

    let mut changed = tree.clone();
    changed.nodes[1].id = 9;
    assert!(validate_tree(&changed).is_err());

    let mut changed = tree.clone();
    changed.nodes[0].kind = NodeKind::Movetext;
    assert!(validate_tree(&changed).is_err());

    let mut changed = tree.clone();
    changed.nodes[1].parent_id = Some(changed.nodes.len() as u64);
    assert!(validate_tree(&changed).is_err());

    let mut changed = tree.clone();
    changed.nodes[1].child_ordinal = Some(1);
    assert!(validate_tree(&changed).is_err());

    let root_children = tree
        .nodes
        .iter()
        .filter(|node| node.parent_id == Some(0))
        .map(|node| node.id as usize)
        .collect::<Vec<_>>();
    assert!(root_children.len() >= 2);
    let mut gap = tree.clone();
    gap.nodes[root_children[1]].start_token += 1;
    assert!(validate_tree(&gap).is_err());

    let mut overlap = tree.clone();
    overlap.nodes[root_children[1]].start_token -= 1;
    assert!(validate_tree(&overlap).is_err());

    let mut outside_parent = tree.clone();
    let leaf = outside_parent
        .nodes
        .iter()
        .position(|node| node.parent_id.is_some() && !is_container(node.kind))
        .expect("foglia");
    outside_parent.nodes[leaf].end_token = outside_parent.nodes[leaf].end_token.saturating_add(2);
    assert!(validate_tree(&outside_parent).is_err());

    let mut outside_stream = tree.clone();
    outside_stream.nodes[1].end_token = tree.token_count + 1;
    assert!(validate_tree(&outside_stream).is_err());

    let mut json = serde_json::to_value(tree).expect("JSON");
    json["nodes"][0]["kind"] = serde_json::Value::String("future".to_owned());
    assert!(serde_json::from_value::<ParseTree>(json).is_err());
    let mut json = serde_json::to_value(tree).expect("JSON");
    json["nodes"][0]["status"] = serde_json::Value::String("future".to_owned());
    assert!(serde_json::from_value::<ParseTree>(json).is_err());

    fs::remove_dir_all(fixture.root).expect("pulizia workspace");
}

#[test]
fn validation_rejects_tree_and_diagnostic_corruptions() {
    let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 ( ) @\n");
    let original = read_parse(&fixture.root, &fixture.parse);
    let loaded = load_tokenization_manifest(&fixture.root, &fixture.tokenization, None)
        .expect("tokenizzazione");

    let mut missing = original.clone();
    missing.trees.clear();
    assert!(validate_parse_manifest(&missing, &loaded).is_err());

    let mut duplicate = original.clone();
    duplicate.trees.push(duplicate.trees[0].clone());
    assert!(validate_parse_manifest(&duplicate, &loaded).is_err());

    let mut wrong_segment = original.clone();
    wrong_segment.trees[0].segment_ordinal += 1;
    assert!(validate_parse_manifest(&wrong_segment, &loaded).is_err());

    let diagnostic = original.trees[0]
        .diagnostics
        .first()
        .expect("diagnostica")
        .clone();
    let mut wrong_ordinal = original.trees[0].clone();
    wrong_ordinal.diagnostics[0].ordinal = 1;
    assert!(validate_tree(&wrong_ordinal).is_err());

    let mut wrong_node = original.trees[0].clone();
    wrong_node.diagnostics[0].node_id = wrong_node.nodes.len() as u64;
    assert!(validate_tree(&wrong_node).is_err());

    let mut outside = original.trees[0].clone();
    outside.diagnostics[0].start_token = 0;
    outside.diagnostics[0].end_token = outside.token_count;
    if diagnostic.node_id != 0 {
        assert!(validate_tree(&outside).is_err());
    }

    let mut json = serde_json::to_value(&original.trees[0]).expect("JSON");
    json["diagnostics"][0]["code"] = serde_json::Value::String("future".to_owned());
    assert!(serde_json::from_value::<ParseTree>(json).is_err());

    fs::remove_dir_all(fixture.root).expect("pulizia workspace");
}

#[test]
fn verification_rejects_structurally_valid_reparse_changes() {
    let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 ( ) @\n");
    let original = read_parse(&fixture.root, &fixture.parse);
    let loaded = load_tokenization_manifest(&fixture.root, &fixture.tokenization, None)
        .expect("tokenizzazione");

    let mut changed_kind = original.clone();
    let symbol = changed_kind.trees[0]
        .nodes
        .iter_mut()
        .find(|node| node.kind == NodeKind::MoveSymbolCandidate)
        .expect("move symbol");
    symbol.kind = NodeKind::Result;
    assert!(validate_parse_manifest(&changed_kind, &loaded).is_ok());
    write_parse(&fixture.root, &fixture.parse, &changed_kind);
    assert!(verify_parse(&fixture.root, &fixture.parse).is_err());

    let mut changed_status = original.clone();
    let symbol = changed_status.trees[0]
        .nodes
        .iter_mut()
        .find(|node| node.kind == NodeKind::MoveSymbolCandidate)
        .expect("move symbol");
    symbol.status = NodeStatus::Recovered;
    assert!(validate_parse_manifest(&changed_status, &loaded).is_ok());
    write_parse(&fixture.root, &fixture.parse, &changed_status);
    assert!(verify_parse(&fixture.root, &fixture.parse).is_err());

    let mut changed_parent = original.clone();
    let tree = &mut changed_parent.trees[0];
    let container_index = tree
        .nodes
        .iter()
        .position(|node| node.kind == NodeKind::Movetext)
        .expect("movetext");
    let container_id = container_index as u64;
    let wrapper_id = container_id + 1;
    let wrapper = FlatNode {
        id: wrapper_id,
        parent_id: Some(container_id),
        child_ordinal: Some(0),
        kind: NodeKind::Movetext,
        status: tree.nodes[container_index].status,
        start_token: tree.nodes[container_index].start_token,
        end_token: tree.nodes[container_index].end_token,
    };
    for node in tree.nodes.iter_mut().skip(container_index + 1) {
        node.id += 1;
        if node.parent_id == Some(container_id) {
            node.parent_id = Some(wrapper_id);
        } else if node.parent_id.is_some_and(|parent| parent > container_id) {
            node.parent_id = node.parent_id.map(|parent| parent + 1);
        }
    }
    for diagnostic in &mut tree.diagnostics {
        if diagnostic.node_id > container_id {
            diagnostic.node_id += 1;
        }
    }
    tree.nodes.insert(container_index + 1, wrapper);
    assert!(validate_parse_manifest(&changed_parent, &loaded).is_ok());
    write_parse(&fixture.root, &fixture.parse, &changed_parent);
    assert!(verify_parse(&fixture.root, &fixture.parse).is_err());

    let mut removed_diagnostic = original;
    removed_diagnostic.trees[0].diagnostics.pop();
    assert!(validate_parse_manifest(&removed_diagnostic, &loaded).is_ok());
    write_parse(&fixture.root, &fixture.parse, &removed_diagnostic);
    assert!(verify_parse(&fixture.root, &fixture.parse).is_err());

    fs::remove_dir_all(fixture.root).expect("pulizia workspace");
}

#[test]
fn verification_rejects_changes_anywhere_in_the_verified_chain() {
    let cases = ["tokenization", "segmentation", "source", "object"];
    for case in cases {
        let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
        match case {
            "tokenization" => {
                let mut value: serde_json::Value = serde_json::from_slice(
                    &crate::inspector::common::logical_json_bytes(
                        &fixture.root.join(&fixture.tokenization),
                        "tokenization test",
                    )
                    .expect("tokenization"),
                )
                .expect("JSON tokenization");
                value["tokenized_at_unix_ms"] = serde_json::Value::from(1);
                fs::write(
                    fixture.root.join(&fixture.tokenization),
                    serde_json::to_vec_pretty(&value).expect("JSON"),
                )
                .expect("modifica tokenization");
            }
            "segmentation" => {
                let mut value: serde_json::Value = serde_json::from_slice(
                    &crate::inspector::common::logical_json_bytes(
                        &fixture.root.join(&fixture.segmentation),
                        "segmentation test",
                    )
                    .expect("segmentation"),
                )
                .expect("JSON segmentation");
                value["segmented_at_unix_ms"] = serde_json::Value::from(1);
                fs::write(
                    fixture.root.join(&fixture.segmentation),
                    serde_json::to_vec_pretty(&value).expect("JSON"),
                )
                .expect("modifica segmentation");
            }
            "source" => {
                let mut value: serde_json::Value = serde_json::from_slice(
                    &fs::read(fixture.root.join(&fixture.source_manifest)).expect("source"),
                )
                .expect("JSON source");
                value["imported_at_unix_ms"] = serde_json::Value::from(1);
                fs::write(
                    fixture.root.join(&fixture.source_manifest),
                    serde_json::to_vec_pretty(&value).expect("JSON"),
                )
                .expect("modifica source");
            }
            "object" => {
                let loaded = load_segmentation_manifest(&fixture.root, &fixture.segmentation, None)
                    .expect("segmentation");
                fs::write(loaded.object_path, b"altered").expect("modifica oggetto");
            }
            _ => unreachable!(),
        }
        assert!(
            verify_parse(&fixture.root, &fixture.parse).is_err(),
            "{case}"
        );
        fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    }
}

#[test]
fn verification_rejects_traversal_and_external_manifest_symlinks() {
    let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
    let original = read_parse(&fixture.root, &fixture.parse);
    let mut traversal = original.clone();
    traversal.tokenization_manifest = "../outside.json".to_owned();
    write_parse(&fixture.root, &fixture.parse, &traversal);
    assert!(verify_parse(&fixture.root, &fixture.parse).is_err());
    write_parse(&fixture.root, &fixture.parse, &original);

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let outside = test_root();
        fs::create_dir_all(&outside).expect("directory esterna");
        let external_parse = outside.join("parse.json");
        fs::copy(fixture.root.join(&fixture.parse), &external_parse).expect("copia parse");
        let parse_link = fixture.root.join("parse-link.json");
        symlink(&external_parse, &parse_link).expect("symlink parse");
        assert!(verify_parse(&fixture.root, &parse_link).is_err());

        let external_tokenization = outside.join("tokenization.json");
        let tokenization_bytes =
            fs::read(fixture.root.join(&fixture.tokenization)).expect("tokenization");
        fs::write(&external_tokenization, &tokenization_bytes).expect("copia tokenization");
        let tokenization_link = fixture.root.join("tokenization-link.json");
        symlink(&external_tokenization, &tokenization_link).expect("symlink tokenization");
        let mut linked = original;
        linked.tokenization_manifest = "tokenization-link.json".to_owned();
        linked.tokenization_manifest_blake3 =
            blake3::hash(&tokenization_bytes).to_hex().to_string();
        linked.tokenization_manifest_bytes = tokenization_bytes.len() as u64;
        write_parse(&fixture.root, &fixture.parse, &linked);
        assert!(verify_parse(&fixture.root, &fixture.parse).is_err());
        assert_eq!(
            fs::read(&external_tokenization).expect("tokenization esterna"),
            tokenization_bytes
        );
        fs::remove_dir_all(outside).expect("pulizia esterna");
    }

    fs::remove_dir_all(fixture.root).expect("pulizia workspace");
}

#[cfg(unix)]
#[test]
fn parse_rejects_an_external_publication_directory_symlink() {
    use std::os::unix::fs::symlink;

    let fixture = make_fixture(b"[Event \"One\"]\n\n1. e4 *\n");
    fs::remove_dir_all(fixture.root.join("inspector/parses")).expect("rimuovi parses");
    let outside = test_root();
    fs::create_dir_all(&outside).expect("directory esterna");
    symlink(&outside, fixture.root.join("inspector/parses")).expect("symlink parses");
    assert!(parse(&fixture.root, &fixture.tokenization).is_err());
    assert_eq!(fs::read_dir(&outside).expect("directory").count(), 0);
    fs::remove_dir_all(fixture.root).expect("pulizia workspace");
    fs::remove_dir_all(outside).expect("pulizia esterna");
}
