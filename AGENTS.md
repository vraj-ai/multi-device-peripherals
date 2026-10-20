# Agent instructions

`mdp` shares one mouse and one keyboard between a Mac and a Windows PC.

## Verify command

`.agents/skills/verify-mdp/verify-mdp`

Run it from the repo root and read the transcript before a change counts as done. It runs the rules test, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, the e2e peers, the fake platform, `cargo build -p mdp`, the CLI in a scratch home, and `mdp selftest`. A new clippy warning fails the run. A `GAP` (selftest off Windows and macOS, the ignored loopback bench, two real machines) does not.

## Boundaries

- Two machines: a Mac and a Windows PC. Linux is not a Peer.
- One cargo workspace (`crates/mdp-core`, `crates/mdp`) and one binary, `mdp`. OS code stays in `crates/mdp/src/platform`. `mdp-core` is platform-free.
- One peer loop: `run::serve` in `crates/mdp/src/run.rs`.
- One byte cap: `crates/mdp-core/src/limits.rs` (`MAX_FRAME_BYTES`, `CLIPBOARD_MAX_BYTES`).
- One skill home: `.agents/skills/verify-mdp`. `.cursor/skills/verify-mdp` and `.claude/skills/verify-mdp` are symlinks to it.
- Self-merge is off (fleet ADR 0003). Open the pull request against `main` and leave it.
- Do not recreate `CONTEXT/progress.md`.

## Enforced by

| Rule | Check |
|---|---|
| No `CONTEXT/progress.md` | `cargo test -p mdp --test rules` (`progress-md`) |
| One 1 MiB literal, clipboard fits the frame | `limits.rs` plus `max_clipboard_frame_fits_the_wire_cap` and `rules` (`mib-owner`) |
| Latency bullet names a real e2e test | `rules` (`latency-name`) |
| Clipboard cap name is `CLIPBOARD_MAX_BYTES` | `rules` (`clipboard-owner`) |
| Skill paths are symlinks to `.agents/skills/verify-mdp` | `rules` (`skill-home`) |
| No Deskflow / Input Leap crate (ADR 0001) | `rules` (`no-deskflow`) |
| CI runs windows-latest and macos-latest jobs (ubuntu-latest allowed as a build/test runner, not a Peer) | `rules` (`ci-peers`) and `.github/workflows/ci.yml` |
| `mdp-core` has no OS UI crates | `rules` (`core-platform-free`) |
| One Tokio runtime, `run::serve` | `rules` (`one-loop`) |
| Workflows do not merge PRs | `rules` (`self-merge`) |
| One workspace, one `mdp` binary | `rules` (`one-binary`) |
| Linux is not a Peer | `rules` (`linux-not-peer`); `mdp selftest` is `GAP` off Windows and macOS |
| Stuck keys released | `cargo test -p mdp-core stuck_keys_released_on_crossing` |
| Disconnect returns the cursor within 1 s | `cargo test -p mdp --test e2e kill_link_while_remote_restores_focus_within_1s` |
| Physical input claims Source (ADR 0002) | `cargo test -p mdp --test e2e physical_input_on_sink_claims_source` |
| Escape chord returns Focus | `cargo test -p mdp-core escape_chord_restores_local_focus` |
| No plaintext frame before a pinned peer | `cargo test -p mdp-core pinned_link_refuses_impostor_and_hides_plaintext` |
| Pairing code, reject, impostor | `cargo test -p mdp-core pairing_code_is_six_digits_and_stable` and the other `link::` tests |
| Clipboard does not echo, pre-connect copy stays, toggle, oversize | `verify-mdp drive clipboard` |
| Arrangement mirror and save | `verify-mdp drive arrangement` |
| UI tokens match `CONTEXT/ui-design.md` | `cargo test -p mdp tokens_match_ui_design_md` |
| Format, warnings, the rest of the suite | `verify-mdp` (`fmt`, `clippy -D warnings`, `test`) |

## Unenforced rules

Judgment calls. Nothing fails when an agent skips them.

- Hard, cross-module, or unknown-cause work follows poteto-mode. Reason: no command can see which playbook an agent picked. TODO: a check that a structural diff names the playbook it used.
- Review follows interrogate and leaves the verdict unapplied. Reason: a review verdict is a judgment. TODO: a check that a review comment is not also a commit by the same agent in that run.
- A risky refactor follows blast-radius and runs code before more edits. Reason: "risky" is a judgment. TODO: a diff-size check is the wrong proxy; leave it until one exists that reads the blast.
- `ship` one ticket, `grill` when the goal is unclear, `issues` for tracker work, `snapshot` for unfinished work. Reason: ticket shape is a judgment. TODO: a check that a PR touches one tracker id.
- Read `CONTEXT/architecture.md`, `CONTEXT/glossary.md`, `CONTEXT/adr/`, and `CONTEXT/ui-design.md` before the matching kind of change. Reason: reading is not observable. TODO: the rules test covers the invariants that can be checked; the rest stay here.
- Glossary avoid-words (client, server, master, slave) in new prose. Reason: those words are legitimate in sockets, tests, and OS APIs, so a word ban false-fires. TODO: a check scoped to `CONTEXT/` headings once the glossary list is the source of the ban.
- LAN cursor p95 under 10 ms on two real machines. Reason: the loopback test is one host. TODO: a two-machine probe. Until then it stays a verify-mdp gap.
- The two-machine list in `.agents/skills/verify-mdp/SKILL.md`. Reason: this agent has one OS. TODO: do not mark those items passed from `FakePlatform`.
