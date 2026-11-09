# 0081 Catalog rows from Claude Code mods

- Status: decided
- Kind: product
- Phase/slice: 0/appendix A
- Vision: V8
- Date: 2026-10-07

## Question
Which rows from the Claude Code mods post join appendix A?

## Facts
Verified 2026-10-07.
- Anthropic, "Getting started with Claude Code mods", 2026-10-01, https://claude.dev/blog/getting-started-with-claude-code-mods/: a mod is a JavaScript or TypeScript hook module in a plugin; each hook observes, rewrites or answers an event, and `ui.render` draws named components. Claude Code builds AGENTS.md support and its `/diff` pane as mods, and writes mods on request with hot reload.
- Blast Radius holds `rm -rf`, `git reset --hard`, `git clean`, force pushes and migrations, measures them with the tools' own dry runs (`git status --porcelain`, `git clean -n`, `git log HEAD..origin/main`) and offers Proceed or Cancel; the post calls it "a safety net, not a permission system".
- Token Weather draws the window's fill, a 12-turn chart and the last turn's growth above the prompt. Replay Theater records each edit in a turn and steps through the diffs with `/replay`.
- Pi 1.0.1's `ctx.ui` provides dialogs, notifications, status text, widgets and custom components (`docs/extensions.md:244-246`).
- Pi 1.0.1's system prompt routes questions about extensions, themes, skills, the TUI and the SDK to Pi's README, `docs/` and `examples/` (`dist/core/system-prompt.js:86-93`); Jarvis releases ship `docs/` without `examples/` (0044).

## Options
| Option | Pros | Cons |
|---|---|---|
| Dry-run preview (2.4) | The approval shows what would be destroyed; deterministic, no model call | Dry runs exist only for some commands; a moment before the overlay |
| Context gauge (1.11, 3.4) | V9's smooth rollover is visible before it happens | One more status element |
| Edit replay (6.10) | A turn's edits reviewed in order | Overlaps git diff and Pi's export |
| Self-extension (1.11) | "Build me an extension" works against Jarvis's config dir and `/reload` | Pi already routes to its docs; fixing the `examples/` route may reopen 0044 |
| None | — | — |

## Recommendation
All four.

## Choice
All four.

## Reason
Not given.

## Consequences
- Appendix A gains four rows: dry-run preview (2.4), context gauge (1.11, 3.4), edit replay (6.10) and self-extension (1.11). Each is still decided in its slice (0008).
