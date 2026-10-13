# Contributing

## Language

Everything is written in English: code, comments, commit messages, pull requests, issues and documentation.

## Workflow

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

## Original game data

Never commit original game files, or anything derived from them: extracted sprites, texts, decompiled code and the like. RustyZoo must contain no original code or assets.

Tests that need real game data are marked `#[ignore]` and read the install path of the game from the `ZT_DATA_DIR` environment variable. Run them with `cargo test -- --ignored`.

## Dependencies

Add a dependency only when the code uses it, and justify it in the pull request.

## Code rules

- Write the smallest thing that works. No speculative abstraction, no placeholder modules or files for later, no configuration for values that never change.
- Every non-trivial piece of logic has one small runnable check, usually a unit test.
- Malformed game or mod files must produce errors, never panics. Untrusted input never panics.
