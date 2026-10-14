# Vendored code

## nihav

A trimmed copy of the Indeo 5 video decoder of [NihAV](https://nihav.org), written by Kostya Shishkov, used to play the intro movies (`src/movie.rs`).

- Upstream: <https://git.nihav.org/?p=nihav.git> (the repository offers snapshots only, no `git clone`).
- Snapshot: commit `7552f869cc1eef89325284d1f98964c9d8ab3143` of `master`, committed 2026-10-12 12:56 UTC ("add support for GIF in MOV"); fetched on 2026-10-14.
- Licence: upstream's README says "licensed under GNU Affero Public License - see COPYING" and `COPYING` is the text of version 3; no version number or "only" is stated there, and the source files carry no header. Section 14 of that text lets the user choose any published version when the program names none, so the SPDX lines and the `Cargo.toml` fields say `AGPL-3.0-or-later`. The AGPL text is kept in `vendor/nihav/COPYING`, and every vendored `.rs` file starts with an SPDX line and a pointer here. The rest of RustyZoo is GPL-3.0-or-later and the root `Cargo.toml` says `GPL-3.0-or-later AND AGPL-3.0-or-later`, since `vendor/` ships in the package. The GPLv3 and the AGPLv3 (section 13) allow the two to be combined, and the combined program is covered by both.
- Crates (names and layout as upstream, for an easy comparison): `nihav-core` (frames, pixel formats, the bit reader and the codebooks), `nihav-codec-support` (the zigzag table) and `nihav-indeo` (the Indeo 5 decoder, with the code shared with Indeo 4).

What was removed from the snapshot: every other codec, the demuxers, muxers, encoders, the deflate decompressor, the scaler and sound converter, the DSP, the test and image-writing helpers, the byte reader and the unit tests (they need sample files that are not here). The three crates were then compiled as one program that only builds an Indeo 5 decoder and decodes a packet, and every function, method, constant, type and trait the compiler reported as never used was deleted, in rounds, together with the `Display` and `FromStr` impls (nothing prints or parses a format) and the audio side (audio buffers, sample and channel formats, buffer pools); about 1,400 lines of `nihav-core` and `nihav-codec-support` remain (from about 4,700 after the first cut). Cargo features were dropped. `Cargo.toml` files were cut to the used dependencies and marked `publish = false`.

What was changed in the code that stays:

- `nihav-indeo/src/codecs/indeo5.rs`: `recombine_plane` stopped one sample short of the edge of each band, which left the last two columns and rows of pictures that use four luma bands (such as the 800x600 intro movie of the trial) black. It now recombines a copy of the bands with the last sample repeated once and cuts the result; the picture then equals the output of FFmpeg's decoder on frames both decode.
- `nihav-indeo/src/codecs/indeo5.rs`: a key frame whose picture size differs from the size the decoder was opened with is invalid (checked right after the header is read, before any buffer is allocated); upstream took any size up to 8191x8191 from the stream.
- `nihav-indeo/src/codecs/mod.rs`, `lib.rs`: one entry point, `indeo5_decoder()`, instead of the decoder registry; the `validate!` check prints nothing in debug builds.
- `nihav-core/src/codecs/mod.rs`, `options.rs`: references to the removed byte reader and deflate options dropped.

To refresh: fetch a newer snapshot (`?p=nihav.git;a=snapshot;h=refs/heads/master;sf=tgz`), repeat the removals, re-apply the change above, update the commit and date here, and check every frame of the intro movies against the previous output.
