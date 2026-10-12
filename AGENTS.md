## Current work

M3 Max (T6031) bring-up, driven from the M1 Air from 2026-10-12. Read `docs/agents/handoff.md` before doing anything, then `work/t6031-kit-refusals/README.md` (in the kernel tree: `.scratch/t6031-kit-refusals/`).

**Push every step of progress to the fork, right away** (Naeem, 2026-10-12), so nothing is lost if a machine or a session is: kernel fix branches to `origin` (`malik-na/linux-aurora`), and specs, tickets, tools and results to `agent-docs` under `work/`. The fork is public: never push firmware dumps or crash logs (Apple firmware memory), or anything with serial numbers, MAC addresses or credentials. Nothing goes to `omacom` unless Naeem asks.

## Agent skills

### Issue tracker

Issues live in GitHub Issues on `malik-na/linux-aurora` (not the `omacom` upstream). See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one root `CONTEXT.md` + `docs/adr/`, both kept on `agent-docs` like the files below. See `docs/agents/domain.md`.

### Where these files live

`AGENTS.md`, `CLAUDE.md`, `CONTEXT.md`, `docs/adr/` and `docs/agents/` are personal and must never reach a kernel commit or an omacom PR. In this tree they are symlinks into the `agent-docs` worktree at `../linux-aurora-agent-docs` (an orphan branch pushed to `origin` only). Commit and push changes to them from that worktree, never from this tree.
