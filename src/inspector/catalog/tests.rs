use super::*;
use crate::inspector::{interpretation, parsing, segmentation, source, tokenization};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    interpretation: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn fixture(bytes: &[u8]) -> Fixture {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("antiskill-catalog-{}-{n}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let p = root.join("source.pgn");
    fs::write(&p, bytes).unwrap();
    let i = source::import_source(&root, &p).unwrap();
    let s = segmentation::segment_source(&root, &i.manifest_path).unwrap();
    let t = tokenization::tokenize(&root, &s.path).unwrap();
    let p = parsing::parse(&root, &t.path).unwrap();
    let x = interpretation::create_interpretation(&root, &p.path).unwrap();
    Fixture {
        root,
        interpretation: x.path,
    }
}

#[test]
fn streaming_catalog_requires_strictly_increasing_segment_ordinals_with_constant_state() {
    let mut previous = None;
    require_increasing_segment_ordinal(&mut previous, 4).unwrap();
    require_increasing_segment_ordinal(&mut previous, 9).unwrap();
    assert!(require_increasing_segment_ordinal(&mut previous, 9).is_err());
    assert_eq!(previous, Some(9));
    assert!(require_increasing_segment_ordinal(&mut previous, 3).is_err());
    assert_eq!(previous, Some(9));
}

#[test]
fn streaming_catalog_loader_does_not_semantically_replay_interpretation() {
    let source = include_str!("mod.rs");
    let loader = source
        .split("pub(super) fn load_streaming_catalog")
        .nth(1)
        .unwrap()
        .split("#[derive(Serialize)]")
        .next()
        .unwrap();
    assert!(!loader.contains("for_each_verified_game"));
}

#[test]
fn catalog_creation_verifies_interpretation_during_the_output_pass() {
    let f = fixture(b"[Event \"X\"]\n\n1. e4 *\n");
    let path = f.root.join(&f.interpretation);
    let logical =
        crate::inspector::common::logical_json_bytes(&path, "interpretation test").unwrap();
    let text = String::from_utf8(logical).unwrap();
    let mutated = text.replacen("\"raw_san\": \"e4\"", "\"raw_san\": \"d4\"", 1);
    assert_ne!(
        mutated, text,
        "fixture must expose the semantic field under test"
    );
    crate::inspector::common::replace_logical_json(&path, mutated.as_bytes());

    assert!(
        create_catalog(&f.root, &f.interpretation).is_err(),
        "semantic Interpretation corruption accepted by fused Catalog creation"
    );
}

#[test]
fn catalog_mainline_rav_all_tags_duplicates_utf8_ranges_hash_and_idempotence() {
    let bytes="[Event \"Città\"]\n[Site \"X\"]\n[Date \"2026.08.01\"]\n[Round \"1\"]\n[White \"Èva\"]\n[Black \"Bob\"]\n[Result \"1-0\"]\n[White \"Duplicate\"]\n[Technical \"a\\\"b\\\\c\"]\n\n1. e4 (1. d4) e5 1-0\n".as_bytes();
    let f = fixture(bytes);
    let a = create_catalog(&f.root, &f.interpretation).unwrap();
    let b = create_catalog(&f.root, &f.interpretation).unwrap();
    assert!(a.new_manifest);
    assert!(!b.new_manifest);
    assert_eq!(a.path, b.path);
    assert_eq!(a.manifest_blake3, b.manifest_blake3);
    let manifest = load_catalog(&f.root, &a.path).unwrap().manifest;
    assert_eq!(manifest.games.len(), 1);
    let g = &manifest.games[0];
    assert_eq!(g.id, 0);
    assert_eq!(g.tags.len(), 9);
    assert_eq!(
        g.tags.iter().map(|t| t.ordinal).collect::<Vec<_>>(),
        (0..9).collect::<Vec<_>>()
    );
    assert_eq!(
        g.tags
            .iter()
            .filter(|t| t.name.as_deref() == Some("White"))
            .count(),
        2
    );
    assert_eq!(g.tags[0].value.as_deref(), Some("Città"));
    let blob = &bytes[g.source_start as usize..g.source_end as usize];
    assert_eq!(
        g.source_game_blake3,
        blake3::hash(blob).to_hex().to_string()
    );
    assert!(!String::from_utf8(
        crate::inspector::common::logical_json_bytes(&f.root.join(&a.path), "catalog test")
            .unwrap()
    )
    .unwrap()
    .contains("timestamp"));
    let json = String::from_utf8(
        crate::inspector::common::logical_json_bytes(&f.root.join(&a.path), "catalog test")
            .unwrap(),
    )
    .unwrap();
    assert!(
        json.contains("\"catalog_profile\": \"antiskill_game_catalog_v1\""),
        "profilo congelato"
    );
    assert!(
        !json.contains("\"parse_manifest\":"),
        "parse_manifest top-level assente"
    );
    assert!(
        json.contains("\"interpretation_status\": \""),
        "enum Catalog status snake_case"
    );
    assert!(
        json.contains("\"variant\": \"standard\""),
        "enum Catalog variant indipendente"
    );
    verify_catalog(&f.root, &a.path).unwrap();
}

#[test]
fn query_filters_are_deterministic() {
    let f=fixture("[Event \"Open Uno\"]\n[Date \"2026.01.01\"]\n[White \"Àlice\"]\n[Black \"Bob\"]\n[Result \"1-0\"]\n\n1. e4 1-0\n\n[Event \"Open Due\"]\n[UTCDate \"2026.01.02\"]\n[White \"Carol\"]\n[Black \"ALICE\"]\n[Result \"0-1\"]\n\n1. d4 0-1\n".as_bytes());
    let c = create_catalog(&f.root, &f.interpretation).unwrap();
    let q = Query {
        root: f.root.clone(),
        catalog: c.path.clone(),
        filters: QueryFilters {
            player: Some("alice".into()),
            ..QueryFilters::default()
        },
    };
    let out = query_catalog(&q).unwrap();
    assert!(out.contains("MATCHES\t1\n"));
    let all = Query {
        filters: QueryFilters::default(),
        ..q
    };
    let one = query_catalog(&all).unwrap();
    let two = query_catalog(&all).unwrap();
    assert_eq!(one, two);
    assert!(one.contains("MATCHES\t2"));
}

#[test]
fn validation_and_exact_recalculation_reject_mutations() {
    let f = fixture(b"[Event \"X\"]\n\n1. e4 *\n");
    let c = create_catalog(&f.root, &f.interpretation).unwrap();
    let path = f.root.join(&c.path);
    let canonical = crate::inspector::common::logical_json_bytes(&path, "catalog test").unwrap();
    let mut m = load_catalog(&f.root, &c.path).unwrap().manifest;
    m.games[0].source_game_blake3 = "0".repeat(64);
    fs::write(&path, serialized(&m).unwrap()).unwrap();
    assert!(
        verify_catalog(&f.root, &c.path).is_err(),
        "raw_segment_blake3 modificato accettato"
    );
    fs::write(&path, &canonical[..canonical.len() - 1]).unwrap();
    assert!(
        verify_catalog(&f.root, &c.path).is_err(),
        "newline modificata accettata"
    );
}

#[test]
fn query_rejects_empty_player_and_event() {
    let q = Query {
        root: PathBuf::new(),
        catalog: PathBuf::new(),
        filters: QueryFilters {
            player: Some(String::new()),
            ..QueryFilters::default()
        },
    };
    assert!(query_catalog(&q).unwrap_err().contains("vuota"));
    let q = Query {
        filters: QueryFilters {
            event: Some(" \t".into()),
            ..QueryFilters::default()
        },
        ..q
    };
    assert!(query_catalog(&q).unwrap_err().contains("vuota"));
}

fn node(
    id: u64,
    kind: crate::inspector::parsing::model::NodeKind,
) -> crate::inspector::parsing::model::FlatNode {
    crate::inspector::parsing::model::FlatNode {
        id,
        parent_id: Some(0),
        child_ordinal: Some(id),
        kind,
        status: crate::inspector::parsing::model::NodeStatus::Valid,
        start_token: 0,
        end_token: 1,
    }
}

#[test]
fn tag_decode_precedence_and_byte_semantics_are_exhaustive() {
    use crate::inspector::parsing::model::NodeKind;
    let name = node(1, NodeKind::TagNameCandidate);
    let name2 = node(2, NodeKind::TagNameCandidate);
    let value = node(3, NodeKind::TagValueCandidate);
    let value2 = node(4, NodeKind::TagValueCandidate);
    struct Case<'a> {
        label: &'static str,
        unexpected: bool,
        names: Vec<&'a crate::inspector::parsing::model::FlatNode>,
        values: Vec<&'a crate::inspector::parsing::model::FlatNode>,
        name_bytes: Option<&'a [u8]>,
        value_bytes: Option<&'a [u8]>,
        status: TagDecodeStatusV1,
        name: Option<&'static str>,
        decoded: Option<&'static str>,
    }
    let cases = vec![
        Case {
            label: "figlio strutturalmente inatteso",
            unexpected: true,
            names: vec![&name],
            values: vec![&value],
            name_bytes: Some(b"Event"),
            value_bytes: Some(b"\"X\""),
            status: TagDecodeStatusV1::InvalidStructure,
            name: None,
            decoded: None,
        },
        Case {
            label: "nome mancante",
            unexpected: false,
            names: vec![],
            values: vec![&value],
            name_bytes: None,
            value_bytes: Some(b"\"X\""),
            status: TagDecodeStatusV1::MissingName,
            name: None,
            decoded: None,
        },
        Case {
            label: "nomi multipli",
            unexpected: false,
            names: vec![&name, &name2],
            values: vec![&value],
            name_bytes: Some(b"Event"),
            value_bytes: Some(b"\"X\""),
            status: TagDecodeStatusV1::AmbiguousName,
            name: None,
            decoded: None,
        },
        Case {
            label: "nome non ASCII",
            unexpected: false,
            names: vec![&name],
            values: vec![&value],
            name_bytes: Some(b"Ev\xffnt"),
            value_bytes: Some(b"\"X\""),
            status: TagDecodeStatusV1::InvalidNameEncoding,
            name: None,
            decoded: None,
        },
        Case {
            label: "valore mancante senza candidato",
            unexpected: false,
            names: vec![&name],
            values: vec![],
            name_bytes: Some(b"Event"),
            value_bytes: None,
            status: TagDecodeStatusV1::MissingValue,
            name: Some("Event"),
            decoded: None,
        },
        Case {
            label: "valore candidato non terminato",
            unexpected: false,
            names: vec![&name],
            values: vec![&value],
            name_bytes: Some(b"Event"),
            value_bytes: Some(b"\"open"),
            status: TagDecodeStatusV1::MissingValue,
            name: Some("Event"),
            decoded: None,
        },
        Case {
            label: "barra inversa finale incompleta",
            unexpected: false,
            names: vec![&name],
            values: vec![&value],
            name_bytes: Some(b"Event"),
            value_bytes: Some(b"\"open\\\""),
            status: TagDecodeStatusV1::MissingValue,
            name: Some("Event"),
            decoded: None,
        },
        Case {
            label: "valori multipli",
            unexpected: false,
            names: vec![&name],
            values: vec![&value, &value2],
            name_bytes: Some(b"Event"),
            value_bytes: Some(b"\"X\""),
            status: TagDecodeStatusV1::AmbiguousValue,
            name: Some("Event"),
            decoded: None,
        },
        Case {
            label: "valore UTF-8 valido",
            unexpected: false,
            names: vec![&name],
            values: vec![&value],
            name_bytes: Some(b"Event"),
            value_bytes: Some("\"Città\"".as_bytes()),
            status: TagDecodeStatusV1::Decoded,
            name: Some("Event"),
            decoded: Some("Città"),
        },
        Case {
            label: "valore UTF-8 invalido",
            unexpected: false,
            names: vec![&name],
            values: vec![&value],
            name_bytes: Some(b"Event"),
            value_bytes: Some(b"\"\xff\""),
            status: TagDecodeStatusV1::InvalidValueEncoding,
            name: Some("Event"),
            decoded: None,
        },
        Case {
            label: "escape quote valido",
            unexpected: false,
            names: vec![&name],
            values: vec![&value],
            name_bytes: Some(b"Event"),
            value_bytes: Some(b"\"a\\\"b\""),
            status: TagDecodeStatusV1::Decoded,
            name: Some("Event"),
            decoded: Some("a\"b"),
        },
        Case {
            label: "escape backslash valido",
            unexpected: false,
            names: vec![&name],
            values: vec![&value],
            name_bytes: Some(b"Event"),
            value_bytes: Some(b"\"a\\\\b\""),
            status: TagDecodeStatusV1::Decoded,
            name: Some("Event"),
            decoded: Some("a\\b"),
        },
        Case {
            label: "escape sconosciuto",
            unexpected: false,
            names: vec![&name],
            values: vec![&value],
            name_bytes: Some(b"Event"),
            value_bytes: Some(b"\"a\\qb\""),
            status: TagDecodeStatusV1::InvalidEscape,
            name: Some("Event"),
            decoded: None,
        },
    ];
    for c in cases {
        let nr = c.name_bytes.map(|b| (0, b.len() as u64, b));
        let vr = c.value_bytes.map(|b| (0, b.len() as u64, b));
        let got = extraction::decode(c.unexpected, &c.names, &c.values, nr, vr);
        assert_eq!(got.0, c.status, "decode_status: {}", c.label);
        assert_eq!(got.1.as_deref(), c.name, "name: {}", c.label);
        assert_eq!(got.2.as_deref(), c.decoded, "value: {}", c.label);
    }
}

