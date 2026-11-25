//! The steps of a launch as reusable pieces: the CLI (`anyxboxx run`) chains them like the
//! script did, the graphical launcher (`crates/anyxboxx-launcher`) drives them one by one.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{RunArgs, RunError, settings};

pub const GAME_EXE: &str = "game_scarlett_master.exe";
/// The game's two DLLs translated with its shaders by `--precompile`.
pub const PRECOMPILED_DLLS: [&str; 2] = ["XboxBvhBuilder.dll", "amd_fidelityfx_upscaler_xbox.dll"];
pub const SHADERS: &str = "shaders.rpf";

/// Environment variables given to VIctory, in the order they are set.
pub type Env = Vec<(String, OsString)>;

/// Where VIctory's sources and its built `victory.exe` are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Victory {
    pub manifest: PathBuf,
    pub exe: PathBuf,
}

impl Victory {
    /// VIctory in `repo`; its target folder is `third_party\victory\target` unless
    /// `CARGO_TARGET_DIR` is set (VIctory is its own Cargo workspace).
    pub fn locate(repo: &Path) -> Result<Self, RunError> {
        let victory = repo.join("third_party").join("victory");
        let manifest = victory.join("Cargo.toml");
        if !manifest.is_file() {
            return Err(RunError::NoVictory);
        }
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map_or_else(|| victory.join("target"), PathBuf::from);
        Ok(Victory {
            manifest,
            exe: target.join("diag").join("victory.exe"),
        })
    }

    /// `cargo build` of the diag profile with diagnostics: does nothing when victory.exe is
    /// current, rebuilds it after a pull that changes third_party/victory.
    pub fn build_command(&self) -> Command {
        let mut command = Command::new("cargo");
        command.args(["build", "--profile", "diag", "-p", "victory"]);
        command.args(["--features", "diagnostics", "--manifest-path"]);
        command.arg(&self.manifest);
        command
    }

    /// `victory translate --cache --pipelines --used-pipelines` on shaders.rpf, the game and its
    /// two DLLs, with the game's environment (the cache key depends on it). Only these files of
    /// the game folder are read, never the folder around it.
    pub fn translate_command(&self, game_dir: &Path, env: &Env) -> Result<Command, RunError> {
        let shaders = game_dir.join(SHADERS);
        let dlls: Vec<PathBuf> = PRECOMPILED_DLLS.iter().map(|d| game_dir.join(d)).collect();
        for needed in std::iter::once(&shaders).chain(&dlls) {
            if !needed.is_file() {
                return Err(RunError::PrecompileInput(needed.display().to_string()));
            }
        }
        let mut command = Command::new(&self.exe);
        command.args(["translate", "--cache", "--pipelines", "--used-pipelines"]);
        command
            .arg(&shaders)
            .arg(game_dir.join(GAME_EXE))
            .args(&dlls);
        command.envs(pairs(env));
        Ok(command)
    }

    /// `victory run <game> --execute` with the game's environment.
    pub fn game_command(&self, game_dir: &Path, env: &Env) -> Command {
        let mut command = Command::new(&self.exe);
        command
            .arg("run")
            .arg(game_dir.join(GAME_EXE))
            .arg("--execute");
        command.envs(pairs(env));
        command
    }
}

/// The game executable of `game_dir`, which must exist.
pub fn game_exe(game_dir: &Path) -> Result<PathBuf, RunError> {
    let exe = game_dir.join(GAME_EXE);
    if exe.exists() {
        Ok(exe)
    } else {
        Err(RunError::NoGame(game_dir.display().to_string()))
    }
}

/// The game's environment, checked like the script (the XG library must exist).
pub fn environment(args: &RunArgs) -> Result<Env, RunError> {
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

fn pairs(env: &Env) -> impl Iterator<Item = (&OsStr, &OsStr)> {
    env.iter()
        .map(|(k, v)| (OsStr::new(k.as_str()), v.as_os_str()))
}

/// Waits for the command; a process killed without exit code counts as 1, like PowerShell.
pub fn status(command: &mut Command, name: &str) -> Result<i32, RunError> {
    let status = command
        .status()
        .map_err(|e| RunError::Spawn(name.to_string(), e))?;
    Ok(status.code().unwrap_or(1))
}
