use super::model::{
    FlatNode, NodeKind, NodeStatus, ParseManifest, ParseTree, MAX_ARENA_DEPTH, MAX_VARIATION_DEPTH,
    PARSE_SCHEMA_VERSION,
};
use crate::inspector::common::validate_relative_path;
use crate::inspector::tokenization::LoadedTokenizationManifest;

pub(super) fn validate_parse_manifest(
    manifest: &ParseManifest,
    tokenization: &LoadedTokenizationManifest,
) -> Result<(), String> {
    validate_parse_shape(manifest)?;
    if manifest.source_blake3 != tokenization.manifest.source_blake3
        || manifest.source_bytes != tokenization.manifest.source_bytes
    {
        return Err("Parse incoerente con la tokenizzazione".to_owned());
    }
    if manifest.trees.len() != tokenization.manifest.streams.len() {
        return Err("Numero di alberi diverso dagli stream".to_owned());
    }
    for (tree, stream) in manifest.trees.iter().zip(&tokenization.manifest.streams) {
        if tree.segment_ordinal != stream.segment_ordinal {
            return Err("Albero assente, duplicato o fuori ordine".to_owned());
        }
        if tree.token_count != stream.tokens.len() as u64 {
            return Err("token_count diverso dallo stream".to_owned());
        }
        validate_tree(tree)?;
    }
    Ok(())
}

pub(super) fn validate_parse_shape(manifest: &ParseManifest) -> Result<(), String> {
    validate_parse_shape_fields(
        manifest.schema_version,
        &manifest.tokenization_manifest,
        &manifest.tokenization_manifest_blake3,
        manifest.tokenization_manifest_bytes,
        &manifest.source_blake3,
        manifest.source_bytes,
    )
}

pub(super) fn validate_parse_shape_fields(
    schema_version: u32,
    tokenization_manifest: &str,
    tokenization_manifest_blake3: &str,
    tokenization_manifest_bytes: u64,
    source_blake3: &str,
    source_bytes: u64,
) -> Result<(), String> {
    if schema_version != PARSE_SCHEMA_VERSION {
        return Err(format!("Versione parse non supportata: {}", schema_version));
    }
    validate_relative_path(tokenization_manifest, "tokenization_manifest")?;
    validate_blake3(
        tokenization_manifest_blake3,
        "Hash BLAKE3 della tokenizzazione non valido",
    )?;
    if tokenization_manifest_bytes == 0 {
        return Err("La dimensione della tokenizzazione deve essere positiva".to_owned());
    }
    validate_blake3(source_blake3, "Hash BLAKE3 della sorgente non valido")?;
    if source_bytes == 0 {
        return Err("La dimensione della sorgente deve essere positiva".to_owned());
    }
    Ok(())
}

