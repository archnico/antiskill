//! Strict identity crosswalk produced by the shared national identity registry.
//! Only explicitly VERIFIED FSI↔FIDE links can be used for ratings.
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub(super) struct VerifiedLinks {
    pub(super) links: HashMap<String, String>,
    pub(super) total: usize,
    pub(super) review: usize,
}
pub(super) fn read_links(path: &Path) -> Result<VerifiedLinks, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .from_path(path)
        .map_err(|e| format!("Crosswalk CSV {}: {e}", path.display()))?;
    let heads = rdr
        .headers()
        .map_err(|e| format!("Crosswalk header: {e}"))?;
    if heads.iter().collect::<Vec<_>>() != ["fsi_id", "fide_id", "status"] {
        return Err("Crosswalk: intestazione esatta richiesta: fsi_id,fide_id,status".into());
    }
    let mut links = HashMap::new();
    let mut seen = HashSet::new();
    let mut total = 0usize;
    let mut review = 0usize;
    for result in rdr.records() {
        let rec = result.map_err(|e| format!("Crosswalk record: {e}"))?;
        if rec.len() != 3 {
            return Err("Crosswalk con colonne mancanti".into());
        }
        let fsi = rec[0].trim();
        let fide = rec[1].trim();
        let status = rec[2].trim();
        if ![fsi, fide]
            .iter()
            .all(|x| !x.is_empty() && *x != "0" && x.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(format!(
                "Crosswalk: ID non numerico alla riga {}",
                total + 2
            ));
        }
        if !seen.insert(fsi.to_string()) {
            return Err("Crosswalk: FSI ID duplicato, provenienza ambigua".into());
        }
        total += 1;
        match status {
            "VERIFIED" => {
                links.insert(fsi.into(), fide.into());
            }
            "REVIEW" | "SOURCE_ASSERTED" => review += 1,
            _ => return Err("Crosswalk: stato identità non riconosciuto".into()),
        }
    }
    if total == 0 || links.is_empty() {
        return Err("Crosswalk vuoto o senza identità verificate".into());
    }
    let mut inverse = HashSet::new();
    for fide in links.values() {
        if !inverse.insert(fide) {
            return Err("Crosswalk: FIDE ID associato a due FSI ID".into());
        }
    }
    Ok(VerifiedLinks {
        links,
        total,
        review,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_duplicate_or_unverified_overrides() {
        let root =
            std::env::temp_dir().join(format!("antiskill-crosswalk-test-{}", std::process::id()));
        std::fs::write(
            &root,
            "fsi_id,fide_id,status\n101,28101,VERIFIED\n102,28102,REVIEW\n",
        )
        .unwrap();
        let links = read_links(&root).unwrap();
        assert_eq!(links.links.len(), 1);
        assert_eq!(links.review, 1);
        std::fs::write(
            &root,
            "fsi_id,fide_id,status\n101,28101,VERIFIED\n101,28102,VERIFIED\n",
        )
        .unwrap();
        assert!(read_links(&root).is_err());
        let _ = std::fs::remove_file(root);
    }
}
