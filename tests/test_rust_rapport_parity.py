"""Parity of `anyxboxx rapport` (crates/anyxboxx-report) with `scarlett.py --rapport` (emu/report.py).

Same executables and package folders, built from source by pe_builder; the Rust report receives the
Python HLE registry as its table of emulated functions, so both must print the same text, or both
refuse the target.
"""
import os
import subprocess
from pathlib import Path

import pefile
import pytest

from emu import hle
from emu.launch import resolve
from emu.memory import THUNKS
from emu.report import analyse
from pe_builder import build_pe

REPO = Path(__file__).resolve().parents[1]
BINARY = REPO / "target" / "release" / ("anyxboxx.exe" if os.name == "nt" else "anyxboxx")
CONFIG = """<?xml version="1.0" encoding="utf-8"?>
<Game configVersion="1">
  <Identity Name="Test.Game" Publisher="CN=Test" Version="1.0.0.0"/>
  <TitleId>{title}</TitleId>
  <ShellVisuals DefaultDisplayName="Test game"/>
  <ExecutableList>
    <Executable Name="pc.exe" TargetDeviceFamily="PC"/>
    <Executable Name="{exe}" TargetDeviceFamily="Scarlett"/>
  </ExecutableList>
</Game>
"""
IMPORTS = [("kernel32.dll", "ExitProcess"), ("xgameruntime.dll", "XGameRuntimeInitialize"),
           ("unknown.dll", "Foo"), ("helper.dll", "Seven"), ("xmem.dll", 12), ("KERNEL32.dll", "Sleep"),
           ("vendor.dll", "Seven"), ("api-ms-win-core-synch-l1-2-0.dll", "WaitForSingleObjectEx")]


@pytest.fixture(scope="module")
def binary():
    if not BINARY.is_file():
        if os.environ.get("ANYXBOXX_REQUIRE_RUNTIME") == "1":
            pytest.fail(f"{BINARY} absent: cargo build --release (root workspace).")
        pytest.skip("anyxboxx not built (cargo build --release at the root)")
    return BINARY


@pytest.fixture(scope="module")
def registry(tmp_path_factory):
    """The Python HLE registry as `family!function` lines: what emu/hle.resolve finds."""
    path = tmp_path_factory.mktemp("registry") / "emulated.txt"
    keys = set(hle.REGISTRY) | set(hle.GUEST_CODE)
    path.write_text("\n".join(f"{group}!{name}" for group, name in sorted(keys)) + "\n", encoding="utf-8")
    return path


def python_report(target, extra):
    try:
        exe, _ = resolve(target)
        return analyse(exe, extra).text()
    except (ValueError, OSError, pefile.PEFormatError):
        return None


def rust_report(binary, registry, target, extra):
    command = [str(binary), "rapport", str(target), *map(str, extra), "--emulated-list", str(registry)]
    done = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", timeout=60)
    if done.returncode != 0:
        assert done.stderr.startswith("anyxboxx rapport: "), done.stderr
        return None
    return done.stdout.removesuffix("\n")


def ret(iat):
    return b"\xc3"


def exe_with(folder, imports=IMPORTS, **options):
    folder.mkdir(parents=True, exist_ok=True)
    return build_pe(folder / "game.exe", ret, imports=imports, dll=False, **options)


def package(folder, encoding="utf-8", exe="game.exe", title="0x1234ABCD"):
    exe_with(folder)
    (folder / "helper.dll").write_bytes(b"")
    text = CONFIG.format(exe=exe, title=title)
    if encoding == "utf-16":   # what the GDK tools write: BOM, UTF-16 LE, declaration saying so
        data = b"\xff\xfe" + text.replace('encoding="utf-8"', 'encoding="utf-16"').encode("utf-16-le")
    else:
        data = text.encode("utf-8")
    (folder / "MicrosoftGame.Config").write_bytes(data)
    return folder


def case_imports(tmp):
    exe = exe_with(tmp / "game")
    (tmp / "game" / "helper.dll").write_bytes(b"")
    return exe, []


def case_extra_dirs(tmp):
    exe, _ = case_imports(tmp)
    (tmp / "vendor").mkdir()
    (tmp / "vendor" / "VENDOR.DLL").write_bytes(b"")
    return exe, [tmp / "vendor"]


def case_many_missing(tmp):
    return exe_with(tmp / "game", imports=[("absent.dll", f"F{n:02}") for n in range(45)]), []


def case_no_imports(tmp):
    return exe_with(tmp / "game", imports=[]), []


def case_relocated_tls(tmp):
    # Unusable preferred base and a relocation table: moved to the default base; empty TLS directory.
    return exe_with(tmp / "game", base=THUNKS, pointers={0x80: 0x10}, tls=0x40), []


CASES = [
    ("imports", case_imports),
    ("extra-dirs", case_extra_dirs),
    ("many-missing", case_many_missing),
    ("no-imports", case_no_imports),
    ("relocated-tls", case_relocated_tls),
    ("package-utf8", lambda tmp: (package(tmp / "pkg"), [])),
    ("package-utf16", lambda tmp: (package(tmp / "pkg", encoding="utf-16"), [])),
]
REFUSED = [
    ("folder-without-manifest", lambda tmp: (exe_with(tmp / "pkg").parent, [])),
    ("manifest-with-dtd", lambda tmp: (
        (package(tmp / "pkg") / "MicrosoftGame.Config").write_text(
            '<!DOCTYPE x [<!ENTITY a "b">]>' + CONFIG.format(exe="game.exe", title="1"), encoding="utf-8")
        and tmp / "pkg", [])),
    ("entry-missing", lambda tmp: (package(tmp / "pkg", exe="absent.exe"), [])),
    ("entry-outside", lambda tmp: (package(tmp / "pkg", exe="..\\game.exe"), [])),
    ("not-a-pe", lambda tmp: ((tmp / "x.exe").write_bytes(b"MZ junk") and tmp / "x.exe", [])),
]


@pytest.mark.parametrize("make", [c[1] for c in CASES], ids=[c[0] for c in CASES])
def test_rust_report_prints_the_python_text(binary, registry, tmp_path, make):
    target, extra = make(tmp_path)
    python = python_report(target, extra)
    assert python is not None
    assert rust_report(binary, registry, target, extra) == python


@pytest.mark.parametrize("make", [c[1] for c in REFUSED], ids=[c[0] for c in REFUSED])
def test_rust_report_refuses_what_python_refuses(binary, registry, tmp_path, make):
    target, extra = make(tmp_path)
    assert python_report(target, extra) is None
    assert rust_report(binary, registry, target, extra) is None


def test_default_table_is_victorys_hle_registry(binary, tmp_path):
    exe, _ = case_imports(tmp_path)
    done = subprocess.run([str(binary), "rapport", str(exe)], capture_output=True, text=True, encoding="utf-8",
                          timeout=60)
    assert done.returncode == 0, done.stderr
    assert done.stdout.startswith("Executable: game.exe\nImported functions: 8 · emulated: ")
    assert "  - unknown.dll!Foo" in done.stdout          # bound by no runtime
    assert "  - kernel32.dll!ExitProcess" not in done.stdout  # a host DLL VIctory lets through
