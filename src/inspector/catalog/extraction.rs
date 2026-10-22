use super::manifest::*;
use crate::inspector::interpretation::manifest::{GameRecordV1, GameStatusV1, VariantKindV1};
use crate::inspector::parsing::model::{FlatNode, NodeKind, NodeStatus, ParseTree};
use crate::inspector::tokenization::TokenStream;

#[cfg(test)]
pub(super) fn extract_games(
    v: &crate::inspector::interpretation::manifest::VerifiedInterpretation,
) -> Result<Vec<CatalogGameRecordV1>, String> {
    let trees = &v.parse.manifest.trees;
    let streams = &v.parse.tokenization.manifest.streams;
    let source = &v.parse.tokenization;
    if trees.len() != streams.len() || trees.len() != v.manifest.games.len() {
        return Err("Relazione uno-a-uno fra Interpretation, Parse e Tokenization assente".into());
    }
    source.verify_source()?;
    let result = trees
        .iter()
        .zip(streams)
        .zip(&v.manifest.games)
        .enumerate()
        .map(|(i, ((tree, stream), game))| {
            let expected_index =
                u64::try_from(i).map_err(|_| "Indice partita oltre u64".to_owned())?;
            let raw = source.read_source_range(stream.start, stream.end)?;
            as_catalog_record(expected_index, game, tree, stream, &raw)
        })
        .collect::<Result<Vec<_>, String>>();
    if result.is_ok() {
        source.verify_source()?;
    }
    result
}

pub(super) fn as_catalog_record(
    index: u64,
    game: &GameRecordV1,
    tree: &ParseTree,
    stream: &TokenStream,
    blob: &[u8],
) -> Result<CatalogGameRecordV1, String> {
    if tree.segment_ordinal != stream.segment_ordinal
        || game.segment_ordinal != tree.segment_ordinal
        || game.id != index
        || game.parse_tree_index != index
    {
        return Err("Allineamento partita/ParseTree/TokenStream non valido".into());
    }
    let blob_bytes = u64::try_from(blob.len()).map_err(|_| "blob oltre u64".to_owned())?;
    let range = stream
        .end
        .checked_sub(stream.start)
        .ok_or("Intervallo partita invertito")?;
    if range == 0 || range > blob_bytes {
        return Err("Intervallo SourceBlob della partita non valido".into());
    }
    Ok(CatalogGameRecordV1 {
        id: game.id,
        interpretation_game_id: game.id,
        parse_tree_index: game.parse_tree_index,
        segment_ordinal: game.segment_ordinal,
        interpretation_status: interpretation_status(game.status),
        variant: catalog_variant(game.variant),
        source_start: stream.start,
        source_end: stream.end,
        source_game_blake3: blake3::hash(blob).to_hex().to_string(),
        tags: extract_tags_at(tree, stream, stream.start, blob)?,
    })
}

#[cfg(test)]
pub(super) fn extract_tags(
    tree: &ParseTree,
    stream: &TokenStream,
    blob: &[u8],
) -> Result<Vec<CatalogTagRecordV1>, String> {
    extract_tags_at(tree, stream, 0, blob)
}

fn extract_tags_at(
    tree: &ParseTree,
    stream: &TokenStream,
    source_start: u64,
    blob: &[u8],
) -> Result<Vec<CatalogTagRecordV1>, String> {
    let root = tree.nodes.first().ok_or("ParseTree senza radice")?;
    let mut arena_children = vec![Vec::new(); tree.nodes.len()];
    for (index, node) in tree.nodes.iter().enumerate().skip(1) {
        let parent = usize::try_from(node.parent_id.ok_or("Nodo CST senza parent")?)
            .map_err(|_| "parent CST oltre usize")?;
        arena_children
            .get_mut(parent)
            .ok_or("parent CST inesistente")?
            .push(index);
    }
    let mut tags = Vec::new();
    let root_index = usize::try_from(root.id).map_err(|_| "ID radice CST oltre usize")?;
    let root_children = arena_children
        .get(root_index)
        .ok_or("ID radice CST fuori arena")?;
    for (section_ordinal, section) in root_children
        .iter()
        .filter_map(|index| tree.nodes.get(*index))
        .filter(|node| node.kind == NodeKind::TagSection)
        .enumerate()
    {
        let section_index = usize::try_from(section.id).map_err(|_| "ID TagSection oltre usize")?;
        let section_children = arena_children
            .get(section_index)
            .ok_or("ID TagSection fuori arena")?;
        for pair in section_children
            .iter()
            .filter_map(|index| tree.nodes.get(*index))
            .filter(|node| node.kind == NodeKind::TagPair)
        {
            let pair_index = usize::try_from(pair.id).map_err(|_| "ID TagPair oltre usize")?;
            let pair_children = arena_children
                .get(pair_index)
                .ok_or("ID TagPair fuori arena")?
                .iter()
                .filter_map(|index| tree.nodes.get(*index))
                .collect::<Vec<_>>();
            tags.push(extract_tag_at(
                u64::try_from(tags.len()).map_err(|_| "Ordinal tag oltre u64")?,
                u64::try_from(section_ordinal).map_err(|_| "Ordinal sezione oltre u64")?,
                section,
                pair,
                &pair_children,
                stream,
                source_start,
                blob,
            )?);
        }
    }
    Ok(tags)
}

