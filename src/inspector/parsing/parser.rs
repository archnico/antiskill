use super::model::{
    Diagnostic, DiagnosticCode, FlatNode, NodeKind, NodeStatus, ParseTree, MAX_VARIATION_DEPTH,
};
use crate::inspector::tokenization::{Token, TokenKind, TokenStream};
use std::collections::HashMap;

#[derive(Clone)]
struct RecursiveNode {
    temp_id: u64,
    kind: NodeKind,
    status: NodeStatus,
    start: usize,
    end: usize,
    children: Vec<RecursiveNode>,
}

struct PendingDiagnostic {
    creation_ordinal: u64,
    code: DiagnosticCode,
    node_temp_id: u64,
    start: usize,
    end: usize,
}

struct Parser<'a> {
    tokens: &'a [Token],
    next_temp_id: u64,
    diagnostics: Vec<PendingDiagnostic>,
}

enum TagAttempt {
    Parsed(RecursiveNode),
    Malformed { end: usize },
    NotTag,
}

pub(super) fn parse_stream(stream: &TokenStream) -> Result<ParseTree, String> {
    let mut parser = Parser {
        tokens: &stream.tokens,
        next_temp_id: 0,
        diagnostics: Vec::new(),
    };
    let root = parser.parse_root();
    flatten_tree(
        stream.segment_ordinal,
        stream.tokens.len(),
        root,
        parser.diagnostics,
    )
}

impl<'a> Parser<'a> {
    fn parse_root(&mut self) -> RecursiveNode {
        let end = self.tokens.len();
        let mut children = Vec::new();
        let mut cursor = 0;
        if let Some((tag_section, next)) = self.parse_tag_section() {
            children.push(tag_section);
            cursor = next;
        }
        if cursor < end {
            children.push(self.parse_movetext(cursor, end, false, 0).0);
        }
        self.container(NodeKind::GameCandidate, 0, end, children, NodeStatus::Valid)
    }

    fn parse_tag_section(&mut self) -> Option<(RecursiveNode, usize)> {
        let limit = self.tokens.len();
        let first = match self.try_tag_pair(0, limit) {
            TagAttempt::Parsed(node) => node,
            TagAttempt::Malformed { .. } | TagAttempt::NotTag => return None,
        };
        let mut cursor = first.end;
        let mut children = vec![first];
        loop {
            let whitespace_start = cursor;
            while cursor < self.tokens.len() && self.tokens[cursor].kind == TokenKind::Whitespace {
                cursor += 1;
            }
            match self.try_tag_pair(cursor, limit) {
                TagAttempt::Parsed(pair) => {
                    if whitespace_start < cursor {
                        children.push(self.leaf(
                            NodeKind::Trivia,
                            NodeStatus::Valid,
                            whitespace_start,
                            cursor,
                        ));
                    }
                    cursor = pair.end;
                    children.push(pair);
                }
                TagAttempt::Malformed { .. } | TagAttempt::NotTag => {
                    cursor = whitespace_start;
                    break;
                }
            }
        }
        let end = children.last().expect("tag section non vuota").end;
        Some((
            self.container(NodeKind::TagSection, 0, end, children, NodeStatus::Valid),
            cursor,
        ))
    }

