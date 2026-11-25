//! Reads VIctory's output: the progress lines of `victory translate --cache` and the lines the
//! launcher keeps for its summary. VIctory writes its progress as
//! `<file>: <done> of <total> containers (<pct>%), <secs> s; all files so far: ...` (the
//! pipeline step says `pipelines to make` instead of `containers`), with `\r` between updates on a
//! terminal and `\n` otherwise; the launcher's pipe is never a terminal.

use std::sync::OnceLock;

use regex::Regex;

/// One progress report of a translate run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub file: String,
    pub done: u64,
    pub total: u64,
    pub percent: u64,
    /// `containers` or `pipelines to make`, as VIctory names them.
    pub what: String,
}

impl Progress {
    /// The fraction for a progress bar, between 0 and 1.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.done.min(self.total) as f32) / (self.total as f32)
    }
}

fn progress_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?P<file>.*): (?P<done>\d+) of (?P<total>\d+) (?P<what>containers|pipelines to make) \((?P<pct>\d+)%\)",
        )
        .expect("the progress pattern is valid")
    })
}

/// Parses one line of VIctory's output; `None` for any other line.
pub fn parse_progress(line: &str) -> Option<Progress> {
    let caps = progress_regex().captures(line.trim_end())?;
    let number = |name: &str| caps[name].parse::<u64>().ok();
    Some(Progress {
        file: caps["file"].to_string(),
        done: number("done")?,
        total: number("total")?,
        percent: number("pct")?,
        what: caps["what"].to_string(),
    })
}

/// Splits a byte stream into lines on `\n` and `\r`, keeping a partial line for the next chunk.
#[derive(Debug, Default)]
pub struct LineSplitter {
    pending: Vec<u8>,
}

impl LineSplitter {
    /// Feeds bytes and returns the complete lines they end (empty lines dropped).
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for &byte in bytes {
            if byte == b'\n' || byte == b'\r' {
                if !self.pending.is_empty() {
                    lines.push(String::from_utf8_lossy(&self.pending).into_owned());
                    self.pending.clear();
                }
            } else {
                self.pending.push(byte);
            }
        }
        lines
    }

    /// The last partial line, when the stream ends without a newline.
    pub fn finish(&mut self) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        let line = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        Some(line)
    }
}

/// The last progress report and the last few lines, for the summary.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    pub last_progress: Option<Progress>,
    pub last_line: Option<String>,
    pub lines: u64,
}

impl Summary {
    /// The summary after one more line of output.
    pub fn with_line(&self, line: &str) -> Self {
        Summary {
            last_progress: parse_progress(line).or_else(|| self.last_progress.clone()),
            last_line: Some(line.to_string()),
            lines: self.lines + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_FORMAT: &str = "C:\\Games\\pkg\\shaders.rpf: 1280 of 25689 containers (4%), 12.5 s; all files so far: 1100 cached, 150 translated, 20 cached refusals, 10 newly refused";

    #[test]
    fn parses_the_translate_progress_line() {
        let progress = parse_progress(REAL_FORMAT).expect("progress line");
        assert_eq!(progress.file, "C:\\Games\\pkg\\shaders.rpf");
        assert_eq!(
            (progress.done, progress.total, progress.percent),
            (1280, 25689, 4)
        );
        assert_eq!(progress.what, "containers");
    }

    #[test]
    fn parses_the_pipeline_step_line() {
        let line = "C:\\x\\shaders.rpf: 7 of 90 pipelines to make (7%), 3 s; 5 made, 2 found in the library";
        let progress = parse_progress(line).expect("progress line");
        assert_eq!(progress.what, "pipelines to make");
        assert_eq!(progress.total, 90);
    }

    #[test]
    fn other_lines_are_not_progress() {
        assert!(parse_progress("containers   25689 in 270.1 s: 0 already cached").is_none());
        assert!(parse_progress("cache        C:\\x (13000 MB)").is_none());
        assert!(parse_progress("").is_none());
    }

    #[test]
    fn fraction_is_clamped_and_zero_safe() {
        let mut progress = parse_progress(REAL_FORMAT).expect("progress line");
        assert!((progress.fraction() - 1280.0 / 25689.0).abs() < 1e-6);
        progress.total = 0;
        assert_eq!(progress.fraction(), 0.0);
        progress.total = 10;
        progress.done = 99;
        assert_eq!(progress.fraction(), 1.0);
    }

    #[test]
    fn splitter_handles_carriage_returns_and_split_chunks() {
        let mut splitter = LineSplitter::default();
        let mut lines = splitter.push(b"first\r\nsec");
        lines.extend(splitter.push(b"ond\r"));
        lines.extend(splitter.push(b"\n\n"));
        assert_eq!(lines, vec!["first", "second"]);
        assert_eq!(splitter.finish(), None);
    }

    #[test]
    fn splitter_returns_an_unterminated_last_line_on_finish() {
        let mut splitter = LineSplitter::default();
        assert!(splitter.push(b"partial").is_empty());
        assert_eq!(splitter.finish().as_deref(), Some("partial"));
    }

    #[test]
    fn summary_keeps_the_last_progress_across_other_lines() {
        let summary = Summary::default()
            .with_line(REAL_FORMAT)
            .with_line("some other line");
        assert_eq!(summary.lines, 2);
        assert_eq!(summary.last_progress.map(|p| p.done), Some(1280));
        assert_eq!(summary.last_line.as_deref(), Some("some other line"));
    }
}
