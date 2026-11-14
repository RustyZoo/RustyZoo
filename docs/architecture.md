# Architecture

A map of where things live: one line per module, by layer.
Details are in each module's own doc comment (`//!` at the top of `src/<name>.rs` or `src/<name>/mod.rs`);
read those first.
This file changes only when a module is added, removed or changes its role.

One Cargo package.
The core library (`src/lib.rs`) has no platform code: it reads the game data, runs the simulation and draws into memory (an RGBA `Framebuffer`, in software).
The program (`src/main.rs`) holds the commands and the window;
SDL3 is used only there, built from source and linked statically.

## Game data and files

- `vfs`: read-only file system over the install folder's loose files and archives, searched as the game does.
- `config`: reader of the game's INI-like files; every ini value is read through it.
- `pe`: bounded reader for the resources of the game's DLLs.
- `strings`, `usertext`: the game's texts from the language libraries; the texts of user-created content.
- `install`: finds the game folder when none is given: the remembered one, the usual install places, the registry.
- `userdir`: the per-user folder: settings, saved games, the `zoo.ini` laid over the game's.
- `snapshot`: the numbered JPEG or BMP file the snapshot button writes.
- `zoo`: maps and saved games (`.zoo`): parse, write, upgrade.
- `entity`, `avail`: the catalogue of entity types and animations; which types can be bought.
- `award`, `lists`: award definitions and held awards; the scenario and freeform lists of the menu.

## Pictures, text and sound

- `sprite`, `bitmap`, `tga`, `ground`: sprite files and palettes; root bitmaps; terrain textures; one ground tile.
- `anim`: animation sets and the choice of a picture.
- `text`: embedded font with Arial's metrics; wrapping, drawing, number, money and date formats.
- `cursor`: the mouse cursors read from the resource DLLs.
- `audio`, `music`, `soundscape`: sound playback on SDL3; menu music; the ambient one-shots of a game.
- `movie`, `intro`, `startup`: AVI and Indeo 5 decoding; which intro movies play; the start-up picture.
- `zoom`, `camera`: the zoom levels; the camera and its scroll range.
- `minimap`: the minimap of the main bar: the tiles as a diamond in the view's rotation, the view frame and the click that centres the view.
- `render`: draws a whole map at the three zoom levels: staging, ordering, picking.

## Interface

- `ui`: the tree of interface elements read from the layout files: layout, widgets, drawing.
- `menu`: the main menu and its screens.
- `panel`, `status`: the information panels; the zoo status window tabs.
- `build`: the current tool and the construction and objects panels.
- `input`: keyboard and mouse of the map view, turned into commands.
- `app`: the game state of the map view and its pass, driven by commands: log, undo, saves, interface actions.
- `timing`: the three timers of the main loop and the calendar.
- `framelog`, `probe`: the run log with frame hashes; what a windowed run reports.

## World and editing

- `world`: the map as the tools change it: per-tile flags, who stands where, edit primitives, undo.
- `place`: the pending object, placing and removing objects, paths and fences.
- `terrain`, `exhibit`, `path`: ground shaping; the regions the exhibit fences make; the path finder.
- `rubble`, `flight`, `cheat`: rubble; flying ambient objects; name cheats.
- `money`, `month`: cash and accounts; month-end charges.

## Simulation

- `sim`: the step (managers in the original's order), the generator (`sim::Rng`), the state hash, `sim::drive::Run`.
- `scenario`, `goal`, `research`, `marketing`: scenario files and goals; the units' goals; the research tree; the marketing levels, charge and guest bonus.
- `unit`, `behaviour`: live state of animals, guests and staff with their per-step update; behaviour scripts run by a state machine per unit.
- `animal`, `guest`, `staff`: needs and life of an animal; guest arrival and state; hiring and wages.
- `building`, `show`: guest buildings; shows, tricks and performers.

## Program and tools

- `window` (SDL3 window and loop), `run` (headless scripted run), `boot` (start-up into the main menu).
- `cli`, `findcmd`, `simcmd`, `layout`, `textcmd`: the commands (see docs/usage.md); they print text or PNG files.
- `mcp`: a Model Context Protocol server over stdio that drives a `sim::drive::Run`; a development aid.
- `fixtures`: helpers shared by the unit tests.

## Data flow

1. Load: `vfs` finds the install folder; `config`, `strings`, `entity` and the other catalogues read it at start; `zoo` parses a map or save into a `World`.
2. Step: `App::pass(now, commands)` takes the clock and the commands (input, scripts, replay), applies them to the world, and runs `sim::Sim::step` on the
   simulation clock: managers, units, goals, behaviours, money.
3. Render: the same pass draws the map (`render`), then the interface (`ui`, `panel`, `menu`) into the framebuffer; the window presents it only when the pass painted a frame.

## Determinism

`rustyzoo sim <map> --ticks N` steps a map with no interface and prints one hash of the state (`sim::hash`);
CI compares it across Linux, Windows and macOS, and on ARM Linux and Windows.
All of it lives in `sim` and the modules it steps:

- Whole numbers of the original's widths; wrapping arithmetic where the original wraps.
- `f32` operations in the original's order; no `f64` where the original has a single, no fused multiply-add, no transcendental functions.
- No iteration over a `HashMap` or `HashSet`; anything walked is a `Vec` in a fixed order (`ci/check-hash-iteration.sh`).
- The generator is the only source of chance, never seeded from the clock or an address; the step never reads the wall clock, the environment, a file or the draw.

## Tests

Unit tests build synthetic bytes in code;
tests on real data are `#[ignore]` and read `ZT_DATA_DIR`.
`ci/gate.sh` is the local gate, `ci/check-trials.sh` runs the commands on the two official trials, `ci/check-playable.sh` plays the games listed in `ci/`.
See CONTRIBUTING.md.
