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

  `ci/gate.sh [trials dir]` runs them all (plus the ignored full-game tests when `ZT_DATA_DIR` is set, again on a second install when `ZT_DATA_DIR_FR` is set, the freeform game of `ci/playable-full.txt` on each of the two installs, `ci/check-trials.sh` on the trials dir, and the games of `ci/playable.txt` and `ci/playable-tank.txt` on the two trials as CI plays them) and prints one line per step (the long play and trial steps run side by side once the release binary is built).

## Original game data

Never commit original game files, or anything derived from them: extracted sprites, texts, decompiled code and the like. RustyZoo must contain no original code or assets.

Tests that need real game data are marked `#[ignore]` and read the install path of the game from the `ZT_DATA_DIR` environment variable. Run them with `cargo test -- --ignored`. They expect either the English or the French install and pick their expected values by the language of `lang0.dll`.

## Clean room

- Write the engine only from descriptions of the original's behaviour, never from its code, a disassembly or a decompilation. Do not paste or paraphrase such code into the repository, a comment, a commit or a pull request.
- Why: RustyZoo contains no original code or assets (see Original game data). Code written from the original's code would carry it in.

## Pins

- `ci/check-trials.sh` and `ci/playable-full.txt` pin seeds, step counts and expected values. Never change a seed or loosen an expected value to make a check pass.
- A pin moves only with a verified cause shown in the pull request, for example the check passing again with the new behaviour switched off. Try a longer run first, then a seed search starting at 1, and list the seeds tried in the pull request.
- Why: a pin that is edited until it passes hides the regression it exists to catch.

## Documentation rows

- `README.md` and `docs/architecture.md` merge as a union (`.gitattributes`), so a rebase can succeed while leaving the old and the new copy of a line side by side, and GitHub's own merge then conflicts.
- After every rebase run `ci/check-doc-rows.sh` and keep one line per row. It fails on two table rows with the same first cell, two bullets with the same bold lead, and two command lines with the same command in a code block.

## Windows CI

- The trials and playable checks also run on Windows. Do not use process substitution (`<(...)`) in a check line: the native Windows program cannot open it. Write a temporary file instead (see `scripted` in `ci/check-trials.sh`).

## Dependencies

Add a dependency only when the code uses it, and justify it in the pull request.

## Code rules

- Write the smallest thing that works. No speculative abstraction, no placeholder modules or files for later, no configuration for values that never change.
- Every non-trivial piece of logic has one small runnable check, usually a unit test.
- Malformed game or mod files must produce errors, never panics. Untrusted input never panics.
