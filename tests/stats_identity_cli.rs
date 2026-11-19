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

fn with_profiles(root: &Path, crosswalk: &Path, fsi: &str, fide: &str) -> PathBuf {
    let crosswalk_sha = Command::new("sha256sum").arg(crosswalk).output().unwrap();
    let crosswalk_sha = String::from_utf8(crosswalk_sha.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    let registry_sha = "9".repeat(64);
    let fide_blake3 = "a".repeat(64);
    let month = "2026-11";
    let refdir = root.join("raw/refs");
    fs::create_dir_all(&refdir).unwrap();
    fs::write(
        refdir.join("fide-rating-combined-2026-11.json"),
        serde_json::to_vec(&json!({"blake3":fide_blake3})).unwrap(),
    )
    .unwrap();
    let pdir = root.join("durable/identity/profile-indexes");
    fs::create_dir_all(&pdir).unwrap();
    let p = pdir.join("profiles.json");
    let profile = json!({"fsi_id":fsi,"fide_id":fide,"full_name":"ROSSI MARIO",
       "surname":"ROSSI","given_name":"MARIO","birth_date":null,"birth_year":1990,
       "nationality_code":"ITA","fide_federation":"ITA",
       "fsi_category":"1N","fsi_category_source":"fsi-official-roster",
       "fide_title":"FM","fide_standard":1843,"fide_rapid":null,"fide_blitz":1710,
       "fide_rating_status":"found"});
    let db = json!({"schema":"antiskill.stats.player-profiles.v1","fingerprint":"fingerprint1",
       "publication_allowed":false,"month":month,"registry_sha256":registry_sha,
       "crosswalk_sha256":crosswalk_sha,"fide_blake3":fide_blake3,
       "fsi_sources":[],"players":{fsi:profile}});
    fs::write(&p, serde_json::to_vec(&db).unwrap()).unwrap();
    let sha = Command::new("sha256sum").arg(&p).output().unwrap();
    let sha = String::from_utf8(sha.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    let pointer = json!({"schema":"antiskill.stats.player-profiles-pointer.v1",
       "profile_sha256":sha,"publication_allowed":false,
       "profile_path":"durable/identity/profile-indexes/profiles.json",
       "month":month,"fingerprint":"fingerprint1","registry_sha256":registry_sha,
       "crosswalk_sha256":crosswalk_sha,"fide_blake3":fide_blake3});
    fs::write(
        root.join("durable/identity/profiles-current.json"),
        serde_json::to_vec(&pointer).unwrap(),
    )
    .unwrap();
    p
}
#[test]
fn enriched_profile_both_directions_and_missing_ratings_are_null() {
    let root = work();
    let c = export(
        &root,
        "verified-fsi-fide-2026-11-19.csv",
        &[("192691", "28551931", "VERIFIED")],
        "2026-11-19T12:00:00+00:00",
    );
    with_profiles(&root, &c, "192691", "28551931");
    let r = run(&root, &["--fsi", "192691", "--format", "json"]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let j = as_json(&r);
    assert_eq!(j["profile"]["surname"], "ROSSI");
    assert_eq!(j["profile"]["given_name"], "MARIO");
    assert_eq!(j["profile"]["birth_year"], 1990);
    assert_eq!(j["profile"]["birth_date"], Value::Null);
    assert_eq!(j["profile"]["fide_rapid"], Value::Null);
    assert_eq!(j["profile"]["fide_standard"], 1843);
    assert_eq!(j["profile"]["fide_blitz"], 1710);
    assert_eq!(j["profile"]["nationality_code"], "ITA");
    assert_eq!(j["profile"]["fide_federation"], "ITA");
    assert_eq!(j["profile"]["fide_rating_month"], "2026-11");
    let rev = run(&root, &["--fide", "28551931", "--format", "json"]);
    assert!(rev.status.success());
    assert_eq!(as_json(&rev)["profile"]["fide_standard"], 1843);
    let csv = run(&root, &["--fsi", "192691", "--format", "csv"]);
    assert!(csv.status.success());
    let csv = String::from_utf8(csv.stdout).unwrap();
    assert!(csv.contains("ROSSI MARIO,ROSSI,MARIO,1990,,ITA,ITA,1N,FM,1843,,1710,2026-11"));
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn profile_tamper_or_identity_disagreement_cannot_silently_enrich() {
    let root = work();
    let c = export(
        &root,
        "verified-fsi-fide-2026-11-19.csv",
        &[("192691", "28551931", "VERIFIED")],
        "2026-11-19T12:00:00+00:00",
    );
    let p = with_profiles(&root, &c, "192691", "28551931");
    fs::write(&p, b"{}").unwrap();
    let bad = run(&root, &["--fsi", "192691", "--format", "json"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("SHA256"));
    fs::remove_dir_all(root).unwrap();
}
