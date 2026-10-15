# Self-test

Self-test checks this computer's input hooks. `mdp selftest` injects a key and nudges the cursor, confirms those injected events were not captured, and puts the cursor back.

## Sub-features

- `hooks-silent`: capture does not report the injected key or the injected move.
- `cursor-restored`: the cursor ends where it started.
- `unsupported-host`: on an OS that is not Windows or macOS, the command refuses instead of pretending the hooks ran.

## How to get to it (user POV)

- In a terminal on the computer being checked, run `mdp selftest`.
- The troubleshooting table in the README points at the same command.
- A healthy Windows or macOS run prints `selftest: Desktop … ok` and exits 0. The cursor may jump by a few points and return.

## Driving it with verify-mdp

Preconditions:

- `verify-mdp doctor` passed.
- `cargo build -p mdp` has produced the debug binary (a full `verify-mdp` run builds it before this step).
- On Windows or macOS, expect the cursor to move slightly and an F24 key to be injected. Do not run it while the person is typing.

- **Run it.** Run `.cursor/skills/verify-mdp/verify-mdp drive selftest`.
- **Windows or macOS.** Result `PASS`. Stdout contains `selftest: Desktop` and ` ok`, and the exit code is 0.
- **Any other OS.** Result `GAP`. The exit code is non-zero and stderr contains `native input hooks exist only on Windows and macOS`. Exit 0 on that host is a failure.

## Gotchas

- The command is per computer. A pass on the Mac says nothing about the Windows hooks, and the reverse. Both machines need their own run; that pair of runs is listed with the two-machine checks.
- On Linux a GAP is the honest result. Do not install a fake capture device to turn it into a pass.
- Self-test does not open a link and does not write `config.toml`. The drive uses a scratch home and fails if a config file appears.
- The cursor restore uses the OS cursor position on Windows. Other OSes do not have that getter yet; combined with the missing hooks, only Windows and macOS are target results.
