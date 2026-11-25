//! `anyxboxx rapport <exe | package folder> [DLL folders...]`: the compatibility report of
//! `scarlett.py --rapport`, in Rust.
//!
//! By default a function counts as emulated when VIctory binds it (the runtime that runs the
//! game): its HLE registry, or a host DLL its default policy lets through. `--emulated-list`
//! takes another runtime's table instead, one
//! `family!function` per line; the parity test passes the Python HLE registry this way.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyxboxx_report::{ReportError, analyse, resolve_target};
use clap::Args;
use victory_hle::{HleResolver, HostPolicy};
use victory_pe::ImportSymbol;

#[derive(Debug, Clone, Args)]
pub struct RapportArgs {
    /// Scarlett executable, or a package folder holding MicrosoftGame.Config.
    pub target: PathBuf,
    /// More folders of DLLs the title may load (e.g. Visual C++ redistributables).
    pub extra_dirs: Vec<PathBuf>,
    /// Table of emulated functions, one `family!function` per line (see `family`).
    #[arg(long)]
    pub emulated_list: Option<PathBuf>,
}

/// Groups the DLL names that expose the same functions, like `emu/hle/core.family`.
pub fn family(dll: &str) -> String {
    let lower = dll.to_lowercase();
    let name = lower.strip_suffix(".dll").unwrap_or(&lower);
    let kernel = ["kernel32", "kernelx", "kernelbase", "ntdll", "advapi32"];
    if name == "xgameruntime" {
        "gdk".into()
    } else if kernel.contains(&name)
        || name.starts_with("api-ms-win-core-")
        || name.starts_with("api-ms-win-eventing-")
    {
        "kernel".into()
    } else if name == "ucrtbase"
        || name == "msvcrt"
        || name.starts_with("vcruntime140")
        || name.starts_with("api-ms-win-crt-")
    {
        "crt".into()
    } else {
        name.to_string()
    }
}

fn table(path: &Path) -> Result<BTreeSet<String>, ReportError> {
    Ok(std::fs::read_to_string(path)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// What VIctory binds when it runs the game: its HLE registry, then the host DLLs its default
/// policy lets through (Windows API sets the console shares with the PC).
fn victory_resolves(dll: &str, function: &str) -> bool {
    let symbol = match function.strip_prefix('#').map(str::parse::<u16>) {
        Some(Ok(ordinal)) => ImportSymbol::Ordinal(ordinal),
        _ => ImportSymbol::Name {
            hint: 0,
            name: function.to_string(),
        },
    };
    HleResolver::new(HostPolicy::default())
        .classify(dll, &symbol)
        .is_some()
}

/// Prints the report; returns the text so tests can check it.
pub fn rapport(args: &RapportArgs) -> Result<String, ReportError> {
    let exe = resolve_target(&args.target)?;
    let extra: Vec<&Path> = args.extra_dirs.iter().map(PathBuf::as_path).collect();
    let report = match &args.emulated_list {
        Some(list) => {
            let known = table(list)?;
            analyse(&exe, &extra, |dll, function| {
                known.contains(&format!("{}!{function}", family(dll)))
            })?
        }
        None => analyse(&exe, &extra, victory_resolves)?,
    };
    Ok(report.text())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_group_the_dlls_like_the_python_registry() {
        assert_eq!(family("XGameRuntime.dll"), "gdk");
        assert_eq!(family("KERNEL32.dll"), "kernel");
        assert_eq!(family("api-ms-win-core-synch-l1-2-0.dll"), "kernel");
        assert_eq!(family("vcruntime140_1.dll"), "crt");
        assert_eq!(family("api-ms-win-crt-heap-l1-1-0.dll"), "crt");
        assert_eq!(family("xmem.dll"), "xmem");
        assert_eq!(family("d3d12_xs"), "d3d12_xs");
    }

    #[test]
    fn victory_resolves_its_hle_and_host_functions_only() {
        assert!(
            victory_hle::registry()
                .iter()
                .any(|m| m.serves("xgameruntime.dll"))
        );
        assert!(!victory_resolves(
            "xgameruntime.dll",
            "NoSuchFunctionAnywhere"
        ));
        assert!(!victory_resolves("unknown.dll", "Foo"));
    }
}
