//! The window's state and the actions behind its buttons. Long work (folder checks, VIctory's
//! translate and game) runs on threads; this module drains their results once per frame.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use crate::cache;
use crate::host::{self, Adapter, GpuVendor, Host};
use crate::job::{self, Event, Step};
use crate::launch;
use crate::processes::Tasklist;
use crate::profile::{Profile, Profiles};
use crate::progress::{Progress, Summary};
use crate::session::{self, Settings};
use crate::setup::{self, Item};
use crate::verify::{self, Report};

/// Lines kept in the log view.
const LOG_CAPACITY: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Setup,
    Files,
    Shaders,
    Play,
}

/// The game folder check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verify {
    NotRun,
    Running,
    Done(Report),
    Failed(String),
}

/// A job running on its thread, and whether it is the shader step.
pub struct Running {
    pub events: Receiver<Event>,
    pub shaders_only: bool,
}

pub struct App {
    pub repo: Option<PathBuf>,
    pub profiles: Profiles,
    pub host: Host,
    pub gpu: GpuVendor,
    pub adapter_name: Option<String>,
    adapters: Option<Vec<Adapter>>,
    pub setup: Vec<Item>,
    pub settings: Settings,
    pub tab: Tab,
    pub verify: Verify,
    verify_rx: Option<Receiver<Result<Report, String>>>,
    pub running: Option<Running>,
    pub log: VecDeque<String>,
    pub summary: Summary,
    pub notice: Option<String>,
    pub confirm_clean: bool,
    pub cache_dir: Option<PathBuf>,
}

/// The checklist as the machine is now; `adapters` is what DXGI listed (None when it failed).
fn read_setup(adapters: Option<&[crate::host::Adapter]>) -> Vec<Item> {
    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let program_files = std::env::var_os("ProgramFiles(x86)")
        .map_or_else(|| PathBuf::from("C:\\Program Files (x86)"), PathBuf::from);
    let kits = program_files.join("Windows Kits").join("10").join("bin");
    let facts = setup::probe(
        &PathBuf::from(system_root),
        &kits,
        adapters.map(<[_]>::to_vec),
    );
    setup::checklist(&facts)
}

impl App {
    /// The window's first state: the setup checklist, the GPU and the host are read now.
    pub fn new() -> Result<Self, String> {
        let profiles = Profiles::embedded().map_err(|e| e.to_string())?;
        let host = host::detect_host();
        let adapters = host::enumerate_adapters().ok();
        let gpu = adapters
            .as_deref()
            .map_or(GpuVendor::Unknown, host::vendor_of);
        let adapter_name = adapters
            .as_deref()
            .and_then(host::main_adapter)
            .map(|a| a.name.clone());
        Ok(App {
            repo: anyxboxx::run::find_repo(None),
            profiles,
            host,
            gpu,
            adapter_name,
            setup: read_setup(adapters.as_deref()),
            adapters,
            settings: Settings::default(),
            tab: Tab::Setup,
            verify: Verify::NotRun,
            verify_rx: None,
            running: None,
            log: VecDeque::new(),
            summary: Summary::default(),
            notice: None,
            confirm_clean: false,
            cache_dir: cache::current_cache_dir(),
        })
    }

    /// The profile for this host and GPU.
    pub fn profile(&self) -> &Profile {
        self.profiles.select(self.host, self.gpu)
    }

    /// Reads the checklist again (after the user installed something).
    pub fn refresh_setup(&mut self) {
        self.setup = read_setup(self.adapters.as_deref());
    }

    /// The repository folder, or an empty path when none was found (the plan is then only shown).
    pub fn repo_or_empty(&self) -> PathBuf {
        self.repo.clone().unwrap_or_default()
    }

