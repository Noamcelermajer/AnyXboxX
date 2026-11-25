//! `anyxboxx run`: builds VIctory (cargo does nothing when it is current) and launches it on the
//! user's own game folder. Rust replacement of `tools/lancer_victory.ps1`, step for step: same
//! checks, same build command, same environment, same precompile step, same exit code.

mod settings;
mod steps;

use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};

pub use settings::TUNED;
pub use steps::{Env, GAME_EXE, PRECOMPILED_DLLS, SHADERS, Victory, environment, game_exe, status};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum XgMode {
    /// Serve the title from xg_xs.dll.
    Real,
    /// Keep VIctory's layouts and log every difference with the library.
    Compare,
}

impl XgMode {
    pub fn as_str(self) -> &'static str {
        match self {
            XgMode::Real => "real",
            XgMode::Compare => "compare",
        }
    }
}

/// Options of `anyxboxx run`; the PowerShell names (`--GameDir`…) are accepted as aliases.
#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    /// Folder of the game package (kept outside this repository); must hold game_scarlett_master.exe.
    #[arg(long, alias = "GameDir")]
    pub game_dir: PathBuf,
    /// Passed unchanged to VICTORY_SKIP_PIPELINES.
    #[arg(long, alias = "SkipPipelines")]
    pub skip_pipelines: Option<String>,
    /// Do not set the tuned variables (only VICTORY_GAME_DIR is set).
    #[arg(long, alias = "NoTunedSettings")]
    pub no_tuned_settings: bool,
    /// Turn on device-removed data (VICTORY_D3D12_DRED); slows each frame.
    #[arg(long, alias = "Dred")]
    pub dred: bool,
    /// Translate shaders.rpf, the game and its two DLLs into the cache before the game.
    #[arg(long, alias = "Precompile")]
    pub precompile: bool,
    /// Your own copy of the GDK's offline PC xg_xs.dll (never in this repository).
    #[arg(long, alias = "XgDll")]
    pub xg_dll: Option<PathBuf>,
    /// With --xg-dll: `real` serves the title from the library, `compare` logs differences.
    #[arg(long, alias = "XgMode", value_enum, default_value = "real")]
    pub xg_mode: XgMode,
    /// Set VICTORY_CARD_REHASH_RUNS=1 (hash adjacent GPU memory ranges in batches).
    #[arg(long, alias = "BatchCardRehash")]
    pub batch_card_rehash: bool,
    /// Repository root (default: ANYXBOXX_REPO, then the folders above the binary and the
    /// current folder that hold third_party/victory/Cargo.toml).
    #[arg(long)]
    pub repo: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("third_party/victory/Cargo.toml is missing: this copy of AnyXboxX is incomplete.")]
    NoVictory,
    #[error("game_scarlett_master.exe not found in '{0}'.")]
    NoGame(String),
    #[error("cargo build failed (exit code {0}).")]
    BuildFailed(i32),
    #[error("xg_xs.dll not found: '{0}'.")]
    NoXgDll(String),
    #[error("--precompile needs '{0}'.")]
    PrecompileInput(String),
    #[error("victory translate failed (exit code {0}).")]
    TranslateFailed(i32),
    #[error("cannot start {0}: {1}")]
    Spawn(String, std::io::Error),
}

/// Runs the launch like the script, step for step; returns VIctory's exit code.
pub fn run(args: &RunArgs, repo: &Path) -> Result<i32, RunError> {
    let victory = Victory::locate(repo)?;
    game_exe(&args.game_dir)?;
    println!("Building VIctory (diag profile, diagnostics feature)...");
    let code = status(&mut victory.build_command(), "cargo")?;
    if code != 0 {
        return Err(RunError::BuildFailed(code));
    }
    let env = environment(args)?;
    if args.precompile {
        let mut translate = victory.translate_command(&args.game_dir, &env)?;
        println!(
            "Precompiling shaders and pipelines into the cache (several minutes the first time)..."
        );
        match status(&mut translate, "victory")? {
            0 => {}
            code => return Err(RunError::TranslateFailed(code)),
        }
    }
    status(&mut victory.game_command(&args.game_dir, &env), "victory")
}

/// Repository root: `--repo`, `ANYXBOXX_REPO`, then the first folder above the binary or the
/// current folder that holds third_party/victory/Cargo.toml.
pub fn find_repo(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(repo) = explicit {
        return Some(repo.to_path_buf());
    }
    if let Some(repo) = std::env::var_os("ANYXBOXX_REPO") {
        return Some(PathBuf::from(repo));
    }
    let starts = [
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf)),
        std::env::current_dir().ok(),
    ];
    starts.into_iter().flatten().find_map(|start| {
        start
            .ancestors()
            .find(|dir| {
                dir.join("third_party")
                    .join("victory")
                    .join("Cargo.toml")
                    .is_file()
            })
            .map(Path::to_path_buf)
    })
}

#[cfg(test)]
mod tests;
