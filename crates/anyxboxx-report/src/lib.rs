//! Compatibility report (`scarlett.py --rapport`, `emu/report.py`): every imported function is
//! emulated, provided by a DLL of the package, or missing (it would return `E_NOTIMPL`).
//!
//! Which functions count as emulated is a parameter: the parity test passes the Python HLE
//! registry, the `anyxboxx rapport` command passes the runtime it uses.

use std::collections::BTreeSet;
use std::path::Path;

use anyxboxx_pe::{LoadError, default_allocate, read_pe};

pub mod package;

pub use package::resolve_target;

/// Missing functions listed in the text before "… and N more".
pub const MAX_LISTED: usize = 40;
const SYSTEM_PREFIXES: [&str; 2] = ["api-ms-win-", "ext-ms-"];
const SYSTEM_DLLS: [&str; 10] = [
    "kernel32.dll",
    "kernelbase.dll",
    "kernelx.dll",
    "ntdll.dll",
    "ucrtbase.dll",
    "msvcrt.dll",
    "xgameruntime.dll",
    "user32.dll",
    "advapi32.dll",
    "ole32.dll",
];

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error(transparent)]
    Load(#[from] LoadError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Package(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub executable: String,
    pub total: usize,
    pub emulated: Vec<String>,
    pub missing: Vec<String>,
    pub provided: Vec<String>,
    pub has_tls: bool,
    pub relocated: bool,
}

impl Report {
    /// Share of imports resolved: emulated or provided by the package.
    pub fn coverage(&self) -> f64 {
        if self.total == 0 {
            return 100.0;
        }
        100.0 * (self.emulated.len() + self.provided.len()) as f64 / self.total as f64
    }

    pub fn text(&self) -> String {
        let yes_no = |b: bool| if b { "yes" } else { "no" };
        let mut lines = vec![
            format!("Executable: {}", self.executable),
            format!(
                "Imported functions: {} · emulated: {} · provided by the package: {} ({:.1} % resolved)",
                self.total,
                self.emulated.len(),
                self.provided.len(),
                self.coverage()
            ),
            format!(
                "TLS: {} · relocated: {}",
                yes_no(self.has_tls),
                yes_no(self.relocated)
            ),
        ];
        if !self.missing.is_empty() {
            lines.push("Missing functions (return E_NOTIMPL):".to_string());
            lines.extend(
                self.missing
                    .iter()
                    .take(MAX_LISTED)
                    .map(|name| format!("  - {name}")),
            );
            if self.missing.len() > MAX_LISTED {
                lines.push(format!("  … and {} more", self.missing.len() - MAX_LISTED));
            }
        }
        lines.join("\n")
    }
}

/// Normalised DLL name: lower case, `.dll` added when there is no extension.
pub fn dll_name(name: &str) -> String {
    let name = name.to_lowercase();
    if name.ends_with(".dll") || name.ends_with(".exe") {
        name
    } else {
        name + ".dll"
    }
}

pub fn is_system(name: &str) -> bool {
    let name = dll_name(name);
    SYSTEM_DLLS.contains(&name.as_str()) || SYSTEM_PREFIXES.iter().any(|p| name.starts_with(p))
}

fn package_files(folder: &Path) -> BTreeSet<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return BTreeSet::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect()
}

/// Classifies every import of `exe`; `extra_dirs` are more DLL folders (e.g. VC++ redistributables).
/// `emulated(dll, function)` says whether the runtime implements a function.
pub fn analyse(
    exe: &Path,
    extra_dirs: &[&Path],
    emulated: impl Fn(&str, &str) -> bool,
) -> Result<Report, ReportError> {
    let image = read_pe(&std::fs::read(exe)?, default_allocate)?;
    let parent = exe.parent().unwrap_or(Path::new("."));
    let files: BTreeSet<String> = std::iter::once(parent)
        .chain(extra_dirs.iter().copied())
        .flat_map(package_files)
        .collect();
    let unique: BTreeSet<String> = image
        .imports
        .iter()
        .map(|i| format!("{}!{}", i.dll, i.name))
        .collect();
    let mut names: Vec<String> = unique.into_iter().collect();
    names.sort_by_key(|name| name.to_lowercase());
    let (mut found, mut provided, mut missing) = (Vec::new(), Vec::new(), Vec::new());
    for name in &names {
        let (dll, function) = name.split_once('!').expect("dll!function");
        if !is_system(dll) && files.contains(&dll_name(dll)) {
            provided.push(name.clone());
        } else if emulated(dll, function) {
            found.push(name.clone());
        } else {
            missing.push(name.clone());
        }
    }
    Ok(Report {
        executable: exe
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        total: names.len(),
        emulated: found,
        missing,
        provided,
        has_tls: image.tls.is_some(),
        relocated: image.relocated,
    })
}