pub(super) fn validate_tree(tree: &ParseTree) -> Result<(), String> {
    if tree.token_count == 0 || tree.nodes.is_empty() {
        return Err("Un albero deve contenere token e nodi".to_owned());
    }
    let root = &tree.nodes[0];
    if root.id != 0
        || root.parent_id.is_some()
        || root.child_ordinal.is_some()
        || root.kind != NodeKind::GameCandidate
        || root.start_token != 0
        || root.end_token != tree.token_count
    {
        return Err("Radice della parse non valida".to_owned());
    }
    let mut children = vec![Vec::<&FlatNode>::new(); tree.nodes.len()];
    let mut depths = vec![0_usize; tree.nodes.len()];
    let mut variation_depths = vec![0_usize; tree.nodes.len()];
    for (index, node) in tree.nodes.iter().enumerate() {
        if node.id != index as u64 {
            return Err("ID dei nodi non consecutivi".to_owned());
        }
        if node.start_token >= node.end_token || node.end_token > tree.token_count {
            return Err("Intervallo di nodo non valido o fuori dallo stream".to_owned());
        }
        if node.kind == NodeKind::Unparsed && node.status != NodeStatus::Unparsed {
            return Err("Nodo unparsed con status diverso da unparsed".to_owned());
        }
        if node.kind != NodeKind::Unparsed && node.status == NodeStatus::Unparsed {
            return Err("Status unparsed su kind diverso da unparsed".to_owned());
        }
        if index == 0 {
            continue;
        }
        let parent_id = node.parent_id.ok_or("Nodo non radice senza parent_id")?;
        let parent_index =
            usize::try_from(parent_id).map_err(|_| "parent_id non rappresentabile".to_owned())?;
        if parent_index >= index {
            return Err("parent_id inesistente o non precedente".to_owned());
        }
        let child_ordinal = node
            .child_ordinal
            .ok_or("Nodo non radice senza child_ordinal")?;
        if child_ordinal != children[parent_index].len() as u64 {
            return Err("child_ordinal duplicato o non consecutivo".to_owned());
        }
        let parent = &tree.nodes[parent_index];
        depths[index] = depths[parent_index] + 1;
        if depths[index] > MAX_ARENA_DEPTH {
            return Err("Profondità dell'arena parse oltre il limite".to_owned());
        }
        variation_depths[index] =
            variation_depths[parent_index] + usize::from(node.kind == NodeKind::Variation);
        if variation_depths[index] > MAX_VARIATION_DEPTH {
            return Err("Profondità delle variazioni oltre il limite".to_owned());
        }
        if node.start_token < parent.start_token || node.end_token > parent.end_token {
            return Err("Figlio fuori dall'intervallo del genitore".to_owned());
        }
        children[parent_index].push(node);
    }
    for (parent_index, direct) in children.iter().enumerate() {
        let parent = &tree.nodes[parent_index];
        if direct.is_empty() {
            if is_container(parent.kind) {
                return Err("Nodo contenitore senza figli".to_owned());
            }
            continue;
        }
        if !is_container(parent.kind) {
            return Err("Nodo foglia con figli".to_owned());
        }
        let has_incomplete = direct
            .iter()
            .any(|child| child.status == NodeStatus::Incomplete);
        let has_recovery = direct
            .iter()
            .any(|child| matches!(child.status, NodeStatus::Recovered | NodeStatus::Unparsed));
        if has_incomplete && parent.status != NodeStatus::Incomplete {
            return Err("Status incomplete non propagato al contenitore".to_owned());
        }
        if !has_incomplete && has_recovery && parent.status == NodeStatus::Valid {
            return Err("Status recovered non propagato al contenitore".to_owned());
        }
        let mut cursor = parent.start_token;
        for child in direct {
            if child.start_token != cursor {
                return Err("Figli con buco, sovrapposizione o ordine errato".to_owned());
            }
            cursor = child.end_token;
        }
        if cursor != parent.end_token {
            return Err("I figli non partizionano il genitore".to_owned());
        }
    }
    let mut preorder = Vec::new();
    collect_preorder(0, &children, &mut preorder);
    if preorder != (0..tree.nodes.len() as u64).collect::<Vec<_>>() {
        return Err("Nodi non serializzati in preordine".to_owned());
    }
    for (index, diagnostic) in tree.diagnostics.iter().enumerate() {
        if diagnostic.ordinal != index as u64 {
            return Err("Ordinali delle diagnostiche non consecutivi".to_owned());
        }
        let node = tree
            .nodes
            .get(diagnostic.node_id as usize)
            .ok_or("Diagnostica con node_id inesistente")?;
        if diagnostic.start_token >= diagnostic.end_token
            || diagnostic.start_token < node.start_token
            || diagnostic.end_token > node.end_token
        {
            return Err("Intervallo diagnostica fuori dal nodo".to_owned());
        }
    }
    Ok(())
}

fn collect_preorder(node_id: u64, children: &[Vec<&FlatNode>], output: &mut Vec<u64>) {
    let mut stack = vec![node_id];
    while let Some(current) = stack.pop() {
        output.push(current);
        for child in children[current as usize].iter().rev() {
            stack.push(child.id);
        }
    }
}

pub(super) fn is_container(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::GameCandidate
            | NodeKind::TagSection
            | NodeKind::TagPair
            | NodeKind::Movetext
            | NodeKind::Variation
    )
}

fn validate_blake3(value: &str, error: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(error.to_owned())
    }
}
