# État de la branche `rust/full`

Version anglaise : [../rust-full-status.md](../rust-full-status.md). Plan : [architecture-rust.md](architecture-rust.md).

## La tester en local (Windows)

```powershell
git fetch origin
git checkout rust/full
git submodule update --init
cargo build --release --workspace

# Ligne de commande, remplace tools\lancer_victory.ps1 (mêmes options, même environnement) :
.\target\release\anyxboxx.exe run --game-dir "C:\Users\ANAKIN\Downloads\GTA6.Xbox.v1.0.2.1-CYBERLEEK" --xg-dll "C:\Users\ANAKIN\Downloads\GTA6.Xbox.v1.0.2.1-CYBERLEEK\dll\xg_xs.dll" --precompile

# Lanceur graphique (double-clic sur target\release\anyxboxx-launcher.exe, ou) :
.\target\release\anyxboxx-launcher.exe
```

- `anyxboxx run` compile d'abord VIctory (`cargo build --profile diag -p victory --features diagnostics`), comme le script : le premier lancement prend plusieurs minutes. `--precompile` remplit le cache de shaders avant le jeu (environ 4,5 minutes la première fois, mesuré le 25/11/2026). Le premier lancement à froid peut planter pendant la compilation des pipelines ; relancer une fois le cache rempli.
- Les noms d'options PowerShell sont acceptés aussi (`--GameDir`, `--XgDll`, `--XgMode`, `--Precompile`, `--Dred`, `--NoTunedSettings`, `--SkipPipelines`, `--BatchCardRehash`). `anyxboxx run --help` les liste.
- Un seul jeu à la fois : fermer d'abord un jeu déjà lancé. Le lanceur graphique le vérifie lui-même.

### Lanceur graphique (`crates/anyxboxx-launcher`)

Il est fait avec egui via eframe 0.34 : du Rust pur, sans liaison C++ ni installeur. La fenêtre compte quatre pages avec des barres de progression, un journal et quelques boîtes de dialogue, qu'egui (mode immédiat) dessine en quelques centaines de lignes. Le Win32 brut aurait demandé des boucles de messages et une mise en page écrites à la main pour les mêmes pages. La logique vit dans des modules simples que la fenêtre ne fait qu'appeler : les 80 tests n'ouvrent jamais de fenêtre.

| Page | Ce qu'elle fait | Limites |
| --- | --- | --- |
| Installation | Vérifie la chaîne Rust, `d3d12.dll`, le runtime VC++ (présent, version non vérifiée), `dxc` du SDK Windows et le GPU via DXGI. Liens vers les pages officielles seulement ; rien n'est téléchargé. | La version du pilote affiche « inconnue » : le dépôt ne fixe encore aucun minimum. |
| Fichiers | Dossier du jeu (chemin saisi) : présence, fichiers vides, bornes des sections PE (exe ou DLL tronqué), bornes de la table d'en-tête RPF7, et un manifeste SHA-256 des fichiers requis et des `.rpf` de premier niveau, calculé en local. Rien n'est copié ni envoyé. | Il n'y a pas de hachés de référence : le manifeste est un relevé, pas une preuve. Une archive coupée après des tables intactes n'est pas détectée. La disposition RPF7 n'a pas été vérifiée sur un vrai fichier. |
| Shaders | `victory translate --cache --pipelines --used-pipelines … --jobs N` sur `shaders.rpf`, l'exe et ses deux DLL seulement, avec l'environnement du jeu (la clé du cache en dépend). La progression est lue dans les lignes « N of M containers (x%) » de VIctory. Refusé pendant que `victory.exe` ou le jeu tourne. | |
| Jouer | Profil choisi selon le système et le GPU (`profiles.toml`), surcharges dans le panneau avancé, `xg_xs.dll` facultative (real ou compare), compilation puis lancement par les mêmes étapes qu'`anyxboxx run`, emplacement des sauvegardes, nettoyage du cache de shaders (demande confirmation ; seul un dossier nommé `cache` peut être supprimé). | VIctory n'écrit les i/s que dans le titre de sa fenêtre : pas de ligne d'i/s à afficher. Le chemin des sauvegardes affiché est le dossier de données de VIctory, pas le sous-dossier du titre. |

Profils (`crates/anyxboxx-launcher/profiles.toml`, un test chacun) :

