use std::process::Command;

fn command(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_antiskill"))
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[test]
fn top_level_help_separates_product_from_laboratorio() {
    let output = command(&["--help"]);
    assert!(output.status.success());
    let help = stdout(&output);

    assert!(help.contains("Archivio scacchistico locale, verificabile"));
    assert!(help.contains("ARCHIVIO:"));
    assert!(help.contains("FONTI:"));
    assert!(help.contains("TECNICI:"));
    assert!(help.contains("LABORATORIO:"));
    assert!(help.contains("    lab          Strumenti sperimentali"));
    assert!(help.contains("antiskill lab --help"));

    for legacy in [
        "    theory       ",
        "    positions    ",
        "    player       ",
        "    ladder       ",
    ] {
        assert!(!help.contains(legacy), "legacy surface leaked: {legacy}");
    }
}

#[test]
fn lab_help_exposes_only_current_experimental_surface() {
    for args in [
        ["lab"].as_slice(),
        &["lab", "help"],
        &["lab", "--help"],
        &["lab", "-h"],
    ] {
        let output = command(args);
        assert!(output.status.success(), "{}", stderr(&output));
        let help = stdout(&output);
        assert!(help.contains("antiskill lab <COMANDO>"));
        assert!(help.contains("    theory       "));
        assert!(!help.contains("positions"));
        assert!(!help.contains("ladder"));
    }
}

#[test]
fn theory_lives_under_lab_and_legacy_alias_is_explicit() {
    let canonical = command(&["lab", "theory", "--help"]);
    assert!(canonical.status.success(), "{}", stderr(&canonical));
    assert!(stdout(&canonical).contains("antiskill lab theory <COMANDO>"));
    assert!(canonical.stderr.is_empty());

    let legacy = command(&["theory", "--help"]);
    assert!(legacy.status.success());
    assert!(stdout(&legacy).contains("antiskill lab theory <COMANDO>"));
    assert!(stderr(&legacy).contains("alias legacy"));
    assert!(stderr(&legacy).contains("antiskill lab theory"));
}

#[test]
fn removed_legacy_stubs_are_unknown_top_level_commands() {
    for command_name in ["positions", "player", "ladder"] {
        let output = command(&[command_name]);
        assert!(!output.status.success());
        assert!(stderr(&output).contains(&format!("Comando sconosciuto: {command_name}")));
    }
}

#[test]
fn unknown_lab_command_fails_without_falling_into_product() {
    let output = command(&["lab", "unknown"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("Sottocomando lab sconosciuto: unknown"));
    assert!(stdout(&output).contains("antiskill lab <COMANDO>"));
}
