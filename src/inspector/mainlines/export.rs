use super::load_verified;
use crate::inspector::occurrences::publish_output;
use std::path::Path;

pub(in crate::inspector) fn export_jsonl(
    root: &Path,
    mainlines: &Path,
    output: Option<&Path>,
) -> Result<Vec<u8>, String> {
    let verified = load_verified(root, mainlines)?;
    let mut bytes = Vec::new();
    for record in &verified.manifest.mainlines {
        serde_json::to_writer(&mut bytes, record)
            .map_err(|error| format!("Serializzazione JSONL canonical mainline: {error}"))?;
        bytes.push(b'\n');
    }
    if let Some(path) = output {
        publish_output(root, path, &bytes, ".mainlines-jsonl-export")?;
        Ok(Vec::new())
    } else {
        Ok(bytes)
    }
}
