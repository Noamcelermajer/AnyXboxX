//! The launch steps, built on the shared pieces of `anyxboxx::run` (the same commands as
//! `anyxboxx run` and the PowerShell script): build VIctory, optionally translate the shaders into
//! the cache, then run the game. The environment is the same for the translate and the game.

use std::path::PathBuf;

use anyxboxx::run::{Env, RunError, Victory};

use crate::job::Step;

/// What a launch needs, decided by the window.
#[derive(Debug, Clone)]
pub struct Plan {
    pub repo: PathBuf,
    pub game_dir: PathBuf,
    pub env: Env,
    /// Worker threads of the translate step; `None` lets VIctory use every core.
    pub jobs: Option<usize>,
}

/// The steps of a launch, in order.
pub fn play_steps(plan: &Plan, precompile: bool) -> Result<Vec<Step>, RunError> {
    let victory = Victory::locate(&plan.repo)?;
    anyxboxx::run::game_exe(&plan.game_dir)?;
    let mut steps = vec![Step {
        label: "Building VIctory".to_string(),
        command: victory.build_command(),
    }];
    if precompile {
        steps.push(precompile_step(&victory, plan)?);
    }
    steps.push(Step {
        label: "Running the game".to_string(),
        command: victory.game_command(&plan.game_dir, &plan.env),
    });
    Ok(steps)
}

/// The shader preloading alone (VIctory's translate into the cache).
pub fn precompile_steps(plan: &Plan) -> Result<Vec<Step>, RunError> {
    let victory = Victory::locate(&plan.repo)?;
    anyxboxx::run::game_exe(&plan.game_dir)?;
    Ok(vec![
        Step {
            label: "Building VIctory".to_string(),
            command: victory.build_command(),
        },
        precompile_step(&victory, plan)?,
    ])
}

fn precompile_step(victory: &Victory, plan: &Plan) -> Result<Step, RunError> {
    let mut command = victory.translate_command(&plan.game_dir, &plan.env)?;
    if let Some(jobs) = plan.jobs {
        command.arg("--jobs").arg(jobs.to_string());
    }
    Ok(Step {
        label: "Preloading shaders".to_string(),
        command,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::Path;
    use std::process::Command;

    /// The arguments a command receives, for the assertions.
    fn args_of(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn has_victory(repo: &Path) -> bool {
        repo.join("third_party")
            .join("victory")
            .join("Cargo.toml")
            .is_file()
    }

    /// A repository with a VIctory manifest and a game folder holding the files translate needs.
    /// The temp folders are returned so they live as long as the test.
    fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Plan) {
        let repo = tempfile::tempdir().expect("tempdir");
        let victory = repo.path().join("third_party").join("victory");
        std::fs::create_dir_all(&victory).expect("mkdir");
        std::fs::write(victory.join("Cargo.toml"), b"[workspace]\n").expect("write");
        let game = tempfile::tempdir().expect("tempdir");
        for name in [
            "game_scarlett_master.exe",
            "shaders.rpf",
            "XboxBvhBuilder.dll",
            "amd_fidelityfx_upscaler_xbox.dll",
        ] {
            std::fs::write(game.path().join(name), b"x").expect("write");
        }
        let plan = Plan {
            repo: repo.path().to_path_buf(),
            game_dir: game.path().to_path_buf(),
            env: vec![("VICTORY_LATE_WAIT".to_string(), OsString::from("3"))],
            jobs: Some(4),
        };
        (repo, game, plan)
    }

    #[test]
    fn play_runs_build_then_game_without_precompile() {
        let (_repo, _game, plan) = fixture();
        let steps = play_steps(&plan, false).expect("steps");
        let labels: Vec<&str> = steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["Building VIctory", "Running the game"]);
        let game_args = args_of(&steps[1].command);
        assert_eq!(game_args.first().map(String::as_str), Some("run"));
        assert!(game_args.iter().any(|a| a == "--execute"));
    }

    #[test]
    fn precompile_step_passes_the_jobs_limit_after_the_inputs() {
        let (_repo, _game, plan) = fixture();
        let steps = precompile_steps(&plan).expect("steps");
        let args = args_of(&steps[1].command);
        assert_eq!(args.first().map(String::as_str), Some("translate"));
        let at = args
            .iter()
            .position(|a| a == "--jobs")
            .expect("--jobs given");
        assert_eq!(args.get(at + 1).map(String::as_str), Some("4"));
    }

    #[test]
    fn no_jobs_limit_means_no_flag() {
        let (_repo, _game, mut plan) = fixture();
        plan.jobs = None;
        let steps = precompile_steps(&plan).expect("steps");
        assert!(!args_of(&steps[1].command).iter().any(|a| a == "--jobs"));
    }

    #[test]
    fn a_missing_game_exe_refuses_the_launch() {
        let (_repo, _game, mut plan) = fixture();
        plan.game_dir = PathBuf::from("Z:/no/such/game/folder");
        assert!(play_steps(&plan, true).is_err());
    }

    #[test]
    fn a_repository_without_victory_is_an_error() {
        let (_repo, _game, mut plan) = fixture();
        let empty = tempfile::tempdir().expect("tempdir");
        plan.repo = empty.path().to_path_buf();
        assert!(!has_victory(&plan.repo));
        assert!(play_steps(&plan, false).is_err());
    }
}
