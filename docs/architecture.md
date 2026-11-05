# Architecture

A map of where things live. Each module has one entry below; the details are in the module's own doc comment (`//!` at the top of `src/<name>.rs` or `src/<name>/mod.rs`), so start reading there. The plan and its progress are in the [milestones](https://github.com/RustyZoo/RustyZoo/milestones).

## Layout

One Cargo package, no workspace until a second consumer exists.

- The core library (`src/lib.rs`) has no platform code. It reads the game data and draws into memory (`Framebuffer`, RGBA, in software, as the original's blitter does).
- The program (`src/main.rs`, with the commands in `src/cli/`) holds the commands and the window. SDL3 is used only there, built from source and linked statically.

## Modules of the core

Game data and files:

- `vfs`: read-only file system over the install folder's loose files and archives, searched as the game does.
- `config`: reader of the game's INI-like files (`.ai`, `.cfg`, `.lyt`, ...). Every ini value is read through it.
- `pe`: bounded reader for the resources of the game's DLLs.
- `strings`: the game's texts, merged from the `lang*.dll` libraries.
- `usertext`: the texts of user-created content (`.uca`, `.ucb`, `.ucs`).
- `userdir`: the per-user folder: settings, saved games, the `zoo.ini` laid over the game's.
- `zoo`: maps and saved games (`.zoo`): parse, write, upgrade; `zoo/parity.rs` checks the round trip.
- `entity`: the catalogue of entity types and their animations.
- `avail`: which entity types can be bought.
- `award`: award definitions and the awards a zoo holds.
- `lists`: the scenario and freeform lists behind the main menu screens.

Pictures, text and sound:

- `sprite`: sprite files and palettes: decode, draw, tint, PNG output.
- `bitmap`: root bitmaps (grid, highlight, cliffs) turned into sprites.
- `tga`: the uncompressed true-colour TGA of the terrain textures.
- `ground`: the picture of one ground tile.
- `anim`: animation sets (`.ani`) and the choice of a picture.
- `text`: embedded Liberation Sans with Arial's metrics; wrapping, drawing, number, money and date formats.
- `cursor`: the mouse cursors read from the resource DLLs.
- `audio`, `music`: sound playback on SDL3; the menu music's loudness and fade.
- `movie`, `intro`: the AVI and Indeo 5 decoder; which intro movies play.
- `startup`: the start-up picture, its progress bar, the product version.
- `zoom`, `camera`: the three zoom levels; the camera, its scroll range and the full-screen rules.
- `render`: drawing a whole map at the three zoom levels.

Interface:

- `ui`: the tree of interface elements read from the layout files: layout, resolution, widgets, drawing.
- `menu`: the main menu and its screens.
- `panel`: the information panels (exhibit, animal, guest, staff, tank, ...).
- `status`: the zoo status window's Awards and Finances tabs.
- `build`: the current tool and the panels of the construction and objects tools.
- `input`: keyboard and mouse of the map view, as commands.
- `app`: the game state of the map view and its pass, driven by commands; the log, undo, saves and the interface actions of a running game.
- `timing`: the three timers of the main loop and the calendar.
- `framelog`, `probe`: the log lines of a run with the frame hashes; what a windowed run reports.

The world and its editing:

- `world`: the map as the tools change it: the zoo flag per tile, the index of who stands where, edits, undo.
- `place`: the pending object, and placing and removing objects, paths and fences.
- `terrain`: shaping and painting the ground.
- `exhibit`: the regions the exhibit fences make.
- `path`: the path finder.
- `rubble`, `flight`: turning to rubble; the flying objects of the date hooks.
- `cheat`: the name cheats.
- `money`, `month`: cash and the accounts; the month-end charges.

The simulation:

- `sim`: the step: the managers in the original's order, the generator (`sim::Rng`), the state hash, `sim::drive::Run`.
- `scenario`, `goal`, `research`: scenario files and goals; the units' goals; the research tree.
- `unit`: the live state of animals, guests and staff and the update each runs per step.
- `behaviour`: the behaviour scripts parsed into operations and run by a state machine per unit.
- `animal`, `guest`, `staff`: needs and life of an animal; guest arrival and start state; the Hire panel and wages.
- `building`, `show`: guest buildings; shows, tricks and performers.

## Platform and commands

- `window` (`src/window/`): `rustyzoo run <map>` in an SDL3 window with the loop of the original: events, one `App::pass`, present a frame only when the pass painted one. `run` without a map starts up (`boot.rs`) and opens the main menu; `run --headless` (`run.rs`) drives the same pass on a scripted clock, so a logged run replays to the same frame hashes.
- `cli` (`src/cli/`), `findcmd`, `simcmd`, `layout`, `textcmd`: the commands (`ls`, `cat`, `sim`, `check-*`, `dump-*`, `text-width`, `window-smoke`, ...); see docs/usage.md. They print text or PNG files, which is how the core is checked on real data.
- `mcp` (`src/mcp/`): a Model Context Protocol server over stdio that drives a `sim::drive::Run`. A development aid; it changes no game behaviour.
- `fixtures`: helpers the unit tests share.

## Game data

The game folder is the install folder, the one that holds `zoo.ini`, found with `--data <dir>`, then `ZT_DATA_DIR`. Everything read from a file is untrusted: malformed data gives an error, never a panic. Nothing is ever written into the game folder, and nothing derived from the game is committed. Lookup rules are in `vfs`.

## Design choices

- **Entities.** No class hierarchy: one `Kind` enum and flat properties read as the original reads them.
- **Simulation.** Like the original: a variable step and three independent timers (drawing, interface, simulation). A test mode with a fixed seed and fixed steps gives the same state hash everywhere.
- **Text.** Liberation Sans with the metrics of Arial, rendered by `fontdue` without hinting, so the glyphs are the same on every system.
- **Beyond the original.** Improvements such as interface scaling come after parity.

## Determinism

`rustyzoo sim <map> --ticks N` steps a map without an interface and prints one hash of the state (`sim::hash::state`). CI compares it across Linux, Windows and macOS. The rules of the simulation:

- Whole numbers of the original's widths; wrapping arithmetic where the original wraps, never an overflow that panics in debug.
- `f32` for money and the other decimal values, operations in the original's order; no `f64` where the original has a single, no fused multiply-add.
- No transcendental float function (`sin`, `cos`, `exp`, `ln`, `powf`, ...): results differ between libraries.
- No iteration over a `HashMap` or `HashSet`; anything walked is a `Vec` in a fixed order (`ci/check-hash-iteration.sh`; a walk whose order does not matter carries `// order-free: <why>`).
- The generator is the only source of chance, never seeded from the clock or an address.
- Nothing in the step reads the wall clock, the environment, a file or the draw.

## Dependencies

A dependency is added by the milestone that first uses it: `sdl3`, `zip`, `png`, `flate2`, `fontdue`, and a trimmed copy of NihAV's Indeo 5 decoder in `vendor/nihav` (see `vendor/README.md`).

## Tests

- Unit tests build synthetic bytes in code. Tests on real data are `#[ignore]` and read `ZT_DATA_DIR`: `cargo test -- --ignored`.
- `ci/gate.sh` is the local gate; `ci/check-trials.sh` runs the commands on the two official trials; `ci/check-playable.sh` plays the games listed in `ci/` (`docs/playing.md` says how a person plays the same). See CONTRIBUTING.md.