    fn try_tag_pair(&mut self, start: usize, limit: usize) -> TagAttempt {
        if start >= limit || self.kind(start) != Some(TokenKind::LeftBracket) {
            return TagAttempt::NotTag;
        }
        let mut cursor = start + 1;
        let mut parts = vec![self.leaf(NodeKind::Delimiter, NodeStatus::Valid, start, start + 1)];
        while self.kind_before(cursor, limit) == Some(TokenKind::Whitespace) {
            parts.push(self.leaf(NodeKind::Trivia, NodeStatus::Valid, cursor, cursor + 1));
            cursor += 1;
        }
        if self.kind_before(cursor, limit) != Some(TokenKind::Symbol) {
            return TagAttempt::Malformed {
                end: self.bracket_led_end(start, limit),
            };
        }
        parts.push(self.leaf(
            NodeKind::TagNameCandidate,
            NodeStatus::Valid,
            cursor,
            cursor + 1,
        ));
        cursor += 1;
        let whitespace_start = cursor;
        while self.kind_before(cursor, limit) == Some(TokenKind::Whitespace) {
            cursor += 1;
        }
        if cursor == whitespace_start {
            return TagAttempt::Malformed {
                end: self.bracket_led_end(start, limit),
            };
        }
        parts.push(self.leaf(
            NodeKind::Trivia,
            NodeStatus::Valid,
            whitespace_start,
            cursor,
        ));
        match self.kind_before(cursor, limit) {
            Some(TokenKind::UnterminatedQuotedString) => {
                parts.push(self.leaf(
                    NodeKind::TagValueCandidate,
                    NodeStatus::Incomplete,
                    cursor,
                    cursor + 1,
                ));
                let end = cursor + 1;
                let node =
                    self.container(NodeKind::TagPair, start, end, parts, NodeStatus::Incomplete);
                self.diagnostic(&node, DiagnosticCode::UnterminatedTagValue, cursor, end);
                TagAttempt::Parsed(node)
            }
            Some(TokenKind::QuotedString) => {
                parts.push(self.leaf(
                    NodeKind::TagValueCandidate,
                    NodeStatus::Valid,
                    cursor,
                    cursor + 1,
                ));
                cursor += 1;
                while self.kind_before(cursor, limit) == Some(TokenKind::Whitespace) {
                    parts.push(self.leaf(NodeKind::Trivia, NodeStatus::Valid, cursor, cursor + 1));
                    cursor += 1;
                }
                if self.kind_before(cursor, limit) == Some(TokenKind::RightBracket) {
                    parts.push(self.leaf(
                        NodeKind::Delimiter,
                        NodeStatus::Valid,
                        cursor,
                        cursor + 1,
                    ));
                    TagAttempt::Parsed(self.container(
                        NodeKind::TagPair,
                        start,
                        cursor + 1,
                        parts,
                        NodeStatus::Valid,
                    ))
                } else if cursor == limit {
                    let node = self.container(
                        NodeKind::TagPair,
                        start,
                        cursor,
                        parts,
                        NodeStatus::Incomplete,
                    );
                    self.diagnostic(&node, DiagnosticCode::UnterminatedTagPair, start, cursor);
                    TagAttempt::Parsed(node)
                } else {
                    TagAttempt::Malformed {
                        end: self.bracket_led_end(start, limit),
                    }
                }
            }
            _ => TagAttempt::Malformed {
                end: self.bracket_led_end(start, limit),
            },
        }
    }

    fn parse_movetext(
        &mut self,
        start: usize,
        end: usize,
        stop_at_right: bool,
        variation_depth: usize,
    ) -> (RecursiveNode, usize) {
        let mut children = Vec::new();
        let mut cursor = start;
        while cursor < end {
            if stop_at_right && self.kind(cursor) == Some(TokenKind::RightParenthesis) {
                break;
            }
            let node = match self.kind(cursor).expect("cursor entro tokens") {
                TokenKind::Whitespace => {
                    self.leaf(NodeKind::Trivia, NodeStatus::Valid, cursor, cursor + 1)
                }
                TokenKind::EscapeLine => {
                    self.leaf(NodeKind::EscapeLine, NodeStatus::Valid, cursor, cursor + 1)
                }
                TokenKind::BraceComment | TokenKind::SemicolonComment => {
                    self.leaf(NodeKind::Comment, NodeStatus::Valid, cursor, cursor + 1)
                }
                TokenKind::UnterminatedBraceComment => {
                    let node = self.leaf(
                        NodeKind::Comment,
                        NodeStatus::Incomplete,
                        cursor,
                        cursor + 1,
                    );
                    self.diagnostic(
                        &node,
                        DiagnosticCode::UnterminatedComment,
                        cursor,
                        cursor + 1,
                    );
                    node
                }
                TokenKind::MoveNumberCandidate => {
                    self.leaf(NodeKind::MoveNumber, NodeStatus::Valid, cursor, cursor + 1)
                }
                TokenKind::Symbol => self.leaf(
                    NodeKind::MoveSymbolCandidate,
                    NodeStatus::Valid,
                    cursor,
                    cursor + 1,
                ),
                TokenKind::ResultCandidate => {
                    self.leaf(NodeKind::Result, NodeStatus::Valid, cursor, cursor + 1)
                }
                TokenKind::NagNumeric | TokenKind::NagSymbolic => {
                    self.leaf(NodeKind::Nag, NodeStatus::Valid, cursor, cursor + 1)
                }
                TokenKind::LeftParenthesis if variation_depth < MAX_VARIATION_DEPTH => {
                    self.parse_variation(cursor, end, variation_depth + 1)
                }
                TokenKind::LeftParenthesis => {
                    let mut range_end = cursor + 1;
                    while range_end < end
                        && self.kind(range_end) == Some(TokenKind::LeftParenthesis)
                    {
                        range_end += 1;
                    }
                    let node =
                        self.leaf(NodeKind::Unparsed, NodeStatus::Unparsed, cursor, range_end);
                    self.diagnostic(
                        &node,
                        DiagnosticCode::UnexpectedTokenRange,
                        cursor,
                        range_end,
                    );
                    node
                }
                TokenKind::RightParenthesis => {
                    let node =
                        self.leaf(NodeKind::Unparsed, NodeStatus::Unparsed, cursor, cursor + 1);
                    self.diagnostic(
                        &node,
                        DiagnosticCode::StrayRightParenthesis,
                        cursor,
                        cursor + 1,
                    );
                    node
                }
                TokenKind::LeftBracket => {
                    let tag_limit = if stop_at_right {
                        self.tokens[cursor..end]
                            .iter()
                            .position(|token| token.kind == TokenKind::RightParenthesis)
                            .map_or(end, |relative| cursor + relative)
                    } else {
                        end
                    };
                    let diagnostics_before = self.diagnostics.len();
                    let (range_end, code) = match self.try_tag_pair(cursor, tag_limit) {
                        TagAttempt::Parsed(pair) => {
                            (pair.end, DiagnosticCode::TagPairOutsideTagSection)
                        }
                        TagAttempt::Malformed { end } => (end, DiagnosticCode::MalformedTagPair),
                        TagAttempt::NotTag => unreachable!(),
                    };
                    self.diagnostics.truncate(diagnostics_before);
                    let node =
                        self.leaf(NodeKind::Unparsed, NodeStatus::Unparsed, cursor, range_end);
                    self.diagnostic(&node, code, cursor, range_end);
                    node
                }
                _ => {
                    let range_end = self.unparsed_end(cursor, end, stop_at_right);
                    let node =
                        self.leaf(NodeKind::Unparsed, NodeStatus::Unparsed, cursor, range_end);
                    self.diagnostic(
                        &node,
                        DiagnosticCode::UnexpectedTokenRange,
                        cursor,
                        range_end,
                    );
                    node
                }
            };
            cursor = node.end;
            children.push(node);
        }
        (
            self.container(
                NodeKind::Movetext,
                start,
                cursor,
                children,
                NodeStatus::Valid,
            ),
            cursor,
        )
    }

