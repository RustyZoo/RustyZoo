# Security policy

## Reporting a vulnerability

Report it privately: open the [Security tab](https://github.com/RustyZoo/RustyZoo/security) of the repository and choose "Report a vulnerability". Please do not open a public issue or pull request for it.

Say what you found, how to reproduce it (a small input file or command is best) and which version or commit you tested. Do not attach original game files.

## Scope

In scope:

- the engine, including code execution, memory unsafety or a write outside the intended folder (for example when opening a game archive, a mod or a saved game);
- the file parsers: a crafted file that does more than fail with an error;
- the CI configuration and scripts in this repository.

A panic, hang or wrong result on a malformed file with no wider effect is a bug: use the bug report form.

Not in scope: the original game itself, and cheating in a single-player game.

## What to expect

We aim to acknowledge a report within a week, keep you informed while it is fixed, and credit you in the fix if you want. There is no bounty.