- **Windows + NVIDIA :** ton jeu de réglages (les six valeurs `TUNED` d'`anyxboxx run`, sans DRED, sans relais). Les 13 interrupteurs de la PR #199 sont à `0` et marqués **en attente de ta confirmation** : `VICTORY_SHADER_OPT`, `PS_HALVES`, `L6_HEAP_CLAMP`, `L6_NAN_GUARD`, `L6_NULL_LIBRARY`, `L6_PLACED_UAV`, `L6_PLANE_PASS`, `L6_PSV_INTERP`, `L9_DSTORAGE_GUARDS`, `L9_GUARD_TITLE_VIEWS`, `TEXTURE_SWEEP`, `TEXTURE_GATE_S`, `PIPELINE_GATE_S`. Les 13 existent dans le code de VIctory. `VICTORY_SHADER_OPT` fait partie de la clé du cache de shaders : le changer refait le cache.
- **Linux (Proton) + AMD :** les valeurs de l'essai de la PR #199 (RX 9070 XT, RADV, GE-Proton11-12) :
  - `SHADER_LOOP_LIMIT=256`, `LATE_WAIT=0`, `WIDE_BACK=0`, `PIPELINE_WAIT_MS=50`
  - `TEXTURE_BUDGET_MB=8192` (pour une carte de 16 Go), `TEXTURE_NO_WAIT=1`
  - `CARD_BUFFERS_MIN_MB=1`, `CARD_BUFFERS_MB=1024`
  - `VKD3D_CONFIG=descriptor_heap`
- **Windows + AMD, Intel, autres :** valeurs par défaut de VIctory, marquées « non mesuré ».

## Composant par composant

| Composant (avant) | Langage avant | Maintenant sur `rust/full` | Vérification de parité | Ancien code |
| --- | --- | --- | --- | --- |
| `tools/lancer_victory.ps1` | PowerShell | `crates/anyxboxx` (`anyxboxx run`) | `tests/test_anyxboxx_run.py` : les deux lanceurs tournent avec un faux cargo et un faux victory.exe et doivent faire les mêmes appels, avec le même environnement `VICTORY_*` et le même code de sortie (9 cas) | gardé comme oracle |
| `emu/loader.py` (chargeur PE) | Python | `crates/anyxboxx-pe` (tables lues par `victory-pe`) | `tests/test_rust_pe_parity.py` : 13 images fabriquées à partir de sources donnent le même JSON, protections des pages comprises | gardé comme oracle |
| `emu/report.py`, `emu/config.py` (`--rapport`) | Python | `crates/anyxboxx-report` + `anyxboxx rapport` | `tests/test_rust_rapport_parity.py` : avec le registre HLE Python, le même texte sur 7 cibles et les mêmes refus sur 5 | gardé comme oracle |
| `rust/` (textures, son, passerelle RDNA 2) | Rust | déplacé dans `crates/` (phase 1) ; un seul espace Cargo | `test_textures.py`, `test_audio.py`, `test_tombox_bridge.py` existants, inchangés | `rust/Cargo.toml` supprimé |
| `runtime/xcrdapi` (`XCRDAPI.dll`) | C++ | `crates/anyxboxx-xcrdapi` (cdylib) | `tests/test_rust_dlls.py` lance `test_xcrdapi.py` contre la DLL Rust, et sa table d'exports égale celle de la DLL C++ | C++ toujours compilé |
| `runtime/kernelx` (`kernelx.dll`) | C++ | `crates/anyxboxx-kernelx` (2 661 redirections) | `test_kernelx.py` contre la DLL Rust ; table d'exports identique | C++ toujours compilé |
| `runtime/d3d12_xs` (`d3d12_xs.dll`) | C++ | `crates/anyxboxx-d3d12-xs` | `test_d3d12_virtual_memory.py` et `test_runtime_cpp.py` contre la DLL Rust ; table d'exports identique | C++ toujours compilé |
| Lanceur graphique | (nouveau) | `crates/anyxboxx-launcher` | 80 tests Rust (profils, vérification des fichiers, lignes de progression, environnement, un seul jeu à la fois, chemin du cache) | |

**Encore en C++ :**

- `runtime/xgameruntime` (3 500 lignes) et `runtime/xmem` (800 lignes) : phase 3. Selon la décision D3, leur logique va dans des modules de `victory-hle`.
- `runtime/tls` (`anyxboxx_tls.dll`) a un vrai obstacle. La DLL prête son propre bloc TLS au jeu dès l'octet 0. Le bloc TLS d'une cdylib Rust contient aussi les variables de thread de la std Rust, que le jeu écraserait. Il faut une cdylib `no_std` ou une disposition dédiée.
- `runtime/launcher` (`anyxboxx.exe`) : à supprimer en phase 3 (décision D4).
- `runtime/gpu_runner` : phase 4.
- `runtime/third_party/libhttpclient-task` : part avec xgameruntime.

**Encore en Python :**

- `--banc`, `jour_j.py`, `--carte`, `--traduction` : phase 2, pas faits.
- `emu/modules.py`.
- Le bac à sable Unicorn et ses fonctions HLE : abandonnés dans l'état final (décision D2).
- RDNA 2 (`emu/rdna2*.py`, 10 900 lignes) : phase 4.
- `tools/*.py`, et la suite pytest, oracle de chaque portage ci-dessus.

**Rien n'est encore supprimé.** La règle : d'abord la parité, la suppression à une étape suivante. Pour essayer les DLL Rust dans le runtime natif, copier `target\release\{xcrdapi,kernelx,d3d12_xs}.dll` par-dessus `runtime\build\bin` (`XCRDAPI.dll` est le même fichier que `xcrdapi.dll`). Le jeu lui-même tourne par VIctory, qui n'utilise pas ces DLL.

## Vérifications sur cette branche (25/11/2026, Windows 11, Rust 1.98.1, Python 3.12)

```
python -m pytest                       → 1176 passed, 9 skipped
python -m ruff check --select W605,F,E9 PrototypeScarlett tests tools → All checks passed!
cargo test --release --workspace       → 121 passed, 0 failed
cargo clippy --release --workspace --all-targets → 0 warnings
cargo fmt --check                      → propre
```

Le runtime C++ a été recompilé depuis cette branche pour ces passages. Un `runtime/build` plus ancien, compilé avant le commit `def7a65`, échoue sur `test_first_exception_names_the_module_and_rva`.

## Constats en chemin

- À ce commit, VIctory ne compile pas sans sa fonctionnalité `diagnostics` (`victory_gpu::queue::presented` en dépend mais sert hors d'elle). L'espace racine dépend de `victory-hle` avec cette fonctionnalité, comme VIctory est toujours compilé.
- VIctory n'écrit les i/s que dans le titre de sa fenêtre. Une ligne d'i/s sur stdout dans VIctory permettrait aux deux lanceurs de l'afficher.
