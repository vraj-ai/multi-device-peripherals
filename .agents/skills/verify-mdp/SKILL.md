---
name: verify-mdp
description: "Prove mdp, the Mac and Windows shared mouse and keyboard app, from its CLI, cargo tests, e2e.rs, mdp selftest, and the fake platform. Use after every mdp change, before calling work done, or when checking crossing, pairing, clipboard, arrangement, or selftest."
---

# Verify mdp

`mdp` is a desktop app plus a CLI. A person uses the tray and the Arrange, Pairing, and (on macOS) permissions windows. The same binary also accepts `run`, `ui`, `pair`, and `selftest`. Agents on one machine cannot move a real cursor between a Mac and a Windows PC. The harness below proves the platform-free path: `cargo test`, the loopback peers in `crates/mdp/tests/e2e.rs` (real Noise TCP, `FakePlatform` desktops), `mdp selftest` where the OS has input hooks, and the fake-platform unit tests. Read `features/README.md` before driving, then follow the feature file.

## Launch

There is no server to keep up. Build once, then start each drive as its own command.

```sh
cargo build -p mdp
```

Ready means the debug binary's `--version` prints `mdp ` plus the `version` in `crates/mdp/Cargo.toml` (today `mdp 0.1.0`). Cargo writes it to `$CARGO_TARGET_DIR/debug/mdp` when that variable is set, otherwise `target/debug/mdp`. `mdp.exe` is the Windows name. The harness asks `cargo metadata` for the target directory, so a build that succeeds is the binary later steps run.

Rust stable is pinned by `rust-toolchain.toml`, with rustfmt and clippy. On Windows and macOS that is enough. On Linux the product is not a Peer, but the suite still has to compile: install `pkg-config`, `libgtk-3-dev`, and `libxdo-dev` (and the libraries pkg-config then asks for, typically `libgdk-pixbuf-2.0-dev`, `libpango1.0-dev`, `libcairo2-dev`, `libatk1.0-dev`, `libxkbcommon-dev`). Missing GTK fails the build. A missing `libxdo` fails the link (`-lxdo`). Either failure is a failed run, not a two-machine gap.

Do not launch `mdp`, `mdp ui`, `mdp run`, or `mdp pair` for verification. Those commands load or create the real config (`directories` crate: `%APPDATA%\mdp\config\config.toml` on Windows, `~/Library/Application Support/mdp/config.toml` on macOS, `$XDG_CONFIG_HOME/mdp/config.toml` on Linux) and `run` / `pair` / the app bind `0.0.0.0:24800`. `mdp pair` also clears the pinned peer. Two copies must not share that file or port. The tests bind `127.0.0.1:0` and use temp config dirs; prefer them.

Teardown is `verify-mdp cleanup`. It does not delete the evidence directory.

## Doctor

Run this first, and again whenever a drive looks wrong:

```sh
.agents/skills/verify-mdp/verify-mdp doctor
```

It is read-only. Pass means all of the following:

- `cargo` and `rustc` are on `PATH`.
- `Cargo.toml` members are `crates/mdp-core` and `crates/mdp`.
- The host OS is printed (`uname -s`).
- TCP `24800` is reported free or in use. In use does not fail the doctor. It means a live peer is already here: do not start another one.
- If the debug binary already exists (`$CARGO_TARGET_DIR/debug/mdp` or `mdp.exe`, otherwise `target/debug/`), `--version` matches `crates/mdp/Cargo.toml`. A missing binary does not fail the doctor; build, then run doctor again before trusting CLI output.

Fail the doctor when the toolchain or workspace check fails, or when an existing binary's version disagrees with the manifest. Do not drive a tree that fails the doctor.

## Drive

The harness is the `verify-mdp` script plus the cargo tests it runs. Prefer the script over retyping filters. From the repo root:

```sh
.agents/skills/verify-mdp/verify-mdp drive crossing
.agents/skills/verify-mdp/verify-mdp drive pairing
.agents/skills/verify-mdp/verify-mdp drive clipboard
.agents/skills/verify-mdp/verify-mdp drive arrangement
.agents/skills/verify-mdp/verify-mdp drive selftest
```

A drive passes only when each named test's own cargo line ends in `... ok`. A filter that matches nothing, or a test left `#[ignore]`, fails the drive even when `cargo test` exits 0.

A full proof, which is what "done" requires, is:

```sh
.agents/skills/verify-mdp/verify-mdp
```

That runs doctor, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, the `e2e` integration test, the `fake_platform` unit tests, `cargo build -p mdp`, `mdp --help` and `mdp --version` inside a scratch home, and `mdp selftest`.

