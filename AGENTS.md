# AgentFlex

`npx agentflex`: an animated, shareable terminal card of the user's coding-agent activity (sessions, tokens, tokens per day, peak parallel sessions, agent share, hour-by-weekday heatmap) with 1200×630 PNG export. Standalone Rust binary distributed through npm platform packages; also reachable as `corral flex`.

## Document navigation

- Read the documents whose described content is relevant to the current task.
- [Card design](~/Codes/Corral/cli/docs/design/SHAREABLE_WRAP_CARD_DESIGN.md): product decisions, card content, motion storyboard, image export, speed target, distribution and platform requirements (§10 supersedes earlier sections).
- [Stats engine](~/Codes/SessKit/docs/NATIVE_STATS_ENGINE_DESIGN.md): the SessKit `sesskit-stats` crate this binary consumes; parsing rules live there, never here.

## Hard constraints

- No history parsing in this repository: all session data comes from `sesskit-stats`.
- macOS, Linux and Windows (x64 and arm64) are supported targets.
- Card UI, motion and image export are owned by the primary model; packaging and CI may be delegated.
