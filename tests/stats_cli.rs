use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn work() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "antiskill-stats-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
fn cmd(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(["stats", "--help"])
        .output()
        .unwrap(); // help remains available with no archive
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .arg("stats")
        .args(args)
        .args(["--root", root.to_str().unwrap()])
        .output()
        .unwrap()
}
fn fixture(root: &Path) {
    let key = "fsi-members-region-tos-2026";
    let lines = [
        "100;ALFA;ONE;2009;NC;FI;TOSCANA;ITA;17029;FIRENZE SCACCHI;FI;TOSCANA;AG;1;AG",
        "101;BETA;TWO;2013;2N;PI;TOSCANA;ITA;17031;LA TORRE;PI;TOSCANA;TR;2;TR",
        "102;GAMMA;THREE;2017;NC;FI;TOSCANA;ITA;17029;FIRENZE SCACCHI;FI;TOSCANA;AG;3;AG",
    ]
    .join("\n")
        + "\n";
    let bytes = lines.as_bytes();
    let hash = blake3::hash(bytes).to_hex().to_string();
    let path = format!("raw/objects/blake3/{}/{}", &hash[..2], hash);
    let dest = root.join(&path);
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::write(&dest, bytes).unwrap();
    let dir = root.join("raw/manifests").join(key);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("2026.json"),
        serde_json::to_vec(&json!({
            "source_key":key,"http_status":200,"object_path":path,"bytes":bytes.len(),
            "blake3":hash,"fetched_at_unix_ms":1795000000000_u64
        }))
        .unwrap(),
    )
    .unwrap();
    let report = root.join("reports/rankings-fixtures");
    fs::create_dir_all(&report).unwrap();
    fs::write(report.join("top3.json"),serde_json::to_vec(&json!({
        "schema":"antiskill.rankings.v1","period":"2026-11","scope":"top3-region",
        "rating":"fide-standard","filters":"membership-region=TOS, membership-year=2026",
        "certified":false,"membership_authority":"fsi-official-csv","rating_authority":"fide-official-xml",
        "groups":[{"cohort":"U18","sex":"absolute","population":2,"players":[
            {"rank":1,"name":"PERSON ONE","fsi_id":"100","fide_id":"200","club":"17029","rating":1800,"rating_delta":42},
            {"rank":2,"name":"PERSON TWO","fsi_id":"101","fide_id":"201","club":"17031","rating":1700,"rating_delta":null}
        ]},{"cohort":"U18","sex":"female","population":1,"players":[
            {"rank":1,"name":"PERSON TWO","fsi_id":"101","fide_id":"201","club":"17031","rating":1700,"rating_delta":null}
        ]}],
        "coverage":{"membership_types":{"AG":2,"TR":1},"club_provinces":{"FI":2,"PI":1},"residence_provinces":{"FI":2,"PI":1}}
    })).unwrap()).unwrap();
}
#[test]
fn groups_multi_dimension_by_club_and_membership_type() {
    let r = work();
    fixture(&r);
    let out = cmd(
        &r,
        &[
            "members",
            "--region",
            "TOS",
            "--year",
            "2026",
            "--by",
            "club,type",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["meta"]["population_rows"], 3);
    assert_eq!(j["meta"]["distinct_fsi_ids"], 3);
    assert_eq!(j["rows"][0]["memberships"], 2);
    assert_eq!(j["rows"][0]["club"], "17029");
    assert_eq!(j["rows"][0]["type"], "AG");
    let youth = cmd(
        &r,
        &[
            "members", "--region", "TOS", "--year", "2026", "--under", "14", "--by", "club",
            "--format", "json",
        ],
    );
    assert!(youth.status.success());
    let j: Value = serde_json::from_slice(&youth.stdout).unwrap();
    assert_eq!(j["meta"]["matched_rows"], 2);
    fs::remove_dir_all(r).unwrap();
}
#[test]
fn top3_and_coverage_exact_report_selection() {
    let r = work();
    fixture(&r);
    let out = cmd(
        &r,
        &[
            "top3", "--region", "TOS", "--month", "2026-11", "--cohort", "U18", "--sex", "female",
            "--format", "json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["rows"].as_array().unwrap().len(), 1);
    assert_eq!(j["rows"][0]["player"], "PERSON TWO");
    let count = cmd(
        &r,
        &[
            "coverage",
            "--region",
            "TOS",
            "--month",
            "2026-11",
            "--by",
            "club-province",
            "--format",
            "json",
        ],
    );
    assert!(
        count.status.success(),
        "{}",
        String::from_utf8_lossy(&count.stderr)
    );
    let j: Value = serde_json::from_slice(&count.stdout).unwrap();
    assert_eq!(j["rows"][0]["club-province"], "FI");
    assert_eq!(j["rows"][0]["memberships"], 2);
    let absent = cmd(&r, &["top3", "--region", "LOM", "--month", "2026-11"]);
    assert!(!absent.status.success());
    assert!(String::from_utf8_lossy(&absent.stderr).contains("Report assente"));
    fs::remove_dir_all(r).unwrap();
}
#[test]
fn broken_hash_and_unscoped_queries_fail_closed() {
    let r = work();
    fixture(&r);
    assert!(!cmd(&r, &["members", "--year", "2026"]).status.success());
    let object = fs::read_dir(r.join("raw/objects/blake3"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let object = fs::read_dir(object)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(object, b"malicious replacement").unwrap();
    let out = cmd(&r, &["members", "--region", "TOS", "--year", "2026"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("BLAKE3/size"));
    fs::remove_dir_all(r).unwrap();
}

#[test]
fn latest_period_prefers_only_authoritative_managed_report_and_rejects_other_ambiguity() {
    let root = work();
    fixture(&root);
    let first = root.join("reports/rankings-fixtures/top3.json");
    let mut old: Value = serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
    old["period"] = json!("2026-10");
    fs::write(
        root.join("reports/rankings-fixtures/oct.json"),
        serde_json::to_vec(&old).unwrap(),
    )
    .unwrap();
    let res = cmd(
        &root,
        &[
            "top3", "--region", "TOS", "--cohort", "U18", "--format", "json",
        ],
    );
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    let j: Value = serde_json::from_slice(&res.stdout).unwrap();
    assert_eq!(j["meta"]["period"], "2026-11");
    let duplicate = root.join("reports/rankings-fixtures/duplicate.json");
    fs::copy(&first, &duplicate).unwrap();
    let bad = cmd(&root, &["top3", "--region", "TOS"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("report compatibili"));
    let managed = root.join("reports/rankings-2026-11-managed");
    fs::create_dir_all(&managed).unwrap();
    let mut promoted: Value = serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
    promoted["groups"][0]["players"][0]["rating"] = json!(1900);
    fs::write(
        managed.join("top3.json"),
        serde_json::to_vec(&promoted).unwrap(),
    )
    .unwrap();
    let latest = cmd(
        &root,
        &[
            "top3", "--region", "TOS", "--cohort", "U18", "--format", "json",
        ],
    );
    assert!(
        latest.status.success(),
        "{}",
        String::from_utf8_lossy(&latest.stderr)
    );
    let j: Value = serde_json::from_slice(&latest.stdout).unwrap();
    assert_eq!(j["rows"][0]["rating"], 1900);
    let explicit = cmd(
        &root,
        &[
            "top3", "--region", "TOS", "--month", "2026-11", "--format", "json",
        ],
    );
    assert!(
        explicit.status.success(),
        "{}",
        String::from_utf8_lossy(&explicit.stderr)
    );
    fs::remove_dir_all(root).unwrap();
}
