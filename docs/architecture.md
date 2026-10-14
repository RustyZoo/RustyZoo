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

SDL3 (window, input, audio) is built from source and linked statically, so no system SDL is needed. `run` plays a map in the window with the loop of the original; the keys of the map view become commands in `src/input.rs`, and the loop reads the cursor for the edge scroll. `window-smoke` presents a test pattern.

### The window

`rustyzoo run <map>` (`src/window.rs`) plays the map in a window; `run --headless` (`src/run.rs`) runs the same `App::pass` on a scripted clock without any window, and prints the log lines that `run --log` also writes, so a log replayed headless gives the same frame hashes.

- **Loop.** As in the original: take every waiting event, run one `App::pass` (draw, interface, simulation timers against one clock reading), present a frame only when the pass painted one or the system asks for a redraw, then wait for an event or until the next timer is due. No spinning. While the window is hidden or minimised the loop blocks. That state is read from the window flags on every iteration, not tracked through the Minimized and Restored events, so it cannot stay asleep after a restore that comes as another event.
- **Sizes.** The frame is as large as the window in points (the size of the ini file, cut to the usable area of the display, or `--size`), in a streaming texture that SDL scales to the output with the nearest pixel. The window asks for a high pixel density, so on macOS and Wayland the output has the display's pixels. On Windows and X11 the window size is in physical pixels, also at a display scaling of 200%: the window and the picture are not enlarged. The run checks that the renderer's output equals the window in pixels, and prints the window size, its size in pixels, the output size, the display scale and the pixel density.
- **Measures.** `--timing MS` prints the gaps between the firings of each timer on the real clock (count, minimum, median, 95th percentile, maximum, the expected period, a histogram). `--shot FILE` reads back the presented pixels into a PNG. Both are information for a person or CI; no value fails a run.
- **CI.** Every system runs the window with the dummy video driver, the software renderer and the dummy audio driver, and checks sizes, painted frames and the replay of the log. One extra step per system opens a window on the runner's real video driver (X11 on a virtual display on Linux, Cocoa, Windows), uploads the `--shot` picture and the printed sizes and scale as an artifact, and prints the timing summary. Screenshots of game data are artifacts only and are never committed.

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
- CI builds on Linux, Windows and macOS, runs `window-smoke`, and runs the commands on the two official trials (`ci/check-trials.sh`), including a windowed run on the dummy video driver, a timing summary and a window screenshot on the real video driver (see The window).
- Nothing derived from the game is committed.
