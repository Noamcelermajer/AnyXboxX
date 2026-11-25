# State of the `rust/full` branch

French version: [fr/rust-full-status.md](fr/rust-full-status.md). Plan: [architecture-rust.md](architecture-rust.md).

## Test it locally (Windows)

```powershell
git fetch origin
git checkout rust/full
git submodule update --init
cargo build --release --workspace

# Command line, replaces tools\lancer_victory.ps1 (same options, same environment):
.\target\release\anyxboxx.exe run --game-dir "C:\Users\ANAKIN\Downloads\GTA6.Xbox.v1.0.2.1-CYBERLEEK" --xg-dll "C:\Users\ANAKIN\Downloads\GTA6.Xbox.v1.0.2.1-CYBERLEEK\dll\xg_xs.dll" --precompile

# Graphical launcher (double-click target\release\anyxboxx-launcher.exe, or):
.\target\release\anyxboxx-launcher.exe
```

- `anyxboxx run` first builds VIctory (`cargo build --profile diag -p victory --features diagnostics`), like the script, so the first launch takes several minutes. `--precompile` fills the shader cache before the game (about 4.5 minutes the first time, measured on 2026-11-25). The first cold launch may crash while pipelines compile; launch again once the cache is full.
- The PowerShell option names are accepted too (`--GameDir`, `--XgDll`, `--XgMode`, `--Precompile`, `--Dred`, `--NoTunedSettings`, `--SkipPipelines`, `--BatchCardRehash`). `anyxboxx run --help` lists them.
- One game at a time: close any running game first. The graphical launcher checks this itself.

### Graphical launcher (`crates/anyxboxx-launcher`)

It is built with egui through eframe 0.34: pure Rust, no C++ binding, no installer. The window has four pages with progress bars, a log and a few dialogs, which immediate-mode egui draws in a few hundred lines. Raw Win32 would have meant hand-written message loops and layout for the same pages. The logic lives in plain modules that the window only calls, so the 80 tests never open a window.

| Page | What it does | Limits |
| --- | --- | --- |
| Setup | Checks the Rust toolchain, `d3d12.dll`, the VC++ runtime (present, version not checked), `dxc` from the Windows SDK, and the GPU through DXGI. Links to official pages only; nothing is downloaded. | Driver version shows "unknown": the repository records no minimum yet. |
| Files | Game folder (typed path): presence, empty files, PE section bounds (a truncated exe or DLL), RPF7 header table bounds, and a SHA-256 manifest of the required files and top-level `.rpf`, computed locally. Nothing is copied or uploaded. | There are no reference hashes, so the manifest is a record, not proof. An archive cut after intact tables is not detected. The RPF7 layout was not checked against a real file. |
| Shaders | `victory translate --cache --pipelines --used-pipelines … --jobs N` on `shaders.rpf`, the exe and its two DLLs only, with the game's environment (the cache key depends on it). Progress is read from VIctory's "N of M containers (x%)" lines. It refuses to run while `victory.exe` or the game is running. | |
| Play | Profile chosen by OS and GPU (`profiles.toml`), overrides in the advanced panel, optional `xg_xs.dll` (real or compare), build then launch through the same steps as `anyxboxx run`, saves location, clean shader cache (asks for confirmation; only a folder named `cache` can be deleted). | VIctory prints fps only in its window title, so there is no fps line to show. The saves path shown is VIctory's data folder, not the per-title subfolder. |

Profiles (`crates/anyxboxx-launcher/profiles.toml`, one test each):

- **Windows + NVIDIA:** your working set (the six `TUNED` values of `anyxboxx run`, no DRED, no relay). The 13 switches from PR #199 are set to `0` and marked **pending your confirmation**: `VICTORY_SHADER_OPT`, `PS_HALVES`, `L6_HEAP_CLAMP`, `L6_NAN_GUARD`, `L6_NULL_LIBRARY`, `L6_PLACED_UAV`, `L6_PLANE_PASS`, `L6_PSV_INTERP`, `L9_DSTORAGE_GUARDS`, `L9_GUARD_TITLE_VIEWS`, `TEXTURE_SWEEP`, `TEXTURE_GATE_S`, `PIPELINE_GATE_S`. All 13 exist in VIctory's source. `VICTORY_SHADER_OPT` is part of the shader cache key, so changing it refills the cache.
- **Linux (Proton) + AMD:** the values of PR #199's run (RX 9070 XT, RADV, GE-Proton11-12):
  - `SHADER_LOOP_LIMIT=256`, `LATE_WAIT=0`, `WIDE_BACK=0`, `PIPELINE_WAIT_MS=50`
  - `TEXTURE_BUDGET_MB=8192` (sized for a 16 GB card), `TEXTURE_NO_WAIT=1`
  - `CARD_BUFFERS_MIN_MB=1`, `CARD_BUFFERS_MB=1024`
  - `VKD3D_CONFIG=descriptor_heap`
