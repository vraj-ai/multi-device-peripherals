# Agent Instructions

Read `CONTEXT/architecture.md` before changing project structure or durable decisions.
Read `CONTEXT/progress.md` for the current verified milestone pointer.

Keep durable decisions in `CONTEXT/architecture.md`, keep progress bounded, and
use the project's tests, builds, git history, and goal artifacts as evidence.
Do not turn `CONTEXT/progress.md` into a session diary.

Read `CONTEXT/glossary.md` for language and `CONTEXT/ui-design.md` before any UI work.

## Agent skills

- Tracker: `docs/agents/issue-tracker.md`
- Labels: `docs/agents/triage-labels.md`
- Domain docs: `docs/agents/domain.md`

## Commands

- Test: `cargo test --workspace`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Format: `cargo fmt --all -- --check`