    /// Where VIctory keeps saves and data: `VICTORY_DATA_DIR`, else `%LOCALAPPDATA%\victory\data`.
    pub fn data_dir_label(&self) -> String {
        let data = std::env::var_os("VICTORY_DATA_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .map(|l| PathBuf::from(l).join("victory").join("data"))
            });
        data.map_or("unknown".to_string(), |d| d.display().to_string())
    }

    pub fn is_busy(&self) -> bool {
        self.running.is_some() || matches!(self.verify, Verify::Running)
    }

    /// The last progress report of the translate, for the bar.
    pub fn progress(&self) -> Option<&Progress> {
        self.summary.last_progress.as_ref()
    }

    /// Starts the folder check on a thread.
    pub fn start_verify(&mut self) {
        let Some(dir) = self.settings.game_path() else {
            self.notice = Some("Type the game folder first.".to_string());
            return;
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = verify::verify_folder(&dir).map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        self.verify = Verify::Running;
        self.verify_rx = Some(rx);
    }

    /// Starts the play steps, or the shader preloading alone; refused while a game runs.
    pub fn start(&mut self, precompile_only: bool) {
        if self.is_busy() {
            return;
        }
        if let Err(error) = session::guard(&self.settings, &Tasklist) {
            self.notice = Some(error.to_string());
            return;
        }
        let profile = self.profile().clone();
        let Some(repo) = self.repo.clone() else {
            self.notice = Some(
                "AnyXboxX folder not found: run the launcher from the repository (target\\release).".to_string(),
            );
            return;
        };
        let plan = match session::make_plan(&self.settings, &repo, &profile) {
            Ok(plan) => plan,
            Err(error) => {
                self.notice = Some(error.to_string());
                return;
            }
        };
        let steps: Result<Vec<Step>, anyxboxx::run::RunError> = if precompile_only {
            launch::precompile_steps(&plan)
        } else {
            launch::play_steps(&plan, false)
        };
        match steps {
            Ok(steps) => self.spawn_steps(steps, precompile_only),
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    fn spawn_steps(&mut self, steps: Vec<Step>, shaders_only: bool) {
        let (tx, rx) = mpsc::channel();
        job::spawn(steps, tx);
        self.running = Some(Running {
            events: rx,
            shaders_only,
        });
        self.summary = Summary::default();
        self.notice = None;
    }

    /// Cleans the shader cache after the user confirmed.
    pub fn clean_cache(&mut self) {
        self.confirm_clean = false;
        let Some(dir) = self.cache_dir.clone() else {
            self.notice = Some("No cache folder: LOCALAPPDATA is not set.".to_string());
            return;
        };
        self.notice = Some(match cache::clean_cache(&dir) {
            Ok(cache::CleanOutcome::Removed) => format!("Shader cache removed: {}", dir.display()),
            Ok(cache::CleanOutcome::NothingToClean) => "No shader cache to remove.".to_string(),
            Err(error) => error.to_string(),
        });
    }

    /// Drains the results of the threads started earlier.
    pub fn poll(&mut self) {
        self.poll_verify();
        let Some(running) = &self.running else {
            return;
        };
        let shaders_only = running.shaders_only;
        let events: Vec<Event> = running.events.try_iter().collect();
        let mut outcome = None;
        for event in events {
            match &event {
                Event::Finished(code) => outcome = Some(finish_message(*code, shaders_only)),
                Event::Failed(error) => outcome = Some(format!("Could not run: {error}")),
                Event::Started(_) | Event::Line(_) => {}
            }
            self.record(event);
        }
        if let Some(message) = outcome {
            self.running = None;
            self.notice = Some(message);
        }
    }

    fn poll_verify(&mut self) {
        let Some(rx) = &self.verify_rx else {
            return;
        };
        if let Ok(result) = rx.try_recv() {
            self.verify = match result {
                Ok(report) => Verify::Done(report),
                Err(error) => Verify::Failed(error),
            };
            self.verify_rx = None;
        }
    }

    fn record(&mut self, event: Event) {
        let line = match event {
            Event::Started(label) => format!("== {label} =="),
            Event::Line(line) => {
                self.summary = self.summary.with_line(&line);
                line
            }
            Event::Finished(code) => format!("== finished, exit code {code} =="),
            Event::Failed(error) => format!("!! {error}"),
        };
        self.log.push_back(line);
        while self.log.len() > LOG_CAPACITY {
            self.log.pop_front();
        }
    }
}

fn finish_message(code: i32, shaders_only: bool) -> String {
    match (code, shaders_only) {
        (0, true) => "Shader preloading finished.".to_string(),
        (0, false) => "VIctory exited normally.".to_string(),
        (code, _) => format!("VIctory stopped with exit code {code}: see the log."),
    }
}
