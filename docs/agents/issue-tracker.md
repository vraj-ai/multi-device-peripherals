# Issue tracker

GitHub Issues on `vraj-ai/multi-device-peripherals` (plain GitHub, not Linear-synced).

| Action | Command |
|---|---|
| Create | `gh issue create --title "<t>" --body-file <f> --label ready-for-agent` |
| Read | `gh issue view <n> --comments` |
| List | `gh issue list --label ready-for-agent` |
| Comment | `gh issue comment <n> --body "<b>"` |
| Label | `gh issue edit <n> --add-label <l>` / `--remove-label <l>` |
| Close | `gh issue close <n> --comment "<why>"` |

Blocking uses GitHub native dependencies:
`gh api --method POST repos/vraj-ai/multi-device-peripherals/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`
where `<blocker-db-id>` = `gh api repos/vraj-ai/multi-device-peripherals/issues/<n> --jq .id`.
Every ticket body also carries a `Blocked by` line as the fallback.

Commits reference tickets with a `Refs: #<n>` trailer. PRs into `main`, one per ticket.
