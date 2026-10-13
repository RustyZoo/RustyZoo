# RustyZoo

RustyZoo is an open-source reimplementation of the Zoo Tycoon (2001) game engine, in the spirit of [OpenRCT2](https://openrct2.io/): a standalone program, written in Rust, that loads the data of your own copy of the game. It targets Windows 11 (x64), macOS (arm64 and x86_64, as a universal binary) and Linux (x86_64).

## Status

Pre-alpha. Nothing is playable yet: the repository currently contains a small core library and a command that opens a window and an audio device to prove that the toolchain and CI work.

## You need the original game

RustyZoo only loads the data of an existing installation, so you need your own copy of Zoo Tycoon: Complete Collection. This project contains no original code or assets. It is not affiliated with or endorsed by Microsoft or Blue Fang Games.

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
```

Files are found the way the game finds them: in the `.ztd` archives of the directories listed in `[resource] path` of `zoo.ini`, the earlier directory winning, ignoring letter case, with `/` or `\` as separator.

## Roadmap

- [x] Foundations: repository, CI, a window and an audio device
- [ ] Assets: read the original game's archives and file formats
- [ ] Static world: draw terrain, objects and animals from the original data
- [ ] Window: game loop, camera and input
- [ ] UI: the in-game interface
- [ ] Building: place and remove terrain, paths, fences and buildings
- [ ] Life: animals, guests and staff, up to the first playable freeform game
- [ ] Full game: scenarios, campaigns and everything else the original has
- [ ] Packaging: ready-to-run releases for Windows, macOS and Linux

## Related projects

- [OpenZT](https://github.com/openzt1/openzt): a work-in-progress reimplementation of Zoo Tycoon (2001), written in Rust.
- [zt1-engine](https://github.com/sharkwouter/zt1-engine): an unfinished open-source Zoo Tycoon engine, written in C++.

## Licence

RustyZoo is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. See [LICENSE](LICENSE).
