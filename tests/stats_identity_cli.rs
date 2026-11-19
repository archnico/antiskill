use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn work() -> PathBuf {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("stats-identity-{}-{t}", std::process::id()));
    fs::create_dir_all(p.join("durable/identity")).unwrap();
    p
}
fn export(root: &Path, file: &str, rows: &[(&str, &str, &str)], at: &str) -> PathBuf {
    let p = root.join("durable/identity").join(file);
    let mut w = csv::Writer::from_path(&p).unwrap();
    w.write_record(["fsi_id", "fide_id", "status"]).unwrap();
    for r in rows {
        w.write_record([r.0, r.1, r.2]).unwrap();
    }
    w.flush().unwrap();
    let out = Command::new("sha256sum").arg(&p).output().unwrap();
    let sha = String::from_utf8(out.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    let count = |s: &str| rows.iter().filter(|r| r.2 == s).count();
    let meta = json!({"schema":"antiskill.identity-crosswalk-export.v1",
  "source_table":"identity_link","rights_class":"internal","publication_allowed":false,
  "source_db_sha256":"9".repeat(64),"crosswalk_sha256":sha,"exported_at":at,
  "records":rows.len(),"status_counts":{
  "VERIFIED":count("VERIFIED"),"REVIEW":count("REVIEW"),
  "SOURCE_ASSERTED":count("SOURCE_ASSERTED")}});
    fs::write(
        format!("{}.provenance.json", p.display()),
        serde_json::to_vec(&meta).unwrap(),
    )
    .unwrap();
    p
}
fn run(root: &Path, opts: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["stats", "identity", "--root", root.to_str().unwrap()])
        .args(opts)
        .output()
        .unwrap()
}
fn as_json(o: &Output) -> Value {
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
fn both_directions_formats_and_provenance() {
    let r = work();
    export(
        &r,
        "verified-fsi-fide-2026-11-19.csv",
        &[
            ("192691", "28551931", "VERIFIED"),
            ("191617", "28574249", "VERIFIED"),
            ("100007", "805424", "REVIEW"),
        ],
        "2026-11-19T12:00:00+00:00",
    );
    let o = run(&r, &["--fsi", "000192691", "--format", "json"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let j = as_json(&o);
    assert_eq!(j["status"], "verified");
    assert_eq!(j["fsi_id"], "192691");
    assert_eq!(j["fide_id"], "28551931");
    assert_eq!(j["certified"], false);
    assert_eq!(j["provenance"]["publication_allowed"], false);
    let o = run(&r, &["--fide", "28551931", "--format", "json"]);
    assert!(o.status.success());
    assert_eq!(as_json(&o)["fsi_id"], "192691");
    let o = run(&r, &["--fide", "28551931", "--format", "csv"]);
    assert!(o.status.success());
    assert!(String::from_utf8(o.stdout)
        .unwrap()
        .contains("fide,28551931,192691,28551931,verified"));
    let o = run(&r, &["--fsi", "192691"]);
    assert!(o.status.success());
    assert!(String::from_utf8(o.stdout)
        .unwrap()
        .contains("FIDE ID: 28551931"));
    fs::remove_dir_all(r).unwrap();
}
#[test]
fn unverified_ambiguous_missing_are_not_resolved() {
    let r = work();
    export(
        &r,
        "verified-fsi-fide-2026-11-19.csv",
        &[
            ("192691", "28551931", "VERIFIED"),
            ("100007", "805424", "REVIEW"),
            ("100008", "805425", "SOURCE_ASSERTED"),
            ("100009", "28551931", "REVIEW"),
        ],
        "2026-11-19T12:00:00+00:00",
    );
    let o = run(&r, &["--fsi", "100007", "--format", "json"]);
    assert_eq!(o.status.code(), Some(3));
    assert_eq!(as_json(&o)["status"], "review");
    assert_eq!(as_json(&o)["fide_id"], Value::Null);
    assert_eq!(as_json(&o)["candidates"][0]["fide_id"], "805424");
    let o = run(&r, &["--fide", "805425", "--format", "json"]);
    assert_eq!(o.status.code(), Some(3));
    assert_eq!(as_json(&o)["status"], "source_asserted");
    let o = run(&r, &["--fide", "28551931", "--format", "json"]);
    assert_eq!(o.status.code(), Some(3));
    assert_eq!(as_json(&o)["status"], "ambiguous");
    assert_eq!(as_json(&o)["fsi_id"], Value::Null);
    assert_eq!(as_json(&o)["candidates"].as_array().unwrap().len(), 2);
    let o = run(&r, &["--fsi", "555555555", "--format", "json"]);
    assert_eq!(o.status.code(), Some(4));
    assert_eq!(as_json(&o)["status"], "not_found");
    fs::remove_dir_all(r).unwrap();
}
#[test]
fn tampering_duplicates_invalid_flags_fail_closed() {
    let r = work();
    let path = export(
        &r,
        "verified-fsi-fide-2026-11-19.csv",
        &[("192691", "28551931", "VERIFIED")],
        "2026-11-19T12:00:00+00:00",
    );
    assert_eq!(
        run(&r, &["--fsi", "192691", "--fide", "28551931"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(run(&r, &["--fsi", "123; rm /"]).status.code(), Some(2));
    assert_eq!(
        run(&r, &["--fsi", "192691", "--format", "xml"])
            .status
            .code(),
        Some(2)
    );
    fs::write(&path, b"fsi_id,fide_id,status\n192691,9000,VERIFIED\n").unwrap();
    let o = run(&r, &["--fsi", "192691", "--format", "json"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("SHA256"));
    export(
        &r,
        "verified-fsi-fide-2026-11-20.csv",
        &[("1", "100", "VERIFIED"), ("2", "100", "VERIFIED")],
        "2026-11-20T12:00:00+00:00",
    );
    let o = run(&r, &["--fide", "100"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("FIDE VERIFIED"));
    fs::remove_dir_all(r).unwrap();
}
#[test]
fn latest_snapshot_is_selected_but_explicit_is_possible() {
    let r = work();
    let old = export(
        &r,
        "verified-fsi-fide-2026-11-19.csv",
        &[("192691", "28551931", "VERIFIED")],
        "2026-11-19T08:00:00+00:00",
    );
    export(
        &r,
        "verified-fsi-fide-2026-11-19-abc.csv",
        &[("192691", "12345678", "VERIFIED")],
        "2026-11-19T12:00:00+00:00",
    );
    let o = run(&r, &["--fsi", "192691", "--format", "json"]);
    assert!(o.status.success());
    assert_eq!(as_json(&o)["fide_id"], "12345678");
    let o = run(
        &r,
        &[
            "--fsi",
            "192691",
            "--format",
            "json",
            "--crosswalk",
            old.to_str().unwrap(),
        ],
    );
    assert!(o.status.success());
    assert_eq!(as_json(&o)["fide_id"], "28551931");
    fs::remove_dir_all(r).unwrap();
}

#[test]
fn accepts_versioned_and_legacy_provenance_formats() {
    let root = work();
    let old = export(
        &root,
        "verified-fsi-fide-2026-11-19.csv",
        &[("192691", "28551931", "VERIFIED")],
        "2026-11-19T08:00:00+00:00",
    );
    let old_proof = std::path::PathBuf::from(format!("{}.provenance.json", old.display()));
    let new_proof = old.with_extension("provenance.json");
    let mut note: Value = serde_json::from_slice(&fs::read(&old_proof).unwrap()).unwrap();
    note["total_rows"] = note["records"].take();
    fs::write(&new_proof, serde_json::to_vec(&note).unwrap()).unwrap();
    fs::remove_file(&old_proof).unwrap();
    let result = run(&root, &["--fsi", "192691", "--format", "json"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(as_json(&result)["fide_id"], "28551931");
    fs::remove_dir_all(root).unwrap();
}
