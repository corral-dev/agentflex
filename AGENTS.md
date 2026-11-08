# AgentFlex

`npx agentflex`: an animated, shareable terminal card of the user's coding-agent activity (sessions, tokens, tokens per day, peak parallel sessions, agent share, hour-by-weekday heatmap) with 1200×630 PNG export. Standalone Rust binary distributed through npm platform packages; also reachable as `corral flex`.

## Document navigation

- Read the documents whose described content is relevant to the current task.
- [Card design](~/Codes/Corral/cli/docs/design/SHAREABLE_WRAP_CARD_DESIGN.md): product decisions, card content, motion storyboard, image export, speed target, distribution and platform requirements (§10 supersedes earlier sections).
- [Stats engine](~/Codes/SessKit/docs/NATIVE_STATS_ENGINE_DESIGN.md): the SessKit `sesskit-stats` crate this binary consumes; parsing rules and the shared time aggregates (active intervals, peak concurrency, weekday × hour histogram) live there, never here.

## Hard constraints

- No history parsing in this repository: all session data comes from `sesskit-stats`, and time aggregates (active intervals, peak concurrency, weekday × hour histogram) come from `sesskit_stats::aggregate`, never re-implemented here.
- `sesskit-stats` is a git dependency pinned to one reviewed SessKit commit (`rev` in `Cargo.toml`), so a SessKit change never alters a build or a card silently. Bump the `rev` deliberately and check the card values against the previous build; never track a branch. To build against a local, unpushed SessKit, override per command: `cargo build --config 'patch."https://github.com/x0c/sesskit".sesskit-stats.path="../SessKit/crates/sesskit-stats"'`; it rewrites the `sesskit-stats` entry in `Cargo.lock`, which must not be committed.
- macOS, Linux and Windows (x64 and arm64) are supported targets.
- Card UI, motion and image export are owned by the primary model; packaging and CI may be delegated.

## Release

- The version lives only in `Cargo.toml`; npm package versions are stamped from it. Commit `release: X.Y.Z`, then push an annotated `vX.Y.Z` tag to `github`: `.github/workflows/release.yml` builds the six targets on native runners, checks tag = `Cargo.toml` version and publishes the platform packages before the launcher (already published versions are skipped). A `workflow_dispatch` run builds everything without publishing; use it to prove runner labels before tagging.
- After the run: create the GitHub Release with bilingual notes (English first), poll `npm view <pkg>@X.Y.Z` for all seven packages until they resolve (a fresh publish answers 404 for a few minutes), then verify `npx agentflex@latest` in an empty directory.
- README demo (`docs/demo.gif`, `docs/card.png`): `uv run --no-project --with pillow python scripts/demo.py` from a tree that contains only committed or owned changes, since it renders with that tree's binary and fictional data.
