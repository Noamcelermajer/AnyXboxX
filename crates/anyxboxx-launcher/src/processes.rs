//! The one-game-at-a-time guard: refuses to start VIctory while a VIctory or game process runs.
//! The process list comes from a `ProcessSource`, so the guard is tested with a fake list. When
//! the list cannot be read the launch is refused too: the guard never assumes that no game runs.

use std::io;
use std::process::Command;

/// Image names of the processes that mean a game is running.
pub const GAME_PROCESSES: [&str; 2] = ["victory.exe", "game_scarlett_master.exe"];

/// Where the running process names come from.
pub trait ProcessSource {
    fn image_names(&self) -> io::Result<Vec<String>>;
}

/// The real list: `tasklist /FO CSV /NH`, which Windows ships (Proton may not: then the check fails
/// and the launch is refused, unless the user states the check is not needed).
pub struct Tasklist;

impl ProcessSource for Tasklist {
    fn image_names(&self) -> io::Result<Vec<String>> {
        let output = Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "tasklist exited with {}",
                output.status
            )));
        }
        Ok(parse_tasklist_csv(&String::from_utf8_lossy(&output.stdout)))
    }
}

/// The first field (image name) of each CSV line: `"victory.exe","1234",...`.
pub fn parse_tasklist_csv(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix('"')?;
            let end = rest.find('"')?;
            Some(rest[..end].to_string())
        })
        .collect()
}

/// Why a launch is refused.
#[derive(Debug, thiserror::Error)]
pub enum GuardError {
    #[error("'{0}' is already running: close the game (or VIctory) first")]
    GameRunning(String),
    #[error(
        "cannot read the process list ({0}): the launch is refused, see the advanced panel to skip this check"
    )]
    Unknown(String),
}

/// `Ok` when no game process runs; the names are compared without regard to case.
pub fn ensure_no_game_running(source: &impl ProcessSource) -> Result<(), GuardError> {
    let names = source
        .image_names()
        .map_err(|e| GuardError::Unknown(e.to_string()))?;
    match running_game(&names) {
        Some(name) => Err(GuardError::GameRunning(name)),
        None => Ok(()),
    }
}

/// The first game process in `names`, as listed in `GAME_PROCESSES`.
pub fn running_game(names: &[String]) -> Option<String> {
    names
        .iter()
        .find(|name| {
            GAME_PROCESSES
                .iter()
                .any(|game| game.eq_ignore_ascii_case(name.trim()))
        })
        .map(|name| name.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(io::Result<Vec<String>>);

    impl ProcessSource for Fake {
        fn image_names(&self) -> io::Result<Vec<String>> {
            match &self.0 {
                Ok(names) => Ok(names.clone()),
                Err(e) => Err(io::Error::new(e.kind(), e.to_string())),
            }
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_the_first_csv_field_of_each_line() {
        let text = "\"explorer.exe\",\"1200\",\"Console\",\"1\",\"120,000 K\"\r\n\"victory.exe\",\"77\",\"Console\",\"1\",\"9 K\"\r\n";
        assert_eq!(
            parse_tasklist_csv(text),
            vec!["explorer.exe", "victory.exe"]
        );
    }

    #[test]
    fn ignores_lines_that_are_not_csv_records() {
        assert_eq!(
            parse_tasklist_csv("INFO: No tasks are running.\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_running_victory_or_game_blocks_the_launch() {
        let fake = Fake(Ok(names(&["explorer.exe", "Victory.EXE"])));
        assert!(matches!(
            ensure_no_game_running(&fake),
            Err(GuardError::GameRunning(name)) if name == "Victory.EXE"
        ));
        let game = Fake(Ok(names(&["game_scarlett_master.exe"])));
        assert!(ensure_no_game_running(&game).is_err());
    }

    #[test]
    fn no_game_process_lets_the_launch_go() {
        let fake = Fake(Ok(names(&["explorer.exe", "notvictory.exe"])));
        assert!(ensure_no_game_running(&fake).is_ok());
    }

    #[test]
    fn an_unreadable_process_list_refuses_the_launch() {
        let fake = Fake(Err(io::Error::other("no tasklist")));
        assert!(matches!(
            ensure_no_game_running(&fake),
            Err(GuardError::Unknown(_))
        ));
    }
}