- **Windows + AMD, Intel, others:** VIctory's defaults, marked "unmeasured".

## Component by component

| Component (before) | Language before | Now on `rust/full` | Parity check | Old code |
| --- | --- | --- | --- | --- |
| `tools/lancer_victory.ps1` | PowerShell | `crates/anyxboxx` (`anyxboxx run`) | `tests/test_anyxboxx_run.py`: both launchers run with a fake cargo and victory.exe and must make the same calls, with the same `VICTORY_*` environment and the same exit code (9 cases) | kept as oracle |
| `emu/loader.py` (PE loader) | Python | `crates/anyxboxx-pe` (tables parsed by `victory-pe`) | `tests/test_rust_pe_parity.py`: 13 images built from source give the same JSON, page protections included | kept as oracle |
| `emu/report.py`, `emu/config.py` (`--rapport`) | Python | `crates/anyxboxx-report` + `anyxboxx rapport` | `tests/test_rust_rapport_parity.py`: given the Python HLE registry, the same text on 7 targets and the same refusals on 5 | kept as oracle |
| `rust/` (texture, audio, RDNA 2 bridge) | Rust | moved into `crates/` (phase 1); one Cargo workspace | existing `test_textures.py`, `test_audio.py`, `test_tombox_bridge.py` unchanged | `rust/Cargo.toml` deleted |
| `runtime/xcrdapi` (`XCRDAPI.dll`) | C++ | `crates/anyxboxx-xcrdapi` (cdylib) | `tests/test_rust_dlls.py` runs `test_xcrdapi.py` against the Rust DLL, and its export table equals the C++ DLL's | C++ still built |
| `runtime/kernelx` (`kernelx.dll`) | C++ | `crates/anyxboxx-kernelx` (2,661 forwarders) | `test_kernelx.py` against the Rust DLL; identical export table | C++ still built |
| `runtime/d3d12_xs` (`d3d12_xs.dll`) | C++ | `crates/anyxboxx-d3d12-xs` | `test_d3d12_virtual_memory.py` and `test_runtime_cpp.py` against the Rust DLL; identical export table | C++ still built |
| Graphical launcher | (new) | `crates/anyxboxx-launcher` | 80 Rust tests (profiles, file checks, progress lines, environment, one-game guard, cache path) | |

**Still C++:**

- `runtime/xgameruntime` (3,500 lines) and `runtime/xmem` (800 lines): phase 3. Decision D3 says their logic goes into `victory-hle` modules.
- `runtime/tls` (`anyxboxx_tls.dll`) has a real blocker. The DLL lends its own TLS block to the game from offset 0. A Rust cdylib's TLS block also holds Rust std's thread-locals, which the game would overwrite. It needs a `no_std` cdylib or a dedicated layout.
- `runtime/launcher` (`anyxboxx.exe`): to be deleted in phase 3 (decision D4).
- `runtime/gpu_runner`: phase 4.
- `runtime/third_party/libhttpclient-task`: goes with xgameruntime.

**Still Python:**

- `--banc`, `jour_j.py`, `--carte`, `--traduction`: phase 2, not done.
- `emu/modules.py`.
- The Unicorn sandbox and its HLE handlers: dropped in the end state (decision D2).
- RDNA 2 (`emu/rdna2*.py`, 10,900 lines): phase 4.
- `tools/*.py`, and the pytest suite, which is the oracle of every port above.

**Nothing has been deleted yet.** The rule is parity first, deletion in a later step. To try the Rust DLLs in the native runtime, copy `target\release\{xcrdapi,kernelx,d3d12_xs}.dll` over `runtime\build\bin` (`XCRDAPI.dll` is the same file as `xcrdapi.dll`). The game itself runs through VIctory, which does not use these DLLs.

## Checks on this branch (2026-11-25, Windows 11, Rust 1.98.1, Python 3.12)

```
python -m pytest                       → 1217 passed, 9 skipped
python -m ruff check --select W605,F,E9 PrototypeScarlett tests tools → All checks passed!
cargo test --release --workspace       → 121 passed, 0 failed
cargo clippy --release --workspace --all-targets → 0 warnings
cargo fmt --check                      → clean
```

The C++ runtime was rebuilt from this branch for these runs. An older `runtime/build`, compiled before commit `def7a65`, fails `test_first_exception_names_the_module_and_rva`.

## Findings along the way

- VIctory does not compile without its `diagnostics` feature at this commit (`victory_gpu::queue::presented` is gated by it but used outside it). The root workspace depends on `victory-hle` with that feature, the way VIctory is always built.
- VIctory writes fps only to its window title. A stdout fps line in VIctory would let both launchers show it.
