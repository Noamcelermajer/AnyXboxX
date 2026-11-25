use std::ffi::OsString;
use std::path::PathBuf;

use clap::Parser;

use super::*;

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    run: RunArgs,
}

fn parse(args: &[&str]) -> RunArgs {
    Cli::parse_from(std::iter::once("anyxboxx").chain(args.iter().copied())).run
}

fn names(env: &[(String, OsString)]) -> Vec<&str> {
    env.iter().map(|(k, _)| k.as_str()).collect()
}

#[test]
fn tuned_settings_are_set_by_default_and_can_be_turned_off() {
    let env = settings::base(&parse(&["--game-dir", "G"]));
    assert_eq!(
        env[0],
        ("VICTORY_GAME_DIR".to_string(), OsString::from("G"))
    );
    assert_eq!(env.len(), 1 + TUNED.len());
    assert!(
        TUNED
            .iter()
            .all(|(k, v)| env.iter().any(|(n, x)| n == k && x == v))
    );
    let env = settings::base(&parse(&["--game-dir", "G", "--no-tuned-settings"]));
    assert_eq!(names(&env), ["VICTORY_GAME_DIR"]);
}

#[test]
fn the_relay_and_dred_are_never_in_the_tuned_table() {
    assert!(
        TUNED
            .iter()
            .all(|(k, _)| !k.contains("RELAY") && !k.contains("DRED") && !k.contains("XG"))
    );
}

#[test]
fn opt_in_switches_add_their_variable() {
    let env = settings::base(&parse(&[
        "--game-dir",
        "G",
        "--no-tuned-settings",
        "--dred",
        "--batch-card-rehash",
        "--skip-pipelines",
        "3",
    ]));
    assert_eq!(
        names(&env),
        [
            "VICTORY_GAME_DIR",
            "VICTORY_D3D12_DRED",
            "VICTORY_CARD_REHASH_RUNS",
            "VICTORY_SKIP_PIPELINES"
        ]
    );
    assert_eq!(env[3].1, OsString::from("3"));
    // An empty value is not passed, like `-SkipPipelines ''` in the script.
    let env = settings::base(&parse(&[
        "--game-dir",
        "G",
        "--no-tuned-settings",
        "--skip-pipelines",
        "",
    ]));
    assert_eq!(names(&env), ["VICTORY_GAME_DIR"]);
}

#[test]
fn powershell_names_are_accepted() {
    let args = parse(&[
        "--GameDir",
        "G",
        "--XgDll",
        "x.dll",
        "--XgMode",
        "compare",
        "--Precompile",
    ]);
    assert_eq!(args.game_dir, PathBuf::from("G"));
    assert_eq!(args.xg_mode, XgMode::Compare);
    assert!(args.precompile);
}

#[test]
fn a_missing_xg_library_is_an_error() {
    let args = parse(&["--game-dir", "G", "--xg-dll", "does-not-exist/xg_xs.dll"]);
    assert!(matches!(environment(&args), Err(RunError::NoXgDll(_))));
}

#[test]
fn the_xg_library_is_passed_with_an_absolute_path_and_its_mode() {
    let dll = std::env::current_exe().unwrap();
    let args = parse(&[
        "--game-dir",
        "G",
        "--no-tuned-settings",
        "--xg-dll",
        dll.to_str().unwrap(),
    ]);
    let env = environment(&args).unwrap();
    assert_eq!(
        names(&env),
        ["VICTORY_GAME_DIR", "VICTORY_XG_DLL", "VICTORY_XG_MODE"]
    );
    assert!(PathBuf::from(&env[1].1).is_absolute());
    assert_eq!(env[2].1, OsString::from("real"));
}

#[test]
fn a_repository_without_victory_is_refused_before_building() {
    let empty = std::env::temp_dir().join("anyxboxx-run-no-victory");
    let args = parse(&["--game-dir", "G"]);
    assert!(matches!(run(&args, &empty), Err(RunError::NoVictory)));
}