    fn parse_variation(
        &mut self,
        start: usize,
        context_end: usize,
        variation_depth: usize,
    ) -> RecursiveNode {
        let mut children =
            vec![self.leaf(NodeKind::Delimiter, NodeStatus::Valid, start, start + 1)];
        let inner_start = start + 1;
        let (inner, cursor) = if inner_start < context_end {
            self.parse_movetext(inner_start, context_end, true, variation_depth)
        } else {
            (
                self.container(
                    NodeKind::Movetext,
                    inner_start,
                    inner_start,
                    Vec::new(),
                    NodeStatus::Valid,
                ),
                inner_start,
            )
        };
        let has_inner = inner.start < inner.end;
        let substantive = inner
            .children
            .iter()
            .any(|child| child.kind != NodeKind::Trivia);
        if has_inner {
            children.push(inner);
        }
        let mut base_status = NodeStatus::Valid;
        let end;
        if cursor < context_end && self.kind(cursor) == Some(TokenKind::RightParenthesis) {
            children.push(self.leaf(NodeKind::Delimiter, NodeStatus::Valid, cursor, cursor + 1));
            end = cursor + 1;
            if !substantive {
                base_status = NodeStatus::Recovered;
            }
        } else {
            end = cursor;
            base_status = NodeStatus::Incomplete;
        }
        let node = self.container(NodeKind::Variation, start, end, children, base_status);
        if base_status == NodeStatus::Incomplete {
            self.diagnostic(&node, DiagnosticCode::UnterminatedVariation, start, end);
        } else if !substantive {
            self.diagnostic(&node, DiagnosticCode::EmptyVariation, start, end);
        }
        node
    }

    fn unparsed_end(&self, start: usize, end: usize, stop_at_right: bool) -> usize {
        let mut cursor = start + 1;
        while cursor < end {
            if (stop_at_right && self.kind(cursor) == Some(TokenKind::RightParenthesis))
                || self.can_start_movetext(cursor)
            {
                break;
            }
            cursor += 1;
        }
        cursor
    }