#[test]
fn node_range_rejects_unrepresentable_and_out_of_bounds_input_without_panicking() {
    use crate::inspector::parsing::model::NodeKind;
    let stream = crate::inspector::tokenization::TokenStream {
        segment_ordinal: 0,
        start: 0,
        end: 1,
        tokens: vec![crate::inspector::tokenization::Token {
            ordinal: 0,
            start: 0,
            end: 1,
            kind: crate::inspector::tokenization::TokenKind::Symbol,
        }],
    };
    for (label, start, end) in [
        ("token u64 massimo", u64::MAX, u64::MAX),
        ("end oltre stream", 0, 2),
        ("range vuoto", 0, 0),
    ] {
        let mut n = node(0, NodeKind::TagPair);
        n.start_token = start;
        n.end_token = end;
        assert!(
            extraction::node_range(&n, &stream, b"x").is_err(),
            "accesso accettato: {label}"
        );
    }
    let mut bad_stream = stream;
    bad_stream.tokens[0].end = u64::MAX;
    let n = node(0, NodeKind::TagPair);
    assert!(
        extraction::node_range(&n, &bad_stream, b"x").is_err(),
        "byte u64 massimo accettato"
    );
}

#[test]
fn persistent_validation_rejects_labeled_identity_order_range_and_decode_mutations() {
    type Mutation = (&'static str, Box<dyn Fn(&mut CatalogManifestV1)>);
    let f = fixture(b"[Event \"X\"]\n[White \"A\"]\n\n1. e4 *\n");
    let base = load_catalog(
        &f.root,
        &create_catalog(&f.root, &f.interpretation).unwrap().path,
    )
    .unwrap()
    .manifest;
    let mutations: Vec<Mutation> = vec![
        ("schema errato", Box::new(|m| m.schema_version = 2)),
        (
            "profilo errato",
            Box::new(|m| m.catalog_profile = "bad".into()),
        ),
        (
            "Interpretation assoluta",
            Box::new(|m| m.interpretation_manifest = "/tmp/x".into()),
        ),
        (
            "Interpretation traversal",
            Box::new(|m| m.interpretation_manifest = "../x".into()),
        ),
        (
            "hash Interpretation",
            Box::new(|m| m.interpretation_manifest_blake3 = "x".into()),
        ),
        (
            "byte Interpretation",
            Box::new(|m| m.interpretation_manifest_bytes = 0),
        ),
        (
            "hash Parse",
            Box::new(|m| m.parse_manifest_blake3 = "x".into()),
        ),
        ("byte Parse", Box::new(|m| m.parse_manifest_bytes = 0)),
        ("hash Source", Box::new(|m| m.source_blake3 = "x".into())),
        ("byte Source", Box::new(|m| m.source_bytes = 0)),
        ("game id", Box::new(|m| m.games[0].id = 1)),
        (
            "interpretation_game_id",
            Box::new(|m| m.games[0].interpretation_game_id = 1),
        ),
        (
            "parse_tree_index",
            Box::new(|m| m.games[0].parse_tree_index = 1),
        ),
        (
            "segment_ordinal duplicato",
            Box::new(|m| {
                let mut g = m.games[0].clone();
                g.id = 1;
                g.parse_tree_index = 1;
                m.games.push(g);
            }),
        ),
        ("ordinal tag", Box::new(|m| m.games[0].tags[0].ordinal = 1)),
        (
            "node name duplicato",
            Box::new(|m| {
                let id = m.games[0].tags[0].name_node_ids[0];
                m.games[0].tags[0].name_node_ids.push(id)
            }),
        ),
        (
            "node value duplicato",
            Box::new(|m| {
                let id = m.games[0].tags[0].value_node_ids[0];
                m.games[0].tags[0].value_node_ids.push(id);
            }),
        ),
        (
            "section_ordinal decrescente",
            Box::new(|m| {
                m.games[0].tags[0].section_ordinal = 1;
                m.games[0].tags[1].section_ordinal = 0;
            }),
        ),
        (
            "range opzionale incompleto",
            Box::new(|m| m.games[0].tags[0].name_end_token = None),
        ),
        (
            "range candidato esterno",
            Box::new(|m| m.games[0].tags[0].name_start_byte = Some(u64::MAX)),
        ),
        (
            "decoded incoerente",
            Box::new(|m| m.games[0].tags[0].value = None),
        ),
        (
            "missing_name incoerente",
            Box::new(|m| m.games[0].tags[0].decode_status = TagDecodeStatusV1::MissingName),
        ),
        (
            "multiple_names incoerente",
            Box::new(|m| m.games[0].tags[0].decode_status = TagDecodeStatusV1::AmbiguousName),
        ),
        (
            "missing_value incoerente",
            Box::new(|m| m.games[0].tags[0].decode_status = TagDecodeStatusV1::MissingValue),
        ),
        (
            "multiple_values incoerente",
            Box::new(|m| m.games[0].tags[0].decode_status = TagDecodeStatusV1::AmbiguousValue),
        ),
        (
            "invalid_name incoerente",
            Box::new(|m| m.games[0].tags[0].decode_status = TagDecodeStatusV1::InvalidNameEncoding),
        ),
        (
            "invalid_value incoerente",
            Box::new(|m| {
                m.games[0].tags[0].decode_status = TagDecodeStatusV1::InvalidValueEncoding
            }),
        ),
        (
            "invalid_escape incoerente",
            Box::new(|m| m.games[0].tags[0].decode_status = TagDecodeStatusV1::InvalidEscape),
        ),
        (
            "unexpected incoerente",
            Box::new(|m| m.games[0].tags[0].decode_status = TagDecodeStatusV1::InvalidStructure),
        ),
    ];
    for (label, mutate) in mutations {
        let mut candidate = base.clone();
        mutate(&mut candidate);
        assert!(
            validation::validate(&candidate).is_err(),
            "mutazione accettata: {label}"
        );
    }
}

#[test]
fn validation_rejects_duplicate_tag_pair_node_id_independently() {
    let f = fixture(b"[Event \"X\"]\n[White \"A\"]\n\n1. e4 *\n");
    let mut manifest = load_catalog(
        &f.root,
        &create_catalog(&f.root, &f.interpretation).unwrap().path,
    )
    .unwrap()
    .manifest;
    assert!(manifest.games[0].tags.len() >= 2);
    let duplicate = manifest.games[0].tags[0].tag_pair_node_id;
    manifest.games[0].tags[1].tag_pair_node_id = duplicate;
    assert!(
        validation::validate(&manifest).is_err(),
        "tag_pair_node_id duplicato accettato nonostante ordinal e candidati distinti"
    );
}

#[test]
fn verified_loader_requires_exact_canonical_catalog_path() {
    let f = fixture(b"[Event \"X\"]\n\n1. e4 *\n");
    let catalog = create_catalog(&f.root, &f.interpretation).unwrap();
    assert!(
        verify_catalog(&f.root, &catalog.path).is_ok(),
        "percorso canonico rifiutato"
    );
    let bytes = fs::read(f.root.join(&catalog.path)).unwrap();
    fs::create_dir(f.root.join("copies")).unwrap();
    let copy = PathBuf::from("copies/catalog-copy.json");
    fs::write(f.root.join(&copy), &bytes).unwrap();
    assert!(
        verify_catalog(&f.root, &copy).is_err(),
        "copia byte-identica in altra directory accettata"
    );
    fs::create_dir(f.root.join("inspector/altro")).unwrap();
    let correct_name =
        PathBuf::from("inspector/altro").join(format!("{}.json", catalog.manifest_blake3));
    fs::write(f.root.join(&correct_name), bytes).unwrap();
    assert!(
        verify_catalog(&f.root, &correct_name).is_err(),
        "nome hash corretto sotto parent diverso accettato"
    );
}

#[test]
fn singular_ranges_require_exactly_one_candidate_and_missing_value_accepts_incomplete_candidate() {
    let f = fixture(b"[Event \"X\"]\n\n1. e4 *\n");
    let base = load_catalog(
        &f.root,
        &create_catalog(&f.root, &f.interpretation).unwrap().path,
    )
    .unwrap()
    .manifest;
    let mut m = base.clone();
    let t = &mut m.games[0].tags[0];
    t.name_node_ids.push(99);
    t.name = None;
    t.value = None;
    t.decode_status = TagDecodeStatusV1::AmbiguousName;
    t.name_start_token = None;
    t.name_end_token = None;
    t.name_start_byte = None;
    t.name_end_byte = None;
    assert!(
        validation::validate(&m).is_ok(),
        "nomi multipli con range null"
    );
    let mut m = base.clone();
    let t = &mut m.games[0].tags[0];
    t.name_node_ids.truncate(1);
    t.name = Some("Event".into());
    t.decode_status = TagDecodeStatusV1::MissingValue;
    t.value_node_ids.truncate(1);
    t.value = None;
    assert!(
        validation::validate(&m).is_ok(),
        "MissingValue con candidato incompleto"
    );
    let mut m = base;
    let t = &mut m.games[0].tags[0];
    t.value_node_ids.push(100);
    t.decode_status = TagDecodeStatusV1::AmbiguousValue;
    t.value = None;
    t.value_start_token = None;
    t.value_end_token = None;
    t.value_start_byte = None;
    t.value_end_byte = None;
    assert!(
        validation::validate(&m).is_ok(),
        "valori multipli con range null"
    );
    assert_eq!(
        serde_json::to_string(&TagDecodeStatusV1::AmbiguousName).unwrap(),
        "\"ambiguous_name\""
    );
    assert_eq!(
        serde_json::to_string(&TagDecodeStatusV1::AmbiguousValue).unwrap(),
        "\"ambiguous_value\""
    );
    assert_eq!(
        serde_json::to_string(&TagDecodeStatusV1::InvalidStructure).unwrap(),
        "\"invalid_structure\""
    );
}

#[test]
fn extraction_never_selects_first_of_multiple_candidates_and_preserves_exact_unique_ranges() {
    use crate::inspector::parsing::model::{NodeKind, NodeStatus};
    use crate::inspector::tokenization::{Token, TokenKind, TokenStream};
    let blob = "[Event \"Città\"]".as_bytes();
    let tokens = vec![
        Token {
            ordinal: 0,
            start: 0,
            end: 1,
            kind: TokenKind::LeftBracket,
        },
        Token {
            ordinal: 1,
            start: 1,
            end: 6,
            kind: TokenKind::Symbol,
        },
        Token {
            ordinal: 2,
            start: 6,
            end: 7,
            kind: TokenKind::Whitespace,
        },
        Token {
            ordinal: 3,
            start: 7,
            end: 15,
            kind: TokenKind::QuotedString,
        },
        Token {
            ordinal: 4,
            start: 15,
            end: 16,
            kind: TokenKind::RightBracket,
        },
    ];
    let stream = TokenStream {
        segment_ordinal: 0,
        start: 0,
        end: 16,
        tokens,
    };
    let section = crate::inspector::parsing::model::FlatNode {
        id: 1,
        parent_id: Some(0),
        child_ordinal: Some(0),
        kind: NodeKind::TagSection,
        status: NodeStatus::Valid,
        start_token: 0,
        end_token: 5,
    };
    let pair = crate::inspector::parsing::model::FlatNode {
        id: 2,
        parent_id: Some(1),
        child_ordinal: Some(0),
        kind: NodeKind::TagPair,
        status: NodeStatus::Valid,
        start_token: 0,
        end_token: 5,
    };
    let name = crate::inspector::parsing::model::FlatNode {
        id: 3,
        parent_id: Some(2),
        child_ordinal: Some(1),
        kind: NodeKind::TagNameCandidate,
        status: NodeStatus::Valid,
        start_token: 1,
        end_token: 2,
    };
    let value = crate::inspector::parsing::model::FlatNode {
        id: 4,
        parent_id: Some(2),
        child_ordinal: Some(3),
        kind: NodeKind::TagValueCandidate,
        status: NodeStatus::Valid,
        start_token: 3,
        end_token: 4,
    };
    let unique =
        extraction::extract_tag(0, 0, &section, &pair, &[&name, &value], &stream, blob).unwrap();
    assert_eq!(
        (
            unique.name_start_token,
            unique.name_end_token,
            unique.name_start_byte,
            unique.name_end_byte
        ),
        (Some(1), Some(2), Some(1), Some(6)),
        "intervallo nome unico esatto"
    );
    assert_eq!(
        (
            unique.value_start_token,
            unique.value_end_token,
            unique.value_start_byte,
            unique.value_end_byte
        ),
        (Some(3), Some(4), Some(7), Some(15)),
        "intervallo valore unico esatto"
    );
    assert_eq!(
        &blob[unique.name_start_byte.unwrap() as usize..unique.name_end_byte.unwrap() as usize],
        b"Event",
        "range nome ricostruisce byte"
    );
    assert_eq!(
        &blob[unique.value_start_byte.unwrap() as usize..unique.value_end_byte.unwrap() as usize],
        "\"Città\"".as_bytes(),
        "range valore ricostruisce byte"
    );
    let name2 = crate::inspector::parsing::model::FlatNode {
        id: 5,
        ..name.clone()
    };
    let multiple_names = extraction::extract_tag(
        0,
        0,
        &section,
        &pair,
        &[&name, &name2, &value],
        &stream,
        blob,
    )
    .unwrap();
    assert_eq!(multiple_names.name_node_ids, vec![3, 5]);
    assert_eq!(
        (
            multiple_names.name_start_token,
            multiple_names.name_end_token,
            multiple_names.name_start_byte,
            multiple_names.name_end_byte
        ),
        (None, None, None, None),
        "range nomi multipli null"
    );
    let value2 = crate::inspector::parsing::model::FlatNode {
        id: 6,
        ..value.clone()
    };
    let multiple_values = extraction::extract_tag(
        0,
        0,
        &section,
        &pair,
        &[&name, &value, &value2],
        &stream,
        blob,
    )
    .unwrap();
    assert_eq!(multiple_values.value_node_ids, vec![4, 6]);
    assert_eq!(
        (
            multiple_values.value_start_token,
            multiple_values.value_end_token,
            multiple_values.value_start_byte,
            multiple_values.value_end_byte
        ),
        (None, None, None, None),
        "range valori multipli null"
    );
}

#[test]
fn query_ignores_utcdate_as_a_chronological_field() {
    let f=fixture("[Event \"Open Uno\"]\n[Date \"2026.01.01\"]\n[White \"Àlice Rossi\"]\n[Black \"Bob\"]\n[Result \"1-0\"]\n[White \"Alias\"]\n\n1. e4 1-0\n\n[Event \"Open Due\"]\n[UTCDate \"2026.01.02\"]\n[White \"Carol\"]\n[Black \"ALICE\"]\n[Result \"0-1\"]\n\n1. d4 0-1\n".as_bytes());
    let c = create_catalog(&f.root, &f.interpretation).unwrap();
    struct Case {
        label: &'static str,
        player: Option<&'static str>,
        event: Option<&'static str>,
        date: Option<&'static str>,
        total: &'static str,
    }
    for case in [
        Case {
            label: "nessun filtro",
            player: None,
            event: None,
            date: None,
            total: "2",
        },
        Case {
            label: "White substring Unicode",
            player: Some("àLICE"),
            event: None,
            date: None,
            total: "1",
        },
        Case {
            label: "Black case insensitive",
            player: Some("alice"),
            event: None,
            date: None,
            total: "1",
        },
        Case {
            label: "event substring",
            player: None,
            event: Some("open"),
            date: None,
            total: "2",
        },
        Case {
            label: "Date esatta",
            player: None,
            event: None,
            date: Some("2026.01.01"),
            total: "1",
        },
        Case {
            label: "UTCDate non interrogabile",
            player: None,
            event: None,
            date: Some("2026.01.02"),
            total: "0",
        },
        Case {
            label: "nessun risultato player",
            event: None,
            date: None,
            player: Some("Nobody"),
            total: "0",
        },
        Case {
            label: "filtri AND",
            player: Some("Bob"),
            event: Some("Due"),
            date: None,
            total: "0",
        },
        Case {
            label: "nessun risultato",
            player: Some("Nobody"),
            event: None,
            date: None,
            total: "0",
        },
    ] {
        let q = Query {
            root: f.root.clone(),
            catalog: c.path.clone(),
            filters: QueryFilters {
                player: case.player.map(str::to_owned),
                event: case.event.map(str::to_owned),
                date: case
                    .date
                    .map(|v| query::parse_cli_date(&v.replace('.', "-")).unwrap()),
                ..QueryFilters::default()
            },
        };
        let out = query_catalog(&q).unwrap();
        assert_eq!(
            out.lines().next().unwrap(),
            format!("MATCHES\t{}", case.total),
            "query: {}",
            case.label
        );
    }
}

#[test]
fn frozen_query_contract_dates_case_duplicates_and_json_arrays() {
    let f=fixture("[white \"Àlice\\\" A\"]\n[WHITE \"Alias\\\\One\"]\n[Black \"Bob\"]\n[EVENT \"Open Città\"]\n[date \"2024.02.29\"]\n[Date \"????.??.??\"]\n[Result \"1-0\"]\n\n1. e4 1-0\n\n[White \"Carol\"]\n[Black \"ALICE\"]\n[Event \"Other\"]\n[Date \"2023.02.29\"]\n[Date \"2024.03.01\"]\n[Result \"0-1\"]\n\n1. d4 0-1\n".as_bytes());
    let c = create_catalog(&f.root, &f.interpretation).unwrap();
    let make = |player: Option<&str>,
                event: Option<&str>,
                date: Option<&str>,
                from: Option<&str>,
                to: Option<&str>| Query {
        root: f.root.clone(),
        catalog: c.path.clone(),
        filters: QueryFilters {
            player: player.map(str::to_owned),
            event: event.map(str::to_owned),
            date: date.map(|v| query::parse_cli_date(v).unwrap()),
            date_from: from.map(|v| query::parse_cli_date(v).unwrap()),
            date_to: to.map(|v| query::parse_cli_date(v).unwrap()),
        },
    };
    let all = query_catalog(&make(None, None, None, None, None)).unwrap();
    assert!(all.starts_with("MATCHES\t2\n"), "nessun filtro");
    let first = all.lines().nth(1).unwrap();
    assert!(
        first.contains("[\"Àlice\\\" A\",\"Alias\\\\One\"]"),
        "array JSON duplicati/escape: {first}"
    );
    assert!(first.contains("[\"Open Città\"]"), "EVENT case-insensitive");
    assert!(
        first.contains("[\"2024.02.29\",\"????.??.??\"]"),
        "Date duplicate preservate"
    );
    for (label, q, total) in [
        (
            "player Unicode",
            make(Some("àLICE"), None, None, None, None),
            1,
        ),
        (
            "event Unicode",
            make(None, Some("città"), None, None, None),
            1,
        ),
        (
            "data esatta",
            make(None, None, Some("2024-02-29"), None, None),
            1,
        ),
        (
            "from inclusivo",
            make(None, None, None, Some("2024-03-01"), None),
            1,
        ),
        (
            "to inclusivo",
            make(None, None, None, None, Some("2024-02-29")),
            1,
        ),
    ] {
        let out = query_catalog(&q).unwrap();
        assert!(
            out.starts_with(&format!("MATCHES\t{total}\n")),
            "{label}: {out}"
        );
    }
    assert!(query::parse_cli_date("2024-02-29").is_ok(), "bisestile");
    assert!(
        query::parse_cli_date("2023-02-29").is_err(),
        "non bisestile"
    );
    for bad in ["2024.02.29", "24-02-29", "2024-13-01", "2024-04-31"] {
        assert!(
            query::parse_cli_date(bad).is_err(),
            "data CLI accettata: {bad}"
        );
    }
    let conflict = make(None, None, Some("2024-02-29"), Some("2024-01-01"), None);
    assert!(
        query::validate_filters(&conflict.filters).is_err(),
        "date con range"
    );
    let reversed = make(None, None, None, Some("2024-03-02"), Some("2024-03-01"));
    assert!(
        query::validate_filters(&reversed.filters).is_err(),
        "range invertito"
    );
}

#[test]
fn export_game_is_byte_exact_and_refuses_existing_missing_and_external_outputs() {
    let bytes = b"[Event \"X\"]\r\n[White \"A\"]\r\n\r\n1. e4 $1 {note} (1. d4) *\r\n";
    let f = fixture(bytes);
    let c = create_catalog(&f.root, &f.interpretation).unwrap();
    let game = &load_catalog(&f.root, &c.path).unwrap().manifest.games[0];
    let expected = &bytes[game.source_start as usize..game.source_end as usize];
    let output = f.root.join("game.pgn");
    let exported = export_game(&f.root, &c.path, 0, &output).unwrap();
    assert_eq!(
        fs::read(&output).unwrap(),
        expected,
        "CRLF/commenti/NAG/RAV byte-esatti"
    );
    assert_eq!(exported.blake3, blake3::hash(expected).to_hex().to_string());
    assert_eq!(exported.bytes, expected.len() as u64);
    assert!(
        export_game(&f.root, &c.path, 0, &output).is_err(),
        "output sovrascritto"
    );
    assert_eq!(
        fs::read(&output).unwrap(),
        expected,
        "output esistente modificato"
    );
    assert!(
        export_game(&f.root, &c.path, 99, &f.root.join("missing.pgn")).is_err(),
        "game-id inesistente"
    );
    let outside = f.root.with_extension("outside-export.pgn");
    assert!(
        export_game(&f.root, &c.path, 0, &outside).is_err(),
        "parent esterno accettato"
    );
    assert!(
        export_game(&f.root, &c.path, 0, Path::new("../traversal.pgn")).is_err(),
        "output traversal accettato"
    );
    assert_eq!(temporary_count(&f.root), 0, "temporaneo catalog residuo");
    assert_eq!(
        fs::read_dir(&f.root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e
                .file_name()
                .to_string_lossy()
                .contains(".catalog-game-export"))
            .count(),
        0,
        "temporaneo export residuo"
    );
}

#[cfg(unix)]
#[test]
fn export_game_rejects_output_symlink_without_modifying_target() {
    use std::os::unix::fs::symlink;
    let f = fixture(b"[Event \"X\"]\n\n1. e4 *\n");
    let c = create_catalog(&f.root, &f.interpretation).unwrap();
    let outside = f.root.with_extension("outside-export-target");
    fs::write(&outside, b"outside").unwrap();
    let link = f.root.join("game-link.pgn");
    symlink(&outside, &link).unwrap();
    assert!(
        export_game(&f.root, &c.path, 0, &link).is_err(),
        "output symlink accettato"
    );
    assert_eq!(fs::read(&outside).unwrap(), b"outside");
    fs::remove_file(outside).unwrap();
}

#[test]
fn exact_recalculation_rejects_every_labeled_catalog_record_mutation() {
    type Mutation = (&'static str, Box<dyn Fn(&mut CatalogManifestV1)>);
    let f = fixture(
        b"[Event \"One\"]\n[White \"A\"]\n\n1. e4 *\n\n[Event \"Two\"]\n[White \"B\"]\n\n1. d4 *\n",
    );
    let c = create_catalog(&f.root, &f.interpretation).unwrap();
    let path = f.root.join(&c.path);
    let canonical = crate::inspector::common::logical_json_bytes(&path, "catalog test").unwrap();
    let mutations: Vec<Mutation> = vec![
        (
            "nome",
            Box::new(|m| m.games[0].tags[0].name = Some("Site".into())),
        ),
        (
            "valore",
            Box::new(|m| m.games[0].tags[0].value = Some("Changed".into())),
        ),
        (
            "decode_status",
            Box::new(|m| {
                m.games[0].tags[0].decode_status = TagDecodeStatusV1::InvalidEscape;
                m.games[0].tags[0].value = None;
            }),
        ),
        (
            "node_status",
            Box::new(|m| m.games[0].tags[0].node_status = NodeStatusV1::Recovered),
        ),
        (
            "ID nodo",
            Box::new(|m| m.games[0].tags[0].tag_pair_node_id += 1),
        ),
        (
            "intervallo token",
            Box::new(|m| m.games[0].tags[0].start_token += 1),
        ),
        (
            "intervallo byte",
            Box::new(|m| m.games[0].tags[0].start_byte += 1),
        ),
        (
            "intervallo partita",
            Box::new(|m| m.games[0].source_start += 1),
        ),
        (
            "hash segmento",
            Box::new(|m| m.games[0].source_game_blake3 = "0".repeat(64)),
        ),
        (
            "ordine giochi",
            Box::new(|m| {
                m.games.swap(0, 1);
                for (i, g) in m.games.iter_mut().enumerate() {
                    g.id = i as u64;
                    g.parse_tree_index = i as u64;
                }
            }),
        ),
        (
            "ordine tag",
            Box::new(|m| {
                m.games[0].tags.swap(0, 1);
                for (i, t) in m.games[0].tags.iter_mut().enumerate() {
                    t.ordinal = i as u64;
                }
            }),
        ),
        (
            "percorso Interpretation",
            Box::new(|m| m.interpretation_manifest = "inspector/interpretations/alias.json".into()),
        ),
    ];
    let pristine = load_catalog(&f.root, &c.path).unwrap().manifest;
    for (label, mutate) in mutations {
        let mut m = pristine.clone();
        mutate(&mut m);
        fs::write(&path, serialized(&m).unwrap()).unwrap();
        assert!(
            verify_catalog(&f.root, &c.path).is_err(),
            "ricalcolo ha accettato: {label}"
        );
    }
    let mut value: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unknown".into(), serde_json::Value::Bool(true));
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    assert!(
        verify_catalog(&f.root, &c.path).is_err(),
        "campo sconosciuto accettato"
    );
    fs::write(&path, serde_json::to_vec(&pristine).unwrap()).unwrap();
    assert!(
        verify_catalog(&f.root, &c.path).is_err(),
        "whitespace diverso accettato"
    );
    let mut reordered = String::from_utf8(canonical.clone()).unwrap();
    reordered = reordered.replacen(
        "  \"schema_version\": 1,\n  \"catalog_profile\": \"antiskill_game_catalog_v1\",",
        "  \"catalog_profile\": \"antiskill_game_catalog_v1\",\n  \"schema_version\": 1,",
        1,
    );
    fs::write(&path, reordered).unwrap();
    assert!(
        verify_catalog(&f.root, &c.path).is_err(),
        "ordine campi diverso accettato"
    );
    fs::write(&path, &canonical[..canonical.len() - 1]).unwrap();
    assert!(
        verify_catalog(&f.root, &c.path).is_err(),
        "newline diversa accettata"
    );
}

#[cfg(unix)]
#[test]
fn publication_and_catalog_symlink_safety_are_exercised_without_temporary_leaks() {
    use std::os::unix::fs::symlink;
    let f = fixture(b"[Event \"X\"]\n\n1. e4 *\n");
    let first = create_catalog(&f.root, &f.interpretation).unwrap();
    let second = create_catalog(&f.root, &f.interpretation).unwrap();
    assert!(!second.new_manifest, "target identico riusato");
    assert_eq!(
        (
            first.path.clone(),
            first.manifest_blake3.clone(),
            first.manifest_bytes
        ),
        (second.path, second.manifest_blake3, second.manifest_bytes),
        "stesso path/hash/byte"
    );
    let target = f.root.join(&first.path);
    let first_manifest = load_catalog(&f.root, &first.path).unwrap().manifest;
    fs::write(&target, b"different").unwrap();
    assert!(
        create_catalog(&f.root, &f.interpretation).is_err(),
        "target differente accettato"
    );
    assert_eq!(temporary_count(&f.root), 0, "temporaneo dopo collisione");
    fs::remove_file(&target).unwrap();
    let outside = f.root.with_extension("catalog-target-outside");
    fs::write(&outside, b"outside").unwrap();
    symlink(&outside, &target).unwrap();
    assert!(
        create_catalog(&f.root, &f.interpretation).is_err(),
        "target symlink accettato"
    );
    assert_eq!(
        fs::read(&outside).unwrap(),
        b"outside",
        "target esterno modificato"
    );
    assert_eq!(
        temporary_count(&f.root),
        0,
        "temporaneo dopo target symlink"
    );
    fs::remove_file(&target).unwrap();
    let manifest_outside = f.root.with_extension("catalog-manifest-outside");
    fs::write(&manifest_outside, serialized(&first_manifest).unwrap()).unwrap();
    let manifest_link = f.root.join("catalog-link.json");
    symlink(&manifest_outside, &manifest_link).unwrap();
    assert!(
        verify_catalog(&f.root, Path::new("catalog-link.json")).is_err(),
        "manifest Catalog symlink esterno accettato"
    );
    fs::remove_file(manifest_link).unwrap();
    fs::remove_file(outside).unwrap();
    fs::remove_file(manifest_outside).unwrap();
}

#[cfg(unix)]
#[test]
fn catalog_directory_components_reject_symlinks_before_external_writes() {
    use std::os::unix::fs::symlink;
    for (label, catalogs) in [("inspector", false), ("catalogs", true)] {
        let root = test_root_for_catalog(label);
        let outside = root.with_extension("outside");
        if catalogs {
            fs::create_dir_all(root.join("inspector")).unwrap();
        } else {
            fs::create_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&outside).unwrap();
        let link = if catalogs {
            root.join("inspector/catalogs")
        } else {
            root.join("inspector")
        };
        symlink(&outside, link).unwrap();
        let canonical = fs::canonicalize(&root).unwrap();
        assert!(
            crate::inspector::common::prepare_directory_chain(
                &root,
                &canonical,
                &[
                    ("inspector", "directory inspector"),
                    ("catalogs", "directory catalogs")
                ]
            )
            .is_err(),
            "componente symlink accettato: {label}"
        );
        assert_eq!(
            fs::read_dir(&outside).unwrap().count(),
            0,
            "scrittura esterna: {label}"
        );
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }
}

fn temporary_count(root: &Path) -> usize {
    fs::read_dir(root.join("inspector/catalogs"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
                .count()
        })
        .unwrap_or(0)
}
fn test_root_for_catalog(label: &str) -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "antiskill-catalog-{label}-{}-{n}",
        std::process::id()
    ))
}

