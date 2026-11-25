//! VIctory's shader cache: where it is, and removing it. VIctory keeps the cache in
//! `%LOCALAPPDATA%\victory\cache` (or `VICTORY_CACHE_DIR`); its saves and data live in
//! `victory\data`, which this module never touches.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

/// The folder name the cache must have for the launcher to delete it.
pub const CACHE_NAME: &str = "cache";

/// Where VIctory's cache is, from the environment values it reads (`VICTORY_CACHE_DIR`, then
/// `LOCALAPPDATA`, then `XDG_CACHE_HOME`, then `HOME/.cache`, as victory-core does).
pub fn cache_dir(
    override_dir: Option<&OsStr>,
    local_app_data: Option<&OsStr>,
    xdg_cache_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Option<PathBuf> {
    let non_empty = |value: Option<&OsStr>| value.filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = non_empty(override_dir) {
        return Some(dir);
    }
    if let Some(base) = non_empty(local_app_data).or_else(|| non_empty(xdg_cache_home)) {
        return Some(base.join("victory").join(CACHE_NAME));
    }
    non_empty(home).map(|h| h.join(".cache").join("victory").join(CACHE_NAME))
}

/// The cache of the current environment.
pub fn current_cache_dir() -> Option<PathBuf> {
    cache_dir(
        std::env::var_os("VICTORY_CACHE_DIR").as_deref(),
        std::env::var_os("LOCALAPPDATA").as_deref(),
        std::env::var_os("XDG_CACHE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

/// What `clean_cache` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanOutcome {
    Removed,
    NothingToClean,
}

#[derive(Debug, thiserror::Error)]
pub enum CleanError {
    #[error("refusing to delete '{0}': the launcher only deletes a folder named '{CACHE_NAME}'")]
    NotACache(String),
    #[error("cannot delete '{0}': {1}")]
    Io(String, io::Error),
}

/// Deletes the cache folder `dir` and everything in it. Only a folder whose name is `cache` is
/// deleted; anything else is refused, whatever the environment says.
pub fn clean_cache(dir: &Path) -> Result<CleanOutcome, CleanError> {
    let named_cache = dir.file_name().is_some_and(|n| n == CACHE_NAME);
    if !named_cache {
        return Err(CleanError::NotACache(dir.display().to_string()));
    }
    match std::fs::symlink_metadata(dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(CleanOutcome::NothingToClean),
        Err(error) => Err(CleanError::Io(dir.display().to_string(), error)),
        Ok(_) => std::fs::remove_dir_all(dir)
            .map(|()| CleanOutcome::Removed)
            .map_err(|e| CleanError::Io(dir.display().to_string(), e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_over_local_app_data() {
        let dir = cache_dir(
            Some(OsStr::new("D:\\vc")),
            Some(OsStr::new("C:\\Users\\u\\AppData\\Local")),
            None,
            None,
        );
        assert_eq!(dir, Some(PathBuf::from("D:\\vc")));
    }

    #[test]
    fn default_is_local_app_data_victory_cache() {
        let dir = cache_dir(None, Some(OsStr::new("C:\\L")), None, None);
        assert_eq!(
            dir,
            Some(PathBuf::from("C:\\L").join("victory").join("cache"))
        );
    }

    #[test]
    fn falls_back_to_xdg_then_home_and_empty_values_do_not_count() {
        let xdg = cache_dir(
            Some(OsStr::new("")),
            Some(OsStr::new("")),
            Some(OsStr::new("/x")),
            None,
        );
        assert_eq!(xdg, Some(PathBuf::from("/x").join("victory").join("cache")));
        let home = cache_dir(None, None, None, Some(OsStr::new("/h")));
        assert_eq!(
            home,
            Some(
                PathBuf::from("/h")
                    .join(".cache")
                    .join("victory")
                    .join("cache")
            )
        );
        assert_eq!(cache_dir(None, None, None, None), None);
    }

    #[test]
    fn clean_removes_a_folder_named_cache_with_its_content() {
        let root = tempfile::tempdir().expect("tempdir");
        let cache = root.path().join("cache");
        std::fs::create_dir_all(cache.join("shaders")).expect("mkdir");
        std::fs::write(cache.join("shaders").join("x.bin"), b"x").expect("write");
        assert_eq!(clean_cache(&cache).expect("clean"), CleanOutcome::Removed);
        assert!(!cache.exists());
        assert_eq!(
            clean_cache(&cache).expect("clean"),
            CleanOutcome::NothingToClean
        );
    }

    #[test]
    fn clean_refuses_any_folder_not_named_cache() {
        let root = tempfile::tempdir().expect("tempdir");
        let data = root.path().join("data");
        std::fs::create_dir_all(&data).expect("mkdir");
        assert!(matches!(clean_cache(&data), Err(CleanError::NotACache(_))));
        assert!(data.exists(), "a refused folder is left alone");
    }
}
