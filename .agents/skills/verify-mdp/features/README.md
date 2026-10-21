# mdp verification map

This directory is the maintained source for verifying what a person can do with `mdp`. Read this index, then use the matching feature file as the recipe.

## Baseline preconditions

- Repo root is the cargo workspace (`crates/mdp-core`, `crates/mdp`).
- `verify-mdp doctor` has passed on this tree since the last surprising failure.
- Drives use the debug binary and `cargo test`. They do not launch `mdp`, `mdp ui`, `mdp run`, or `mdp pair`.
- Each drive gets its own evidence directory under `/tmp/verify-mdp-evidence/`. Concurrent runs must not share a scratch home.
- Never drive a config directory or TCP port 24800 that belongs to a person's already-running `mdp`.

## Driving conventions

- Start from the feature file's preconditions.
- Treat every command as literal.
- Run drives through `.agents/skills/verify-mdp/verify-mdp`.
- A full "done" proof is `verify-mdp` with no arguments, not a single feature.
- Cleanup removes scratch state and recorded pids only. Proof logs stay.

## Proof and skip reporting

- Capture the command and the resulting test line or CLI output, not only `RESULT PASS`.
- CLI proof includes stdout, stderr, the exit code, and an empty scratch config directory.
- UI proof on this harness is the egui test name and exit code. A live tray screenshot needs a desktop session; say so instead of substituting a different path.
- Record the feature id with the artifact (the evidence directory's step log).
- An unreachable path is reported with the command that was tried and the missing OS or second machine. Do not report it as verified through `FakePlatform`.

## Feature entry contract

Each feature file starts with an H1 and one paragraph of user-visible behavior, then exactly these H2 sections, in order: `Sub-features`, `How to get to it (user POV)`, `Driving it with verify-mdp`, `Gotchas`.

## Features

- [Crossing](./crossing.md) covers the cursor leaving one computer through the shared edge, the keyboard following, the way back, and control returning when the link drops.
- [Pairing](./pairing.md) covers the 6-digit code, confirming or rejecting it, and refusing a machine that was not paired.
- [Clipboard](./clipboard.md) covers UTF-8 text sync, the share toggle, and text that must not cross.
- [Arrangement](./arrangement.md) covers placing the other computer's screen and saving that placement.
- [Self-test](./selftest.md) covers `mdp selftest`, the local check that injected input is not captured again.