Stable handles are the subcommands `run`, `ui`, `pair`, `selftest`, the help text, and these test names:

- Crossing: `cursor_crosses_right_and_back_with_remote_only_injection`, `kill_link_while_remote_restores_focus_within_1s`, `physical_input_on_sink_claims_source`, `stuck_keys_released_on_crossing`, `escape_chord_restores_local_focus`, `disconnect_while_remote_warps_back_and_unsuppresses`
- Pairing: `pairing_code_is_six_digits_and_stable`, `loopback_pairing_exchanges_frames_both_ways`, `rejecting_pairing_code_closes_without_frames`, `pinned_link_refuses_impostor_and_hides_plaintext`, `shows_code_in_two_groups_and_confirms`, `mismatch_rejects`, `expiry_rejects`
- Clipboard: `clipboard_text_syncs_without_echo`, `connecting_does_not_push_what_was_already_copied`, `clipboard_toggle_off_sends_nothing`, `oversize_clipboard_is_not_sent`, `clipboard_at_cap_syncs_and_the_session_continues`
- Arrangement: `arrangement_top_crosses_with_mirrored_wire_frames`, `saved_arrangement_ends_both_sessions_with_the_senders_view`, `drag_right_then_save_emits_right`, `arrow_keys_revert_and_receive`, `mirror_flips_side_and_offset`
- Fake platform: `fake_platform_feeds_physical_and_records_injected`, `fake_platform_rejects_oversize_clipboard`

`FakePlatform` is the production test double for the OS boundary (`crates/mdp-core/src/platform/fake.rs`). The e2e peers still speak real Noise-encrypted TCP on loopback. Do not add a second mock in front of the link. UI tests drive the real egui widgets through `egui_kittest` (button text `Codes match — pair`, `They don't match`, `Save`). That is the window harness. It is not a screenshot of a live tray.

`--help` and `--version` must be run with `HOME`, `XDG_CONFIG_HOME`, and `APPDATA` pointed at an empty scratch directory. The script does this. Afterward that directory must contain no `config.toml`.

## Evidence

Every full run writes a directory:

```
/tmp/verify-mdp-evidence/<run-id>/
```

`latest` in that parent points at the newest run. The directory holds `transcript.txt` (the full combined output), one log per step, `summary.txt`, and `summary.json`. The transcript starts with the git sha, the OS, and the exact commands.

Proof standards:

- Drive the user-facing command or the test that performs that user action. Do not call a setter and call it a crossing.
- Record the command, the exit code, and the resulting state (test name `ok`, stdout, stderr). A final summary line without the step log is not proof.
- CLI proof includes the empty scratch config directory, because `--help` must not write a pairing key.
- `FakePlatform` is allowed only as the existing OS boundary. The link under test is real TCP. `mdp selftest` on Windows or macOS is the real hook path: it injects F24 (`0x73`) and a 12-point cursor nudge and asserts capture stays silent, then warps the cursor home.
- A name is not a skip. `selftest` on Linux exits non-zero on purpose. Observe that exit code and the stderr line. Do not treat the word "selftest" as a pass.
- The ignored loopback bench `mouse_move_loopback_latency_p95_under_10ms` is not part of the default run. It measures one host talking to itself. It does not measure LAN p95.

## Cleanup

```sh
.agents/skills/verify-mdp/verify-mdp cleanup
.agents/skills/verify-mdp/verify-mdp cleanup /tmp/verify-mdp-evidence/<run-id>
```

Cleanup reads `pids` in the run directory and kills only those pids. It then deletes the run's `scratch/` directory (the fake home). It leaves `transcript.txt`, the step logs, `summary.txt`, and `summary.json` in place. Never `killall` by the name `mdp` or `cargo`. Never delete `/tmp/verify-mdp-evidence`.

Run cleanup after a failed drive too, so a scratch home or a recorded pid does not linger. Then check that `transcript.txt` is still in the run directory.

## Helpers

The only helper is the executable `.agents/skills/verify-mdp/verify-mdp`. Invoke it from the repo root. `verify-mdp --help` lists the subcommands.

| Command | What it does |
|---|---|
| `verify-mdp` | Full proof. Exit 0 when every required step passes. A GAP does not fail the exit code. |
| `verify-mdp doctor` | Read-only health check. |
| `verify-mdp drive <feature>` | One feature: `crossing`, `pairing`, `clipboard`, `arrangement`, `selftest`. |
| `verify-mdp cleanup [run-dir]` | Kill recorded pids and remove scratch. Evidence stays. |

Step results in `summary.txt` are `PASS`, `FAIL`, or `GAP`. `summary.json` repeats them as `{"run_id","evidence","head","steps":[{"name","result","log"}],"gaps"}`.

