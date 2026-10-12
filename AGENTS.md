## Agent skills

### Issue tracker

Issues live in GitHub Issues on `malik-na/linux-aurora` (not the `omacom` upstream). See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one root `CONTEXT.md` + `docs/adr/`. See `docs/agents/domain.md`.

### Where these files live

`AGENTS.md`, `CLAUDE.md` and `docs/agents/` are personal and must never reach a kernel commit or an omacom PR. In this tree they are symlinks into the `agent-docs` worktree at `../linux-aurora-agent-docs` (an orphan branch pushed to `origin` only). Commit and push changes to them from that worktree, never from this tree.