    fn can_start_movetext(&self, index: usize) -> bool {
        matches!(
            self.kind(index),
            Some(
                TokenKind::Whitespace
                    | TokenKind::EscapeLine
                    | TokenKind::BraceComment
                    | TokenKind::UnterminatedBraceComment
                    | TokenKind::SemicolonComment
                    | TokenKind::MoveNumberCandidate
                    | TokenKind::Symbol
                    | TokenKind::ResultCandidate
                    | TokenKind::NagNumeric
                    | TokenKind::NagSymbolic
                    | TokenKind::LeftParenthesis
                    | TokenKind::RightParenthesis
                    | TokenKind::LeftBracket
            )
        )
    }

    fn bracket_led_end(&self, start: usize, limit: usize) -> usize {
        self.tokens[start + 1..limit]
            .iter()
            .position(|token| token.kind == TokenKind::RightBracket)
            .map_or(start + 1, |relative| start + relative + 2)
    }

    fn kind_before(&self, index: usize, limit: usize) -> Option<TokenKind> {
        (index < limit).then(|| self.tokens[index].kind)
    }

    fn kind(&self, index: usize) -> Option<TokenKind> {
        self.tokens.get(index).map(|token| token.kind)
    }

    fn leaf(
        &mut self,
        kind: NodeKind,
        status: NodeStatus,
        start: usize,
        end: usize,
    ) -> RecursiveNode {
        self.node(kind, status, start, end, Vec::new())
    }

    fn container(
        &mut self,
        kind: NodeKind,
        start: usize,
        end: usize,
        children: Vec<RecursiveNode>,
        base_status: NodeStatus,
    ) -> RecursiveNode {
        let status = children.iter().fold(base_status, |status, child| {
            combine_status(status, child.status)
        });
        self.node(kind, status, start, end, children)
    }

    fn node(
        &mut self,
        kind: NodeKind,
        status: NodeStatus,
        start: usize,
        end: usize,
        children: Vec<RecursiveNode>,
    ) -> RecursiveNode {
        let temp_id = self.next_temp_id;
        self.next_temp_id += 1;
        RecursiveNode {
            temp_id,
            kind,
            status,
            start,
            end,
            children,
        }
    }

    fn diagnostic(&mut self, node: &RecursiveNode, code: DiagnosticCode, start: usize, end: usize) {
        self.diagnostics.push(PendingDiagnostic {
            creation_ordinal: self.diagnostics.len() as u64,
            code,
            node_temp_id: node.temp_id,
            start,
            end,
        });
    }
}

fn combine_status(parent: NodeStatus, child: NodeStatus) -> NodeStatus {
    if parent == NodeStatus::Incomplete || child == NodeStatus::Incomplete {
        NodeStatus::Incomplete
    } else if parent == NodeStatus::Recovered
        || child == NodeStatus::Recovered
        || child == NodeStatus::Unparsed
    {
        NodeStatus::Recovered
    } else {
        NodeStatus::Valid
    }
}

fn flatten_tree(
    segment_ordinal: u64,
    token_count: usize,
    root: RecursiveNode,
    mut pending: Vec<PendingDiagnostic>,
) -> Result<ParseTree, String> {
    let mut nodes = Vec::new();
    let mut id_map = HashMap::new();
    flatten_node(&root, None, None, &mut nodes, &mut id_map);
    pending.sort_by_key(|diagnostic| (diagnostic.start, diagnostic.creation_ordinal));
    let diagnostics = pending
        .into_iter()
        .enumerate()
        .map(|(ordinal, diagnostic)| {
            Ok(Diagnostic {
                ordinal: ordinal as u64,
                code: diagnostic.code,
                node_id: *id_map
                    .get(&diagnostic.node_temp_id)
                    .ok_or("Diagnostica riferita a nodo interno assente")?,
                start_token: diagnostic.start as u64,
                end_token: diagnostic.end as u64,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ParseTree {
        segment_ordinal,
        token_count: token_count as u64,
        nodes,
        diagnostics,
    })
}

fn flatten_node(
    node: &RecursiveNode,
    parent_id: Option<u64>,
    child_ordinal: Option<u64>,
    nodes: &mut Vec<FlatNode>,
    id_map: &mut HashMap<u64, u64>,
) {
    let mut stack = vec![(node, parent_id, child_ordinal)];
    while let Some((current, current_parent, current_ordinal)) = stack.pop() {
        let id = nodes.len() as u64;
        id_map.insert(current.temp_id, id);
        nodes.push(FlatNode {
            id,
            parent_id: current_parent,
            child_ordinal: current_ordinal,
            kind: current.kind,
            status: current.status,
            start_token: current.start as u64,
            end_token: current.end as u64,
        });
        for (ordinal, child) in current.children.iter().enumerate().rev() {
            stack.push((child, Some(id), Some(ordinal as u64)));
        }
    }
}
