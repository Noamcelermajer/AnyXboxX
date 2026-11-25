//! What the window asks for, turned into a launch plan or a refusal. The window keeps text
//! fields; this module reads them, so the rules are tested without any window.

use std::path::{Path, PathBuf};

use anyxboxx::run::XgMode;

use crate::env::{self, EnvError, Xg};
use crate::launch::Plan;
use crate::processes::{self, GuardError, ProcessSource};
use crate::profile::Profile;

/// The window's fields, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub game_dir: String,
    /// The advanced panel: one `KEY=VALUE` per line.
    pub overrides: String,
    /// Path of the user's own `xg_xs.dll`, empty for none.
    pub xg_dll: String,
    pub xg_mode: XgMode,
    /// Worker threads of the translate step; 0 means every core.
    pub jobs: u32,
    /// The advanced panel's override of the one-game guard (for hosts without `tasklist`).
    pub skip_process_check: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            game_dir: String::new(),
            overrides: String::new(),
            xg_dll: String::new(),
            xg_mode: XgMode::Real,
            jobs: 0,
            skip_process_check: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("choose the game folder first (the one holding game_scarlett_master.exe)")]
    NoGameFolder,
    #[error(transparent)]
    Env(#[from] EnvError),
}

impl Settings {
    /// The game folder as a path, when one was typed.
    pub fn game_path(&self) -> Option<PathBuf> {
        let trimmed = self.game_dir.trim().trim_matches('"');
        (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
    }

    /// The jobs limit, `None` for every core.
    pub fn jobs_limit(&self) -> Option<usize> {
        (self.jobs > 0).then_some(self.jobs as usize)
    }
}

/// The launch plan for `profile`, with the user's overrides and XG library applied.
pub fn make_plan(
    settings: &Settings,
    repo: &Path,
    profile: &Profile,
) -> Result<Plan, SessionError> {
    let game_dir = settings.game_path().ok_or(SessionError::NoGameFolder)?;
    let overrides = env::parse_overrides(&settings.overrides)?;
    let xg_text = settings.xg_dll.trim().trim_matches('"');
    let xg = (!xg_text.is_empty()).then(|| Xg {
        dll: PathBuf::from(xg_text),
        mode: settings.xg_mode,
    });
    let env = env::build_env(&game_dir, profile, &overrides, xg.as_ref())?;
    Ok(Plan {
        repo: repo.to_path_buf(),
        game_dir,
        env,
        jobs: settings.jobs_limit(),
    })
}

/// The one-game guard, unless the user skips it in the advanced panel.
pub fn guard(settings: &Settings, source: &impl ProcessSource) -> Result<(), GuardError> {
    if settings.skip_process_check {
        return Ok(());
    }
    processes::ensure_no_game_running(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::GpuVendor;
    use crate::profile::Profiles;
    use std::io;

    struct Fake(io::Result<Vec<String>>);

    impl ProcessSource for Fake {
        fn image_names(&self) -> io::Result<Vec<String>> {
            match &self.0 {
                Ok(names) => Ok(names.clone()),
                Err(e) => Err(io::Error::new(e.kind(), e.to_string())),
            }
        }
    }

    fn game_folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("game_scarlett_master.exe"), b"x").expect("write");
        dir
    }

    fn profile() -> Profile {
        Profiles::embedded()
            .expect("profiles")
            .select(crate::host::Host::Windows, GpuVendor::Nvidia)
            .clone()
    }

    #[test]
    fn plan_needs_a_game_folder() {
        let settings = Settings::default();
        assert!(matches!(
            make_plan(&settings, Path::new("."), &profile()),
            Err(SessionError::NoGameFolder)
        ));
    }

    #[test]
    fn plan_takes_the_jobs_limit_and_the_profile_env() {
        let dir = game_folder();
        let settings = Settings {
            game_dir: format!("\"{}\"", dir.path().display()),
            jobs: 6,
            ..Settings::default()
        };
        let plan = make_plan(&settings, Path::new("repo"), &profile()).expect("plan");
        assert_eq!(plan.jobs, Some(6));
        assert_eq!(plan.game_dir, dir.path());
        assert!(
            plan.env
                .iter()
                .any(|(k, _)| k == "VICTORY_SHADER_LOOP_LIMIT")
        );
    }

    #[test]
    fn zero_jobs_means_every_core() {
        let settings = Settings {
            jobs: 0,
            ..Settings::default()
        };
        assert_eq!(settings.jobs_limit(), None);
    }

    #[test]
    fn overrides_from_the_panel_reach_the_plan() {
        let dir = game_folder();
        let settings = Settings {
            game_dir: dir.path().display().to_string(),
            overrides: "VICTORY_SHADER_OPT=12".to_string(),
            ..Settings::default()
        };
        let plan = make_plan(&settings, Path::new("repo"), &profile()).expect("plan");
        let value = plan
            .env
            .iter()
            .find(|(k, _)| k == "VICTORY_SHADER_OPT")
            .map(|(_, v)| v.clone());
        assert_eq!(value.as_deref(), Some(std::ffi::OsStr::new("12")));
    }

    #[test]
    fn a_bad_override_refuses_the_plan() {
        let dir = game_folder();
        let settings = Settings {
            game_dir: dir.path().display().to_string(),
            overrides: "NOT_A_VICTORY_KEY=1".to_string(),
            ..Settings::default()
        };
        assert!(matches!(
            make_plan(&settings, Path::new("repo"), &profile()),
            Err(SessionError::Env(EnvError::Key(1, _)))
        ));
    }

    #[test]
    fn a_typed_xg_library_that_is_missing_refuses_the_plan() {
        let dir = game_folder();
        let settings = Settings {
            game_dir: dir.path().display().to_string(),
            xg_dll: "Z:/nowhere/xg_xs.dll".to_string(),
            ..Settings::default()
        };
        assert!(make_plan(&settings, Path::new("repo"), &profile()).is_err());
    }

    #[test]
    fn guard_refuses_a_running_game_unless_skipped() {
        let running = Fake(Ok(vec!["victory.exe".to_string()]));
        assert!(guard(&Settings::default(), &running).is_err());
        let skip = Settings {
            skip_process_check: true,
            ..Settings::default()
        };
        assert!(guard(&skip, &running).is_ok());
    }

    #[test]
    fn guard_refuses_when_the_list_cannot_be_read() {
        let broken = Fake(Err(io::Error::other("no tasklist")));
        assert!(matches!(
            guard(&Settings::default(), &broken),
            Err(GuardError::Unknown(_))
        ));
    }
}
