//! `anyxboxx run`: builds VIctory (cargo does nothing when it is current) and launches it on the
//! user's own game folder. Rust replacement of `tools/lancer_victory.ps1`, step for step: same
//! checks, same build command, same environment, same precompile step, same exit code.

mod settings;

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Args, ValueEnum};

pub const GAME_EXE: &str = "game_scarlett_master.exe";
/// The game's two DLLs translated with its shaders by `--precompile`.
pub const PRECOMPILED_DLLS: [&str; 2] = ["XboxBvhBuilder.dll", "amd_fidelityfx_upscaler_xbox.dll"];
const SHADERS: &str = "shaders.rpf";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum XgMode {
    /// Serve the title from xg_xs.dll.
    Real,
    /// Keep VIctory's layouts and log every difference with the library.
    Compare,
}

impl XgMode {
    fn as_str(self) -> &'static str {
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

/// Runs the launch; returns VIctory's exit code.
pub fn run(args: &RunArgs, repo: &Path) -> Result<i32, RunError> {
    let victory = repo.join("third_party").join("victory");
    let manifest = victory.join("Cargo.toml");
    if !manifest.is_file() {
        return Err(RunError::NoVictory);
    }
    let game_exe = args.game_dir.join(GAME_EXE);
    if !game_exe.exists() {
        return Err(RunError::NoGame(args.game_dir.display().to_string()));
    }
    // VIctory is its own Cargo workspace: its target folder is third_party\victory\target
    // unless CARGO_TARGET_DIR is set.
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| victory.join("target"), PathBuf::from);
    let exe = target.join("diag").join("victory.exe");

    // Always ask cargo: it does nothing when victory.exe is current, and rebuilds it after a
    // pull that changes third_party/victory (an old victory.exe would ignore new settings).
    println!("Building VIctory (diag profile, diagnostics feature)...");
    let build = [
        "build",
        "--profile",
        "diag",
        "-p",
        "victory",
        "--features",
        "diagnostics",
        "--manifest-path",
    ];
    let code = status(Command::new("cargo").args(build).arg(&manifest), "cargo")?;
    if code != 0 {
        return Err(RunError::BuildFailed(code));
    }

    let env = environment(args)?;
    if args.precompile {
        precompile(&exe, &args.game_dir, &game_exe, &env)?;
    }
    status(
        Command::new(&exe)
            .arg("run")
            .arg(&game_exe)
            .arg("--execute")
            .envs(pairs(&env)),
        "victory",
    )
}

/// The game's environment, checked like the script (the XG library must exist).
fn environment(args: &RunArgs) -> Result<Vec<(String, OsString)>, RunError> {
    let mut env = settings::base(args);
    if let Some(dll) = args.xg_dll.as_deref().filter(|p| !p.as_os_str().is_empty()) {
        if !dll.is_file() {
            return Err(RunError::NoXgDll(dll.display().to_string()));
        }
        let absolute =
            std::path::absolute(dll).map_err(|e| RunError::Spawn(dll.display().to_string(), e))?;
        env.extend(settings::xg(&absolute, args.xg_mode.as_str()));
    }
    Ok(env)
}

/// Same environment as the game: the cache key depends on it.
fn precompile(
    exe: &Path,
    game_dir: &Path,
    game_exe: &Path,
    env: &[(String, OsString)],
) -> Result<(), RunError> {
    let shaders = game_dir.join(SHADERS);
    let dlls: Vec<PathBuf> = PRECOMPILED_DLLS.iter().map(|d| game_dir.join(d)).collect();
    for needed in std::iter::once(&shaders).chain(&dlls) {
        if !needed.is_file() {
            return Err(RunError::PrecompileInput(needed.display().to_string()));
        }
    }
    println!(
        "Precompiling shaders and pipelines into the cache (several minutes the first time)..."
    );
    let mut command = Command::new(exe);
    command.args(["translate", "--cache", "--pipelines", "--used-pipelines"]);
    command
        .arg(&shaders)
        .arg(game_exe)
        .args(&dlls)
        .envs(pairs(env));
    match status(&mut command, "victory")? {
        0 => Ok(()),
        code => Err(RunError::TranslateFailed(code)),
    }
}

fn pairs(env: &[(String, OsString)]) -> impl Iterator<Item = (&OsStr, &OsStr)> {
    env.iter()
        .map(|(k, v)| (OsStr::new(k.as_str()), v.as_os_str()))
}

/// Waits for the command; a process killed without exit code counts as 1, like PowerShell.
fn status(command: &mut Command, name: &str) -> Result<i32, RunError> {
    let status = command
        .status()
        .map_err(|e| RunError::Spawn(name.to_string(), e))?;
    Ok(status.code().unwrap_or(1))
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
