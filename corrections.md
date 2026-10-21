# Corrections

Each class is a mistake an agent can copy. The layer is the highest one that holds. Proof is the check failing on the old behavior, then passing on this tree.

## `latency` named a bench that does not exist

Evidence: `CONTEXT/architecture.md` said the p95 budget was measured by the `` `latency` `` loopback bench. The only measurement is `mouse_move_loopback_latency_p95_under_10ms` in `crates/mdp/tests/e2e.rs`.

Layer: check (`latency-name`). The 10 ms LAN budget is a human invariant, so a type cannot name the test. The bullet now cites the real function, and the check fails when the cited name is not a function in `e2e.rs`.

Old behavior, from `bad_patterns_name_the_fix` on a tree that still says `` `latency` ``:

```text
FAIL latency-name: the Crossing latency bullet names no test. Cite `mouse_move_loopback_latency_p95_under_10ms` in crates/mdp/tests/e2e.rs.
```

Now: `repo_rules_hold` passes.

## Prose rules with no owner

These were sentences in `AGENTS.md`, `CONTEXT/architecture.md`, or the ADRs. Nothing failed when an agent broke them. Each check below names what to use instead. `bad_patterns_name_the_fix` rebuilds the old mistake in a temp tree and prints the failure. `repo_rules_hold` is the same checks on this tree.

| Rule | Layer | Why not higher | Old failure |
|---|---|---|---|
| Do not recreate `CONTEXT/progress.md` | check `progress-md` | The file is already gone. A type cannot forbid a path. | `FAIL progress-md: CONTEXT/progress.md is back. Status lives in git history, issues, and pull requests. Do not recreate that file.` |
| One skill home | check `skill-home` | `#40` already made the symlinks. The check stops a copied directory. | `FAIL skill-home: .cursor/skills/verify-mdp is a real file or directory. Edit .agents/skills/verify-mdp only, and keep .cursor/skills/verify-mdp as a symlink to ../../.agents/skills/verify-mdp.` |
| No Deskflow crate (ADR 0001) | check `no-deskflow` | There is no dependency to delete. | `FAIL no-deskflow: crates/mdp/Cargo.toml mentions deskflow. ADR 0001: build our own in this workspace. Do not add that crate.` |
| CI must include Windows and macOS jobs; Linux is a supported CI host, not a Peer OS | check `ci-peers` | The workflow already matched. The check requires `windows-latest` and `macos-latest` and allows an added `ubuntu-latest` build/test job. | `FAIL ci-peers: CI must run a macos-latest job. Windows and macOS are the Peer OSes; ubuntu-latest may be added as a build/test runner but does not replace them.` |
| `mdp-core` stays platform-free | check `core-platform-free` | The crate already had no OS deps. | `FAIL core-platform-free: crates/mdp-core/Cargo.toml depends on eframe. OS code belongs in crates/mdp/src/platform.` |
| One peer loop | check `one-loop` | `run::serve` is already the only runtime. | `FAIL one-loop: crates/mdp/src/other_loop.rs builds a Tokio runtime. Call run::serve in crates/mdp/src/run.rs.` |
| Self-merge is off (fleet ADR 0003) | check `self-merge` | No workflow merged PRs. A type cannot see GitHub. | `FAIL self-merge: .github/workflows/ci.yml merges pull requests. Self-merge is off (fleet ADR 0003). Open the PR and leave it.` |
| One binary | check `one-binary` | The workspace already had one bin. | `FAIL one-binary: Cargo.toml members must be exactly ["crates/mdp-core", "crates/mdp"].` |
| Linux is not a Peer | check `linux-not-peer` | `UnsupportedPlatform` and `run_stub` already exist. | `FAIL linux-not-peer: crates/mdp/src/main.rs must refuse native commands off Windows and macOS with native input hooks exist only on Windows and macOS. Do not make Linux a Peer.` |

`cargo test -p mdp --test rules -- --nocapture` prints those lines from `bad_patterns_name_the_fix` and then `ok`. On this tree `repo_rules_hold` is `ok`.

## Exceptions

- `crates/mdp/tests/rules.rs` skips itself in `no-deskflow`. Reason: the test has to name the forbidden crates. Expiry: 2027-04-20.
- `skill-home` allows a Windows working tree that is not a symlink when `git ls-files` still records mode `120000`. Reason: a checkout without symlink privilege materializes the blob. Expiry: 2027-04-20.

## Left as judgment

Routing (poteto-mode, interrogate, blast-radius, ship, grill, issues, snapshot), glossary avoid-words, and the two-machine list stay in `AGENTS.md` under Unenforced rules. No command can see them. Each line has a TODO for a check.
