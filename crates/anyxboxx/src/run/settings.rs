//! The environment `anyxboxx run` gives VIctory: the same variables, values and conditions as
//! `tools/lancer_victory.ps1`.

use std::ffi::OsString;
use std::path::Path;

use super::RunArgs;

/// Settings measured on 2026-11-24 with the VIctory commit then pinned (fxceless/victory-gta6).
/// No relay mode on purpose: the run judged best had it off, and the fork's issue #89 reports
/// that it makes the characters' skin disappear and overlaps the subtitles.
pub const TUNED: [(&str, &str); 6] = [
    // Translated shader loops stop after 256 iterations (32 cuts the hair strand loops short).
    ("VICTORY_SHADER_LOOP_LIMIT", "256"),
    // Host pipeline wait setting from the fork's pull request #1.
    ("VICTORY_LATE_WAIT", "3"),
    // Wide back-end option of the fork, on in the measured run.
    ("VICTORY_WIDE_BACK", "1"),
    // Card buffer sizing: minimum and total, in MB.
    ("VICTORY_CARD_BUFFERS_MIN_MB", "1"),
    ("VICTORY_CARD_BUFFERS_MB", "1024"),
    // DirectStorage writes into physical pages are reported to the GPU side (fork commits
    // e49864a and bf870de by Yorivel): the first room's walls no longer read stale copies.
    ("VICTORY_DSTORAGE_NOTIFY", "1"),
];

pub const GAME_DIR: &str = "VICTORY_GAME_DIR";
/// VIctory turns DRED on when the variable exists, whatever its value: only set when asked.
pub const DRED: &str = "VICTORY_D3D12_DRED";
pub const CARD_REHASH: &str = "VICTORY_CARD_REHASH_RUNS";
pub const SKIP_PIPELINES: &str = "VICTORY_SKIP_PIPELINES";
pub const XG_DLL: &str = "VICTORY_XG_DLL";
pub const XG_MODE: &str = "VICTORY_XG_MODE";

/// Variables set before the XG library is checked, in the script's order.
pub fn base(args: &RunArgs) -> Vec<(String, OsString)> {
    let mut env = vec![(GAME_DIR.to_string(), args.game_dir.clone().into_os_string())];
    if !args.no_tuned_settings {
        env.extend(
            TUNED
                .iter()
                .map(|(k, v)| ((*k).to_string(), OsString::from(*v))),
        );
    }
    if args.dred {
        env.push((DRED.to_string(), OsString::from("1")));
    }
    if args.batch_card_rehash {
        env.push((CARD_REHASH.to_string(), OsString::from("1")));
    }
    if let Some(skip) = args.skip_pipelines.as_deref().filter(|s| !s.is_empty()) {
        env.push((SKIP_PIPELINES.to_string(), OsString::from(skip)));
    }
    env
}

/// Your own copy of the GDK's offline PC `xg_xs.dll` (never in the repository): Microsoft's
/// library lays out the textures. `absolute` is the resolved path of an existing file.
pub fn xg(absolute: &Path, mode: &str) -> [(String, OsString); 2] {
    [
        (XG_DLL.to_string(), absolute.as_os_str().to_owned()),
        (XG_MODE.to_string(), OsString::from(mode)),
    ]
}
