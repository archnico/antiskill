use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn work() -> PathBuf {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!(
        "antiskill-stats-movement-{}-{t}",
        std::process::id()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn item(id: &str, rank: usize) -> Value {
    json!({"name":format!("PLAYER {id}"),"fsi_id":id,"fide_id":format!("28{id}"),"rank":rank,"rating":2000-rank as i32*10})
}
fn report(month: &str, order: &[&str]) -> Value {
    json!({
        "schema":"antiskill.rankings.v1","scope":"top3-region","rating":"fide-standard",
        "period":month,"fide_month":month,"fsi_as_of":format!("{month}-19"),
        "filters":format!("membership-region=TOS, membership-year={}",&month[..4]),
        "membership_authority":"fsi-official-csv","rating_authority":"fide-official-xml",
        "fsi_manifest_proof":{"verified":true},
        "fsi_source":{"blake3":"a".repeat(64)}, "fide_source":{"blake3":"b".repeat(64)},
        "identity_source":{"blake3":"c".repeat(64)},"certified":false,
        "groups":[{"cohort":"U16","sex":"absolute","population":order.len(),
                    "players":order.iter().enumerate().map(|(i,id)|item(id,i+1)).collect::<Vec<_>>()}]
    })
}
fn save(root: &Path, month: &str, v: &Value) -> PathBuf {
    let p = root.join(format!("{month}.json"));
    fs::write(&p, serde_json::to_vec(v).unwrap()).unwrap();
    p
}
fn cmd(current: &Path, previous: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args([
            "stats",
            "movement",
            "--current",
            current.to_str().unwrap(),
            "--previous",
            previous.to_str().unwrap(),
            "--cohort",
            "U16",
        ])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn movement_identifies_real_entries_exits_and_preserves_deltas() {
    let root = work();
    let old = save(
        &root,
        "2026-10",
        &report("2026-10", &["101", "102", "103", "104"]),
    );
    let now = save(
        &root,
        "2026-11",
        &report("2026-11", &["104", "102", "103", "101"]),
    );
    let out = cmd(&now, &old, &["--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["meta"]["unresolved_count"], 0);
    assert_eq!(j["rows"].as_array().unwrap().len(), 4);
    assert_eq!(j["rows"][0]["movement"], "entered");
    assert_eq!(j["rows"][0]["previous_rank"], 4);
    assert_eq!(j["rows"][0]["current_rank"], 1);
    assert_eq!(j["rows"][0]["rank_change"], 3);
    assert_eq!(j["rows"][3]["movement"], "exited");
    assert_eq!(j["rows"][3]["fsi_id"], "101");
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn refuses_incomplete_lists_or_doubtful_sources() {
    let root = work();
    let mut old = report("2026-10", &["101", "102", "103", "104"]);
    let now = save(
        &root,
        "2026-11",
        &report("2026-11", &["104", "102", "103", "101"]),
    );
    old["groups"][0]["population"] = json!(5);
    let p = save(&root, "2026-10", &old);
    let out = cmd(&now, &p, &[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("troncato"));
    old["groups"][0]["population"] = json!(4);
    old["fsi_manifest_proof"]["verified"] = json!(false);
    fs::write(&p, serde_json::to_vec(&old).unwrap()).unwrap();
    assert!(!cmd(&now, &p, &[]).status.success());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn prevents_false_new_and_bad_month_or_identity_merge() {
    let root = work();
    let old = save(
        &root,
        "2026-10",
        &report("2026-10", &["101", "102", "103", "104"]),
    );
    let now = save(&root, "2026-11", &report("2026-11", &["105", "102", "103"]));
    let out = cmd(&now, &old, &["--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["rows"][0]["movement"], "unknown-prior");
    assert_eq!(j["rows"][0]["previous_rank"], Value::Null);
    assert_eq!(j["rows"][3]["movement"], "unknown-current");
    let mut conflicting = report("2026-11", &["101", "102", "103", "104"]);
    conflicting["groups"][0]["players"][0]["fide_id"] = json!("999999");
    fs::write(&now, serde_json::to_vec(&conflicting).unwrap()).unwrap();
    let out = cmd(&now, &old, &[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Conflitto"));
    conflicting["groups"][0]["players"][0]["fide_id"] = json!("28101");
    conflicting["period"] = json!("2026-12");
    conflicting["fide_month"] = json!("2026-12");
    conflicting["fsi_as_of"] = json!("2026-12-19");
    fs::write(&now, serde_json::to_vec(&conflicting).unwrap()).unwrap();
    assert!(!cmd(&now, &old, &[]).status.success());
    fs::remove_dir_all(root).unwrap();
}
