use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const HEADER:&str = "Nominativo;Elo;Categoria;K Italia;Federazione;Anno Nascita;ID FSI;Sex;FIN;Elo FIDE Std;K Elo FIDE Std;Games Elo FIDE Std;Elo FIDE Rapid;K Elo FIDE Rapid;Games Elo FIDE Rapid;Elo FIDE Blitz;K Elo FIDE Blitz;Games Elo FIDE Blitz;Attivo;Data Nascita;Provincia Residenza;Regione Residenza;Provincia Tesseramento;Regione Tesseramento;Cittadinanza;Data Ultimo Torneo;Circolo Attuale;Circolo Anno Precedente;Circolo Due Anni Precedenti;Anno Ultimo Tesseramento;Tipo Tessera;Numero Tessera;\n";
fn row(
    name: &str,
    fsi: &str,
    birth: i32,
    sex: &str,
    elo: i32,
    std: i32,
    rapid: i32,
    club: &str,
) -> String {
    format!("{name};{elo};1N;40;ITA;{birth};{fsi};{sex};28{fsi};{std};20;0;{rapid};20;0;0;0;0;1;;;;;TOS;;;{club};;;2026;O;;\n")
}
fn temp_dir() -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!(
        "antiskill-rankings-test-{}-{n}",
        std::process::id()
    ));
    fs::create_dir(&p).unwrap();
    p
}
fn fixtures(root: &Path) -> (PathBuf, PathBuf) {
    let prev = root.join("oct.csv");
    let curr = root.join("nov.csv");
    let mut old = HEADER.to_string();
    let mut new = HEADER.to_string();
    old.push_str(&row(
        "ALFA ORIGINALE",
        "101",
        2018,
        "F",
        1400,
        1400,
        1500,
        "17029",
    ));
    old.push_str(&row("BRAVO", "102", 2018, "M", 1500, 1500, 1500, "17029"));
    old.push_str(&row("CHARLIE", "103", 2018, "M", 1490, 1490, 1450, "17029"));
    old.push_str(&row("DELTA", "104", 2018, "M", 1480, 1480, 1440, "17029"));
    old.push_str(&row(
        "OTHER CLUB",
        "105",
        2018,
        "M",
        1399,
        1399,
        1410,
        "17030",
    ));
    old.push_str(&row(
        "MAESTRO ADULTO",
        "106",
        1980,
        "M",
        2200,
        2200,
        2100,
        "17029",
    ));
    new.push_str(&row(
        "ALFA NUOVO NOME",
        "101",
        2018,
        "F",
        1600,
        1600,
        1550,
        "17029",
    ));
    new.push_str(&row("BRAVO", "102", 2018, "M", 1510, 1510, 1600, "17029"));
    new.push_str(&row("CHARLIE", "103", 2018, "M", 1490, 1490, 1450, "17029"));
    new.push_str(&row("DELTA", "104", 2018, "M", 1480, 1480, 1440, "17029"));
    new.push_str(&row(
        "BIMBO PICCOLO",
        "110",
        2021,
        "M",
        1470,
        1470,
        1399,
        "17029",
    ));
    new.push_str(&row(
        "SENTINEL", "111", 2018, "F", 1399, 1399, 1399, "17029",
    ));
    new.push_str(&row("SENZA RATING", "112", 2018, "M", 0, 0, 0, "17029"));
    new.push_str(&row(
        "OTHER CLUB",
        "105",
        2018,
        "M",
        1430,
        1430,
        1410,
        "17030",
    ));
    new.push_str(&row(
        "MAESTRO ADULTO",
        "106",
        1980,
        "M",
        2200,
        2200,
        2100,
        "17029",
    ));
    fs::write(&prev, old).unwrap();
    fs::write(&curr, new).unwrap();
    (prev, curr)
}
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .arg("rankings")
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn top3_uses_fsi_identity_and_full_previous_population() {
    let root = temp_dir();
    let (p, c) = fixtures(&root);
    let out = cli(&[
        "top3",
        "--snapshot",
        c.to_str().unwrap(),
        "--month",
        "2026-11",
        "--previous",
        p.to_str().unwrap(),
        "--previous-month",
        "2026-10",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["schema"], "antiskill.rankings.v1");
    assert_eq!(j["source"]["records"], 9);
    assert_ne!(j["source"]["blake3"], j["previous_source"]["blake3"]);
    let u8 = j["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["cohort"] == "U08" && g["sex"] == "absolute")
        .unwrap();
    assert_eq!(u8["population"], 6); // includes the very young, but excludes 0/1399
    assert_eq!(u8["players"].as_array().unwrap().len(), 3);
    let winner = &u8["players"][0];
    assert_eq!(winner["fsi_id"], "101");
    assert_eq!(winner["rating_delta"], 200); // old ranking was 4th, yet rating is known
    assert_eq!(winner["previous_rank"], 4);
    assert_eq!(winner["entered_top"], true);
    assert_eq!(winner["rank_change"], 3);
    let female = j["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["cohort"] == "U08" && g["sex"] == "female")
        .unwrap();
    assert_eq!(female["players"][0]["name"], "ALFA NUOVO NOME");
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn club_rapid_u18_is_distinct_and_excludes_adults_and_other_clubs() {
    let root = temp_dir();
    let (_, c) = fixtures(&root);
    let out = cli(&[
        "club",
        "--snapshot",
        c.to_str().unwrap(),
        "--month",
        "2026-11",
        "--club",
        "17029",
        "--rating",
        "rapid",
        "--under",
        "18",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["groups"][0]["cohort"], "U18");
    assert_eq!(j["groups"][0]["players"][0]["name"], "BRAVO");
    let players = j["groups"][0]["players"].as_array().unwrap();
    assert!(players
        .iter()
        .all(|p| p["club"] == "17029" && p["birth_year"].as_i64().unwrap() >= 2008));
    assert!(players
        .iter()
        .all(|p| p["rating"].as_i64().unwrap() > 0 && p["rating"] != 1399));
    assert_eq!(j["previous_source"], Value::Null);
    assert_eq!(j["groups"][0]["players"][0]["rating_delta"], Value::Null);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn rejects_identical_snapshots_and_missing_comparison_period() {
    let root = temp_dir();
    let (_, c) = fixtures(&root);
    let path = c.to_str().unwrap();
    let missing = cli(&[
        "top3",
        "--snapshot",
        path,
        "--month",
        "2026-11",
        "--previous",
        path,
    ]);
    assert!(!missing.status.success());
    let identical = cli(&[
        "top3",
        "--snapshot",
        path,
        "--month",
        "2026-11",
        "--previous",
        path,
        "--previous-month",
        "2026-10",
    ]);
    assert!(!identical.status.success());
    assert!(String::from_utf8_lossy(&identical.stderr).contains("identici"));
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn csv_export_and_no_silent_overwrite() {
    let root = temp_dir();
    let (_, c) = fixtures(&root);
    let path = c.to_str().unwrap();
    let file = root.join("rankings.csv");
    let out = file.to_str().unwrap();
    let args = [
        "top3",
        "--snapshot",
        path,
        "--month",
        "2026-11",
        "--format",
        "csv",
        "--output",
        out,
    ];
    let first = cli(&args);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let contents = fs::read_to_string(&file).unwrap();
    assert!(contents.starts_with("period,scope,rating_type"));
    assert!(contents.contains("ALFA NUOVO NOME"));
    assert!(!cli(&args).status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ambiguous_name_fallback_never_merges_conflicting_federation_ids() {
    let root=temp_dir();
    let old=root.join("old.csv");
    let new=root.join("new.csv");
    fs::write(&old,format!("{}{}",HEADER,row("STESSO NOME","201",2018,"F",1410,1410,0,"17029"))).unwrap();
    fs::write(&new,format!("{}{}",HEADER,row("STESSO NOME","301",2018,"F",1650,1650,0,"17029"))).unwrap();
    let out=cli(&["top3","--snapshot",new.to_str().unwrap(),"--month","2026-11",
        "--previous",old.to_str().unwrap(),"--previous-month","2026-10"]);
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let j:Value=serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["groups"][0]["players"][0]["rating_delta"],Value::Null);
    assert_eq!(j["groups"][0]["players"][0]["previous_rank"],Value::Null);
    fs::remove_dir_all(root).unwrap();
}