## Pass and fail

A full run is a pass only when every required step is `PASS`. `GAP` is recorded and does not by itself fail the process. Any `FAIL` fails the process. Do not reinterpret a failure as a gap.

| Step | Required | Pass | Fail |
|---|---|---|---|
| `doctor` | yes | cargo and rustc present, workspace members match, and an already-built binary's version matches the manifest | toolchain missing, wrong workspace, or version mismatch |
| `fmt` | yes | `cargo fmt --all -- --check` exits 0 | non-zero exit |
| `clippy` | yes | `cargo clippy --workspace --all-targets -- -D warnings` exits 0 | non-zero exit |
| `test` | yes | `cargo test --workspace` exits 0 | any non-ignored test fails, or the command errors |
| `e2e` | yes | `cargo test -p mdp --test e2e` exits 0 and the log contains `cursor_crosses_right_and_back_with_remote_only_injection` with `ok` | the crossing test is missing or any e2e test fails |
| `fake-platform` | yes | `cargo test -p mdp-core fake_platform` exits 0 and both `fake_platform_feeds_physical_and_records_injected` and `fake_platform_rejects_oversize_clipboard` are `ok` | either test fails |
| `build` | yes | `cargo build -p mdp` exits 0 and the debug binary exists under cargo's target directory | build error or missing binary |
| `cli` | yes | `--help` exits 0 and lists `run`, `ui`, `pair`, and `selftest`; `--version` equals `mdp <manifest version>`; scratch home has no `config.toml` | any of those miss, or a config file appears |
| `selftest` | attempt always | On Windows or macOS: exit 0 and stdout contains `selftest: Desktop` and ` ok`. On any other OS: `GAP` when exit is non-zero and stderr contains `native input hooks exist only on Windows and macOS` | exit 0 on a non-peer OS, a target-OS failure, or any other stderr |

`cargo test --workspace` skips `#[ignore]` tests. `mouse_move_loopback_latency_p95_under_10ms` showing as ignored is not a failure. The script records that skip as a gap.

An ignored test that the user asked to run is a separate invocation: `cargo test -p mdp --test e2e mouse_move_loopback_latency_p95_under_10ms -- --ignored --nocapture`. Pass for that invocation is p95 under 10 ms on the loopback it prints. That number is not the LAN budget in `CONTEXT/architecture.md`.

## Two real machines

<!-- two-machines:start -->
These need a real Mac and a real Windows PC on the link. A passing `verify-mdp` run does not cover them. Do not mark them passed from `FakePlatform` or from one computer.

- The cursor crossing a real shared edge and appearing on the other machine's monitor, then coming back.
- The keyboard following Focus, with the sink's own layout (the Windows key acting as Command on the Mac).
- Touching the physical mouse or keyboard on the current sink and watching that machine become Source.
- The same 6-digit pairing code on both screens, confirmed by a person on both, then a third machine refused.
- mDNS discovery on a real LAN, including the Windows firewall prompt for TCP 24800.
- A manual `peer = "host:port"` (including a Tailscale MagicDNS name) between the two machines.
- Real clipboard text syncing both ways, and text copied before connect staying put.
- Saving an Arrangement on one machine and seeing the mirrored tiles on the other, then crossing the new edge.
- Ctrl+Alt+Shift+Esc on the Source returning the real cursor and releasing real held keys.
- Pulling the network while Focus is remote and seeing the local cursor warp back within 1 second.
- The macOS Accessibility and Input Monitoring screen clearing only after both grants, on a real Mac.
- Tray glyphs on both menu bars changing as Focus moves (dashed grey, amber on this machine, amber on the other).
- Start at login surviving a reboot (Windows Run key, macOS LaunchAgent).
- Local-LAN cursor-motion latency p95 under 10 ms. The ignored loopback bench is one host, not this measurement.
- `mdp selftest` on each machine: injected F24 and the cursor nudge never show up as captured input, and the cursor is back where it started.
- A stuck key on the real keyboard is released at the crossing.
- The Windows lock screen and UAC stay uncontrollable. That is the accepted boundary, checked by looking, not by a unit test.
<!-- two-machines:end -->

One real Windows or macOS machine, still short of two, is enough for `mdp selftest` and for opening the tray. The Windows `hook_decision` tests and the macOS tap tests compile only on those OSes; CI on `windows-latest` and `macos-latest` is where they run. This skill's Linux run records that as a gap when `uname` is not Darwin or Windows.

## Maintain

When the app's commands or windows change, run `/maintain-verification-skill` so this map stays honest. It should cover every file under `features/`.
