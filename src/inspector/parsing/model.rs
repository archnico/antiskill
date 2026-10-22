use serde::{Deserialize, Serialize};

pub(super) const PARSE_SCHEMA_VERSION: u32 = 1;
pub(in crate::inspector) const MAX_VARIATION_DEPTH: usize = 256;
pub(super) const MAX_ARENA_DEPTH: usize = MAX_VARIATION_DEPTH * 2 + 2;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(in crate::inspector) struct ParseManifest {
    pub(in crate::inspector) schema_version: u32,
    pub(in crate::inspector) tokenization_manifest: String,
    pub(in crate::inspector) tokenization_manifest_blake3: String,
    pub(in crate::inspector) tokenization_manifest_bytes: u64,
    pub(in crate::inspector) source_blake3: String,
    pub(in crate::inspector) source_bytes: u64,
    pub(in crate::inspector) parsed_at_unix_ms: u128,
    pub(in crate::inspector) trees: Vec<ParseTree>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(in crate::inspector) struct ParseTree {
    pub(in crate::inspector) segment_ordinal: u64,
    pub(in crate::inspector) token_count: u64,
    pub(in crate::inspector) nodes: Vec<FlatNode>,
    pub(in crate::inspector) diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(in crate::inspector) struct FlatNode {
    pub(in crate::inspector) id: u64,
    pub(in crate::inspector) parent_id: Option<u64>,
    pub(in crate::inspector) child_ordinal: Option<u64>,
    pub(in crate::inspector) kind: NodeKind,
    pub(in crate::inspector) status: NodeStatus,
    pub(in crate::inspector) start_token: u64,
    pub(in crate::inspector) end_token: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(in crate::inspector) struct Diagnostic {
    pub(in crate::inspector) ordinal: u64,
    pub(in crate::inspector) code: DiagnosticCode,
    pub(in crate::inspector) node_id: u64,
    pub(in crate::inspector) start_token: u64,
    pub(in crate::inspector) end_token: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::inspector) enum NodeKind {
    GameCandidate,
    TagSection,
    TagPair,
    Movetext,
    Variation,
    Delimiter,
    Trivia,
    EscapeLine,
    TagNameCandidate,
    TagValueCandidate,
    MoveNumber,
    MoveSymbolCandidate,
    Result,
    Nag,
    Comment,
    Unparsed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::inspector) enum NodeStatus {
    Valid,
    Recovered,
    Incomplete,
    Unparsed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::inspector) enum DiagnosticCode {
    MalformedTagPair,
    UnterminatedTagPair,
    UnterminatedTagValue,
    UnterminatedComment,
    UnterminatedVariation,
    EmptyVariation,
    StrayRightParenthesis,
    TagPairOutsideTagSection,
    UnexpectedTokenRange,
}
