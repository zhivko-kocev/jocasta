# 0451 More omp settings for his /settings pass

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
Which further omp settings go into his reference config (0420) for the `/settings` pass?

## Facts
Verified 2026-10-23.
- His Claude Code plugins reach every omp session through project-scope entries (`src/discovery/helpers.ts:1311-1331`, `discovery/claude-plugins.ts:32,47`); `disabledProviders: [claude-plugins]` turns that provider off (`src/capability/index.ts:306-308`).
- Keyless Parallel web search sends the query plus `session_id` and `model_name` to search.parallel.ai (`SRC/web/search/providers/parallel.ts:83-93`); his `web` role uses it.
- `find.enabled: auto` starts sending up to 20 files' passages to the `judge` role as soon as a qualifying judge is assigned (`src/tools/settings.ts:529`).
- `startup.setupWizard` shows onboarding scenes once per setup version, and `startup.changelogMode: hidden` keeps changelog chrome out of panes (omp `settings.md` §Interaction).
- `collab.autoStart` defaults to `off`; when not off, every session is hosted through `wss://my.omp.sh` (https://omp.sh/docs/collab).
- `secrets.enabled` replaces configured strings and credential-shaped tokens before provider requests, from `~/.omp/agent/secrets.yml` (omp `secrets.md`).
- Ctrl+L (`app.live.toggle`) starts Codex-backed realtime voice, sending mic audio and the session to OpenAI (`src/live/transport.ts:18`; `pi-tui/src/app-keybindings.ts:261-263`).
- His advisor runs in every pane; `WATCHDOG.md` gives it reviewer-only guidance that stays out of the main context (https://omp.sh/docs/advisor).
- `idleTimeoutMs` in `lsp.json` shuts idle language servers down and is off by default (omp `docs/lsp-config.md`); each pane holds its own servers.

## Options
| Option | Pros | Cons |
|---|---|---|
| The listed settings | Plugins, web search, find and live voice stop sending data; panes stay free of wizard and changelog chrome; idle servers freed | More for him to set by hand |
| Plus a `modelRoles.review` role at high effort | Reviews cheaper and faster than `slow` at max | Another role to keep |
| Plus a local model for the `memory` role | Transcript excerpts stay on the machine | Weaker extraction |

## Recommendation
The listed settings.

## Choice
`config/omp-config.yml` (his reference, 0420) gains: `disabledProviders: [claude-plugins]` (his Claude Code plugins stop loading into omp; `discovery/claude-plugins.ts:32,47`), `modelRoles.web: web/duckduckgo` (off parallel.ai), `find.enabled: off`, `startup.setupWizard: false`, `startup.changelogMode: hidden`, `github.enabled: true`, `collab.autoStart: off` (pinned), `secrets.enabled: true` (with his own `~/.omp/agent/secrets.yml`, never in the repo). Outside config.yml, documented in the same file's comments and README: `~/.omp/agent/keybindings.yml` `app.live.toggle: []` (Ctrl+L no longer starts omp's live voice), `~/.omp/agent/WATCHDOG.md` (the advisor's brief: principle 6 and the client-name rule; Jarvis ships the text as `config/WATCHDOG.md`), `~/.omp/agent/lsp.json` `idleTimeoutMs: 300000`. He sets them himself (0420).

## Reason
"The listed settings". He rejected a dedicated `modelRoles.review` at high effort and a local model for the `memory` role.

## Consequences
- Amends 0398 (more values) and 0420 (the reference grows; he still sets it by hand).
- `secrets.yml` and his client names never enter the repo (principle 6).
- `config/WATCHDOG.md` is a shipped text chosen by the builder and listed as built.
- Amended by 0478: Auto QA off, Claude Code plugins off and DuckDuckGo web search also ride in every Jarvis pane.
- Amended by 0479: setup writes `keybindings.yml`, `WATCHDOG.md` and `lsp.json` after a diff and his `y`; they are no longer by hand.
