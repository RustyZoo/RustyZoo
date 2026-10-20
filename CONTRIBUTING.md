# Contributing

## Language

Everything is written in English: code, comments, commit messages, pull requests, issues and documentation.

## Workflow

- Every change starts from an issue. If there is none, open one first. Comment on an issue to take it. Questions and decisions are discussed on the issue.
- The pull request says `Closes #n` and carries the milestone of the issue.
- Work on a branch named `m<N>/<slug>`, where `<N>` is the milestone number and `<slug>` a short description, for example `m0/engine-skeleton`.
- Commit subjects are in the imperative mood and at most 72 characters: `Add framebuffer test pattern`.
- One pull request per change. Pull requests are squash-merged.
- The pull request body lists the acceptance commands together with their output.
- Before opening a pull request, these must pass:

  ```
  cargo fmt --check
  cargo clippy --all-targets -- -D warnings
  cargo test
  ```

  `ci/gate.sh [trials dir]` runs them all (plus the ignored full-game tests when `ZT_DATA_DIR` is set, again on a second install when `ZT_DATA_DIR_FR` is set, the freeform game of `ci/playable-full.txt` on each of the two installs, and `ci/check-trials.sh` on the trials dir) and prints one line per step.

## Original game data

Never commit original game files, or anything derived from them: extracted sprites, texts, decompiled code and the like. RustyZoo must contain no original code or assets.

Tests that need real game data are marked `#[ignore]` and read the install path of the game from the `ZT_DATA_DIR` environment variable. Run them with `cargo test -- --ignored`. They expect either the English or the French install and pick their expected values by the language of `lang0.dll`.

## Dependencies

Add a dependency only when the code uses it, and justify it in the pull request.

## Code rules

- Write the smallest thing that works. No speculative abstraction, no placeholder modules or files for later, no configuration for values that never change.
- Every non-trivial piece of logic has one small runnable check, usually a unit test.
- Malformed game or mod files must produce errors, never panics. Untrusted input never panics.