#[cfg(test)]
pub(super) fn extract_tag(
    ordinal: u64,
    section_ordinal: u64,
    section: &FlatNode,
    pair: &FlatNode,
    children: &[&FlatNode],
    stream: &TokenStream,
    blob: &[u8],
) -> Result<CatalogTagRecordV1, String> {
    extract_tag_at(
        ordinal,
        section_ordinal,
        section,
        pair,
        children,
        stream,
        0,
        blob,
    )
}

#[allow(clippy::too_many_arguments)]
fn extract_tag_at(
    ordinal: u64,
    section_ordinal: u64,
    section: &FlatNode,
    pair: &FlatNode,
    children: &[&FlatNode],
    stream: &TokenStream,
    source_start: u64,
    blob: &[u8],
) -> Result<CatalogTagRecordV1, String> {
    let names: Vec<_> = children
        .iter()
        .copied()
        .filter(|n| n.kind == NodeKind::TagNameCandidate)
        .collect();
    let values: Vec<_> = children
        .iter()
        .copied()
        .filter(|n| n.kind == NodeKind::TagValueCandidate)
        .collect();
    let unexpected = children.iter().any(|n| {
        !matches!(
            n.kind,
            NodeKind::Delimiter
                | NodeKind::Trivia
                | NodeKind::TagNameCandidate
                | NodeKind::TagValueCandidate
        )
    });
    let (start_byte, end_byte, _) = node_range_at(pair, stream, source_start, blob)?;
    let unique_name = if names.len() == 1 {
        names.first().copied()
    } else {
        None
    };
    let unique_value = if values.len() == 1 {
        values.first().copied()
    } else {
        None
    };
    let name_range = unique_name.and_then(|n| node_range_at(n, stream, source_start, blob).ok());
    let value_range = unique_value.and_then(|n| node_range_at(n, stream, source_start, blob).ok());
    let (decode_status, name, value) = decode(unexpected, &names, &values, name_range, value_range);
    Ok(CatalogTagRecordV1 {
        ordinal,
        section_node_id: section.id,
        section_ordinal,
        tag_pair_node_id: pair.id,
        node_status: status(pair.status),
        name_node_ids: names.iter().map(|n| n.id).collect(),
        value_node_ids: values.iter().map(|n| n.id).collect(),
        decode_status,
        name,
        value,
        start_token: pair.start_token,
        end_token: pair.end_token,
        start_byte,
        end_byte,
        name_start_token: unique_name.map(|n| n.start_token),
        name_end_token: unique_name.map(|n| n.end_token),
        name_start_byte: name_range.map(|r| r.0),
        name_end_byte: name_range.map(|r| r.1),
        value_start_token: unique_value.map(|n| n.start_token),
        value_end_token: unique_value.map(|n| n.end_token),
        value_start_byte: value_range.map(|r| r.0),
        value_end_byte: value_range.map(|r| r.1),
    })
}

pub(super) fn decode(
    unexpected: bool,
    names: &[&FlatNode],
    values: &[&FlatNode],
    name_range: Option<(u64, u64, &[u8])>,
    value_range: Option<(u64, u64, &[u8])>,
) -> (TagDecodeStatusV1, Option<String>, Option<String>) {
    if unexpected {
        return (TagDecodeStatusV1::InvalidStructure, None, None);
    }
    if names.is_empty() {
        return (TagDecodeStatusV1::MissingName, None, None);
    }
    if names.len() > 1 {
        return (TagDecodeStatusV1::AmbiguousName, None, None);
    }
    let Some((_, _, nb)) = name_range else {
        return (TagDecodeStatusV1::InvalidNameEncoding, None, None);
    };
    if nb.is_empty() || !nb.is_ascii() {
        return (TagDecodeStatusV1::InvalidNameEncoding, None, None);
    }
    let name = match String::from_utf8(nb.to_vec()) {
        Ok(name) => name,
        Err(_) => return (TagDecodeStatusV1::InvalidNameEncoding, None, None),
    };
    if values.is_empty() {
        return (TagDecodeStatusV1::MissingValue, Some(name), None);
    }
    if values.len() > 1 {
        return (TagDecodeStatusV1::AmbiguousValue, Some(name), None);
    }
    let Some((_, _, vb)) = value_range else {
        return (TagDecodeStatusV1::InvalidValueEncoding, Some(name), None);
    };
    if vb.len() < 2 || vb.first() != Some(&b'\"') || vb.last() != Some(&b'\"') {
        return (TagDecodeStatusV1::MissingValue, Some(name), None);
    }
    let Some(inner) = vb.get(1..vb.len().saturating_sub(1)) else {
        return (TagDecodeStatusV1::MissingValue, Some(name), None);
    };
    if inner.last() == Some(&b'\\') {
        return (TagDecodeStatusV1::MissingValue, Some(name), None);
    }
    let mut decoded = Vec::with_capacity(inner.len());
    let mut input = inner.iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'\\' {
            let Some(escaped) = input.next() else {
                return (TagDecodeStatusV1::MissingValue, Some(name), None);
            };
            if !matches!(escaped, b'\\' | b'\"') {
                return (TagDecodeStatusV1::InvalidEscape, Some(name), None);
            }
            decoded.push(escaped);
        } else {
            decoded.push(byte);
        }
    }
    match String::from_utf8(decoded) {
        Ok(value) => (TagDecodeStatusV1::Decoded, Some(name), Some(value)),
        Err(_) => (TagDecodeStatusV1::InvalidValueEncoding, Some(name), None),
    }
}

