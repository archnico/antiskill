use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp() -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!(
        "antiskill-rankings-official-{}-{n}",
        std::process::id()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
}
const HEAD:&str="Nominativo;Elo;Categoria;K Italia;Federazione;Anno Nascita;ID FSI;Sex;FIN;Elo FIDE Std;K Elo FIDE Std;Games Elo FIDE Std;Elo FIDE Rapid;K Elo FIDE Rapid;Games Elo FIDE Rapid;Elo FIDE Blitz;K Elo FIDE Blitz;Games Elo FIDE Blitz;Attivo;Data Nascita;Provincia Residenza;Regione Residenza;Provincia Tesseramento;Regione Tesseramento;Cittadinanza;Data Ultimo Torneo;Circolo Attuale;Circolo Anno Precedente;Circolo Due Anni Precedenti;Anno Ultimo Tesseramento;Tipo Tessera;Numero Tessera;\n";
fn allin_row(name: &str, fsi: &str, sex: &str, elo: u32, club: &str) -> String {
    format!("{name};{elo};1N;40;ITA;2018;{fsi};{sex};28{fsi};{elo};20;0;{elo};20;0;0;0;0;1;;;;;TOS;;;{club};;;2026;O;;\n")
}
fn fsi_row(id: &str, last: &str, given: &str, club: &str) -> String {
    format!("{id};{last};{given};2018;NC;FI;TOSCANA;ITA;{club};A.S.D. TEST;FI;TOSCANA;TG;15;TG\n")
}
fn pilot(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let a = root.join("allin.csv");
    let f = root.join("members.csv");
    let d = root.join("players_list_xml.xml");
    let allin = format!(
        "{HEAD}{}{}",
        allin_row("ALFA UNO", "101", "F", 1450, "17029"),
        allin_row("BETA DUE", "102", "M", 1500, "17029")
    );
    write(&a, &allin);
    write(
        &f,
        &format!(
            "{}{}{}",
            fsi_row("101", "ALFA", "UNO", "17029"),
            fsi_row("102", "BETA", "DUE", "17029"),
            fsi_row("103", "GAMMA", "TRE", "17029")
        ),
    );
    let fide = r#"<?xml version="1.0" encoding="UTF-8"?>
<playerslist>
<player><fideid>28101</fideid><name>ALFA UNO</name><country>ITA</country>
<sex>F</sex><birthday>2018</birthday><rating>2100</rating><rapid_rating>1950</rapid_rating><blitz_rating>1850</blitz_rating></player>
<player><fideid>28102</fideid><name>BETA DUE</name><sex>M</sex><birthday>2018</birthday>
<rating>2050</rating><rapid_rating>1980</rapid_rating><blitz_rating>1870</blitz_rating></player>
<player><fideid>9999</fideid><rating>2200</rating><birthday>1980</birthday></player>
</playerslist>"#;
    write(&d, fide);
    (a, f, d)
}
fn fixture_manifest(root: &Path, fsi_csv: &Path) -> PathBuf {
    let path = root.join("fsi-manifest.json");
    let bytes = fs::read(fsi_csv).unwrap();
    let payload = serde_json::json!({
        "schema_version":1,
        "source_key":"fsi-members-club-17029-2026",
        "source_url":"https://www.federscacchi.com/fsi/FSI_Programmi/tesserati_csv.php",
        "fetched_at_unix_ms":1795046400000u64,
        "http_status":200,
        "blake3":blake3::hash(&bytes).to_hex().to_string(),
        "bytes":bytes.len()
    });
    write(&path, &serde_json::to_string_pretty(&payload).unwrap());
    path
}
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .arg("rankings")
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn official_overlay_replaces_stale_allin_ratings_and_uses_fsi_census() {
    let root = temp();
    let (a, f, d) = pilot(&root);
    let manifest = fixture_manifest(&root, &f);
    let res = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "standard",
        "--fsi-csv",
        f.to_str().unwrap(),
        "--fsi-as-of",
        "2026-11-19",
        "--fsi-manifest",
        manifest.to_str().unwrap(),
        "--fide-xml",
        d.to_str().unwrap(),
        "--fide-month",
        "2026-11",
        "--require-official",
    ]);
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    let j: Value = serde_json::from_slice(&res.stdout).unwrap();
    assert_eq!(j["membership_authority"], "fsi-official-csv");
    assert_eq!(j["rating_authority"], "fide-official-xml");
    assert_eq!(j["certified"], false);
    assert_eq!(j["fsi_manifest_proof"]["verified"], true);
    assert_eq!(j["coverage"]["membership_types"]["TG"], 3);
    assert_eq!(j["coverage"]["membership_records"], 3);
    assert_eq!(j["coverage"]["members_linked_to_allin"], 2);
    assert_eq!(j["coverage"]["standard_rated"], 2);
    assert_eq!(j["coverage"]["fide_records_matched"], 2);
    assert_eq!(j["groups"][0]["population"], 2);
    assert_eq!(j["groups"][0]["players"][0]["rating"], 2100);
    assert_eq!(j["groups"][0]["players"][1]["rating"], 2050);
    assert_eq!(j["groups"][0]["players"][0]["name"], "ALFA UNO");
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn official_mode_rejects_wrong_period_missing_sources_and_partial_top3() {
    let root = temp();
    let (a, f, d) = pilot(&root);
    let required = [
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "rapid",
        "--require-official",
    ];
    assert!(!run(&required).status.success());
    let bad_date = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--fsi-csv",
        f.to_str().unwrap(),
        "--fsi-as-of",
        "2026-04-18",
    ]);
    assert!(!bad_date.status.success());
    let bad_fide_period = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--fide-xml",
        d.to_str().unwrap(),
        "--fide-month",
        "2026-10",
    ]);
    assert!(!bad_fide_period.status.success());
    let wrong_scope = run(&[
        "top3",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--fsi-csv",
        f.to_str().unwrap(),
        "--fsi-as-of",
        "2026-11-19",
    ]);
    assert!(!wrong_scope.status.success());
    assert!(String::from_utf8_lossy(&wrong_scope.stderr).contains("troppo ristretta"));
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn no_silent_fide_rating_fallback_when_player_missing_from_official_list() {
    let root = temp();
    let (a, f, d) = pilot(&root);
    write(
        &d,
        r#"<playerslist>
<player><fideid>28101</fideid><birthday>2018</birthday><rating>1900</rating></player>
</playerslist>"#,
    );
    let res = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "standard",
        "--fsi-csv",
        f.to_str().unwrap(),
        "--fsi-as-of",
        "2026-11-19",
        "--fide-xml",
        d.to_str().unwrap(),
        "--fide-month",
        "2026-11",
    ]);
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    let j: Value = serde_json::from_slice(&res.stdout).unwrap();
    assert_eq!(j["coverage"]["standard_rated"], 1);
    assert_eq!(j["groups"][0]["players"].as_array().unwrap().len(), 1);
    assert_eq!(j["groups"][0]["players"][0]["rating"], 1900);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_bad_fsi_acquisition_manifest() {
    let root = temp();
    let (a, f, d) = pilot(&root);
    let manifest = fixture_manifest(&root, &f);
    let v = serde_json::json!({
        "schema_version":1,"source_key":"fsi-members-club-99999-2026",
        "source_url":"https://www.federscacchi.com/fsi/FSI_Programmi/tesserati_csv.php",
        "http_status":200,"fetched_at_unix_ms":1795046400000u64,
        "blake3":blake3::hash(&fs::read(&f).unwrap()).to_hex().to_string(),
        "bytes":fs::metadata(&f).unwrap().len()
    });
    write(&manifest, &v.to_string());
    let result = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "standard",
        "--fsi-csv",
        f.to_str().unwrap(),
        "--fsi-as-of",
        "2026-11-19",
        "--fsi-manifest",
        manifest.to_str().unwrap(),
        "--fide-xml",
        d.to_str().unwrap(),
        "--fide-month",
        "2026-11",
        "--require-official",
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("scope errato"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn consecutive_official_fide_lists_produce_only_rating_delta_without_rank_claims() {
    let root = temp();
    let (a, f, d) = pilot(&root);
    let manifest = fixture_manifest(&root, &f);
    let old = root.join("previous_october.xml");
    write(
        &old,
        r#"<playerslist>
<player><fideid>28101</fideid><birthday>2018</birthday><rating>2085</rating></player>
<player><fideid>28102</fideid><birthday>2018</birthday><rating>2060</rating></player>
</playerslist>"#,
    );
    let out = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "standard",
        "--fsi-csv",
        f.to_str().unwrap(),
        "--fsi-as-of",
        "2026-11-19",
        "--fsi-manifest",
        manifest.to_str().unwrap(),
        "--fide-xml",
        d.to_str().unwrap(),
        "--fide-month",
        "2026-11",
        "--previous-fide-xml",
        old.to_str().unwrap(),
        "--previous-fide-month",
        "2026-10",
        "--require-official",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["previous_fide_period"], "2026-10");
    assert_eq!(j["previous_fide_matches"], 2);
    let entries = j["groups"][0]["players"].as_array().unwrap();
    assert_eq!(entries[0]["rating_delta"], 15);
    assert_eq!(entries[1]["rating_delta"], -10);
    assert_eq!(entries[0]["previous_rank"], Value::Null);
    assert_eq!(entries[0]["entered_top"], Value::Null);
    assert_eq!(j["certified"], false);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn previous_fide_month_must_be_consecutive() {
    let root = temp();
    let (a, _f, d) = pilot(&root);
    let bad = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "standard",
        "--fide-xml",
        d.to_str().unwrap(),
        "--fide-month",
        "2026-11",
        "--previous-fide-xml",
        d.to_str().unwrap(),
        "--previous-fide-month",
        "2026-09",
    ]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("mese immediatamente precedente"));
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn fide_zip_stream_matches_plain_xml() {
    // Production ZIP is read without expanding the archive on disk.
    if Command::new("zip").arg("-v").output().is_err()
        || Command::new("unzip").arg("-v").output().is_err()
    {
        return;
    }
    let root = temp();
    let (a, f, d) = pilot(&root);
    let archive = root.join("small-fide.zip");
    let status = Command::new("zip")
        .arg("-q")
        .arg(&archive)
        .arg(d.file_name().unwrap())
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let manifest = fixture_manifest(&root, &f);
    let out = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "standard",
        "--fsi-csv",
        f.to_str().unwrap(),
        "--fsi-as-of",
        "2026-11-19",
        "--fsi-manifest",
        manifest.to_str().unwrap(),
        "--fide-xml",
        archive.to_str().unwrap(),
        "--fide-month",
        "2026-11",
        "--require-official",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["coverage"]["standard_rated"], 2);
    assert_eq!(j["fide_source"]["records"], 3);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn verified_registry_links_block_review_and_unverified_rating_claims() {
    let root = temp();
    let (a, f, d) = pilot(&root);
    let manifest = fixture_manifest(&root, &f);
    let links = root.join("identity_verified.csv");
    write(
        &links,
        "fsi_id,fide_id,status\n101,28101,VERIFIED\n102,28102,REVIEW\n",
    );
    let out = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "standard",
        "--fsi-csv",
        f.to_str().unwrap(),
        "--fsi-as-of",
        "2026-11-19",
        "--fsi-manifest",
        manifest.to_str().unwrap(),
        "--fide-xml",
        d.to_str().unwrap(),
        "--fide-month",
        "2026-11",
        "--verified-links",
        links.to_str().unwrap(),
        "--require-official",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["coverage"]["registry_verified_in_scope"], 1);
    assert_eq!(j["coverage"]["registry_nonverified_rows"], 1);
    assert_eq!(j["coverage"]["standard_rated"], 1);
    assert_eq!(j["groups"][0]["players"].as_array().unwrap().len(), 1);
    assert_eq!(j["groups"][0]["players"][0]["name"], "ALFA UNO");
    assert_eq!(
        j["identity_source"]["authority"],
        "verified-crosswalk-from-local-registry"
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn identity_registry_conflicts_do_not_override_fsi_fide_mismatch_silently() {
    let root = temp();
    let (a, f, d) = pilot(&root);
    let manifest = fixture_manifest(&root, &f);
    let links = root.join("identity_conflict.csv");
    write(
        &links,
        "fsi_id,fide_id,status\n101,999999,VERIFIED\n102,28102,VERIFIED\n",
    );
    let out = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "standard",
        "--fsi-csv",
        f.to_str().unwrap(),
        "--fsi-as-of",
        "2026-11-19",
        "--fsi-manifest",
        manifest.to_str().unwrap(),
        "--fide-xml",
        d.to_str().unwrap(),
        "--fide-month",
        "2026-11",
        "--verified-links",
        links.to_str().unwrap(),
        "--require-official",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["coverage"]["registry_conflicts"], 1);
    assert_eq!(j["groups"][0]["players"].as_array().unwrap().len(), 1);
    assert_eq!(j["groups"][0]["players"][0]["fsi_id"], "102");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn standalone_previous_rapid_uses_generic_rating_as_rapid_elo() {
    let root = temp();
    let (a, f, d) = pilot(&root);
    let manifest = fixture_manifest(&root, &f);
    let old = root.join("oct-rapid.xml");
    write(
        &old,
        r#"<playerslist>
<player><fideid>28101</fideid><birthday>2018</birthday><rating>1850</rating></player>
<player><fideid>28102</fideid><birthday>2018</birthday><rating>1980</rating></player>
</playerslist>"#,
    );
    let out = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "rapid",
        "--fsi-csv",
        f.to_str().unwrap(),
        "--fsi-as-of",
        "2026-11-19",
        "--fsi-manifest",
        manifest.to_str().unwrap(),
        "--fide-xml",
        d.to_str().unwrap(),
        "--fide-month",
        "2026-11",
        "--previous-fide-xml",
        old.to_str().unwrap(),
        "--previous-fide-month",
        "2026-10",
        "--previous-fide-kind",
        "rapid",
        "--require-official",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["previous_fide_kind"], "rapid");
    let entries = j["groups"][0]["players"].as_array().unwrap();
    assert_eq!(entries[0]["rating_delta"], 0); // Beta: 1980 -> 1980
    assert_eq!(entries[1]["rating_delta"], 100); // Alfa: 1850 -> 1950
    assert_eq!(entries[0]["previous_rank"], Value::Null);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn standalone_rapid_requires_explicit_kind() {
    let root = temp();
    let (a, _f, d) = pilot(&root);
    let old = root.join("oct-rapid.xml");
    write(
        &old,
        "<playerslist><player><fideid>28101</fideid><rating>1800</rating></player></playerslist>",
    );
    let out = run(&[
        "club",
        "--snapshot",
        a.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "rapid",
        "--fide-xml",
        d.to_str().unwrap(),
        "--fide-month",
        "2026-11",
        "--previous-fide-xml",
        old.to_str().unwrap(),
        "--previous-fide-month",
        "2026-10",
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--previous-fide-kind"));
    fs::remove_dir_all(root).unwrap();
}
