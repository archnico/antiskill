/// Conservative, order-insensitive signature used only to generate or verify
/// player-name identity candidates. It is not authoritative identity evidence.
pub(crate) fn fold_identity_latin(character: char) -> char {
    match character {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' => 'y',
        other => other.to_lowercase().next().unwrap_or(other),
    }
}

pub(crate) fn player_name_signature(raw: &str) -> Option<String> {
    let mut normalized = String::with_capacity(raw.len());
    for character in raw.chars() {
        let folded = fold_identity_latin(character);
        if folded.is_alphanumeric() {
            normalized.push(folded);
        } else {
            normalized.push(' ');
        }
    }
    let mut tokens = normalized.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 {
        return None;
    }
    tokens.sort_unstable();
    Some(tokens.join(" "))
}

#[cfg(test)]
mod tests {
    use super::player_name_signature;

    #[test]
    fn signature_preserves_the_frozen_candidate_semantics() {
        let expected = player_name_signature("Esempio, Auròra").unwrap();
        assert_eq!(player_name_signature("Aurora' Esempio").unwrap(), expected);
        assert_eq!(player_name_signature("Esempio, Aurora`").unwrap(), expected);
        assert_ne!(player_name_signature("Esempio, Bruno").unwrap(), expected);
        assert_eq!(player_name_signature("Madonna"), None);
    }
}
