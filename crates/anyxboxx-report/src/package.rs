//! `MicrosoftGame.Config` of a GDK package (port of `emu/config.py`): the Scarlett executable a
//! package folder declares.

use std::path::{Path, PathBuf};

use crate::ReportError;

pub const MANIFEST: &str = "MicrosoftGame.Config";
const SCARLETT: &str = "scarlett";
const MAX_MANIFEST: u64 = 1024 * 1024;

/// One `<Executable>` of the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executable {
    pub name: String,
    pub device_family: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub folder: PathBuf,
    pub title_id: u32,
    pub display_name: String,
    pub executables: Vec<Executable>,
}

/// A package folder gives its Scarlett executable; any other path is taken as the executable.
pub fn resolve_target(target: &Path) -> Result<PathBuf, ReportError> {
    if target.is_dir() {
        read_package(target)?.scarlett_entry()
    } else {
        Ok(target.to_path_buf())
    }
}

impl Package {
    /// The Scarlett executable, checked to be a file inside the folder.
    pub fn scarlett_entry(&self) -> Result<PathBuf, ReportError> {
        let exe = self
            .executables
            .iter()
            .find(|e| e.device_family.eq_ignore_ascii_case(SCARLETT))
            .ok_or_else(|| ReportError::Package("no Scarlett program in the manifest".into()))?;
        // `absolute` resolves `..` on Windows (GetFullPathNameW), so a name escaping the folder
        // no longer starts with it.
        let path = std::path::absolute(self.folder.join(&exe.name))?;
        if !path.starts_with(&self.folder) {
            return Err(ReportError::Package(
                "the manifest names a file outside the folder".into(),
            ));
        }
        if !path.is_file() {
            let title = [&exe.display_name, &self.display_name]
                .into_iter()
                .find(|t| !t.is_empty())
                .map_or("Game", String::as_str);
            return Err(ReportError::Package(format!(
                "{title}: {} is missing from this folder",
                exe.name
            )));
        }
        Ok(path)
    }
}

pub fn read_package(folder: &Path) -> Result<Package, ReportError> {
    let folder = std::path::absolute(folder)?;
    let manifest = folder.join(MANIFEST);
    if !manifest.is_file() {
        return Err(ReportError::Package(format!(
            "the folder has no {MANIFEST}"
        )));
    }
    if manifest.metadata()?.len() > MAX_MANIFEST {
        return Err(ReportError::Package(
            "manifest refused: file too large".into(),
        ));
    }
    let text = decode(&std::fs::read(&manifest)?);
    // A GDK manifest never declares a DTD: refused whatever the encoding.
    if text.contains("<!DOCTYPE") || text.contains("<!ENTITY") {
        return Err(ReportError::Package(
            "manifest refused: XML entities declared".into(),
        ));
    }
    let document = roxmltree::Document::parse(text.trim_start_matches('\u{feff}'))
        .map_err(|e| ReportError::Package(format!("manifest: {e}")))?;
    let root = document.root_element();
    let child = |name: &str| {
        root.children()
            .find(|n| n.is_element() && n.tag_name().name() == name)
    };
    let executables = root
        .descendants()
        .filter(|n| n.tag_name().name() == "ExecutableList")
        .flat_map(|list| {
            list.children()
                .filter(|n| n.tag_name().name() == "Executable")
        })
        .map(|e| Executable {
            name: e.attribute("Name").unwrap_or_default().to_string(),
            device_family: e
                .attribute("TargetDeviceFamily")
                .unwrap_or_default()
                .to_string(),
            display_name: e
                .attribute("OverrideDisplayName")
                .unwrap_or_default()
                .to_string(),
        })
        .collect();
    Ok(Package {
        title_id: child("TitleId")
            .and_then(|n| n.text())
            .and_then(|t| {
                let digits = t.trim();
                let digits = digits
                    .strip_prefix("0x")
                    .or_else(|| digits.strip_prefix("0X"))
                    .unwrap_or(digits);
                u32::from_str_radix(digits, 16).ok()
            })
            .unwrap_or(0),
        display_name: child("ShellVisuals")
            .and_then(|n| n.attribute("DefaultDisplayName"))
            .unwrap_or_default()
            .to_string(),
        executables,
        folder,
    })
}

/// UTF-16 (with or without BOM, guessed from a zero byte) or UTF-8, like `config._decode`.
fn decode(data: &[u8]) -> String {
    let utf16 = |bytes: &[u8], big: bool| {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|p| {
                if big {
                    u16::from_be_bytes([p[0], p[1]])
                } else {
                    u16::from_le_bytes([p[0], p[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match data {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, false),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, true),
        [0, _, ..] => utf16(data, true),
        [_, 0, ..] => utf16(data, false),
        _ => String::from_utf8_lossy(data).into_owned(),
    }
}