#[cfg(test)]
pub(super) fn node_range<'a>(
    node: &FlatNode,
    stream: &TokenStream,
    blob: &'a [u8],
) -> Result<(u64, u64, &'a [u8]), String> {
    node_range_at(node, stream, 0, blob)
}

fn node_range_at<'a>(
    node: &FlatNode,
    stream: &TokenStream,
    source_start: u64,
    blob: &'a [u8],
) -> Result<(u64, u64, &'a [u8]), String> {
    let token_count =
        u64::try_from(stream.tokens.len()).map_err(|_| "Numero token oltre u64".to_owned())?;
    if node.start_token >= node.end_token || node.end_token > token_count {
        return Err("Intervallo token CST non valido".into());
    }
    let start = usize::try_from(node.start_token).map_err(|_| "start_token CST oltre usize")?;
    let end = usize::try_from(node.end_token).map_err(|_| "end_token CST oltre usize")?;
    let last_index = end.checked_sub(1).ok_or("Intervallo token CST vuoto")?;
    let first = stream
        .tokens
        .get(start)
        .ok_or("start_token CST inesistente")?;
    let last = stream
        .tokens
        .get(last_index)
        .ok_or("end_token CST inesistente")?;
    let blob_bytes = u64::try_from(blob.len()).map_err(|_| "blob oltre u64")?;
    let blob_end = source_start
        .checked_add(blob_bytes)
        .ok_or("Intervallo byte CST oltre u64")?;
    if first.start < source_start || last.end > blob_end {
        return Err("Intervallo byte CST non valido".into());
    }
    let byte_start =
        usize::try_from(first.start - source_start).map_err(|_| "start byte oltre usize")?;
    let byte_end = usize::try_from(last.end - source_start).map_err(|_| "end byte oltre usize")?;
    let bytes = blob
        .get(byte_start..byte_end)
        .ok_or("Intervallo byte CST non valido")?;
    Ok((first.start, last.end, bytes))
}
fn status(s: NodeStatus) -> NodeStatusV1 {
    match s {
        NodeStatus::Valid => NodeStatusV1::Valid,
        NodeStatus::Recovered => NodeStatusV1::Recovered,
        NodeStatus::Incomplete => NodeStatusV1::Incomplete,
        NodeStatus::Unparsed => NodeStatusV1::Unparsed,
    }
}

fn interpretation_status(value: GameStatusV1) -> CatalogInterpretationStatusV1 {
    match value {
        GameStatusV1::Complete => CatalogInterpretationStatusV1::Complete,
        GameStatusV1::Partial => CatalogInterpretationStatusV1::Partial,
        GameStatusV1::UnsupportedVariant => CatalogInterpretationStatusV1::UnsupportedVariant,
        GameStatusV1::InvalidInitialPosition => {
            CatalogInterpretationStatusV1::InvalidInitialPosition
        }
        GameStatusV1::AmbiguousInitialPosition => {
            CatalogInterpretationStatusV1::AmbiguousInitialPosition
        }
        GameStatusV1::UnexpectedStructure => CatalogInterpretationStatusV1::UnexpectedStructure,
    }
}

fn catalog_variant(value: VariantKindV1) -> CatalogVariantV1 {
    match value {
        VariantKindV1::Standard => CatalogVariantV1::Standard,
        VariantKindV1::Chess960 => CatalogVariantV1::Chess960,
        VariantKindV1::Other => CatalogVariantV1::Other,
        VariantKindV1::Unknown => CatalogVariantV1::Unknown,
    }
}
