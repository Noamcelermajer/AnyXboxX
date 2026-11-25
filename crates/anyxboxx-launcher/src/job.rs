//! Runs a list of commands one after the other on a background thread, streaming their output
//! lines to the window. A step that exits non-zero ends the list; the exit code is reported.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use crate::progress::LineSplitter;

/// What the window receives from a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A step is starting.
    Started(String),
    /// One line of a step's output (stdout or stderr).
    Line(String),
    /// The list ended: exit code of the last step run (0 when all passed).
    Finished(i32),
    /// A step could not be started.
    Failed(String),
}

/// One command of a job, with the name the window shows for it.
pub struct Step {
    pub label: String,
    pub command: Command,
}

/// Starts the steps on a thread; events go to `tx`. Returns the thread to join when wanted.
pub fn spawn(steps: Vec<Step>, tx: Sender<Event>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let code = run_steps(steps, &tx);
        // The window may have closed: nothing to report to then.
        let _ = tx.send(code);
    })
}

fn run_steps(steps: Vec<Step>, tx: &Sender<Event>) -> Event {
    for step in steps {
        let _ = tx.send(Event::Started(step.label.clone()));
        match run_one(step.command, tx) {
            Ok(0) => {}
            Ok(code) => return Event::Finished(code),
            Err(error) => return Event::Failed(format!("{}: {error}", step.label)),
        }
    }
    Event::Finished(0)
}

/// Runs one command, forwarding both of its streams line by line; returns its exit code.
fn run_one(mut command: Command, tx: &Sender<Event>) -> std::io::Result<i32> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let readers: Vec<JoinHandle<()>> = [
        child
            .stdout
            .take()
            .map(|s| forward(Box::new(s), tx.clone())),
        child
            .stderr
            .take()
            .map(|s| forward(Box::new(s), tx.clone())),
    ]
    .into_iter()
    .flatten()
    .collect();
    let status = child.wait()?;
    for reader in readers {
        let _ = reader.join();
    }
    // A process killed without an exit code counts as failed, as the PowerShell launcher did.
    Ok(status.code().unwrap_or(1))
}

fn forward(mut stream: Box<dyn Read + Send>, tx: Sender<Event>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut splitter = LineSplitter::default();
        let mut buffer = [0u8; 4096];
        while let Ok(read) = stream.read(&mut buffer) {
            if read == 0 {
                break;
            }
            for line in splitter.push(&buffer[..read]) {
                let _ = tx.send(Event::Line(line));
            }
        }
        if let Some(line) = splitter.finish() {
            let _ = tx.send(Event::Line(line));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn collect(steps: Vec<Step>) -> Vec<Event> {
        let (tx, rx) = mpsc::channel();
        spawn(steps, tx).join().expect("job thread");
        rx.try_iter().collect()
    }

    #[cfg(windows)]
    fn shell(line: &str) -> Command {
        let mut command = Command::new("cmd");
        command.args(["/C", line]);
        command
    }

    #[cfg(windows)]
    #[test]
    fn steps_run_in_order_and_their_output_is_forwarded() {
        let events = collect(vec![
            Step {
                label: "one".into(),
                command: shell("echo first"),
            },
            Step {
                label: "two".into(),
                command: shell("echo second"),
            },
        ]);
        assert_eq!(events.first(), Some(&Event::Started("one".into())));
        assert!(events.contains(&Event::Line("first".into())));
        assert!(events.contains(&Event::Started("two".into())));
        assert_eq!(events.last(), Some(&Event::Finished(0)));
    }

    #[cfg(windows)]
    #[test]
    fn a_failing_step_stops_the_list_with_its_exit_code() {
        let events = collect(vec![
            Step {
                label: "bad".into(),
                command: shell("exit 3"),
            },
            Step {
                label: "never".into(),
                command: shell("echo nope"),
            },
        ]);
        assert_eq!(events.last(), Some(&Event::Finished(3)));
        assert!(!events.iter().any(|e| *e == Event::Started("never".into())));
    }

    #[test]
    fn a_missing_program_is_reported_as_failed() {
        let events = collect(vec![Step {
            label: "ghost".into(),
            command: Command::new("anyxboxx-no-such-program-xyz"),
        }]);
        assert!(matches!(events.last(), Some(Event::Failed(m)) if m.starts_with("ghost:")));
    }
}