#[test]
fn extraction_preserves_multiple_direct_sections_and_ignores_root_tag_pair() {
    use crate::inspector::parsing::model::{FlatNode, NodeKind, NodeStatus, ParseTree};
    use crate::inspector::tokenization::{Token, TokenKind, TokenStream};
    let blob = b"[Event \"A\"][White \"B\"]";
    let spans = [
        (0, 1),
        (1, 6),
        (6, 7),
        (7, 10),
        (10, 11),
        (11, 12),
        (12, 17),
        (17, 18),
        (18, 21),
        (21, 22),
    ];
    let tokens = spans
        .iter()
        .enumerate()
        .map(|(i, (start, end))| Token {
            ordinal: i as u64,
            start: *start,
            end: *end,
            kind: TokenKind::Symbol,
        })
        .collect();
    let make = |id, parent, kind, start, end| FlatNode {
        id,
        parent_id: parent,
        child_ordinal: Some(id),
        kind,
        status: NodeStatus::Valid,
        start_token: start,
        end_token: end,
    };
    let nodes = vec![
        make(0, None, NodeKind::GameCandidate, 0, 10),
        make(1, Some(0), NodeKind::TagSection, 0, 5),
        make(2, Some(1), NodeKind::TagPair, 0, 5),
        make(3, Some(2), NodeKind::TagNameCandidate, 1, 2),
        make(4, Some(2), NodeKind::TagValueCandidate, 3, 4),
        make(5, Some(0), NodeKind::TagPair, 0, 5),
        make(6, Some(0), NodeKind::TagSection, 5, 10),
        make(7, Some(6), NodeKind::TagPair, 5, 10),
        make(8, Some(7), NodeKind::TagNameCandidate, 6, 7),
        make(9, Some(7), NodeKind::TagValueCandidate, 8, 9),
    ];
    let tree = ParseTree {
        segment_ordinal: 0,
        token_count: 10,
        nodes,
        diagnostics: Vec::new(),
    };
    let stream = TokenStream {
        segment_ordinal: 0,
        start: 0,
        end: 22,
        tokens,
    };
    let tags = extraction::extract_tags(&tree, &stream, blob).unwrap();
    assert_eq!(tags.len(), 2, "TagPair fuori sezione non promosso");
    assert_eq!(
        tags.iter().map(|t| t.section_ordinal).collect::<Vec<_>>(),
        vec![0, 1],
        "TagSection diretti in ordine"
    );
    assert_eq!(
        tags.iter()
            .filter_map(|t| t.name.as_deref())
            .collect::<Vec<_>>(),
        vec!["Event", "White"],
        "ordine tag sorgente"
    );
}

#[test]
fn extraction_rejects_broken_one_to_one_alignment_segment_and_source_bounds() {
    let f = fixture(b"[Event \"X\"]\n\n1. e4 *\n");
    let verified =
        interpretation::load_verified_interpretation(&f.root, &f.interpretation).unwrap();
    let mut missing = verified;
    missing.manifest.games.clear();
    assert!(
        extraction::extract_games(&missing).is_err(),
        "relazione uno-a-uno mancante accettata"
    );
    let verified =
        interpretation::load_verified_interpretation(&f.root, &f.interpretation).unwrap();
    let mut segment = verified;
    segment.parse.tokenization.manifest.streams[0].segment_ordinal += 1;
    assert!(
        extraction::extract_games(&segment).is_err(),
        "segment_ordinal incoerente accettato"
    );
    let verified =
        interpretation::load_verified_interpretation(&f.root, &f.interpretation).unwrap();
    let mut bounds = verified;
    bounds.parse.tokenization.manifest.streams[0].end = u64::MAX;
    assert!(
        extraction::extract_games(&bounds).is_err(),
        "intervallo oltre SourceBlob accettato"
    );
}
