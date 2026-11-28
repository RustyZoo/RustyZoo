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

  `ci/gate.sh [trials dir]` runs them all (plus the ignored full-game tests when `ZT_DATA_DIR` is set, again on a second install when `ZT_DATA_DIR_FR` is set, the freeform game of `ci/playable-full.txt` on each of the two installs, the first dung on `med_kids`, `ci/check-trials.sh` on the trials dir, and the games of `ci/playable.txt` and `ci/playable-tank.txt` on the two trials as CI plays them, and `ci/check-scenarios.sh` (every scenario and map plays a month) and `ci/check-ends.sh` (a scenario is won and one lost) on the trials and the two installs, and last, alone, the time of a step and of a headless pass on `med_kids` with about 160 guests, without a pin) and prints one line per step (the long play and trial steps run side by side once the release binary is built).

## Original game data

Never commit original game files, or anything derived from them: extracted sprites, texts, decompiled code and the like. RustyZoo must contain no original code or assets.

Tests that need real game data are marked `#[ignore]` and read the install path of the game from the `ZT_DATA_DIR` environment variable. Run them with `cargo test -- --ignored`. They expect either the English or the French install and pick their expected values by the language of `lang0.dll`. They also run on the other editions (German, Spanish, Italian, Dutch, Chinese, with or without expansions, with upper-case file names): a count, text or map pinned on the complete English or French install is checked there only (`fixtures::is_measured`), and a test that needs an expansion map or a damaged archive prints `skipped` and why.

## Clean room

- Write the engine only from descriptions of the original's behaviour, never from its code, a disassembly or a decompilation. Do not paste or paraphrase such code into the repository, a comment, a commit or a pull request.
- Why: RustyZoo contains no original code or assets (see Original game data). Code written from the original's code would carry it in.

## Pins

- `ci/check-trials.sh` pins what the files hold and what a script fixes (counts, sizes, ids, prices, a picture's hash). Never loosen an expected value to make a check pass.
- What the random stream decides (a birth step, which guest is inside a building, how many pieces a path walks) is a property that fails when the behaviour is lost (`any_seed`), never a step number; a new check does the same. Only the lines marked "Sentinel" (a few hashes and step numbers) are exact stream values, there to show that the stream is deterministic; each reads the run of its property. A change that shifts the stream moves the sentinels: say in the pull request why the stream changed, and give the new values with the cause shown (for example the check passing again with the new behaviour switched off). Do not search for a seed.
- The scripted games of `ci/playable*.txt` are not re-seeded either: their `#probe` line drops a seed whose hires came out as men in seconds, and their `#seeds` line is the safety net (`docs/playing.md`).
- Why: a pin that is edited until it passes hides the regression it exists to catch.

## Documentation

- `docs/architecture.md` is a short module map. Update it only when a module is added, removed or changes its role; never for a change inside a module (its own doc comment says what it does).
- A function over 150 lines fails clippy (`too_many_lines`); a non-test file over 900 lines fails `ci/check-file-size.sh` unless `ci/file-size-allow.txt` lists it, and a listed file may only shrink.

## Windows CI

- The trials and playable checks also run on Windows (on a pull request only once it has the label `full-ci`, added by the merge queue). Do not use process substitution (`<(...)`) in a check line: the native Windows program cannot open it. Write a temporary file instead (see `scripted` in `ci/check-trials.sh`).

## CI runners

- Every CI job runs on a GitHub-hosted runner except `full-game`, which runs on the one self-hosted runner (labels `self-hosted, linux, x64, gamedata, rustyzoo`) that holds the full English and French installs (`ZT_DATA_DIR`, `ZT_DATA_DIR_FR`). It runs, with the English install, then the French one (each with its own home, temp and user folders; the status lines stream to the log prefixed `en:` or `fr:`), the ignored tests (`ci/full-tests.sh`), the scenario sweep and the made-up win and loss (`ci/check-scenarios.sh`, `ci/check-ends.sh`) and the freeform game of `ci/playable-full.txt`. The job `scope` decides whether it runs (output `fullgame`): on every push to master, and on a pull request by the maintainer from this repository that touches `src/`, `Cargo.*` or `ci/` and nothing under `.github/`; never for a fork or a Dependabot branch, so their code never meets the game data. The `ci` check accepts it skipped. Never use `pull_request_target`.
- The game data never leaves the runner: no `upload-artifact` or `actions/cache` step may pick up a folder that holds it, and a step prints no game text (`full-game` prints only test names, totals and status lines, and masks the folders in the logs). The trials are the only game data on the hosted runners.
- The asset commands of `ci/check-trials.sh` run in parallel shards on the hosted runners (job `trials`): `SHARD=<i>/<n>` keeps every n-th check (`ci/shard.sh`, also read by `ci/check-scenarios.sh`), and the job `scope` sets the number of shards in one place. A check never relies on a file or state another check leaves: a shard runs only some of them.

## Update feed

- `release.yml` builds with the repository variable `RUSTYZOO_UPDATE_KEY` (the public key, 64 hex digits; not a secret). Unset, the build never updates. On a tag, the job `feed` (environment `release`) signs `feed.json` with the secret `RUSTYZOO_FEED_KEY` of that environment (the PEM private key), checks the signature against the variable and uploads `feed.json`, `feed.sig` and the update files to the release of the tag (a draft when none exists). Either missing: a warning, nothing signed, the packages are unaffected. Never print the key.
- The owner makes the pair on their own machine: `openssl genpkey -algorithm ed25519 -out feed-key.pem`, then `openssl pkey -in feed-key.pem -pubout -outform DER | tail -c 32 | od -An -vtx1 | tr -d ' \n'` prints the public key. Store the file as the secret of the `release` environment, which only tags `v*` can use (`gh secret set RUSTYZOO_FEED_KEY --env release < feed-key.pem`) and the digits as the variable (`gh variable set RUSTYZOO_UPDATE_KEY --body <digits>`), then keep the file off the repository.

## Dependencies

Add a dependency only when the code uses it, and justify it in the pull request. `cargo deny check` (config in `deny.toml`) checks licences, sources and duplicate versions; CI runs it on every pull request, and the RustSec advisories weekly.

## Code rules

- Write the smallest thing that works. No speculative abstraction, no placeholder modules or files for later, no configuration for values that never change.
- Every non-trivial piece of logic has one small runnable check, usually a unit test.
- Malformed game or mod files must produce errors, never panics. Untrusted input never panics.
