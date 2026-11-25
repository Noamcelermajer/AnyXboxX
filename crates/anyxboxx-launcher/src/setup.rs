//! The setup checklist. Each item is `Ok` only when the launcher found evidence of it; a
//! detection it cannot make is `Unknown`, never `Ok`. Links point to official sources only, and
//! nothing is downloaded or installed by the launcher.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::host::Adapter;

pub const RUST_URL: &str = "https://rustup.rs";
pub const WINDOWS_SDK_URL: &str = "https://developer.microsoft.com/windows/downloads/windows-sdk/";
pub const VC_REDIST_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";
pub const NVIDIA_DRIVER_URL: &str = "https://www.nvidia.com/Download/index.aspx";
pub const AMD_DRIVER_URL: &str = "https://www.amd.com/en/support";
pub const INTEL_DRIVER_URL: &str =
    "https://www.intel.com/content/www/us/en/download-center/home.html";

/// The state of one item of the checklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Evidence found; the text says what was found.
    Ok(String),
    /// Evidence not found.
    Missing(String),
    /// The launcher cannot tell; the text says why.
    Unknown(String),
}

/// One checklist line: what it is, its state, and where to get it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub title: &'static str,
    pub state: State,
    pub needed_for: &'static str,
    pub link: Option<(&'static str, &'static str)>,
}

/// Facts the probes read from the machine; kept apart so the decisions are testable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// The output of `rustc --version`, when it ran.
    pub rustc: Option<String>,
    /// `dxc.exe` found in a Windows SDK bin folder.
    pub dxc: Option<PathBuf>,
    /// `d3d12.dll` in the system folder.
    pub d3d12_dll: bool,
    /// Both VC++ 2015-2022 runtime DLLs in the system folder.
    pub vc_runtime_dlls: bool,
    /// Adapters found (software ones included), when DXGI answered.
    pub adapters: Option<Vec<Adapter>>,
}

/// The whole checklist, in display order.
pub fn checklist(facts: &Facts) -> Vec<Item> {
    vec![
        Item {
            title: "Rust toolchain",
            state: rust_state(facts.rustc.as_deref()),
            needed_for: "only to build VIctory from source (cargo build of third_party/victory)",
            link: Some(("rustup.rs", RUST_URL)),
        },
        Item {
            title: "DirectX 12 runtime (d3d12.dll)",
            state: presence_state(
                facts.d3d12_dll,
                "d3d12.dll in the system folder",
                "d3d12.dll not found",
            ),
            needed_for: "the game renders through Direct3D 12",
            link: None,
        },
        Item {
            title: "VC++ redistributable (x64)",
            state: presence_state(
                facts.vc_runtime_dlls,
                "VCRUNTIME140.dll and MSVCP140.dll present (version not checked)",
                "VCRUNTIME140.dll or MSVCP140.dll not found",
            ),
            needed_for: "the launcher's and VIctory's runtime DLLs",
            link: Some(("Microsoft VC++ redistributable", VC_REDIST_URL)),
        },
        Item {
            title: "Windows SDK dxc.exe",
            state: match &facts.dxc {
                Some(path) => State::Ok(path.display().to_string()),
                None => State::Missing("no dxc.exe under Windows Kits\\10\\bin".to_string()),
            },
            needed_for: "only the GPU tests of the repository (shader compilation check)",
            link: Some(("Windows SDK", WINDOWS_SDK_URL)),
        },
        gpu_item(facts),
    ]
}

fn rust_state(rustc: Option<&str>) -> State {
    match rustc {
        Some(version) => State::Ok(version.to_string()),
        None => State::Missing("rustc not found on PATH".to_string()),
    }
}

fn presence_state(present: bool, found: &str, absent: &str) -> State {
    if present {
        State::Ok(found.to_string())
    } else {
        State::Missing(absent.to_string())
    }
}

/// The GPU driver: the launcher does not know a minimum version, so it reports the adapter and
/// `Unknown` for the driver instead of judging it.
fn gpu_item(facts: &Facts) -> Item {
    let (state, link) = match facts.adapters.as_deref() {
        None => (
            State::Unknown("DXGI did not answer: the driver cannot be read".to_string()),
            None,
        ),
        Some(adapters) => match crate::host::main_adapter(adapters) {
            None => (State::Missing("no hardware GPU found".to_string()), None),
            Some(adapter) => (
                State::Unknown(format!(
                    "{}: no minimum driver version is recorded for this launcher, check the vendor's page",
                    adapter.name
                )),
                vendor_driver_link(adapter.vendor_id),
            ),
        },
    };
    Item {
        title: "GPU driver",
        state,
        needed_for: "the game and VIctory need a current Direct3D 12 driver",
        link,
    }
}

fn vendor_driver_link(vendor_id: u32) -> Option<(&'static str, &'static str)> {
    match vendor_id {
        crate::host::VENDOR_NVIDIA => Some(("NVIDIA drivers", NVIDIA_DRIVER_URL)),
        crate::host::VENDOR_AMD => Some(("AMD drivers", AMD_DRIVER_URL)),
        crate::host::VENDOR_INTEL => Some(("Intel drivers", INTEL_DRIVER_URL)),
        _ => None,
    }
}

