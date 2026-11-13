---
name: agentflex
description: Show the user's coding-agent week as an animated terminal card (sessions, tokens, peak parallel sessions, agent and model share, activity heatmap) or export a 1200x630 PNG for sharing. Use when the user asks for weekly stats, a wrapped-style recap, or to flex token usage.
---

# agentflex

Animated, shareable card of the local coding-agent activity. No install, no account, nothing uploaded: it reads the local histories of Claude Code, Codex, OpenCode, Pi and Google Antigravity and aggregates the last 7 days.

## Run

```bash
npx -y agentflex --png week.png   # render the card image and exit (preferred for agents)
npx -y agentflex --json           # machine-readable totals (period, sessions, tokens, by_runtime, by_model, heatmap)
```

Periods: default 7 days; `--month` for 30 days, `--all` for all retained history. `--no-anim` prints the final frame without animation. Interactive keys (human at a terminal): `s` save PNG, `r` replay, `q` quit.

## Report

After `--png`, read back `--json` and summarize in one or two lines: sessions, tokens (compact, e.g. 7.45B), peak parallel with its timestamp, and the top agent/model. Offer the PNG path for posting. Totals only: never include prompts, file names, or project names.
