"""Parity of the runtime DLLs ported to Rust (crates/anyxboxx-*) with the C++ ones (runtime/).

For each ported DLL, a runtime folder is assembled from the C++ build with that one DLL replaced by
the Rust cdylib, and the pytest files that check the C++ DLL run unchanged against it
(ANYXBOXX_RUNTIME points at the mixed folder). The C++ DLL is deleted only once this passes.
"""
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from conftest import RUNTIME_DIR, runtime_built

REPO = Path(__file__).resolve().parents[1]
RUST_RELEASE = REPO / "target" / "release"
# DLL name in the runtime folder → (Rust cdylib in target/release, pytest files of the C++ DLL).
PORTED = {
    "XCRDAPI.dll": ("xcrdapi.dll", ["tests/test_xcrdapi.py"]),
}
pytestmark = pytest.mark.skipif(sys.platform != "win32", reason="Windows DLLs")


def mixed_runtime(tmp_path, name, rust_file):
    folder = tmp_path / "runtime"
    shutil.copytree(RUNTIME_DIR, folder)
    shutil.copyfile(RUST_RELEASE / rust_file, folder / name)
    return folder


@pytest.mark.parametrize("name", sorted(PORTED))
def test_cpp_tests_pass_against_the_rust_dll(tmp_path, name):
    rust_file, test_files = PORTED[name]
    missing = [p for p in (RUST_RELEASE / rust_file,) if not p.is_file()]
    if missing or not runtime_built():
        if os.environ.get("ANYXBOXX_REQUIRE_RUNTIME") == "1":
            pytest.fail(f"absent: {missing or RUNTIME_DIR} (cargo build --release; cmake runtime/)")
        pytest.skip("Rust DLL or C++ runtime not built")
    folder = mixed_runtime(tmp_path, name, rust_file)
    env = {**os.environ, "ANYXBOXX_RUNTIME": str(folder), "ANYXBOXX_REQUIRE_RUNTIME": "1"}
    done = subprocess.run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", *test_files],
                          cwd=REPO, env=env, capture_output=True, text=True, timeout=600)
    summary = done.stdout.strip().splitlines()[-1] if done.stdout.strip() else done.stderr
    assert done.returncode == 0, done.stdout[-4000:]
    passed = re.search(r"(\d+) passed", summary)
    assert passed and int(passed.group(1)) > 0 and "skipped" not in summary, summary
