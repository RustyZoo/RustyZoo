# Architecture

How RustyZoo is built, and the choices contributors share. What is not written yet is marked **Planned**. The plan and its progress are in the [milestones](https://github.com/RustyZoo/RustyZoo/milestones).

## Layout

One Cargo package, no workspace until a second consumer exists.

- The core library (`src/lib.rs`) has no platform code. It reads the game data and draws into memory.
- The program (`src/main.rs`) holds the commands and the window. SDL3 is used only here.

Modules of the core:

| Module | Role |
| --- | --- |
| `vfs` | Read-only file system over the loose files and archives of the install folder |
| `config` | Reader of the game's INI-like files (`.ai`, `.cfg`, `.lyt`, ...) |
| `pe` | Bounded reader for the resources of the game's resource DLLs |
| `strings` | The game's texts, merged from the `lang*.dll` libraries |
| `sprite` | Sprite files and palettes: decoding, drawing, PNG output |

The core draws in software into an RGBA framebuffer (`Framebuffer`), as the original's software blitter does. The window only presents that framebuffer. Every result can therefore be checked without a screen: text output, PNG pictures, hashes.

## Platform

SDL3 (window, input, audio) is built from source and linked statically, so no system SDL is needed. Today the window only presents a test pattern (`window-smoke`). **Planned:** the game loop, camera and input.

## Game data

The game folder is the install folder, the one that holds `zoo.ini`. It is found with `--data <dir>`, then the `ZT_DATA_DIR` environment variable. **Planned:** a configuration file as a third source.

Files are looked up as the game does: in the directories listed in `[resource] path` of `zoo.ini` (the install folder itself if there is no such key), the earlier directory winning, ignoring letter case, with `/` or `\` as separator. Inside a directory the loose files come first, then the archives (`.ztd` and `.zip`); among the archives of one directory the one whose lower-cased name sorts last wins. Texts come from the resource DLLs, not from the archives. Nothing is ever written into the game folder.

Everything read from a file is untrusted: malformed data gives an error, never a panic.

## Commands

`ls`, `cat`, `dump-strings`, `strings`, `check-config`, `dump-sprite`, `check-sprites` and `window-smoke`; see the README. They read the data through the core and print text or PNG files, which is how the core is checked on real data.

## Design choices

Not implemented yet unless said otherwise.

- **Entities.** No class hierarchy: one `Kind` enum (animal, guest, staff, scenery, building, ...) and flat properties read as the original reads them. Code branches on `Kind` only.
- **Behaviour scripts.** The scripts of the data are parsed at load time into an enum of operations and run by a state machine per unit. An unknown operation is a load error that names it.
- **Simulation.** Like the original: a variable step (elapsed milliseconds times the speed factor, capped by `MaxSimTick`) and three independent timers (drawing, interface, simulation). A test mode with a fixed seed and fixed steps gives the same state hash on the three systems.
- **Text** (from M4). Liberation Sans, which has the metrics of Arial, rendered with `fontdue`. Today texts are only read and printed.
- **Beyond the original.** Improvements such as interface scaling come after parity.

## Dependencies

A dependency is added by the milestone that first uses it. Today: `sdl3`, `zip` and `png`.

## Tests

- Unit tests build synthetic bytes in code.
- Tests on real data are `#[ignore]` and read `ZT_DATA_DIR`; run them with `cargo test -- --ignored`.
- CI builds on Linux, Windows and macOS, runs `window-smoke`, and runs the commands on the two official trials (`ci/check-trials.sh`).
- Nothing derived from the game is committed.
