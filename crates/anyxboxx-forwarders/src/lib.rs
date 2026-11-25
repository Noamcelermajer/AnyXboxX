//! Build-script helper for the runtime DLLs ported from C++: exports PE forwarders
//! (`Name=dll.Name`, the C++ `#pragma comment(linker, "/export:Name=dll.Name")`) from a cdylib.
//!
//! rustc passes its own module-definition file to the linker, so forwarders given as linker
//! options are ignored. As TomBox does (`crates/compat/advapi32/build.rs`, GPL-3.0-or-later),
//! the table becomes a `.def` file, MSVC's `lib.exe` turns it into an export file (`.exp`), and
//! the build script links that file into the DLL.
//!
//! Once an export file is linked, it is the DLL's whole export table: the functions the crate
//! itself exports (`#[unsafe(no_mangle)]`) must be listed too (`own`).

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

const TARGET: &str = "x86_64-pc-windows-msvc";
const PRAGMA: &str = "#pragma comment(linker, \"/export:";

/// `(export, "dll.Function")` pairs from C++ `#pragma comment(linker, "/export:…")` lines.
pub fn parse_pragmas(text: &str) -> Result<Vec<(String, String)>, String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix(PRAGMA))
        .map(|rest| {
            let entry = rest
                .strip_suffix("\")")
                .ok_or_else(|| format!("unexpected line {rest}"))?;
            let (name, target) = entry
                .split_once('=')
                .ok_or_else(|| format!("no forwarder in {entry}"))?;
            Ok((name.to_string(), target.to_string()))
        })
        .collect()
}

/// The module-definition text of `library` exporting its `own` functions and `forwarders`.
pub fn def_text(library: &str, forwarders: &[(String, String)], own: &[&str]) -> String {
    let mut def = format!("LIBRARY {library}\nEXPORTS\n");
    for name in own {
        writeln!(def, "    {name}").expect("writing to a String");
    }
    for (name, target) in forwarders {
        writeln!(def, "    {name}={target}").expect("writing to a String");
    }
    def
}

/// Called from a build script: links `forwarders`, and the crate's `own` exports, into the cdylib
/// being built (MSVC only; other targets get no forwarders and the DLL is not usable there anyway).
pub fn link(library: &str, forwarders: &[(String, String)], own: &[&str]) {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let out = std::env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let out = Path::new(&out);
    let (def, lib, exp) = (
        out.join(format!("{library}_forwarders.def")),
        out.join(format!("{library}_forwarders.lib")),
        out.join(format!("{library}_forwarders.exp")),
    );
    std::fs::write(&def, def_text(library, forwarders, own)).expect("writing the .def file");
    let mut librarian: Command = cc::windows_registry::find(TARGET, "lib.exe")
        .expect("lib.exe (MSVC librarian) not found: needed for the forwarder export table");
    let status = librarian
        .arg("/NOLOGO")
        .arg("/MACHINE:X64")
        .arg(format!("/DEF:{}", def.display()))
        .arg(format!("/OUT:{}", lib.display()))
        .status()
        .expect("running lib.exe");
    assert!(
        status.success() && exp.exists(),
        "lib.exe did not produce {}",
        exp.display()
    );
    println!("cargo:rustc-cdylib-link-arg={}", exp.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pragmas_become_forwarders_and_a_def_file() {
        let text =
            "// header\n#pragma comment(linker, \"/export:CreateFileW=kernel32.CreateFileW\")\n";
        let table = parse_pragmas(text).unwrap();
        assert_eq!(
            table,
            [(
                "CreateFileW".to_string(),
                "kernel32.CreateFileW".to_string()
            )]
        );
        assert_eq!(
            def_text("k", &table, &["Own"]),
            "LIBRARY k\nEXPORTS\n    Own\n    CreateFileW=kernel32.CreateFileW\n"
        );
        assert!(parse_pragmas("#pragma comment(linker, \"/export:NoTarget\")").is_err());
    }
}