/// Reads the facts from this machine. `system_root` is `%SystemRoot%`, `kits_bin` the Windows
/// Kits bin folder (`%ProgramFiles(x86)%\Windows Kits\10\bin`).
pub fn probe(system_root: &Path, kits_bin: &Path, adapters: Option<Vec<Adapter>>) -> Facts {
    let system32 = system_root.join("System32");
    Facts {
        rustc: rustc_version(),
        dxc: find_dxc(kits_bin),
        d3d12_dll: system32.join("d3d12.dll").is_file(),
        vc_runtime_dlls: system32.join("VCRUNTIME140.dll").is_file()
            && system32.join("MSVCP140.dll").is_file(),
        adapters,
    }
}

/// The first line of `rustc --version`, e.g. `rustc 1.98.1 (...)`.
pub fn rustc_version() -> Option<String> {
    let output = Command::new("rustc").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_string())
}

/// `dxc.exe` in `<kits_bin>\<version>\x64`, the newest version folder that has one.
pub fn find_dxc(kits_bin: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(kits_bin)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("x64").join("dxc.exe"))
        .filter(|path| path.is_file())
        .collect();
    candidates.sort();
    candidates.pop()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Adapter;

    fn item<'a>(items: &'a [Item], title: &str) -> &'a Item {
        items.iter().find(|i| i.title == title).expect("item")
    }

    fn adapter(vendor_id: u32) -> Adapter {
        Adapter {
            name: "Test GPU".to_string(),
            vendor_id,
            dedicated_video_memory: 1 << 30,
            software: false,
        }
    }

    #[test]
    fn nothing_found_reports_missing_not_ok() {
        let items = checklist(&Facts::default());
        assert!(matches!(
            item(&items, "Rust toolchain").state,
            State::Missing(_)
        ));
        assert!(matches!(
            item(&items, "DirectX 12 runtime (d3d12.dll)").state,
            State::Missing(_)
        ));
        assert!(matches!(
            item(&items, "GPU driver").state,
            State::Unknown(_)
        ));
    }

    #[test]
    fn found_evidence_reports_ok() {
        let facts = Facts {
            rustc: Some("rustc 1.98.1".to_string()),
            d3d12_dll: true,
            vc_runtime_dlls: true,
            dxc: Some(PathBuf::from("C:/kits/10.0.1/x64/dxc.exe")),
            adapters: None,
        };
        let items = checklist(&facts);
        assert!(
            matches!(item(&items, "Rust toolchain").state, State::Ok(ref v) if v == "rustc 1.98.1")
        );
        assert!(matches!(
            item(&items, "VC++ redistributable (x64)").state,
            State::Ok(_)
        ));
        assert!(matches!(
            item(&items, "Windows SDK dxc.exe").state,
            State::Ok(_)
        ));
    }

    #[test]
    fn gpu_driver_is_never_ok_and_links_the_vendor_page() {
        let facts = Facts {
            adapters: Some(vec![adapter(crate::host::VENDOR_NVIDIA)]),
            ..Facts::default()
        };
        let items = checklist(&facts);
        let gpu = item(&items, "GPU driver");
        assert!(matches!(gpu.state, State::Unknown(_)));
        assert_eq!(gpu.link.map(|l| l.1), Some(NVIDIA_DRIVER_URL));
    }

    #[test]
    fn no_hardware_gpu_is_missing() {
        let facts = Facts {
            adapters: Some(vec![Adapter {
                software: true,
                ..adapter(0x1414)
            }]),
            ..Facts::default()
        };
        assert!(matches!(
            item(&checklist(&facts), "GPU driver").state,
            State::Missing(_)
        ));
    }

    #[test]
    fn probe_finds_system_dlls_in_a_temp_system_root() {
        let root = tempfile::tempdir().expect("tempdir");
        let system32 = root.path().join("System32");
        std::fs::create_dir_all(&system32).expect("mkdir");
        std::fs::write(system32.join("d3d12.dll"), b"x").expect("write");
        std::fs::write(system32.join("VCRUNTIME140.dll"), b"x").expect("write");
        let kits = tempfile::tempdir().expect("tempdir");
        let facts = probe(root.path(), kits.path(), None);
        assert!(facts.d3d12_dll);
        assert!(!facts.vc_runtime_dlls, "both DLLs are needed");
        assert!(facts.dxc.is_none());
    }

    #[test]
    fn find_dxc_takes_the_newest_version_that_has_one() {
        let kits = tempfile::tempdir().expect("tempdir");
        for version in ["10.0.19041.0", "10.0.26100.0"] {
            let x64 = kits.path().join(version).join("x64");
            std::fs::create_dir_all(&x64).expect("mkdir");
            std::fs::write(x64.join("dxc.exe"), b"x").expect("write");
        }
        std::fs::create_dir_all(kits.path().join("10.0.99999.0").join("x64")).expect("mkdir");
        let found = find_dxc(kits.path()).expect("dxc");
        assert!(found.to_string_lossy().contains("10.0.26100.0"));
    }
}
