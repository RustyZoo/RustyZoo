# RustyZoo

RustyZoo is an open-source reimplementation of the Zoo Tycoon (2001) game engine, in the spirit of [OpenRCT2](https://openrct2.io/): a standalone program, written in Rust, that loads the data of your own copy of the game. It targets Windows 11 (x64), macOS (arm64 and x86_64, as a universal binary) and Linux (x86_64).

## Status

Pre-alpha. Nothing is playable yet: the repository currently contains a small core library and a command that opens a window and an audio device to prove that the toolchain and CI work.

## You need the original game

RustyZoo only loads the data of an existing installation, so you need your own copy of Zoo Tycoon: the full game (Complete Collection) or one of the free trials below. This project contains no original code or assets. It is not affiliated with or endorsed by Microsoft or Blue Fang Games.

### The free official trials

Microsoft still hosts two free trials, which you may install and use: [Zoo Tycoon](https://download.microsoft.com/download/zootycoon/Trial/2.0/W9X2KMeXP/EN-US/ZooTycoon.exe) and [Marine Mania](https://download.microsoft.com/download/zootycoon/Trial/1.0b/WIN98XP/EN-US/mardinotrial.exe). RustyZoo never redistributes them. To try RustyZoo without buying the game, run `ci/fetch-trials.sh <dir>`: it downloads both installers from Microsoft, checks their checksums and extracts them, without running anything, with [7-Zip](https://www.7-zip.org/) (`7z` or `7zz` must be on the PATH). It leaves two game folders, `<dir>/base-trial` and `<dir>/expansion-trial`, to pass to `--data`. CI does the same to run the commands on real data (`ci/check-trials.sh` lists them).

## Building

You need CMake and a C compiler (SDL3 is built from source and linked statically, so no system SDL is required) and the Rust toolchain pinned in `rust-toolchain.toml` (installed automatically by [rustup](https://rustup.rs/)).

```
cargo build --release
cargo run --release -- window-smoke
```

On Windows use the Visual Studio build tools (MSVC); on macOS the Xcode command line tools.

On Linux, SDL only builds the real window and sound backends (X11, Wayland, ALSA, PulseAudio, PipeWire, ...) if their development packages are installed. Without them the build still succeeds, but the binary can only use the headless `offscreen` and `dummy` drivers. On Ubuntu 22.04 or newer:

```
sudo apt-get install libasound2-dev libpulse-dev libaudio-dev libfribidi-dev \
  libjack-dev libsndio-dev libx11-dev libxext-dev libxrandr-dev libxcursor-dev \
  libxfixes-dev libxi-dev libxss-dev libxtst-dev libxkbcommon-dev libdrm-dev \
  libgbm-dev libgl1-mesa-dev libgles2-mesa-dev libegl1-mesa-dev libdbus-1-dev \
  libibus-1.0-dev libudev-dev libthai-dev libusb-1.0-0-dev libpipewire-0.3-dev \
  libwayland-dev libdecor-0-dev liburing-dev
```

Other distributions: see [SDL's Linux build dependencies](https://github.com/libsdl-org/SDL/blob/main/docs/README-linux.md).

## Usage

Point RustyZoo at the install folder of your game (the one that holds `zoo.ini`) with `--data`, or with the `ZT_DATA_DIR` environment variable:

```
rustyzoo --data "/path/to/Zoo Tycoon" ls 'animals/lion*'   # list files, sorted; * matches any text
ZT_DATA_DIR="/path/to/Zoo Tycoon" rustyzoo cat animals/lion.ai > lion.ai   # write one file to stdout
rustyzoo --data "/path/to/Zoo Tycoon" dump-strings lang0.dll   # string table of a resource DLL, as ID<TAB>text lines
rustyzoo --data "/path/to/Zoo Tycoon" dump-strings lang0.dll --text LION_GENERAL   # one long text, as stored (Windows-1252)
rustyzoo --data "/path/to/Zoo Tycoon" strings   # the texts as the game resolves them, with the library that supplies each
rustyzoo --data "/path/to/Zoo Tycoon" check-config   # read all configuration files (.ai, .cfg, .lyt, ...), report what is empty or ignored
rustyzoo --data "/path/to/Zoo Tycoon" types   # the entity types (class, type, subtype, name id, footprint, base animation), then counts per class
rustyzoo dump-sprite animals/lion/f/walk/E out/   # one PNG per frame of a sprite file, into out/
rustyzoo check-sprites                            # decode every sprite file; a defective palette is a warning; exit 1 if one fails
rustyzoo --data "/path/to/Zoo Tycoon" which animals/lion.ai   # which file or archive supplies it, and the copies it hides
rustyzoo map-info maps/default.zoo                # version, size, terrain and entities of a map or saved game
rustyzoo check-maps                               # read every .zoo file; exit 1 if one is corrupted
rustyzoo render-ani objects/aviary/idle/idle.ani out/   # frame 0 of an animation set, one PNG per screen direction, into out/
rustyzoo render-ani animals/lion/f out/ --anim walk     # the same for animals/lion/f/walk/walk.ani
rustyzoo check-anis                               # load every animation set and its sprite files; a defective palette is a warning; exit 1 if one fails
rustyzoo check-ground                              # render every cached ground picture again and compare; exit 1 if a covered pixel differs
rustyzoo render-ground ztats/000004 out/g.png     # render one ground picture (key as in the cache names)
rustyzoo render-map maps/default.zoo out.png --rotation 1   # draw a whole map into a PNG at normal zoom, and print what was drawn
rustyzoo render-map maps/default.zoo out.png --view 0 0 800 600 --no-units   # only the 800 x 600 window of the view whose top left pixel is the view-space point (0, 0), without animals, guests and staff
rustyzoo run maps/default.zoo --headless --passes 100 --clock 20 --script keys.txt --png-every 10   # play a map without a window on a scripted clock (a pass every 20 ms), print the commands and timer firings of each pass with the frame hash, and write the frame painted by every 10th pass as a PNG
rustyzoo run maps/default.zoo                     # play a map in a window (see "The window" below)
rustyzoo run maps/default.zoo --headless --replay log.txt   # run again the passes of a saved output, giving the same lines
rustyzoo run maps/default.zoo                      # play a map in a window: arrows, Home, End, Page Up, Page Down and the screen edges scroll, Ctrl+arrows rotate (left, right) and zoom (up, down), Pause or Space pauses, closing the window quits
rustyzoo check-render                             # draw every map in memory, print the summary per map and in total; exit 1 only if a map cannot be drawn
rustyzoo check-bitmaps                           # convert the root bitmaps of the terrain and compare them with the shipped copies; exit 1 on a mismatch
rustyzoo check-wavs                              # decode every .wav file and count them by format; exit 1 if one fails
rustyzoo check-music                             # queue the menu music paused on the audio device and print its format, gain and queued bytes
```

Files are found the way the game finds them: in the directories listed in `[resource] path` of `zoo.ini` (the install folder itself if there is no such key), the earlier directory winning, ignoring letter case, with `/` or `\` as separator. Inside a directory the loose files come first, then the archives (`.ztd` and `.zip`); among the archives of one directory the one whose lower-cased name sorts last wins, so `magnet02.ztd` beats `magnet01.ztd`, which beats `magnet.ztd`.

The names, descriptions and labels the game shows are not in the `.ztd` archives but in the resource DLLs of the install folder (`lang0.dll` and its siblings). `dump-strings` prints the string table of one DLL, newline, tab, backslash and other control characters escaped (`\n`, `\t`, `\\`, `\u{0000}`), followed by the name and size of its `TEXT` entries; `--text` writes the bytes of one of them.

The game loads every `lang*.dll` of the install folder, ordered by file name from the greatest to the smallest (`lang300.dll` first, `lang0.dll` last), and a string number or a `TEXT` name comes from the first library that has it non-empty. `strings` prints that merged view; the `[lib]` and `[language]` entries of `zoo.ini` are not used.

## The window

`rustyzoo run <map>` opens a window and plays the map in it with the loop of the original: it handles all waiting events, runs the three timers (draw, interface, simulation) on the real clock, and sleeps until the next one is due. Closing the window ends the run. Key and mouse input is not wired yet; `--script` feeds commands instead.

- The window is the size of `screenwidth` x `screenheight` in the ini file, cut to the usable area of the display; `--size WxH` overrides it and is not cut.
- `--passes N` ends the run after N passes, `--timing MS` after MS milliseconds. `--log FILE` writes the lines a headless run prints, and `run --headless --replay FILE` plays them again with the same frame hashes. `run --headless` needs no window at all: it takes a scripted clock (`--passes` with `--clock`) or a replay.
- While the window is minimised or hidden, the loop blocks (no drawing, no simulation, almost no CPU) and resumes when it is shown again.
- `--timing MS` prints, for each timer, the count, minimum, median, 95th percentile and maximum of the gaps between its firings in milliseconds, next to the period it should keep, and a histogram of the gaps. It is information: nothing fails on a value.
- `--shot FILE` reads back what the renderer presents and writes it as a PNG, in pixels of the output.

High-density displays: the game picture is as large as the window in points and SDL scales it, with the nearest pixel, to the pixels of the window, so a display with twice the pixels shows the same picture with larger pixels. The run prints the three sizes (window, window in pixels, renderer output; the last two must match) and the display scale. How the window size relates to the pixels depends on the system. On macOS and Wayland the window size is in points: at a scale of 2 an 800x600 window has 1600x1200 pixels. On Windows and X11 it is in physical pixels, even at a display scaling of 200%: the window is then not enlarged, and the picture is not either, so it is small on the screen. RustyZoo keeps that behaviour (the size the ini file asks for is the size in pixels there); scaling the window by the display scale is not done.

CI runs the window on every system with the dummy video driver (`SDL_VIDEO_DRIVER=dummy`, the software renderer and `SDL_AUDIO_DRIVER=dummy`): no window server or sound card is needed, and it checks the three sizes, the frames painted and the replay of the log. A second step opens a real window on the video driver of the runner (X11 on a virtual display on Linux, Cocoa, Windows), reads the pixels back with `--shot`, and uploads the picture with the printed sizes and display scale as the workflow artifact `window-<system>`. A further step prints the `--timing` summary of a ten second run in the log.

## Roadmap

The plan and its progress are in the [milestones](https://github.com/RustyZoo/RustyZoo/milestones); the work in progress is on the public [project board](https://github.com/orgs/RustyZoo/projects/1).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and the [architecture](docs/architecture.md).

## Related projects

- [OpenZT](https://github.com/openzt1/openzt): a work-in-progress reimplementation of Zoo Tycoon (2001), written in Rust.
- [zt1-engine](https://github.com/sharkwouter/zt1-engine): an unfinished open-source Zoo Tycoon engine, written in C++.

## Licence

RustyZoo is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. See [LICENSE](LICENSE).
