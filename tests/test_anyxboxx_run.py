"""`anyxboxx run` (crates/anyxboxx, Rust) replaces tools/lancer_victory.ps1: parity with the script.

Both launchers run against the same fake repository and game, with a fake cargo on the PATH and a
fake victory.exe already built. The fake tool logs every call and the VICTORY_* variables it sees:
the Rust launcher must make the same calls, with the same environment and the same exit code.
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[1]
BINARY = REPO / "target" / "release" / "anyxboxx.exe"
SCRIPT = REPO / "tools" / "lancer_victory.ps1"
CARGO_FAILURE, VICTORY_EXIT = 101, 7

# Fake console tool (.NET Framework, compiled by Windows PowerShell), copied as cargo.exe and victory.exe. Its exit
# code comes from FAKE_TOOL_EXIT_<NAME>_<FIRST ARGUMENT>, else FAKE_TOOL_EXIT_<NAME>, else 0.
FAKE_TOOL = r"""
using System; using System.IO; using System.Linq; using System.Collections;
public static class FakeTool {
    public static int Main(string[] args) {
        string name = Path.GetFileNameWithoutExtension(System.Reflection.Assembly.GetEntryAssembly().Location);
        var env = Environment.GetEnvironmentVariables().Cast<DictionaryEntry>()
            .Select(e => (string)e.Key).Where(k => k.StartsWith("VICTORY_")).OrderBy(k => k, StringComparer.Ordinal)
            .Select(k => k + "=" + Environment.GetEnvironmentVariable(k));
        File.AppendAllText(Environment.GetEnvironmentVariable("FAKE_TOOL_LOG"),
                           name + " " + string.Join(" ", args) + " | " + string.Join(";", env) + Environment.NewLine);
        string key = "FAKE_TOOL_EXIT_" + name.ToUpperInvariant();
        string code = Environment.GetEnvironmentVariable(args.Length > 0 ? key + "_" + args[0].ToUpperInvariant() : key)
                      ?? Environment.GetEnvironmentVariable(key);
        return string.IsNullOrEmpty(code) ? 0 : int.Parse(code);
    }
}
"""


@pytest.fixture(scope="module")
def fake_tool(tmp_path_factory):
    if sys.platform != "win32" or not shutil.which("powershell"):
        pytest.skip("the launchers are Windows programs")
    folder = tmp_path_factory.mktemp("fake-tool")
    source, exe = folder / "FakeTool.cs", folder / "FakeTool.exe"
    source.write_text(FAKE_TOOL, encoding="utf-8")
    subprocess.run(["powershell", "-NoProfile", "-Command",
                    f"Add-Type -Path '{source}' -OutputAssembly '{exe}' -OutputType ConsoleApplication"],
                   check=True, capture_output=True, timeout=120)
    return exe


@pytest.fixture(scope="module")
def binary():
    if not BINARY.is_file():
        if os.environ.get("ANYXBOXX_REQUIRE_RUNTIME") == "1":
            pytest.fail(f"{BINARY} absent: cargo build --release (root workspace).")
        pytest.skip("anyxboxx not built (cargo build --release at the root)")
    return BINARY


def _setup(root, fake_tool, *, game_exe=True, precompile_inputs=False):
    """Fake repository, game folder, target folder with victory.exe, fake cargo on a tools folder."""
    depot = root / "depot"
    (depot / "tools").mkdir(parents=True)
    shutil.copy(SCRIPT, depot / "tools" / "lancer_victory.ps1")
    manifest = depot / "third_party" / "victory" / "Cargo.toml"
    manifest.parent.mkdir(parents=True)
    manifest.write_text("", encoding="utf-8")
    game = root / "game"
    game.mkdir()
    if game_exe:
        (game / "game_scarlett_master.exe").write_bytes(b"")
    if precompile_inputs:
        for name in ("shaders.rpf", "XboxBvhBuilder.dll", "amd_fidelityfx_upscaler_xbox.dll"):
            (game / name).write_bytes(b"")
    (root / "target" / "diag").mkdir(parents=True)
    (root / "tools").mkdir()
    shutil.copy(fake_tool, root / "target" / "diag" / "victory.exe")
    shutil.copy(fake_tool, root / "tools" / "cargo.exe")
    xg = root / "xg_xs.dll"
    xg.write_bytes(b"")
    return depot, game, xg


def _launch(root, command, cargo_exit):
    log = root / "calls.log"
    env = {k: v for k, v in os.environ.items() if not k.startswith("VICTORY_")}
    env.update({"PATH": f"{root / 'tools'}{os.pathsep}{os.environ['PATH']}", "CARGO_TARGET_DIR": str(root / "target"),
                "FAKE_TOOL_LOG": str(log), "FAKE_TOOL_EXIT_CARGO": str(cargo_exit),
                "FAKE_TOOL_EXIT_VICTORY_RUN": str(VICTORY_EXIT)})
    done = subprocess.run(command, env=env, capture_output=True, text=True, timeout=120)
    calls = log.read_text(encoding="utf-8").splitlines() if log.is_file() else []
    # Paths differ only by the root folder of each launch.
    return done, [call.replace(str(root), "<root>") for call in calls]


def _both(tmp_path, fake_tool, binary, ps_options, rust_options, cargo_exit=0, **setup):
    results = []
    for name, rust in (("script", False), ("rust", True)):
        root = tmp_path / name
        depot, game, xg = _setup(root, fake_tool, **setup)
        options = [o.replace("{xg}", str(xg)) for o in (rust_options if rust else ps_options)]
        command = ([str(binary), "run", "--repo", str(depot), "--game-dir", str(game), *options] if rust else
                   ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                    str(depot / "tools" / "lancer_victory.ps1"), "-GameDir", str(game), *options])
        results.append(_launch(root, command, cargo_exit))
    return results


CASES = [
    ("defaults", [], []),
    ("switches", ["-NoTunedSettings", "-Dred", "-BatchCardRehash", "-SkipPipelines", "5"],
     ["--no-tuned-settings", "--dred", "--batch-card-rehash", "--skip-pipelines", "5"]),
    ("xg-compare", ["-XgDll", "{xg}", "-XgMode", "compare"], ["--xg-dll", "{xg}", "--xg-mode", "compare"]),
    ("xg-missing", ["-XgDll", "missing.dll"], ["--xg-dll", "missing.dll"]),
]


@pytest.mark.parametrize("ps_options, rust_options", [c[1:] for c in CASES], ids=[c[0] for c in CASES])
def test_rust_launcher_matches_the_script(tmp_path, fake_tool, binary, ps_options, rust_options):
    (script, script_calls), (rust, rust_calls) = _both(tmp_path, fake_tool, binary, ps_options, rust_options)
    assert script_calls and rust_calls == script_calls, (script.stderr, rust.stderr)
    assert rust.returncode == script.returncode


def test_precompile_translates_with_the_game_environment_first(tmp_path, fake_tool, binary):
    (script, script_calls), (rust, rust_calls) = _both(
        tmp_path, fake_tool, binary, ["-Precompile"], ["--precompile"], precompile_inputs=True)
    assert [c.split()[:2] for c in rust_calls] == [["cargo", "build"], ["victory", "translate"], ["victory", "run"]]
    assert rust_calls == script_calls and rust.returncode == script.returncode == VICTORY_EXIT


def test_precompile_needs_the_shader_archive(tmp_path, fake_tool, binary):
    (script, script_calls), (rust, rust_calls) = _both(tmp_path, fake_tool, binary, ["-Precompile"], ["--precompile"])
    assert [c.split()[0] for c in rust_calls] == ["cargo"] and rust_calls == script_calls
    assert rust.returncode == script.returncode == 1
    assert "shaders.rpf" in rust.stderr


def test_a_failed_build_stops_both_launchers(tmp_path, fake_tool, binary):
    (script, script_calls), (rust, rust_calls) = _both(tmp_path, fake_tool, binary, [], [], cargo_exit=CARGO_FAILURE)
    assert [c.split()[0] for c in rust_calls] == ["cargo"] and rust_calls == script_calls
    assert rust.returncode == script.returncode == 1
    assert f"cargo build failed (exit code {CARGO_FAILURE})" in rust.stderr


def test_a_missing_game_stops_before_building(tmp_path, fake_tool, binary):
    (script, script_calls), (rust, rust_calls) = _both(tmp_path, fake_tool, binary, [], [], game_exe=False)
    assert rust_calls == script_calls == []
    assert rust.returncode == script.returncode == 1
    assert "game_scarlett_master.exe not found" in rust.stderr


def test_the_default_launch_exits_with_victorys_code_and_the_tuned_settings(tmp_path, fake_tool, binary):
    (_, calls), (rust, rust_calls) = _both(tmp_path, fake_tool, binary, [], [])
    assert rust.returncode == VICTORY_EXIT
    build, game = rust_calls
    assert build.startswith("cargo build --profile diag -p victory --features diagnostics --manifest-path "
                            r"<root>\depot\third_party\victory\Cargo.toml | ")
    assert game.startswith(r"victory run <root>\game\game_scarlett_master.exe --execute | ")
    for setting in ("VICTORY_SHADER_LOOP_LIMIT=256", "VICTORY_DSTORAGE_NOTIFY=1", r"VICTORY_GAME_DIR=<root>\game"):
        assert setting in game.split(" | ")[1].split(";")
    assert "VICTORY_D3D12_DRED" not in game and "VICTORY_CARD_REHASH_RUNS" not in game
