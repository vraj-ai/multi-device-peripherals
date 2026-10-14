# Agent instructions

`mdp` shares one mouse and one keyboard between a Mac and a Windows PC. Read `CONTEXT/architecture.md` before changing structure or durable decisions, `CONTEXT/glossary.md` for language, `CONTEXT/adr/` for decisions already made, and `CONTEXT/ui-design.md` before any UI work.

## Route

Apply these yourself. The captain does not name a skill.

- Hard work, a change that crosses modules, or a bug whose cause is unknown: follow poteto-mode and let it pick the playbook.
- Every change: run verify-mdp (`.cursor/skills/verify-mdp/SKILL.md`) and read its transcript before the change counts as done.
- Review: follow interrogate. Leave the verdict unapplied.
- A risky refactor: follow blast-radius and prove the risk by running code before editing further.
- Ship one ticket at a time. Tracker notes live in `docs/agents/issue-tracker.md`.
- End with a closeout snapshot: branch, HEAD sha, what landed, and the next action.

## Authority

Self-merge is OFF (ADR 0003). Open the pull request against `main` and leave it for the captain to merge.

## Commands

- Verify: `.cursor/skills/verify-mdp/verify-mdp`
- Test: `cargo test --workspace`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Format: `cargo fmt --all -- --check`
