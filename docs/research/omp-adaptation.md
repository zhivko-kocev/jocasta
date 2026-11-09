# Adapting omp as Jocasta's harness

The guide for F1 (records 0386, 0387, 0389, 0391, 0396): Zhivko's daily omp is the harness, whole and unforked, adapted with settings, config files and memory only. He walks every setting in `/settings` with this guide; his non-default values become a record and `config/omp-config.yml`, which `jocasta setup` writes into a missing `~/.omp/agent/config.yml`.

- **Version:** omp 18.10.3 (`omp --version` → `omp/18.10.3`), source at tag `v18.10.3` of `can1357/oh-my-pi`; herdr at tag `v0.9.4`. Read 2026-10-13.
- **Citations:** paths are relative to the omp source (`packages/coding-agent/src/…` unless another package is named); `herdr:` marks herdr's source.
- **Settings:** 550 registered settings (`packages/coding-agent/src/config/all-settings.ts`): 414 on the ten `/settings` tabs, 136 in config files only.
- **Columns:** *serves* names the Jocasta record, vision line or principle a setting bears on; *recommendation* is `default` (left out of `config/omp-config.yml`, principle 3) or a value with its reason. ⚠ marks a setting that sends data off the machine beyond the request to the model doing the work.

## Recommended changes from omp's defaults

Every other setting stays at its default. Items marked *his choice* are decided in the walkthrough.

| Setting | Recommended | Why |
|---|---|---|
| `memory.backend` | `mnemopi` | the local SQLite backend (`~/.omp/agent/memories/mnemopi/mnemopi.db`), 0389 |
| `mnemopi.llmMode` | `none` | keeps memory text off the model provider; embeddings run locally after a one-time model download |
| `tools.approvalMode` | `write` | under `yolo` omp's built-in dangerous-command patterns don't prompt (`tools/approval.ts:200-201,255-272`); the guard fails closed (principle 4, 0391) |
| `bash.gitGuard` | `true` | covers part of the retired git-guard (0391) |
| `bash.patterns` | *his choice* | deny or prompt rules that hold even under `yolo` |
| `telemetry.otlpExportEnabled` | `false` | "No telemetry" non-goal (section 4) |
| `dev.autoqa` | `false` | tool-issue reports would go to `qa.omp.sh` |
| `dev.autoqaConsent` | `denied` | reports may carry paths that name clients (principle 6) |
| `startup.checkUpdate` | `false` | upgrades are deliberate (0061, 0383) |
| `marketplace.autoUpdate` | `off` | nothing updates unless he asks (0383) |
| `startup.quiet` | `true` | many panes start (0378, 0388) |
| `recap.enabled` | `false` | idle panes would each pay for a summary call |
| `providers.cacheWarming` | `streaming` | 0170; `idle` re-sends requests between runs |
| `lsp.diagnosticsOnEdit` | `true` | diagnostics in each edit's result (0288) |
| `bashInterceptor.enabled` | `true` | 0149 |
| `compaction.methodOrder` | `["handoff", "soft", "remote", "shake", "snapcompact"]` | the session watcher needs readable summaries (0389); `snapcompact` stores history as images |
| `paste.largeMenuThreshold` | `0` | herdr sends prompts as bracketed pastes (`herdr:src/app/api/agents.rs:637-639`); untested |
| `personality` | `none` | his persona moves to `~/.omp/agent/AGENTS.md` |
| `tts.localVoice`, `speech.voice` | `bm_george` | 0293, if he turns omp's speech on |
| `secrets.enabled` | `true` | omp's redaction replaces Jocasta's (0386) |
| `modelRoles.*`, `retry.fallbackChains`, `defaultThinkingLevel`, `disabledProviders`, `task.agentModelOverrides` | *his choice* | never `auto` thinking (0182); roles and fallbacks are his (0283, 0377) |
| theme, status line, `completion.notify`, `error.notify`, `ask.notify` | after the UX pass | 0395 |
| `tui.renderSvg` | `false` after F3 | drawings go through whiteboard (0393) |

What can't be switched off: the model catalog fetch (`catalog.stencil.so`, plus each logged-in provider's model list) on every launch, the device id on Claude requests, and `read`'s keyless fallbacks for URLs unless `fetch.enabled: false`. Details in the network section.

## What these facts change in the program plan

Each item is detailed with file:line in the next section.
- **`--profile` is an isolated omp home** (`~/.omp/profiles/<name>/agent`: its own logins, sessions and settings). The per-run settings layer is `--config <file>`; evals pass their overlay that way (F1).
- **No startup flag for plan mode.** `plan.defaultOnStartup` opens new sessions in plan mode and leaves resumed ones as they were; a `--config` overlay with it on `omp --continue` gives a new thread plan mode (F2). `/plan` works only typed into the terminal; `/plan <brief>` turns plan mode on and sends the brief, and toggles it off on a session already in plan mode.
- **`/handoff` is a compaction entry** with `method: "handoff"` in the same session file, not a new session; subagent transcripts sit beside it as `<stem>/<agentId>.jsonl` (the watcher skips them).
- **JSON usage leaves out subagents** except through the `task` tool result's `details.usage`; title and memory side calls never appear.
- **`yolo` prompts for nothing built in**; only `bash.patterns`, `tools.approval.<tool>` and computer-use checks still act.
- **herdr's omp integration reports working, blocked and idle, never `done`**; herdr shows `done` only for an unfocused tab that went idle. `pane.agent_status_changed` needs one subscription per pane.
- **herdr's socket for session `jocasta`** is the named pipe `\\.\pipe\` + `%APPDATA%\herdr\sessions\jocasta\herdr.sock`; herdr's own omp extension opens it with Bun's `node:net`. `HERDR_CONFIG_PATH` moves only the config.
- **stdio MCP servers** start in omp's cwd with omp's whole environment, `HERDR_PANE_ID` included; global config `~/.omp/agent/mcp.json`, project config `<cwd>/.omp/mcp.json` (no walk up).
- **SDK:** `ModelRegistry`, `AuthStorage` and `discoverAuthStorage` are public exports of `@oh-my-pi/pi-coding-agent`, `completeSimple` of `@oh-my-pi/pi-ai`.

## Facts F2–F4 rely on

Sources: omp at tag `v18.10.3` (paths relative to omp-src; `packages/coding-agent/src/…` unless another package is named), herdr at tag `v0.9.4` (paths prefixed `herdr:`). Examples are trimmed shapes built from the cited types, not captured output.

### 1. JSON mode (`omp -p <prompt> --mode json`) and print-mode exit codes

**Line format.** One JSON object per stdout line. The first line is the session header (`modes/print-mode.ts:154-160`). Every later line is one `AgentSessionEvent` passed through `printableEvent` (`modes/print-mode.ts:233-236`, `75-104`). That function drops `message_update` snapshots and keeps only the delta (`print-mode.ts:79-89`), and strips `providerPayload` from `message_start`/`message_end`/`turn_end`/`agent_end` (`print-mode.ts:90-100`). Documented in `docs/cli-reference.md:214-217`.

**Event types.** Core events: `agent_start`, `agent_end{messages}`, `turn_start`, `turn_end{message, toolResults}`, `message_start{message}`, `message_update{assistantMessageEvent}`, `message_end{message}`, `tool_execution_start{toolCallId, toolName, args, intent?}`, `tool_execution_update`, `tool_stream_update`, `tool_execution_end{toolCallId, toolName, result, isError?}` (`packages/agent/src/types.ts:1248-1270`). Session events add `auto_compaction_start/end`, `auto_retry_start/end`, `retry_fallback_applied/succeeded`, `ttsr_triggered`, `notice`, `queue_update` and others (`session/agent-session-events.ts:14-91`).

| Need | Where it is |
|---|---|
| Final assistant text | The last `message_end` whose `message.role === "assistant"`: concatenate its `content[]` items with `type: "text"` (`text` field). The `AssistantMessage` shape is at `packages/ai/src/types.ts:1245-1333` and `TextContent` at `packages/ai/src/types.ts:874-877`. `agent_end.messages` carries the full run too (`packages/agent/src/types.ts:1251-1257`). Text mode prints the same thing from `session.getLastAssistantMessage()` (`print-mode.ts:303`, `329-336`). |
| Stop reason / error | `message.stopReason`: `stop`, `length`, `toolUse`, `error` or `aborted` (`packages/ai/src/types.ts:1041`); `errorMessage` and `errorStatus` (`types.ts:1298`, `1306`). |
| Model id | `message.provider` + `message.model` on each assistant message (`packages/ai/src/types.ts:1257-1258`); `upstreamModel` when a router reports the model that actually served the turn (`types.ts:1279`). |
| Usage per turn | `message.usage` (`packages/ai/src/types.ts:1289`) is a `Usage`: `input`, `output`, `cacheRead`, `cacheWrite`, `totalTokens`, optional `reasoningTokens`, `premiumRequests`, `orchestration{}`, `server{webSearch, webFetch}`, and `cost{input, output, cacheRead, cacheWrite, total}` (`packages/catalog/src/types.ts:209-274`). Sum over all assistant `message_end` lines. |
| Subagent usage | **Not included** in the parent's assistant `usage`. Subagent spend appears only in the `task` tool result: `tool_execution_end.result.details.usage` ("Aggregated usage across all subagents", `packages/tui/src/tools/task.ts:2509-2514`), and per spawn in `details.results[i].usage` (`task/executor.ts:2947`, accumulated at `task/executor.ts:2067-2087`). To get the full spend, add assistant usage and task `details.usage`. Hidden side calls (title, memory) are written as `model_usage` session entries (`session/session-entries.ts:85-97`). They are not in the event union (`agent-session-events.ts:14-91`), so the JSON stream does not carry them. |
| Turns | Count `turn_start` (or `turn_end`) events. A turn is one assistant response plus its tool calls (`packages/agent/src/types.ts:1258-1260`). |
| Tool calls | `tool_execution_start` / `tool_execution_end` (`packages/agent/src/types.ts:1267-1270`), or `content[]` items with `type: "toolCall"` (`id`, `name`, `arguments`) in assistant messages (`packages/ai/src/types.ts:1016-1023`). |

Minimal example (shape):

```jsonl
{"type":"session","version":3,"id":"<session-id>","timestamp":"2026-10-13T10:00:00.000Z","cwd":"D:\\work\\demo"}
{"type":"agent_start"}
{"type":"turn_start"}
{"type":"message_start","message":{"role":"user","content":"add a README line","timestamp":1781000000000}}
{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Done"}}
{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"Done."}],"api":"anthropic-messages","provider":"anthropic","model":"<model-id>","usage":{"input":1200,"output":40,"cacheRead":0,"cacheWrite":0,"totalTokens":1240,"cost":{"input":0.0036,"output":0.0006,"cacheRead":0,"cacheWrite":0,"total":0.0042}},"stopReason":"stop","timestamp":1781000000500}}
{"type":"turn_end","message":{"role":"assistant","…":"…"},"toolResults":[]}
{"type":"agent_end","messages":[…]}
```

**Exit codes (print mode, text and JSON alike).**

| Code | When | Source |
|---|---|---|
| 0 | Turn finished (`stopReason` not `error`/`aborted`, or a silent abort) | `modes/print-mode.ts:309-314`, `388` |
| 1 | Last assistant `stopReason` is `error` or `aborted` (not silent, not signal-driven); session-store durability failure; `OMP_MCP_REQUIRE_READY=1` with an MCP server not ready | `modes/print-mode.ts:266-269`, `309-314`, `364-374`, `388`; returned through `postmortem.quit(exitCode)` at `main.ts:2667-2681` |
| 130 / 143 / 129 | SIGINT / SIGTERM / SIGHUP during the run | `packages/utils/src/postmortem.ts:567`, `644`, `648`; noted at `print-mode.ts:113-115` |
| 2 | Invalid or unknown flags; interactive launch without a TTY | `main.ts:2384-2391`, `main.ts:277-283` |
| 1 | Unknown model or no API key at startup; `--api-key` without a model | `main.ts:2568-2582`, `main.ts:2280-2285` |

In JSON mode a failed turn still exits 1 (`print-mode.ts:304-308`, issue #11498 cited there).

### 2. Starting a session in plan mode

- **CLI flag: none for interactive plan mode.** `--plan <id>` sets the *plan model role*, not the mode (`cli/flag-tables.ts:156-158`; `main.ts:1951`; `docs/cli-reference.md:107`). `--plan-yolo` starts read-only plan mode, auto-approves the plan and then implements it. It is the only headless plan flow (`docs/cli-reference.md:133`; `cli/args.ts:368-369`).
- **Setting `plan.defaultOnStartup`** (default `false`, needs `plan.enabled`, default `true`) (`plan-mode/settings.ts:12-35`). Interactive mode enters plan mode only for a *brand-new* session: one with no conversation context and no `mode_change` entry (`modes/interactive-mode.ts:613-625`, applied at `2381-2396`). The comment there says `omp --continue` that finds no session and creates a fresh one still honours it, while a resumed session keeps its stored mode (`interactive-mode.ts:2373-2379`, `2386-2391`). Print mode ignores it and writes a note to stderr (`modes/print-mode.ts:174-192`).
  - Per-process use: put `plan: {defaultOnStartup: true}` in a `--config <file>` overlay (see §3), e.g. `omp --continue --config <thread-overlay.yml>`. Fresh worktree sessions then open in plan mode, and resumed ones keep their mode.
- **`/plan` typed into the TUI.** `/plan` is a TUI-only builtin (`handleTui`) with optional inline text: `/plan <prompt>` enters plan mode and submits `<prompt>` as the first plan turn (`slash-commands/builtin-modes.ts:325-345`; `modes/interactive-mode.ts:5849-5873`). It is a **toggle**: if plan mode is already on, `/plan` exits it (with a confirm dialog when a draft exists) (`interactive-mode.ts:5820-5831`). It is refused while goal or vibe mode is active (`5812-5819`).
- **`/plan` as the positional first prompt (`omp "/plan …"`) does not work.** The CLI initial message goes to `session.prompt()` (`main.ts:795-802`). That path runs only extension, custom-TS and file-based slash commands (`session/agent-session.ts:7319-7342`), not TUI builtins like `/plan`. Use typed input (herdr `pane.send_text` + Enter, or `agent.prompt`) or the setting above.

### 3. `--append-system-prompt` and `--profile`

**`--append-system-prompt <text|file>`** (`cli/flag-tables.ts:185-187`; `docs/cli-reference.md:169`). It accepts either a file path or text (`system-prompt.ts:284-300`):
- a value containing `\n` is used as literal text;
- otherwise omp tries to read it as a file path;
- if the file is missing, the value itself is used as text.

When the flag is absent, omp falls back to a discovered `APPEND_SYSTEM.md`: project first, then user `~/.omp/agent/APPEND_SYSTEM.md` (`main.ts:1331-1342`, `1396`). The flag replaces discovery; it does not add to it (`main.ts:1396`).

**`--profile <name>` is an isolated omp root, not a settings overlay.**
- Location: `~/.omp/profiles/<name>/agent/` (`packages/utils/src/dirs.ts:114-121`, `132-134`).
- What it isolates: **auth, sessions, settings and caches** (`docs/cli-reference.md:76`; `docs/settings.md:24`). It sees *only* its own config: MCP, extensions, AGENTS.md, `agent.db` (`docs/config-usage.md:78-80`; `docs/mcp-config.md:48-57`). Keybindings are the one merged exception (`docs/config-usage.md:82`).
- Settings format: the profile's `config.yml` (YAML) (`docs/settings.md:17`, `24`).
- Merge: none with the default profile.
- Consequence for evals (plan F1 step 3): `--profile eval-run` would run **without his logins**.

**`--config <file>` is the per-run settings overlay** (repeatable) (`cli/flag-tables.ts:122-124`; `main.ts:1884`; `docs/settings.md:21`, `297-306`).
- Format: same YAML as `config.yml`, nested mappings for dotted ids (`docs/settings.md:171-183`).
- Precedence: defaults < global < project `.omp/config.yml` < `--config` overlays < runtime flags < setting env vars (`docs/settings.md:112-127`).
- Merge: objects deep-merge; scalars and arrays replace (`docs/settings.md:162-169`).
- Persistence: never persisted (`docs/settings.md:21`). The `PI_CONFIG_FILES` env list loads before `--config` (`docs/settings.md:158`; `config/settings.ts:674-675`).
- Example: an overlay holding `memory: {backend: off}` keeps his `~/.omp/agent` auth and sessions.

### 4. MCP config files and stdio server cwd/env

**Files** (`discovery/builtin.ts:222-227`; `docs/mcp-config.md:15-31`):

| Scope | Path |
|---|---|
| Project (cwd only, no ancestor walk) | `<cwd>/.omp/mcp.json` (also `.omp/.mcp.json`); fallback `<cwd>/mcp.json` / `.mcp.json` |
| User | `~/.omp/agent/mcp.json` (also `.mcp.json`); a named profile uses its own |

On a name clash the project entry comes first; a project `enabled: false` hides a same-named user server (`docs/mcp-config.md:46`).

**Format** (`docs/mcp-config.md:74-138`):

```json
{
  "mcpServers": {
    "jocasta": { "type": "stdio", "command": "jocasta", "args": ["mcp"], "env": { "EXAMPLE": "1" }, "cwd": "D:\\optional" }
  },
  "disabledServers": []
}
```

stdio fields: `command` (required), `args`, `env`, `cwd`. Shared fields: `enabled`, `timeout`, `instructions` (`docs/mcp-config.md:101-138`).

**cwd.** `config.cwd` if set, else `getProjectDir()` (`mcp/transports/stdio.ts:585`, `603-605`). `getProjectDir()` is the omp process cwd (`packages/utils/src/dirs.ts:207-231`). In a herdr pane started with `cwd` = the worktree, the server therefore starts in the worktree.
- Subagents reuse the parent's MCP connections (`docs/mcp-config.md:116`), so one server per top-level omp process.
- omp started in `~` moves itself to a temp dir unless `--allow-home` (`docs/cli-reference.md:74`).

**env.** `{ ...Bun.env, ...config.env }` (`mcp/transports/stdio.ts:581-584`). The child inherits omp's whole environment, including herdr's `HERDR_PANE_ID`, `HERDR_TAB_ID`, `HERDR_WORKSPACE_ID` and `HERDR_SOCKET_PATH` (set on the pane process: `herdr:src/pane.rs:202-215`, `herdr:src/integration/env.rs:28-33`).

**Print mode** waits for all MCP servers before the first turn, up to `OMP_MCP_TIMEOUT_MS` (default 30 s) (`docs/mcp-config.md:118`; `modes/print-mode.ts:244-270`).

### 5. Session JSONL layout, compaction and `/handoff`

**Root.** `~/.omp/agent/sessions/` (`docs/config-usage.md:80`). The cwd → directory-name mapping is `sessionDirForCwd` → `getDefaultSessionDirName` (`session/session-paths.ts:215-217`, `78-116`):

| cwd location | Dir name | Example |
|---|---|---|
| Under the OS temp dir (checked first) | `-tmp-<rel path, / \ : → ->` | `-tmp-jocasta-demo` |
| Under home | `-<rel path>` | `-src-demo` |
| Elsewhere | `--<abs path, / \ : → ->--` | `D:\jocasta` → `--D--jocasta--` |

Sources: `session-paths.ts:46-54`, `97-109`.

**Files.**
- Session file: `<dir>/<fileSafeTimestamp>_<sessionId>.jsonl` (`session/session-manager.ts:2419`).
- Its artifacts directory: `<dir>/<same stem>/`.
- Subagent transcripts nest inside it as `<stem>/<agentId>.jsonl` (`session-manager.ts:361-369`). A watcher globbing `sessions/**/*.jsonl` also picks up subagent files.

**Lines 1–2.**
- Line 1 is a fixed-width title slot `{"type":"title","v":1,"title":…,"source":…,"updatedAt":…,"pad":…}` (256 bytes) (`session/session-entries.ts:17-33`).
- Line 2 is the header `{"type":"session","version":3,"id","timestamp","cwd", additionalDirectories?, parentSession?, previousSessionFiles?}` (`session-entries.ts:37-58`).
- Format version: `CURRENT_SESSION_VERSION = 3` (`session-entries.ts:15`). Checked against one local file: line 1 is `title`, line 2 is `session` with `version: 3`.

**Entries.** Every later line has `type`, `id`, `parentId`, `timestamp` (`session-entries.ts:73-78`). The full union is at `session-entries.ts:309-325` (`message`, `model_usage`, `compaction`, `branch_summary`, `custom`, `custom_message`, `mode_change`, …).

**Compaction entry** (`session-entries.ts:125-149`):

```json
{"type":"compaction","id":"<entry-id>","parentId":"<prev>","timestamp":"…","summary":"<markdown summary>","shortSummary":"…","firstKeptEntryId":"<id>","tokensBefore":182000,"tokensAfter":24000,"method":"soft","details":{…},"fromExtension":false}
```

- `method` ∈ `remote | snapcompact | handoff | soft | shake`; absent on legacy or extension compactions (`session/compaction-methods.ts:11-41`; `session-entries.ts:133-134`).
- The summary text is in `summary`.

**`/handoff`.** It is **not a new session**. It commits a regular `compaction` entry with `method: "handoff"` in the *current* session file (`session/session-maintenance.ts:2038-2071`).
- `details = {readFiles, modifiedFiles}` and the handoff document sits in `summary` (`docs/handoff-generation-pipeline.md:103-113`).
- Session id, session file and cache key stay the same (`docs/handoff-generation-pipeline.md:113`), so there is no parent link.
- Auto handoff runs when `handoff` is in `compaction.methodOrder` (default order `remote, snapcompact, handoff, shake, soft`; `compaction-methods.ts:44-50`).
- Older sessions may carry `custom_message` entries with `customType: "handoff"` (`docs/handoff-generation-pipeline.md:201`).

**`parentSession`** in the header links forks and new sessions created from another session (`session-manager.ts:1256`, `2430`, `3873`). A watcher classifies `kind` from `method === "handoff"`.

**`--continue`** uses this terminal's breadcrumb if it points at the same cwd; otherwise it takes the newest non-empty session in the cwd's dir, or creates a new one (`session-manager.ts:4221-4331`; breadcrumb writer `session-paths.ts:433-485`). That gives one session line per worktree directory.

### 6. Memory backends — which is local SQLite

The selector is `memory.backend` ∈ `off | local | hindsight | mnemopi | sharpshooter`, default `off` (`memory-backend/resolve.ts:8-29`; `docs/memory.md:3-11`).

| Backend | Storage | Model calls | Network |
|---|---|---|---|
| **`mnemopi`** ("local Mnemopi SQLite memory", `resolve.ts:14`) | `mnemopi.dbPath`, default `~/.omp/agent/memories/mnemopi/mnemopi.db`; per-project banks are sibling DBs under `…/mnemopi/banks/<bank>/mnemopi.db` (`mnemopi/config.ts:73-75`, `230-249`; `packages/utils/src/dirs.ts:1001-1004`) | `mnemopi.llmMode` (default `smol`) resolves the **`memory` model role** (fallback `tiny`, then `smol`) through his logins; `remote` uses `mnemopi.llm*`; `none` disables (`docs/mnemosyne-memory-backend.md:68`, `97`; `mnemopi/backend.ts:568`, `631-632`) | Provider calls for that role. Local embeddings via fastembed (default variant `en` = `BAAI/bge-base-en-v1.5`), which downloads model files when missing (`docs/mnemosyne-memory-backend.md:64`; `packages/mnemopi/src/core/embeddings.ts:96-103`); download host not found in omp source. Remote embedding API only when `embeddingApiUrl`/API model is set (`embeddings.ts:223-265`). |
| `local` | Rollout summaries via `agent.db` SQLite job tables plus files `MEMORY.md`, `memory_summary.md`, `skills/`, `learned.md` (`memory-backend/local-backend.ts:11-18`; `memories/index.ts:146`; `memories/storage.ts:67-68`; `docs/memory.md:73-85`) | Phase 1 `default` role, phase 2 `smol` (`docs/memory.md:104-111`) | Provider calls only |
| `hindsight` | Remote Hindsight server (`hindsight.apiUrl`, default `http://localhost:8888`) | — | Yes (`docs/memory.md` "Hindsight remote backend") |

**What mnemopi stores:**
- completed conversation turns, retained every `mnemopi.retainEveryNTurns` (default 4);
- explicit `retain`/`learn` items;
- extracted facts and embeddings.

Tables: `working_memory`, `episodic_memory`, `memory_embeddings`, `facts` (`packages/mnemopi/src/core/beam/schema.ts:26`, `59`, `275`, `346`).

**When it acts.** It recalls into a `<memories>` block on the first turn and adds context at compaction (`docs/mnemosyne-memory-backend.md:21-27`, `57`). Subagents reuse the parent's state (`docs/mnemosyne-memory-backend.md` "Operational notes").

### 7. Dangerous-command prompts under `tools.approvalMode: yolo`

**Nothing built-in is prompted under `yolo`.** The resolver (`tools/approval.ts:203-273`) works in this order:
1. A tool `deny` → deny.
2. A user `tools.approval.<tool>: deny` → deny (`approval.ts:235-253`).
3. In `yolo`: a tool-declared `policy` (`allow`/`prompt`) wins; otherwise the user policy, or **allow** (`approval.ts:255-272`). An `override` flag alone does not prompt (`approval.ts:200-201`; `docs/approval-mode.md:55`, `72`).

**Critical patterns.** bash's `CRITICAL_BASH_PATTERNS` (`tools/bash.ts:185-230`) yield `{tier:"exec", override:true}` with **no `policy`** (`bash.ts:541-544`, `569-573`), so `yolo` lets them run. They cover:

| Group | Examples |
|---|---|
| Recursive delete of an absolute path | `rm -rf /…`, `--no-preserve-root`, `sudo rm` |
| Recursive permission or owner change on `/` | `chmod -R`/`chown -R … /` |
| Fork bomb | |
| Disk and filesystem | `> /dev/sdX`, `mkfs`, `dd … of=/dev/`, `shred /dev/`, `cryptsetup` |
| Account files | writes or `tee` to `/etc/passwd`, `shadow`, `sudoers` |
| Fetch then execute | `curl`/`wget` piped into a shell, `<(curl …)`, `eval $(curl …)` |
| Host control | `kill -9 1`, `shutdown`/`poweroff`/`reboot`/`halt`, `init 0` |
| Network shell | `nc -e`/`-c` |

All patterns are POSIX-shaped.

**What still prompts or blocks under `yolo`:**
- `bash.patterns` rules (`exec/settings.ts:134`): `approval: prompt` → prompt; `deny` → block (`bash.ts:533-540`, `561-567`, `579-585`).
- `tools.approval.<tool>: prompt|deny` (`tools/settings.ts:295`; `approval.ts:266-271`).
- Provider computer-use `pendingSafetyChecks` always prompt; headless they fail closed (`docs/approval-mode.md:92`; `extensibility/extensions/wrapper.ts:390`).
- Subagents run `yolo`; any call that resolves to `prompt` is rejected there (`docs/approval-mode.md:172`).

The mode itself is `tools.approvalMode` (`tools/settings.ts:312`). `--approval-mode`, `--yolo` and `--auto-approve` override it per run (`docs/approval-mode.md:26`).

### 8. SDK: model registry and logins from `~/.omp/agent` in another Bun process

Superseded by 0463 (2026-10-23): `llm-endpoint.ts` no longer imports omp's SDK or `completeSimple`; it spawns `omp auth-gateway stdio` (see "omp 18.11.0 → 18.12.6" below). Kept as the record of what 0390 built.

**Public exports.**
- `@oh-my-pi/pi-coding-agent`'s `exports` maps `"."` → `./src/index.ts` and `"./*"` → `./src/*.ts`. The package ships TypeScript source for Bun (`packages/coding-agent/package.json` `exports`; `docs/sdk.md:14`).
- Root exports used here:
  - `ModelRegistry` (`index.ts:12` → `config/model-registry.ts`);
  - `AuthStorage` (`index.ts:43` → `session/auth-storage.ts`);
  - `discoverAuthStorage` (`index.ts:40` → `sdk.ts:1015-1030`);
  - `Settings` (`index.ts:18`).
- `resolveRoleSelection` is public through the `./*` wildcard as `@oh-my-pi/pi-coding-agent/config/model-resolver` (`config/model-resolver.ts:1768-1784`).
- `completeSimple` is exported from `@oh-my-pi/pi-ai` root (`packages/ai/src/index.ts:44` → `stream.ts:1559-1574`).
- `docs/sdk.md:19-35` lists `AuthStorage`, `ModelRegistry`, `discoverAuthStorage` as root (not `/sdk`) exports.

**Pieces.**
- `discoverAuthStorage(agentDir = getAgentDir(), {settings, cwd})` opens his stored logins: `agent.db`, OAuth with refresh, broker if configured (`sdk.ts:1015-1030`; auth order `docs/sdk.md:192-201`).
- `new ModelRegistry(authStorage, modelsPath?, {settings})` reads `~/.omp/agent/models.yml` (`config/model-registry.ts:517-548`).
- `refresh(strategy)` accepts `online | offline | online-if-uncached` (`model-registry.ts:553-563`; `packages/catalog/src/model-manager.ts:25`).
- `find(provider, id)` (`model-registry.ts:3173-3176`) and `getApiKey(model)` (`model-registry.ts:3240-3257`).
- `Settings.loadReadOnly({cwd})` reads `config.yml` and project settings without opening `agent.db` or writing (`config/settings.ts:743-745`, `1949-1961`). Checked 2026-10-13 from a separate Bun process importing his global install: `Settings.init({readOnly: true})` returned no model roles, `loadReadOnly()` returned them, and `completeSimple` answered on his `smol` role (import about 3 s once cached).
- `completeSimple(model, context, {apiKey, maxTokens, reasoning, disableReasoning, signal})` (`packages/ai/src/stream.ts:1559-1574`; options `packages/ai/src/types.ts:765-783`; `Context` `types.ts:1634-1640`).

```ts
import { discoverAuthStorage, ModelRegistry, Settings } from "@oh-my-pi/pi-coding-agent";
import { resolveRoleSelection } from "@oh-my-pi/pi-coding-agent/config/model-resolver";
import { completeSimple } from "@oh-my-pi/pi-ai";

const settings = await Settings.loadReadOnly();                       // ~/.omp/agent/config.yml
const authStorage = await discoverAuthStorage(undefined, { settings }); // his /login credentials
const registry = new ModelRegistry(authStorage, undefined, { settings });
await registry.refresh("online-if-uncached");

// explicit id, or an omp role ("default", "smol") from his modelRoles
const model =
  registry.find("anthropic", "<model-id>") ??
  resolveRoleSelection(["default"], settings, registry.getAvailable())?.model;
if (!model) throw new Error("model not available");

const apiKey = await registry.getApiKey(model);
const reply = await completeSimple(
  model,
  { systemPrompt: ["You extract entities."], messages: [{ role: "user", content: "…", timestamp: Date.now() }] },
  { apiKey, maxTokens: 2048 },
);
const text = reply.content.filter(c => c.type === "text").map(c => c.text).join("");
// reply.usage / reply.stopReason / reply.errorMessage as in §1
```

[INFERENCE] Three caveats, none verified:
- The package is pinned in Jocasta's `package.json`, independently of his daily omp install.
- It pulls `@oh-my-pi/pi-natives`.
- `agent.db` is shared with running omp processes, as several omp processes already share it.

### 9. herdr: omp integration, states, socket, env, NDJSON, attach

**`herdr integration install omp`** writes **one file**: `herdr-omp-agent-state.ts` into omp's extensions dir (`herdr:src/integration/mod.rs:39-41`; `herdr:src/integration/targets.rs:89-106`).
- Dir resolution: `PI_CODING_AGENT_DIR/extensions` if set, else `~/<PI_CONFIG_DIR or .omp>/agent/extensions` (`herdr:src/integration/env.rs:42-56`).
- The dir must exist or its parent must exist (`targets.rs:67-78`). Install fails if the Pi and omp dirs coincide (`targets.rs:91-97`), and it removes a legacy Pi extension found in that dir (`targets.rs:100`).
- omp auto-loads user extensions from `~/.omp/agent/extensions` (`docs/extension-loading.md:39`).
- Integration version 10 (`herdr:src/integration/assets/omp/herdr-agent-state.ts:4-5`).

**When the extension is active.** It needs `HERDR_ENV=1`, `HERDR_SOCKET_PATH` and `HERDR_PANE_ID`. It stays off in nested omp (`OMPCODE=1`) (`herdr-agent-state.ts:11-24`) and only claims sessions with a UI (`ctx.hasUI === true`, `herdr-agent-state.ts:330-338`). `omp -p` therefore reports nothing.

**What it reports.**
- `pane.report_agent` with `state` ∈ `working | blocked | idle` (`herdr-agent-state.ts:71`, `167-180`).
- `pane.report_agent_session` with `agent_session_path` (the omp session file) or `agent_session_id` (`herdr-agent-state.ts:137-165`). Exposed as `agent_session` on `pane get/list` and `agent get/list` (`herdr:docs/next/website/src/content/docs/cli-reference.mdx:303`).

**Agent states:**

| State | Trigger | Source |
|---|---|---|
| working | `agent_start` | `herdr-agent-state.ts:392-402` |
| blocked | `tool_approval_requested` (until `…_resolved`), the `ask` tool running, the `herdr:blocked` event, or a retryable provider error still failing after a 2.5 s grace (`HERDR_OMP_RETRY_GRACE_MS`) | `herdr-agent-state.ts:404-437`, `363-373`, `314-328`, `80-82` |
| idle | 250 ms (`HERDR_OMP_IDLE_DEBOUNCE_MS`) after `agent_end`, unless `willContinue` | `herdr-agent-state.ts:439-464`, `79` |
| **done** | Not sent by omp. herdr derives it: an idle completion while the pane is not in the focused active tab sets `seen=false`, and idle+unseen reports `done`; idle+seen reports `idle` | `herdr:src/app/actions.rs:1844-1848`; `herdr:src/app/api_helpers.rs:96-106`; enum `herdr:src/api/schema/common.rs:158-166` |

F4 must therefore treat `working → idle` as completion when the user is watching the tab.

**Socket on Windows for session `jocasta`.**
- Path string: `%APPDATA%\herdr\sessions\jocasta\herdr.sock`. herdr's config dir on Windows is `%APPDATA%\herdr` (`XDG_CONFIG_HOME` overrides) (`herdr:src/config/io.rs:30-59`); named sessions use `<config dir>/sessions/<name>/herdr.sock` (`herdr:src/session.rs:163-173`).
- Named pipe: `\\.\pipe\` + that full path string. herdr binds the path as a namespaced pipe name and writes a marker file at the path (`herdr:src/ipc.rs:44-51`, `66-79`).
- Bun's `node:net` works with it: the omp extension connects the same way from inside omp (`herdr-agent-state.ts:13-14`, `46`).
- `HERDR_CONFIG_PATH` moves only the config file, not the socket (`herdr:src/config.rs:56`; `herdr:src/config/io.rs:195-200`).
- Resolution order: `--session` → `HERDR_SOCKET_PATH` → `HERDR_SESSION` → default (`herdr:docs/next/website/src/content/docs/socket-api.mdx:632-650`).

**Pane env.** Each managed pane gets `HERDR_ENV=1`, `HERDR_SOCKET_PATH`, `HERDR_BIN_PATH`, `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID`, `HERDR_PANE_ID` (`herdr:src/pane.rs:202-215`; `herdr:src/integration/env.rs:28-33`). Plugin popups do not get `HERDR_PANE_ID` (`socket-api.mdx:599-601`).

**NDJSON** (`socket-api.mdx:608-630`, `868-940`). One request per line `{"id","method","params"}`; the response echoes `id` with `result` or `error{code,message}`:

```jsonl
{"id":"req_1","method":"ping","params":{}}
{"id":"req_1","result":{"type":"pong"}}
{"id":"req_2","method":"pane.get","params":{"pane_id":"w9:p9"}}
{"id":"req_2","error":{"code":"not_found","message":"pane not found"}}
```

**Subscriptions.** `events.subscribe` returns `{"id","result":{"type":"subscription_started"}}`, then pushes bare `{"event":…,"data":…}` lines on the same connection (`herdr:src/api/server.rs:948-990`; `herdr:src/api/schema/events.rs:362-389`).
- `pane.agent_status_changed` **requires a `pane_id`** per subscription entry, so it is per pane, not global (`events.rs:75-80`).
- Its data: `{pane_id, workspace_id, agent_status, agent?, title?, display_agent?, state_labels?}` (`events.rs:398-411`).
- Overruns end the stream with `events_lost` (`socket-api.mdx:796-810`).

**Attaching a client to a named session** (`herdr:src/cli/spec.rs:14`; `herdr:src/session.rs:31-83`; `cli-reference.mdx:26`, `145-148`):
- `herdr --session jocasta` launches or attaches;
- `herdr session attach jocasta` attaches;
- `HERDR_SESSION=jocasta herdr` also selects the session (`herdr:src/session.rs:10`).

Session management: `herdr session list|stop|delete` (`herdr:src/cli.rs:425-445`).

## Default network destinations and their off switches

Scope: every host omp 18.10.3 contacts, outside the model provider doing the work, either by default or the first time a feature is used. Paths are relative to omp-src. If the off switch column says **not switchable**, no setting, env var or flag turns that call off.

### Fires without being asked (startup or background)

| what | destination (host) | when it fires | default on? | off switch | source |
|---|---|---|---|---|---|
| Update check (npm/bun install, which is his) | `registry.npmjs.org` (or the registry named in his npm/bun config) | interactive launch only, 5 s timeout; skipped for source checkouts and installs managed by another tool; never in `-p`/`--mode json`/RPC | yes | `startup.checkUpdate: false`; `update.channel` only picks stable or canary | `packages/coding-agent/src/main.ts:239-257`, `:2604-2605`; `packages/coding-agent/src/modes/settings.ts:1097-1100`, `:1109-1113`; `packages/coding-agent/src/cli/update-cli.ts:786-806`; `packages/coding-agent/src/cli/npm-registry.ts:24` |
| Update check (standalone compiled binary) | `build.stencil.so`; requests carry `User-Agent: omp/<version>`, which the service counts in its update-check statistics | same as above | yes, for compiled binaries | `startup.checkUpdate: false`; env `PI_BUILD_URL` points it at another host | `packages/coding-agent/src/main.ts:250-252`; `packages/coding-agent/src/cli/build-service.ts:13-17`, `:44`, `:51-54` |
| Changelog on startup | none: reads the installed package's `CHANGELOG.md` | interactive launch, not on `--continue`/`--resume` | n/a (local file) | `startup.changelogMode: hidden` hides it | `packages/coding-agent/src/main.ts:1151-1163`; `packages/coding-agent/src/config.ts:71-73` |
| Shared model catalog | `catalog.stencil.so/models.json.zstd` (conditional GET with ETag) | background runtime model discovery on every launch: right away in `-p`/headless, after the first frame in the TUI; also for catalog-only providers that have no login | yes | **not switchable** globally; `disabledProviders` (config file only) drops one provider from discovery | `packages/catalog/src/provider-models/openai-compat.ts:85`, `:233-245`; `packages/coding-agent/src/config/model-registry.ts:2393-2421`, `:2434-2452`; `packages/coding-agent/src/sdk.ts:2766-2782` |
| Provider model lists (`/models` etc.) | each logged-in provider's own host | same discovery pass, per authenticated provider; results cached in SQLite, 24 h freshness | yes | **not switchable** globally; `disabledProviders` / `enabledProviders` (config file only) per provider | `packages/coding-agent/src/config/model-registry.ts:1348`, `:2385-2421` |
| Plugin marketplace refresh and update check | the source URL of each marketplace he has added (none are built in) | every launch, all modes; catalogs older than 24 h are refreshed; `auto` also upgrades plugins | yes (`notify`), but with no marketplaces added nothing is contacted | `marketplace.autoUpdate: off` | `packages/coding-agent/src/main.ts:2231-2235`; `packages/coding-agent/src/extensibility/plugins/marketplace-auto-update.ts:11-17`, `:37-45`; `packages/coding-agent/src/extensibility/plugins/marketplace/manager.ts:743-747`; `packages/coding-agent/src/modes/settings.ts:1126-1130` |
| OTLP traces, logs and metrics | whatever endpoint `OTEL_EXPORTER_OTLP_*_ENDPOINT` / `OTEL_EXPORTER_OTLP_ENDPOINT` names; no built-in host | every launch, but only if one of those env vars is set (http/protobuf only) | setting on; nothing sent without an endpoint env var | `telemetry.otlpExportEnabled: false` (takes effect next launch); env `OTEL_SDK_DISABLED=true`; or leave `OTEL_*` endpoints unset | `packages/coding-agent/src/telemetry-settings.ts:8-11`; `packages/coding-agent/src/main.ts:2259-2268`; `packages/coding-agent/src/telemetry-export.ts:82-85`, `:105-124` |
| Auto QA tool-issue reports | `qa.omp.sh/v1/grievances`; each row carries the omp version, `installId` and OS/arch | after the model writes to `xd://report_issue`; the first report asks for consent (TUI popup); pushes only after "granted" or with env `PI_AUTO_QA_PUSH=1` | yes (the instruction is injected into the system prompt and the device is mounted); push needs consent | `dev.autoqa: false` or env `PI_AUTO_QA=0`; answering No sets `dev.autoqaConsent: denied`; env `PI_AUTO_QA_PUSH_URL` / setting `dev.autoqaPush.endpoint` changes the host | `packages/coding-agent/src/tools/report-tool-issue.ts:8-31`, `:379-386`, `:441-442`; `packages/coding-agent/src/tools/settings.ts:961-965`, `:977-983` |
| Install id sent to Claude | Anthropic (the provider), as `metadata.user_id.device_id`, derived from omp's install id | every Claude request on an Anthropic login | yes | **not switchable** | `packages/coding-agent/src/session/session-metadata.ts:1-2`, `:45` |

### Fires on a feature's first use (default-enabled features)

| what | destination (host) | when it fires | default on? | off switch | source |
|---|---|---|---|---|---|
| `web_search` keyless chain | `search.parallel.ai/mcp` first (keyless MCP; `api.parallel.ai` with a key), then the session's own provider (`web/hosted`), `mcp.exa.ai/mcp` (keyless; `api.exa.ai` with a key), Firecrawl, SearXNG, Startpage, DuckDuckGo, Ecosia, Google, Mojeek | each time the model calls `web_search`; walks the chain until one answers | yes | `web_search.enabled: false`; or pin `modelRoles.web` (config file) to one engine; `exa.enabled: false` drops Exa; `searxng.endpoint` / env `SEARXNG_ENDPOINT` for his own instance | `packages/coding-agent/src/priority.json:82-94`; `packages/coding-agent/src/web/search/providers/parallel.ts:23`, `:43`; `packages/coding-agent/src/web/search/providers/exa.ts:23-25`; `packages/coding-agent/src/web/search/index.ts:136-139`, `:175`; `packages/coding-agent/src/tools/settings.ts:775-778`; `packages/coding-agent/src/web/settings.ts:8-11` |
| `read` of a URL: the URL itself, plus site handlers (npm, Hacker News, Hugging Face, Sourcegraph, tldr on GitHub, …) | the site, plus each handler's API host | each time the model reads an `http(s)` URL | yes | `fetch.enabled: false` | `packages/coding-agent/src/tools/settings.ts:700-703`; `packages/coding-agent/src/web/scrapers/npm.ts:31-32`; `packages/coding-agent/src/web/scrapers/hackernews.ts:27`; `packages/coding-agent/src/web/scrapers/sourcegraph.ts:5`; `packages/coding-agent/src/web/scrapers/tldr.ts:4` |
| `read` URL fallback reader: Jina | `r.jina.ai` (keyless) | when the in-process HTML converter, trafilatura and lynx all fail the quality gate; Parallel and Firecrawl are skipped without keys | yes | **not switchable** short of `fetch.enabled: false`: `providers.fetch` only moves one backend to the front, and the rest still run as fallbacks | `packages/coding-agent/src/tools/fetch.ts:569-576`, `:631-666`, `:669-673`; `packages/coding-agent/src/session/settings.ts:1271-1275` |
| `read` URL fallback reader: trafilatura auto-install | PyPI via `uv` or `pip install --user trafilatura` | first time the in-process converter fails on a page and trafilatura isn't on PATH | yes | **not switchable** short of `fetch.enabled: false`; installing trafilatura himself avoids the install | `packages/coding-agent/src/tools/fetch.ts:620-625`; `packages/coding-agent/src/utils/tools-manager.ts:144-148`, `:328-346`, `:395-402` |
| YouTube URL handler: yt-dlp auto-download | `api.github.com/repos/yt-dlp/yt-dlp/releases/latest`, then `github.com/.../releases/download/...` | first `read` of a YouTube URL without yt-dlp on PATH | yes | **not switchable** short of `fetch.enabled: false` | `packages/coding-agent/src/web/scrapers/youtube.ts:150-151`; `packages/coding-agent/src/utils/tools-manager.ts:119-120`, `:179`, `:266` |
| tree-sitter WebAssembly grammars | `github.com/stencil-hq/wasm-grammars/releases/download` | `ast_grep` / `ast_edit` hits a file in one of the 39 languages not linked into the native addon | yes | **not switchable**; env `PI_GRAMMARS_URL` points it at another host | `packages/coding-agent/src/utils/grammars.ts:8-20`, `:38-39`; `packages/coding-agent/src/tools/ast-grep.ts:231-232`; `packages/natives/CHANGELOG.md:82` |
| Chromium for the browser tool | `storage.googleapis.com/chrome-for-testing-public` | first browser use, only if no system Chrome/Edge is found (on Windows the system browser is tried first) | yes | `browser.enabled: false`; env `PUPPETEER_EXECUTABLE_PATH` | `packages/coding-agent/src/tools/browser/launch.ts:249-283`; `packages/utils/src/browsers.ts:8`; `packages/coding-agent/src/tools/browser/settings.ts:7-10` |
| JS eval package installs | npm registry via Bun | when an eval cell asks for packages | yes | `eval.js: false` (env `PI_JS`); `eval.autoProvision: false` stops the environment being created on its own | `packages/coding-agent/src/eval/js/executor.ts:123-145`; `packages/coding-agent/src/eval/settings.ts:21-44` |
| `omp git` commit-author avatars | `www.gravatar.com` (MD5 of each author email), `avatars.githubusercontent.com`, `api.github.com` commits API | opening the `omp git` TUI | yes, inside that command | **not switchable** (don't use `omp git`) | `packages/coding-agent/src/cli/git-tui/avatar.ts:4-6`, `:28-30`, `:53-54`, `:116`; `packages/coding-agent/src/cli/command-help.ts:76-77` |

### Off by default, or only on an explicit command

| what | destination (host) | when it fires | default on? | off switch | source |
|---|---|---|---|---|---|
| `/collab` live session relay | `wss://my.omp.sh` (relay), `my.omp.sh` (browser UI) | `/collab`; or every interactive session when `collab.autoStart` is `view`/`control` | no (`collab.autoStart: off`) | keep `collab.autoStart: off`; `collab.relayUrl` for his own relay | `packages/wire/src/index.ts:425-426`; `packages/coding-agent/src/collab/settings.ts:9-12`, `:46-50`; `packages/coding-agent/src/slash-commands/builtin-collaboration.ts:437` |
| `/share` session snapshot | `my.omp.sh/s` (encrypted blob), or a GitHub gist with `share.store: gist` | `/share` or `omp share` only | no | **not switchable** (manual only); `share.serverUrl` changes the host; `share.redactSecrets` default true | `packages/wire/src/index.ts:428-429`; `packages/coding-agent/src/commands/settings.ts:8-25`; `packages/coding-agent/src/commands/share.ts:5-7` |
| `omp stream` / `omp clip` | `live.omp.sh` (needs a Stencil account) | those commands only | no | **not switchable** (manual only); `stream.serverUrl` / `--server` | `packages/wire/src/stream.ts:16-17`; `packages/coding-agent/src/stream/settings.ts:11-14`; `packages/coding-agent/src/cli/command-help.ts:45-46` |
| Stencil account login | `api.stencil.so`, `auth.stencil.so` | `/login` → Stencil | no | **not switchable** (manual only) | `packages/catalog/src/compat/rules/auth/stencil.kdl:1-5` |
| Skill registry | `skills.omp.sh` | `omp skill …` and `/skills …` only | no | **not switchable** (manual only); `skills.registryUrl` / `--registry` | `packages/wire/src/skillshare.ts:17-18`; `packages/coding-agent/src/extensibility/settings.ts:15-18`; `packages/coding-agent/src/slash-commands/builtin-skills.ts:39-40`; `packages/coding-agent/src/cli/skill-cli.ts:127` |
| Outgoing images as public URLs | default backend chain: provider file APIs, `tailscale funnel` (`*.ts.net`), `cloudflared` (`*.trycloudflare.com`), Litterbox (`litterbox.catbox.moe`, anonymous public host) | when an image is sent to a URL-fetching provider and the feature is on | no (`images.urls.enabled: false`) | keep `images.urls.enabled: false`; `images.urls.backends` sets the chain | `packages/coding-agent/src/blob-broker/settings.ts:8`, `:25-28`, `:38-41`; `packages/coding-agent/src/blob-broker/uploaders-anonymous.ts:14`; `packages/coding-agent/src/blob-broker/exposure.ts:82-84`, `:99-101` |
| Auth broker | `OMP_AUTH_BROKER_URL` / `auth.broker.url` (no default host) | every credential lookup, once set | no | leave `auth.broker.url` unset (config file only) | `packages/coding-agent/src/config/model-settings.ts:42-46`; `packages/coding-agent/src/session/auth-broker-config.ts:10-13` |
| SmolLM word-completion weights (145 MB) | `huggingface.co` | first completion with `spelling.autocomplete: smollm`; the default `auto` resolves to the local n-gram engine and downloads nothing | no | keep `spelling.autocomplete` at `auto`, `ngram` or `off` | `packages/coding-agent/src/modes/settings.ts:968-972`, `:989-991`; `packages/coding-agent/src/predict/client.ts:48-50`, `:228-230`; `packages/coding-agent/src/predict/smollm-weights.ts:26-42` |
| Local tiny models (titles, memory) | `huggingface.co` (transformers.js hub, `onnx-community/*`), plus the runtime via `bun install` from the npm registry | only when the `tiny` role points at a local model; `title.generator` defaults to `fork` and `priority.json` has no `tiny` list | no | leave the `tiny` role unset or on a provider model; env `PI_NO_TITLE` | `packages/coding-agent/src/tiny/models.ts:31-32`; `packages/coding-agent/src/subprocess/worker-runtime.ts:413-415`; `packages/coding-agent/src/utils/title-settings.ts:34-38`; `packages/coding-agent/src/modes/controllers/input-controller.ts:258-270`; `packages/coding-agent/src/priority.json:1-110`; `packages/utils/src/runtime-install.ts:419-423` |
| Mnemopi local embedding model | `huggingface.co` via fastembed (`BAAI/bge-base-en-v1.5`, or `intfloat/multilingual-e5-large`), plus the fastembed runtime via `bun install` | first recall/retain after `memory.backend: mnemopi` | no (`memory.backend: off`) | `mnemopi.noEmbeddings: true` (FTS-only recall); `mnemopi.embeddingApiUrl` sends embeddings to an endpoint he names | `packages/coding-agent/src/memory-backend/settings.ts:12-16`; `packages/coding-agent/src/mnemopi/config.ts:80-87`; `packages/coding-agent/src/mnemopi/embed-client.ts:68-72`, `:226-230`; `packages/mnemopi/src/core/fastembed-model-cache.ts:4-7`; `packages/coding-agent/src/mnemopi/settings.ts:170-173` |
| Hindsight memory | `hindsight.apiUrl`, default `http://localhost:8888` | when `memory.backend: hindsight` | no | `memory.backend` not `hindsight` | `packages/coding-agent/src/hindsight/settings.ts:16-20` |
| Dictation model (Parakeet / Whisper) | `huggingface.co` | first dictation after `stt.enabled: true` | no | keep `stt.enabled: false` | `packages/coding-agent/src/stt/settings.ts:9-12`; `packages/coding-agent/src/stt/asr-worker.ts:49-50`; `packages/coding-agent/src/stt/models.ts:60-61`; `packages/coding-agent/src/priority.json:100-101` |
| Speech model (Kokoro) | `huggingface.co` (`onnx-community/Kokoro-82M-v1.0-ONNX`) | first `speech.enabled` output or `tts` tool call | no | keep `speech.enabled: false` and `speechgen.enabled: false` | `packages/coding-agent/src/tts/settings.ts:41-44`; `packages/coding-agent/src/tools/settings.ts:569-572`; `packages/coding-agent/src/tts/models.ts:68-69`; `packages/coding-agent/src/priority.json:95-96` |

Not found in omp-src: any analytics SDK (PostHog/Sentry-style). The only places `posthog` appears are in comments. Stats (`omp stats`) are a local SQLite dashboard with no upload (`packages/stats/src/live.ts:1-10`).

### Recommended off for Jocasta

The plan's non-goal "No telemetry: Jocasta reports nothing about its use to anyone" (`docs/plan.md:672`) and principle 6, "Nothing that names a client leaves the machine, except in requests to the model provider doing the work" (`docs/plan.md:661`), lead to these values for `config/omp-config.yml`:

- `dev.autoqa: false`: tool-issue reports to `qa.omp.sh` carry the install id and free-text report bodies that can name client code.
- `telemetry.otlpExportEnabled: false`: closes the OTLP path even if an `OTEL_*` endpoint env var leaks in from another tool.
- `startup.checkUpdate: false`: a version ping to npm or `build.stencil.so` on every interactive launch. omp bumps are deliberate (plan, "Versions are those verified today").
- `marketplace.autoUpdate: off`: no background plugin fetches or upgrades.
- `collab.autoStart: off` (default), `images.urls.enabled: false` (default), `auth.broker.url` unset (default): leave these as they are, and write the first two out explicitly so a later default change can't turn them on.
- `fetch.enabled` and `web_search.enabled`: keep both on, since threads need them. The query and URL text reaches third-party search and reader hosts (Parallel, Exa, Jina, …), so whether a thread may search the web for a client name is his call in the walkthrough. The narrowest switch is pinning `modelRoles.web` to the session provider (`web/hosted`).
- Leave `spelling.autocomplete: auto`, `stt.enabled: false`, `speech.enabled: false` and an unset `tiny` role so no model weights download. With `memory.backend: mnemopi` (0389), the first use downloads one embedding model from `huggingface.co` and installs fastembed with `bun install`. That is a one-time install he approves, or he avoids it with `mnemopi.noEmbeddings: true`.

Not switchable, so record them as known egress: the `catalog.stencil.so` model catalog and provider model lists on every launch, the install-id-derived Claude device id, the `r.jina.ai` / trafilatura / yt-dlp read fallbacks, grammar downloads, and `omp git` avatars (Gravatar gets an MD5 of each author email).

## omp 18.10.3 → 18.11.0 (re-checked 2026-10-15)

Method: `npm pack @oh-my-pi/pi-coding-agent@18.10.3 @oh-my-pi/pi-coding-agent@18.11.0` and `@oh-my-pi/pi-ai` at the version each pins exactly (18.10.3 and 18.11.0), extracted and compared file by file (nothing installed; his omp 18.10.4 sits between the two, so 18.10.4's changes are included). 173 files differ, 12 added, 7 removed. Paths below are relative to the 18.11.0 package (`src/…` = omp-src `packages/coding-agent/src/…`; `pi-ai:` = `@oh-my-pi/pi-ai`); `CHANGELOG.md:N` is the package's changelog. No `http(s)://` host appears in 18.11.0's `src/**/*.ts` that 18.10.3 didn't have.

### Seams (plan §5.5's omp list)

- **CLI flags: unchanged.** `src/cli/flag-tables.ts` and `src/cli/args.ts` are byte-identical. The flags Jocasta passes keep their lines: `--config` :122, `--mode` :128, `--model` :143, `--system-prompt` :179, `--append-system-prompt` :185, `--continue` :312/:390, `--no-session`/`--no-tools` :315-316, `--no-extensions`/`--no-skills`/`--no-rules` :327-329.
- **`--append-system-prompt` and `APPEND_SYSTEM.md`: unchanged.** The file-or-text rule is untouched (`src/system-prompt.ts:284-300`); the file only lost Code Mode's `directToolNames`. The boss manual's discovery moved one line: `src/main.ts:1332-1342`, used at :1397 (was :1331-1341, :1396); `src/config.ts` (`findConfigFile`) is identical.
- **Print/JSON mode: unchanged.** `src/modes/print-mode.ts` is identical, so `llm-endpoint.ts`'s `omp -p --mode json` fallback still reads the last assistant `message_end`.
- **Session layout and the entries the watcher and `sessions.ts` read: shapes unchanged, one new rewrite.**
  - `src/session/session-paths.ts` and `src/session/compaction-methods.ts` are identical; `CURRENT_SESSION_VERSION` stays 3 (`src/session/session-entries.ts:16`). Title slot :27, `session` header :38-39, `message` :86-87, `compaction` :133-142 (`method?` :142), `mode_change` :286-287 keep their fields. `/handoff` still writes `method: "handoff"` (`src/session/session-maintenance.ts:2088`, was :2068). Assistant messages are still persisted once at `message_end` (`src/session/agent-session.ts:3950`).
  - Additive only: every entry this process writes carries `runtime` (`session-entries.ts:83`; `{version}` for installed builds, commit/branch/diff hash only from a source checkout, `src/session/runtime-identity.ts:14-28`; CHANGELOG.md:13), and `model_usage` (:92) gains `activeLeafId` (:104). Jocasta ignores unknown fields.
  - **Jocasta must handle:** 18.10.4 (CHANGELOG.md:79) adds a session-file rewrite that *grows* the file: when a snapcompact archive that carries reasoning is read by a model whose classifier refuses quoted reasoning (Claude), omp renders a no-reasoning view, stores it in the compaction entry's `preserveData` and calls `rewriteEntries()` (`src/session/session-maintenance.ts:4590-4597`), which replaces the file atomically (`src/session/session-manager.ts:3584-3588`). `src/frontier/server/sessions.ts` `advance`/`warm` restart from byte 0 only when `size < offset`, so after a growing rewrite they re-read the entries past the old offset and add their cost and `messageTimes` a second time. The fix: also restart from 0 when the file's identity changed (`statSync` `ino`, or `birthtimeMs`, differs from the tail's). `watcher.ts` is safe (it may re-read entries, but `INSERT OR IGNORE` on `(session_file, entry_id)` drops duplicates). Reachable for him when a session snapcompacted under a non-Claude model moves to Claude (`compaction.methodOrder` lists `snapcompact`).
  - `gc.compress` (below) is transparent to Jocasta: on NTFS it sets LZNT1 compression in place with `compact /c`, keeping path, bytes and mtime (`src/cli/gc-compress.ts:1-12`, :241).
- **Plan mode and `xd://propose`: unchanged for Jocasta.** `src/modes/controllers/event-controller.ts` (the propose dispatch, :2196-2235), `src/tools/resolve.ts` and `src/tools/xdev.ts` (the `details.xdev` shape) are identical, so `sessions.ts`'s `planAwaiting` rule holds. Approving still appends `mode_change` `none` (or `plan_paused`) at `src/modes/interactive-mode.ts:5183` (was :5146); the brand-new-session check is :623. Behavior change: `/new` (and its key binding) now applies `plan.defaultOnStartup` (`interactive-mode.ts:7963-7968`, via :4958; CHANGELOG.md:46), so in a pane whose `--config` overlay turns plan-on-start on, `/new` starts in plan mode, as launch already did. Additive extension events `plan_review_requested`/`plan_review_resolved` (`src/extensibility/extensions/types.ts:1270`, :1285) could signal "plan ready" directly; Jocasta doesn't need them.
- **Approvals: resolver unchanged, prompt UI changed.** `resolveApproval` (`src/tools/approval.ts:204`) is untouched; only the prompt's rendering is new (`requestApproval` :414, a one-row summary for the composer). `src/tools/settings.ts` and `src/exec/settings.ts` (`tools.approvalMode`, `tools.approval`, `bash.patterns`, `bash.gitGuard`) are identical. What he sees with `config/omp-config.yml`'s `prompt` rules: the approval replaces the composer and approves with `Ctrl+Enter` (or `Ctrl+Q`), not `Enter` (18.10.4, CHANGELOG.md:73); sending a message while it is pending denies the call and steers the turn with that message (CHANGELOG.md:12).
- **Memory: unchanged for mnemopi.** `src/memory-backend/resolve.ts`, `src/memory-backend/settings.ts` and `src/mnemopi/settings.ts` are identical. `buildDeveloperInstructions` gains the session's `cwd` (`src/memory-backend/types.ts:125`); mnemopi ignores it (`src/mnemopi/backend.ts:139`); the `local` backend now scopes to the session's project directory instead of the settings directory.
- **MCP discovery: file list unchanged.** Still `<cwd>/.omp/mcp.json`, `<cwd>/.omp/.mcp.json`, then `~/.omp/agent/mcp.json` and `.mcp.json` (`src/discovery/builtin.ts:238-241`, was :222-227). The project `.omp` walk now stops at the home and temp-dir ceilings and skips foreign `.omp/agent` layouts (`builtin.ts:81`, :101, :116; `src/discovery/helpers.ts:1037-1069`; CHANGELOG.md:26); the boss's `<data>/home/.omp` is its cwd (depth 0, no `.omp/agent` inside), so it still loads, also under `%TEMP%` for the V3 walkthrough (the ceiling is `%TEMP%` itself). `## MCP Server Instructions` is now ordered by server name (`src/mcp/manager.ts:2004`).
- **The SDK seam `llm-endpoint.ts` imports: unchanged.** `src/index.ts` is identical (`Settings` :18, `export * from "./config/model-registry"` :12, `export * from "./sdk"` :40); `src/config/model-resolver.ts` (`resolveRoleSelection` :1768), `src/config/settings.ts` (`Settings.loadReadOnly` :743) and `src/config/model-registry.ts` (`ModelRegistry` :300, constructor :517, `refresh` :553) are identical; `discoverAuthStorage(agentDir?, options)` keeps its signature (`src/sdk.ts:1020`, was :1015). pi-ai 18.11.0 (pinned exactly in omp's `package.json`): `src/index.ts` identical (`export * from "./stream"` :44) and `completeSimple(model, context, options)` identical (`pi-ai:src/stream.ts:1559-1574`); the only option removed is Code Mode's `toolNamespacesInfo` (`pi-ai:src/types.ts`), which Jocasta doesn't pass.
- **`config/omp-config.yml`: every key still registered with the same type and default.** `bash.gitGuard` `src/exec/settings.ts:120`, `bash.patterns` :133, `theme.dark` `src/modes/settings.ts:59`, `memory.backend` `src/memory-backend/settings.ts:12`, `compaction.methodOrder` `src/session/context-settings.ts:143`, `compaction.thresholdPercent` :158, `compaction.idleEnabled` :303, `compaction.idleThresholdTokens` :315, `telemetry.otlpExportEnabled` `src/telemetry-settings.ts:8`, `dev.autoqa` `src/tools/settings.ts:961`, `dev.autoqaConsent` :1007, `startup.quiet` `src/modes/settings.ts:1060`, `startup.checkUpdate` :1097, `marketplace.autoUpdate` :1126, `recap.enabled` :1269, `paste.largeMenuThreshold` :1040, `providers.cacheWarming` `src/session/settings.ts:1138`, `lsp.diagnosticsOnEdit` `src/lsp/settings.ts:70`. The files declaring them are identical except `context-settings.ts` and `session/settings.ts`, whose changes are listed below.

### Settings added, removed or with a changed default

The settings registry (`src/config/registry.ts`, `src/config/all-settings.ts`) is identical; comparing every `register({ id: … })` site in both packages gives two added and two removed ids, and no changed default or tab. Description-only changes: `retry.modelFallback` (`src/session/settings.ts:690`) and `contextPromotion.enabled` (`src/session/context-settings.ts:77`) now say they are independent (CHANGELOG.md:18); `task.structured` (`src/task/settings.ts:256`) notes the new agent frontmatter `structured: true`. Nothing here sends data off the machine (no ⚠).

| id | tab | default | what it does | recommendation |
|---|---|---|---|---|
| `gc.compress` (added, 18.10.4) | none (config file only) | `false` | `omp gc` also compresses session files idle for `gc.compressAfterDays` with the filesystem's transparent compression (NTFS: `compact /c /i /a /q`, `src/cli/gc-compress.ts:241`); files stay resumable and searchable (`src/cli/gc-settings.ts:29`) | default; his call (disk space only). Jocasta's session readers are unaffected |
| `gc.compressAfterDays` (added, 18.10.4) | none (config file only) | `7` | Idle days before `gc.compress` takes a session file (`src/cli/gc-settings.ts:31`) | default |
| `providers.openai-codex.codeMode` (removed, 18.10.4) | providers | `"off"` | Codex Code Mode is gone; Codex models always get the full direct tool surface (CHANGELOG.md:85; was 18.10.3 `src/session/settings.ts:142`) | nothing to do: `config/omp-config.yml` never set it; its row in the `providers` table below describes 18.10.3 |
| `providers.openai-codex.codeModeDirectTools` (removed, 18.10.4) | providers | `[]` | Removed with Code Mode (CHANGELOG.md:85; was 18.10.3 `src/session/settings.ts:156`) | nothing to do |

## omp 18.11.0 → 18.12.6 (re-checked 2026-10-23)

Method: `npm pack @oh-my-pi/pi-coding-agent@18.11.0 @oh-my-pi/pi-ai@18.11.0 @oh-my-pi/pi-coding-agent@18.12.6` into a temp folder, extracted and compared file by file; his installed 18.12.6 (`~/.bun/install/global/node_modules/@oh-my-pi/pi-coding-agent`) is byte-identical to the packed one, and its pi-ai is 18.12.6 (pinned exactly). 343 files differ, 42 added, 3 removed. Paths below are relative to the 18.12.6 package (`src/…`; `pi-ai:` = `@oh-my-pi/pi-ai` 18.12.6); `CHANGELOG.md:N` is the 18.12.6 package's changelog. The list is plan §5.5's, updated by 0463: the SDK seam is gone and `omp auth-gateway stdio` takes its place.

### Seams (plan §5.5's omp list)

- **CLI flags: unchanged for Jocasta.** `src/cli/flag-tables.ts` keeps `--config` :122, `--append-system-prompt` :185 and `--continue` :314/:392 (was :312/:390). `--approval-mode` (:241) gains `always-ask-cached` (below); `src/cli/args.ts` only retypes `approvalMode` to `ApprovalMode`. `omp usage --json` (`src/commands/usage.ts`, the HUD's quota, 0456) is identical.
- **`--append-system-prompt` and `APPEND_SYSTEM.md`: unchanged.** `src/system-prompt.ts` and `src/config.ts` are identical; the boss manual's discovery moved: `src/main.ts:1355-1365`, used at :1420 (was :1332-1342, :1397). `--config` overlays still feed `Settings.init({ cwd, configFiles })` (`src/main.ts:1910`).
- **Print/JSON mode (the evals runner): event stream unchanged, one behavior added.** Every agent event is still one `printableEvent` line (`src/modes/print-mode.ts:347`, was :235). New: `/compact`, `/handoff`, `/retry` and `/mcp reauth` given as CLI arguments now run as commands (`print-mode.ts:287`; CHANGELOG.md:19); slash text on stdin still goes to the model. No eval case prompt starts with a slash, so `evals/runner.ts` is unaffected. `llm-endpoint.ts` no longer uses `omp -p` (0463).
- **Session layout and the entries the watcher and `sessions.ts` read: shapes unchanged, two additive fields.**
  - `CURRENT_SESSION_VERSION` stays 3 (`src/session/session-entries.ts:16`); `compaction` :145, `model_usage` :106 and `mode_change` :298 keep their fields (were :134, :93, :287). `src/session/compaction-methods.ts` only adds Bedrock to `remote`'s description. `/handoff` still writes `method: "handoff"` (`src/session/session-maintenance.ts:2114`, was :2088). Assistant messages are still persisted once at `message_end` (`src/session/agent-session.ts:3743`, `#persistMessageEnd` :3967). `src/session/sub-sessions.ts` is identical.
  - Additive: `activeLeafId` moved from `model_usage` to every entry (`session-entries.ts:89-96`; an entry written off the active branch replays the leaf captured at append time, :347-348), and the `session` header gains `seanceSource` for read-only `/seance` forks (:63). Jocasta ignores unknown fields.
  - `sessionDirForCwd` is unchanged (`src/session/session-paths.ts:222`, was :215); it also migrates 17.2.5–17.2.8 hashed home-scoped folders into the current one (:87-92, :306-307), a one-time move of old folders.
  - A torn last record in a large session file is now repaired on load as small files were (CHANGELOG.md:363). [INFERENCE] The repair makes the file shorter, which `sessions.ts` already handles (restart when the size drops below its offset or the file's identity changes, 0435).
- **Plan mode and `xd://propose`: unchanged for Jocasta.** `src/tools/resolve.ts` is identical; the propose dispatch moved (`src/modes/controllers/event-controller.ts:2229`, was :2196) with no change to it; `src/tools/xdev.ts` only adds `readOnlyHint` for read-only MCP tools (:466), so `details.xdev`'s `{tool: "propose", mode: "execute", inner: {title, planFilePath}}` holds. Approving still appends `mode_change` `none` (`src/modes/interactive-mode.ts:5440`, was :5248); the brand-new-session check is :638.
- **Approvals: resolver unchanged for `yolo`, `write` and `always-ask`; one mode added.** `resolveApproval` (`src/tools/approval.ts:331`, was :204) takes an optional grant store and branches to `resolveCachedModeApproval` (:182) only for the new `always-ask-cached` (:436); the other modes resolve as before, and the prompt result now carries the matched `policyKey`. `tools.approvalMode` (`src/tools/settings.ts:314`) keeps default `yolo` and adds the value; "Ask and remember" also asks before file reads and can remember a file or folder in `approval-cache.db` (CHANGELOG.md:118). `bash.gitGuard` and `bash.patterns` (`src/exec/settings.ts:123`, :136) are unchanged. What he sees: the prompt lists Approve, Approve `<tool>` for this session, and Deny; typing in the composer denies and steers (CHANGELOG.md:309).
- **Memory: unchanged for mnemopi's settings.** `src/memory-backend/resolve.ts`, `memory-backend/settings.ts`, `memory-backend/types.ts` and `src/mnemopi/settings.ts` are identical. `src/mnemopi/backend.ts`: memory completions now honor the memory role's thinking suffix (CHANGELOG.md:82), `/tan` forks reuse the parent's recall, and explicit saves are stored as `stated`.
- **MCP discovery: file list unchanged.** `<cwd>/.omp/mcp.json`, `<cwd>/.omp/.mcp.json`, `~/.omp/agent/mcp.json`, `.mcp.json` (`src/discovery/builtin.ts:238-241`); `builtin.ts` only accepts symlinked hook files; `src/mcp/manager.ts` adds `updateServerConfig` for `/mcp reauth`.
- **`omp auth-gateway stdio` (new seam, 0463): unchanged since 18.11.0.** `src/cli/auth-gateway-stdio.ts` is identical (`runAuthGatewayStdio` :56, project dir from the process cwd :59, `selectorCandidates` :32 walking `retry.fallbackChains`, serving at :106) and so is `pi-ai:src/auth-gateway/stdio.ts` (the `{ready, version}` line :50, request frames :86-115, responses by `id` in completion order). The chat route parses `system`/`user` messages and `max_completion_tokens ?? max_tokens` (`pi-ai:src/providers/openai-chat-server.ts:87-201`) and drops `response_format` (`pi-ai:src/auth-gateway/server.ts:223-239`), so `llm-endpoint.ts` keeps its JSON instruction and `extractJson`; the reply's `model` is the routed `provider/id` (`openai-chat-server.ts:416-442`). pi-ai's gateway changes since 18.11.0: `server.ts` threads scoped `serve --clients` tokens through key resolution (CHANGELOG.md:338) and strips sampling options by the model's `supportsSamplingParams` instead of for Codex only; `openai-chat-server.ts` adds an SSE keepalive. Added in 18.4.12 (CHANGELOG.md:1287). Checked live 2026-10-23 with a request for a model that doesn't exist (no upstream call): the ready line `{"ready":true,"version":"18.12.6"}`, then `{"id":1,"status":404,…"No available model matches …"}`.
- **The SDK seam: removed from the list (0463).** `llm-endpoint.ts` no longer imports `src/index.ts`, `src/config/model-resolver.ts` (`resolveRoleSelection`) or pi-ai's `completeSimple`.
- **Jocasta's config keys: every key still registered with the same type and default.** From `config/omp-config.yml`, `config/boss.yml`, `config/guard.yml` and the per-project overlay (`overlayText()` in `service.ts`): `bash.gitGuard` `src/exec/settings.ts:123`, `bash.patterns` :136, `collab.autoStart` `src/collab/settings.ts:46`, `compaction.methodOrder` `src/session/context-settings.ts:143`, `compaction.thresholdTokens` :185, `compaction.idleEnabled` :303, `compaction.idleThresholdTokens` :315, `contextFiles.extra` :48, `dev.autoqa` `src/tools/settings.ts:1010`, `dev.autoqaConsent` :1056, `find.enabled` :528, `github.enabled` :761, `disabledProviders` `src/config/model-settings.ts:77`, `modelRoles` :109, `hideThinkingBlock` `src/session/settings.ts:210`, `providers.cacheWarming` :1179, `lsp.diagnosticsOnEdit` `src/lsp/settings.ts:70`, `marketplace.autoUpdate` `src/modes/settings.ts:1139`, `paste.largeMenuThreshold` :1053, `recap.enabled` :1282, `startup.quiet` :1073, `startup.setupWizard` :1098, `startup.checkUpdate` :1110, `startup.changelogMode` :1157, `statusLine.preset` :152, `statusLine.leftSegments` :277, `statusLine.rightSegments` :284, `theme.dark` :59, `memory.backend` `src/memory-backend/settings.ts:12`, `mnemopi.dbPath` `src/mnemopi/settings.ts:9`, `mnemopi.bank` :22, `mnemopi.scoping` :35, `mnemopi.llmMode` :226, `plan.defaultOnStartup` `src/plan-mode/settings.ts:24`, `secrets.enabled` `src/secrets/settings.ts:13`, `skills.customDirectories` `src/extensibility/settings.ts:65`, `task.agentModelOverrides` `src/task/settings.ts:478`, `telemetry.otlpExportEnabled` `src/telemetry-settings.ts:8`.
- **Network:** the only new hosts in `src/**/*.ts` are DeepSeek's search API (`api.deepseek.com`, `src/web/search/providers/deepseek.ts:19`; only with `DEEPSEEK_API_KEY` or a `deepseek` login, CHANGELOG.md:174) and two doc links. `config/omp-config.yml` pins `modelRoles.web: web/duckduckgo`, so Jocasta's panes don't reach it.

### Settings added, removed or with a changed default

Comparing every `register({ id: … })` site in both packages: nine ids added, none removed, no default, type or tab changed. One enum gained a value: `tools.approvalMode` adds `always-ask-cached` (above). Nothing added sends data off the machine (no ⚠).

| id | tab | default | what it does | recommendation |
|---|---|---|---|---|
| `advisor.continueSeverity` | model | `"concern"` | The lowest advisor note severity that wakes the agent after its final answer; lower ones wait for his next message (`src/advisor/settings.ts:65`) | default |
| `async.widgets` | tools | `true` | A live widget per running background job; the `widget` role (default `smol`) writes one spec per job, one model call each (`src/tools/settings.ts:948`) | default; his call in the `/settings` pass (spend) |
| `computer.backgroundFallback` | tools | `"takeover"` | What window input does when its background route is unavailable: take over the foreground or refuse (`src/tools/settings.ts:675`) | default |
| `lsp.requestTimeout` | files | `10` | Seconds to wait for a language server's answers when the agent sets no timeout (`src/lsp/settings.ts:94`) | default |
| `prewalk.auto` | model | `"off"` | A judge decides per task whether to hand off to `prewalk.autoTarget` at the first edit (`src/session/settings.ts:85`; CHANGELOG.md:304) | default (principle 2: no model-judged "auto") |
| `prewalk.autoTarget` | model | `"@smol"` | The model or role `prewalk.auto` hands off to (`src/session/settings.ts:113`) | default |
| `sharpshooter.minPromptLength` | none (config file only) | `16` | Shortest prompt sharpshooter's memory extraction considers (`src/sharpshooter/settings.ts:19`, read at `sharpshooter/extract.ts:195`) | default (Jocasta uses mnemopi) |
| `shellCommandPrefix` | none (config file only) | unset | Pi's shell prefix forwarded to legacy extensions; omp's own shell ignores it (`src/exec/settings.ts:83`) | default |
| `tui.completeScrollback` | appearance | `false` | Sends transcript rows pushed off screen by a growing HUD or dialog straight to scrollback (`src/modes/settings.ts:391`) | default |

## Settings: model and providers tabs

All 100 `/settings` rows on omp 18.10.3's `model` and `providers` tabs, in the order of omp's settings registry (`config/all-settings.ts`). Each is registered in omp-src (`register({ id: … })`). ⚠ = the setting sends data off the machine beyond the request to the model doing the work; its destination is cited under the table. "default" = leave it out of `config/omp-config.yml` (principle 3, deltas only). Env overrides from the dump are shown as "(env `NAME`)".

### `model` tab

| id | default | what it does | serves | recommendation |
|---|---|---|---|---|
| `modelRoleStorage` | `"global"` | Where role assignments from the model selector are saved | 0283 roles, 0387 config home | default: roles live in daily `~/.omp/agent` (0387) |
| `prewalk.enabled` | `false` | Plan on active model, switch to smol at first edit | 0283 roles, principle 2 | default: his choice; mid-task model switch is extra behaviour |
| `images.describeForTextModels` | `true` | ⚠ Sends attached images to a vision model, injects its description | principle 6 | default: goes to a model provider doing the work |
| `defaultThinkingLevel` | `"high"` | Reasoning depth for thinking-capable models | 0182 thinking level, principle 2 | his choice in walkthrough; never `auto` (0182) |
| `hideThinkingBlock` | `false` | Hide thinking blocks in responses | V12 working by hand | default |
| `expandThinkingBlocks` | `false` | Keep finished thinking blocks expanded | V12 working by hand | default |
| `proseOnlyThinking` | `true` | Replace code in thinking summaries with an ellipsis | — | default |
| `omitThinking` | `false` | Ask providers to omit thinking summaries | — | default |
| `externalThinking` | `false` | Private scratchpad; disables provider reasoning | 0182 thinking level | default: keeps pinned provider reasoning (0182) |
| `model.loopGuard.enabled` | `true` | Detect looping reasoning and prose in the stream (env `PI_NO_THINKING_LOOP_GUARD`) | 0152 loop guard, 0181 stop detection | default |
| `model.loopGuard.checkAssistantContent` | `true` | Apply loop guard to prose, not only thinking | 0152 loop guard | default |
| `model.loopGuard.toolCallReminder` | `true` | Nudge Gemini streams that plan without calling tools | 0152 loop guard | default |
| `model.toolCallLoopGuard.enabled` | `true` | Steer after consecutive identical tool calls across turns | 0152 loop guard | default |
| `model.toolCallLoopGuard.threshold` | `5` | Identical calls before the corrective steer | 0152 loop guard | default: 5 matches 0152's five identical calls |
| `model.toolCallLoopGuard.exemptTools` | `["wait"]` | Tools allowed to repeat without tripping the guard | 0152 loop guard | default |
| `inlineToolDescriptors` | `"auto"` | Put tool descriptors in system prompt, strip schema descriptions | 0177 tool-call format | default |
| `includeModelInPrompt` | `true` | Name the active model in the system prompt | 0171 self-references | default |
| `includeWorkspaceTree` | `false` | Render workspace tree in the system prompt | 0180 workspace tree | default: 0180 says never |
| `skillful` | `true` | List available skills in the system prompt | 0386 harness | default |
| `personality` | `"default"` | Communication style block in the system prompt | 0386 harness, principle 3 | `none`: his persona moves to `~/.omp/agent/AGENTS.md` (F1 step 2) |
| `temperature` | `-1` | Sampling temperature; -1 = provider default | 0176 sampling, principle 2 | default: 0176 leaves sampling unset |
| `topP` | `-1` | Nucleus sampling cutoff; -1 = provider default | 0176 sampling | default: 0176 |
| `topK` | `-1` | Top-K sampling; -1 = provider default | 0176 sampling | default: 0176 |
| `minP` | `-1` | Minimum probability threshold; -1 = provider default | 0176 sampling | default: 0176 |
| `presencePenalty` | `-1` | Presence penalty; -1 = provider default | 0176 sampling | default: 0176 |
| `repetitionPenalty` | `-1` | Repetition penalty; -1 = provider default | 0176 sampling | default: 0176 |
| `textVerbosity` | `"medium"` | Response verbosity for OpenAI Responses and Codex | — | default |
| `tier.openai` | `"none"` | OpenAI `service_tier` for requests | 0283 roles (cost) | default: his choice; priority costs more |
| `tier.anthropic` | `"none"` | Claude processing tier; `priority` = fast mode | 0283 roles (cost) | default: his choice; priority costs more |
| `tier.google` | `"none"` | Gemini `serviceTier` for requests | 0283 roles (cost) | default |
| `tier.subagent` | `"inherit"` | Service tier for task/eval subagents | 0286 subagents | default: follows main agent |
| `tier.advisor` | `"none"` | Service tier for the advisor model | 0287 advisor | default |
| `retry.maxRetries` | `10` | Maximum retry attempts on API errors | principle 2 loud failures | default |
| `retry.maxDelayMs` | `300000` | Longest wait between retries before failing fast | principle 2 loud failures | default |
| `retry.waitForUsageReset` | `false` | Sleep until quota reset instead of failing | V11 boss, 0388 threads | default: holds unattended threads and subagents |
| `retry.modelFallback` | `true` | Let retries switch to configured fallback models | 0283 roles and fallbacks | default: 0283 keeps fallbacks |
| `retry.fallbackOn` | `"any"` | Which failures may switch to a fallback model | 0283 roles and fallbacks | default |
| `retry.usageAwareFallback` | `false` | Prefer other accounts before a coding-plan limit hits | 0346 account rotation, 0377 | default: his choice; 0377 dropped rotation |
| `retry.usageReservePct` | `10` | Remaining quota percent treated as near-limit | 0346 account rotation | default |
| `retry.usageReservePolicy` | `"confirm"` | Action when every account is inside the reserve | 0346 account rotation | default |
| `retry.fallbackChains` | `{}` | ⚠ Maps roles/models to fallback selectors; may switch provider | 0283 roles, 0377, principle 6 | his choice in walkthrough: Copilot first, then Anthropic (0377) |
| `retry.fallbackRevertPolicy` | `"cooldown-expiry"` | When to return to the primary model | 0283 roles and fallbacks | default |
| `providers.anthropic.serverSideFallback` | `false` | Retry classifier-blocked Claude requests on Opus server-side | 0283 roles | default |
| `providers.autoThinkingMaxEffort` | `"xhigh"` | Highest effort the `auto` thinking classifier may pick | 0182 thinking level, principle 2 | default: unused when `auto` is never chosen (0182) |
| `advisor.enabled` | `false` | ⚠ Second model (advisor role) reviews each turn | 0287 advisor | default `false`: 0287 off by default, `/advisor` per session |
| `advisor.syncBacklog` | `"off"` | Pause main agent until advisor catches up | 0287 advisor | default |
| `advisor.immuneTurns` | `3` | Turns concerns stay non-interrupting after an interrupt | 0287 advisor | default: 3 matches 0287 |
| `advisor.reviewMode` | `"turn"` | Advisor cadence: every turn or final yield | 0287 advisor | default: 0287 reviews each turn |
| `advisor.reviewInterval` | `1` | Review every Nth eligible update | 0287 advisor | default |
| `advisor.maxNotesPerUpdate` | `4` | Cap on non-blocker advisor notes per update | 0287 advisor | default |
| `advisor.evictStaleResults` | `true` | Replace advisor's old tool output with placeholders | 0287 advisor | default |
| `images.urls.enabled` | `false` | ⚠ Publishes outgoing images via backend chain as URLs | principle 6 | default `false`: avoids public image hosts |
| `images.urls.backends` | `["provider-files","tailscale","cloudflared","litterbox"]` | ⚠ Publish destinations: provider files, Tailscale, cloudflared, litterbox | principle 6 | default while disabled; drop `litterbox` if enabled |
| `images.urls.command` | unset (no default in dump) | Argv template for a command backend | — | default |
| `images.urls.publicBaseUrl` | unset (no default in dump) | Public base URL fronting the blob server | — | default |
| `images.urls.ttlHours` | `72` | Serving window for locally hosted image URLs | — | default |
| `images.urls.bindHost` | `"127.0.0.1"` | Blob server bind host | principle 6 | default: loopback |
| `images.urls.sshTarget` | unset (no default in dump) | ⚠ user@host for ssh reverse forward of images | principle 6 | default: unset |
| `images.urls.sshRemotePort` | `8787` | Remote port of the ssh reverse forward | — | default |

Sources (⚠ rows, `model` tab):
- `images.describeForTextModels`: definition and behaviour text `packages/coding-agent/src/session/settings.ts:180`. Destination: the provider of the vision-capable model omp picks.
- `retry.fallbackChains`: `packages/coding-agent/src/session/settings.ts:834`; its description says a `"provider/*"` entry keeps the model id and swaps the provider. Destination: whichever provider the chain names.
- `advisor.enabled`: `packages/coding-agent/src/advisor/settings.ts:12-21` ("Pair a second model (assigned to the 'advisor' role)"). Destination: the advisor role's provider.
- `images.urls.enabled`, `images.urls.backends`: `packages/coding-agent/src/blob-broker/settings.ts:8,26,39`. Default chain `provider-files`, `tailscale`, `cloudflared`, `litterbox` (`packages/coding-agent/src/blob-broker/settings.ts:8`); litterbox uploads to `https://litterbox.catbox.moe/resources/internals/api.php` (`packages/coding-agent/src/blob-broker/uploaders-anonymous.ts:14`, `packages/coding-agent/src/blob-broker/destinations.ts:573-584`); cloudflared serves from a `*.trycloudflare.com` origin (`packages/coding-agent/src/blob-broker/exposure.ts:82`).
- `images.urls.sshTarget`: `packages/coding-agent/src/blob-broker/settings.ts:116`. Destination: the named ssh host.

### `providers` tab

| id | default | what it does | serves | recommendation |
|---|---|---|---|---|
| `providers.maxInFlightRequests` | `{}` | Cap concurrent LLM requests per provider across omp processes | 0349 in-flight cap | default: 0349 keeps the cap off by default |
| `providers.openai-codex.codeMode` | `"off"` | Route Codex code-mode-only models through eval | 0386 harness | default |
| `providers.openai-codex.codeModeDirectTools` | `[]` | Extra direct tools for Codex Code Mode | 0386 harness | default |
| `providers.ollama-cloud.maxConcurrency` | `3` | Concurrent Ollama Cloud subagent runs per process | — | default |
| `providers.webSearchTimeoutSeconds` | `60` | Per-provider web_search timeout before next fallback | 0347 web search | default |
| `providers.antigravityEndpoint` | `"auto"` | Endpoint routing for google-antigravity providers | — | default |
| `providers.fireworksTier` | `"standard"` | Fireworks serving path: standard or priority | — | default |
| `providers.tinyModelDevice` | `"default"` | Inference backend for local tiny models (titles, memory) (env `PI_TINY_DEVICE`) | 0389 memory | default: CPU ONNX stays local |
| `providers.tinyModelDtype` | `"default"` | Quantization for local tiny models (env `PI_TINY_DTYPE`) | 0389 memory | default |
| `providers.kimiApiFormat` | `"auto"` | API format for Kimi Code provider | — | default |
| `providers.openaiWebsockets` | `"auto"` | Websocket policy for OpenAI Codex models | — | default |
| `providers.openaiLiveSteering` | `true` | Deliver mid-stream messages into GPT-6 responses | V11 boss never blocks | default |
| `providers.muse-code.storeResponses` | `false` | ⚠ Stores prompts and outputs on Meta's servers (env `PI_MUSE_STORE_RESPONSES`) | principle 6 | default `false`: keeps prompts off Meta's side |
| `providers.cacheRetention` | `"auto"` | Prompt-cache retention sent to supporting providers | 0184 cache retention | default: 0184 keeps the provider default |
| `providers.cacheWarming` | `"idle"` | ⚠ Re-sends the last request to the provider before cache expiry | 0170 cache warming | `streaming`: 0170's choice; `idle` also re-sends between runs |
| `providers.streamFirstEventTimeoutSeconds` | `-1` | Watchdog for the first stream event | principle 2 loud failures | default |
| `providers.streamIdleTimeoutSeconds` | `-1` | Watchdog for silence between stream events | principle 2 loud failures | default |
| `providers.openrouterVariant` | `"default"` | Routing-variant suffix for OpenRouter model ids | — | default |
| `live.voice` | `"sol"` | ⚠ Voice for Codex realtime voice sessions (chatgpt.com) | 0392 companion voice | default: voice is the companion's (0392) |
| `tts.localVoice` | `"af_heart"` | Kokoro voice for omp's local TTS | 0293 voice engine | `bm_george`: Jocasta's voice per 0293 |
| `tts.localSpeed` | `1` | Speaking rate of local TTS | 0354 voice speed | default |
| `speech.enabled` | `false` | Speak assistant output aloud as it streams | 0373 reads aloud, 0392 | default `false`: the companion speaks the boss (0392) |
| `speech.mode` | `"assistant"` | What to speak: all, assistant, or final | 0373 reads aloud | default |
| `speech.enhanced` | `false` | ⚠ Rewrites output via tiny/smol model before speaking | 0373, principle 6 | default `false` |
| `speech.voice` | `"af_heart"` | Kokoro voice for speaking output | 0293 voice engine | `bm_george` if he enables speech (0293) |
| `speech.speed` | `1` | Speaking rate for read-aloud | 0354 voice speed | default |
| `providers.fetch` | `"auto"` | ⚠ Reader backend order for URL fetch; remote readers as fallback | 0290 web tools, principle 6 | default: native first; remote readers stay as fallbacks |
| `codexResets.autoRedeem` | `"unset"` | Auto-spend saved Codex rate-limit resets | 0346 account rotation | default `unset`: asks before first spend |
| `codexResets.minBlockedMinutes` | `60` | Minimum block length before auto-redeem | 0346 | default |
| `codexResets.keepCredits` | `0` | Codex resets never auto-spent | 0346 | default |
| `codexResets.salvageHorizonHours` | `12` | Spend near-expiry Codex resets early | 0346 | default |
| `claudeResets.autoRedeem` | `"unset"` | Auto-spend eligible Claude resets | 0346, 0377 | default `unset`: asks before first spend |
| `claudeResets.minBlockedMinutes` | `60` | Minimum block length before Claude auto-redeem | 0346 | default |
| `claudeResets.keepCredits` | `0` | Claude resets kept banked | 0346 | default |
| `claudeResets.salvageHorizonHours` | `12` | Spend near-expiry Claude resets early | 0346 | default |
| `provider.appendOnlyContext` | `"auto"` | Append-only log so provider prefix caches hit | 0184 cache retention | default |
| `telemetry.otlpExportEnabled` | `true` | ⚠ Exports traces, logs, metrics to `OTEL_*` endpoints | 'No telemetry' non-goal, 0048 | `false`: Jocasta reports nothing about its use (non-goal) |
| `exa.enabled` | `true` | ⚠ Exa web search provider (api.exa.ai, keyless mcp.exa.ai) | 0365 keyless Exa, 0347 | default: 0365 chose keyless Exa |
| `exa.searchDelayMs` | `1000` | Minimum delay between Exa requests | 0365 | default |
| `searxng.endpoint` | unset (no default in dump) | ⚠ Self-hosted SearXNG URL for web search (env `SEARXNG_ENDPOINT`) | 0347 web search | default: unset |
| `secrets.enabled` | `false` | Redact configured secrets and credential tokens before sending | 0386 redaction, 0391 | `true`: 0386 relies on omp's redaction |

Sources (⚠ rows, `providers` tab):
- `providers.muse-code.storeResponses`: `packages/coding-agent/src/session/settings.ts:1129`; resolution order option → `PI_MUSE_STORE_RESPONSES` → setting, only for models with `compat.storeResponses` (`packages/ai/src/providers/openai-responses.ts:538-547`). Destination: Meta's servers.
- `providers.cacheWarming`: `packages/coding-agent/src/session/settings.ts:1179-1202` (`idle` "also refresh 5-minute entries between runs"). Destination: the active model provider, as extra billed requests.
- `live.voice`: `packages/coding-agent/src/live/settings.ts:9`; realtime signalling at `${CODEX_BASE_URL}/codex/realtime/calls` (`packages/coding-agent/src/live/transport.ts:18`) with `CODEX_BASE_URL = "https://chatgpt.com/backend-api"` (`packages/catalog/src/wire/codex.ts:5`). Sends only while `/live` runs.
- `speech.enhanced`: `packages/coding-agent/src/tts/settings.ts:73`; each block goes to the `@tiny` role, falling through to the smol chain (`packages/coding-agent/src/tts/speech-enhancer.ts:74-77`). Destination: that model's provider unless it is local.
- `providers.fetch`: `packages/coding-agent/src/session/settings.ts:1272`; order `native`, `trafilatura`, `lynx`, `parallel`, `firecrawl`, `jina`, a chosen backend first and the rest as fallbacks (`packages/coding-agent/src/tools/fetch.ts:569-582`). Remote readers: `https://api.parallel.ai` (`packages/coding-agent/src/web/parallel.ts:5`), `https://api.firecrawl.dev/v2` (`packages/coding-agent/src/web/firecrawl.ts:16`), `https://r.jina.ai/<url>` (`packages/coding-agent/src/tools/fetch.ts:658`). No value disables the remote fallbacks.
- `telemetry.otlpExportEnabled`: `packages/coding-agent/src/telemetry-settings.ts:9-18`; exporters register only when an `OTEL_*` endpoint is set in env (`packages/coding-agent/src/main.ts:2259-2268`). Destination: that endpoint.
- `exa.enabled`: `packages/coding-agent/src/web/settings.ts:9`; `https://api.exa.ai/search` with a key, else keyless `https://mcp.exa.ai/mcp` (`packages/coding-agent/src/web/search/providers/exa.ts:23-24,359-368`). Destination: Exa, carrying the search query.
- `searxng.endpoint`: `packages/coding-agent/src/web/settings.ts:34`, used as the search base URL (`packages/coding-agent/src/web/search/providers/searxng.ts:12`). Destination: that instance.

Not ⚠ but worth knowing: `providers.tinyModelDevice` = `mlx` downloads weights from `https://huggingface.co` (or `HF_ENDPOINT`) (`packages/coding-agent/src/tiny/mlx-server.py:34`).

## Settings: interaction and appearance tabs

Source: omp's settings registry (`config/all-settings.ts`) (omp 18.10.3), rows with `tab` = `interaction` (55) or `appearance` (43), in dump order. `default` in the recommendation column keeps omp's default (principle 3: config holds deltas only). ⚠ = sends data off the machine; bracketed numbers point to Sources.

### Interaction (55)

| id | default | what it does | serves | recommendation |
|---|---|---|---|---|
| `autoResume` | `false` | Resumes newest session in cwd on plain `omp` start. | 0388 | default: threads start with explicit `omp --continue` (0388) |
| `git.enabled` | `true` | Branch, status and PR in status line. ⚠ PR lookup runs `gh pr view` → GitHub [1] | principle 6 | default: PR lookup only reaches the repo's GitHub host |
| `steeringMode` | `one-at-a-time` | Queued steering messages: all at once or one each. | 0392 | default |
| `followUpMode` | `one-at-a-time` | Follow-up drain after a turn: all or one each. | 0392 | default |
| `interruptMode` | `immediate` | Steering interrupts running tools now or waits. | V11, 0392 | default |
| `tui.vimMode` | `false` | Modal Vim editing in the prompt. | — | default |
| `tui.vimModeDisplay` | `text` | Vim mode indicator in the status line. | — | default |
| `loop.mode` | `prompt` | Between `/loop` runs: re-prompt, compact or reset. | — | default |
| `loop.conditionTimeoutMs` | `30000` | Max wait for a `/loop` condition command. | — | default |
| `composer.recallClearedDrafts` | `true` | Ctrl+C-cleared drafts stay in Up/Down history. | — | default |
| `doubleEscapeAction` | `rewind` | Esc twice on empty editor: rewind, tree or nothing. | 0392 | default; F2's `stop` must send one Escape only |
| `input.bareExitOnEmptySession` | `true` | Bare `exit`/`quit`/`q` quits before the first message. | 0392 | default |
| `input.bareSlashCommands` | `false` | Bare command name runs the slash command. | — | default |
| `treeFilterMode` | `default` | Default filter when opening the session tree. | — | default |
| `autocompleteMaxVisible` | `10` | Rows in the autocomplete dropdown (3–20). | — | default |
| `spelling.typoDetection` | `true` | macOS dictionary typo marks. | — | default (macOS only) |
| `spelling.autocomplete` | `auto` | Inline word hints; `auto` is local n-gram. ⚠ `smollm` downloads from huggingface.co [2] | — | default: `auto` stays on-device |
| `composer.predictions` | `false` | Model predicts your next message after each turn. ⚠ model provider [3] | principle 2 | default (`false`): a billed request every turn |
| `spelling.autocorrect` | `false` | macOS spelling autocorrect. | — | default (macOS only) |
| `emojiAutocomplete` | `true` | Expands `:name:` shortcodes and emoticons to emoji. | — | default |
| `paste.largeMenuThreshold` | `100` | Paste line count that opens a wrap/save menu; 0 off. | 0392 | `0`: herdr's `agent.prompt` sends briefs as bracketed pastes [16] |
| `startup.quiet` | `false` | Skips welcome screen and startup status messages. | 0060, 0378 | `true`: many panes start (0388); 0378 kept startup quiet |
| `startup.showSplash` | `false` | Animated setup splash on normal startup. | 0378 | default |
| `startup.setupWizard` | `true` | Shows new onboarding steps once per setup version. | 0396 | default |
| `startup.checkUpdate` | `true` | Checks for a newer omp on start. ⚠ npm registry or build.stencil.so [4] | 0061, 0383 | `false`: 0061 precedent; upgrades are deliberate bumps |
| `update.channel` | `stable` | Channel for `omp update` and the startup check. | 0383 | default (`stable`) |
| `marketplace.autoUpdate` | `notify` | Checks or applies plugin updates on start. ⚠ added marketplace sources [5] | 0383 | `off`: nothing upgrades unasked (0383) |
| `startup.changelogMode` | `summary` | Update notes as summary, full or hidden. | 0378 | default |
| `magicKeywords.enabled` | `true` | Hidden notices for standalone magic keywords. | — | default |
| `magicKeywords.ultrathink` | `true` | Standalone `ultrathink` requests maximum thinking. | 0182 | default: only fires when he types it |
| `magicKeywords.orchestrate` | `true` | Standalone `orchestrate` appends a multi-agent orchestration notice. | — | default |
| `magicKeywords.workflow` | `true` | Standalone `workflowz` appends an eval workflow notice. | — | default |
| `magicKeywords.jevify` | `true` | Standalone `jevify` appends a bulk-judge notice. | — | default |
| `completion.notify` | `on` | Terminal notification when a turn finishes [15]. | 0329, 0392, 0395 | default; UX pass may set `off`: herdr toasts finished panes |
| `error.notify` | `off` | Terminal notification when the agent stops on error. | 0329, 0395 | default; UX pass decides with herdr toasts |
| `ask.timeout` | `0` | Seconds before `ask` auto-picks the recommendation; 0 waits. | V11, 0392 | default: a thread waits for his answer |
| `ask.notify` | `on` | Notification while `ask` waits for input. | 0329, 0395 | default; UX pass decides with herdr's blocked toast |
| `recap.enabled` | `true` | Idle LLM recap of where things stand. ⚠ model provider [6] | 0186, principle 2 | `false`: no background summaries (0186); idle panes would bill |
| `recap.idleSeconds` | `240` | Idle seconds before the recap. | — | default |
| `power.sleepPrevention` | `idle` | Keeps the system awake during active sessions. | V3 | default |
| `features.unexpectedStopDetection` | `mechanical` | Recovers silent stops; `smart` adds a small model. ⚠ `smart` → provider [7] | 0181, principle 2 | default (`mechanical`) matches 0181's mechanical-only rule |
| `tools.approval` | `{}` | Per-tool `allow`/`prompt`/`deny`, honored in every mode [14]. | 0391 | default; add per-tool lines in the walkthrough (0391) |
| `tools.approvalMode` | `yolo` | Default approval: `always-ask`, `write` or `yolo` [14]. | 0391, principle 4 | `write`: guards fail closed; exec asks; subagents still run `yolo` |
| `skills.registryUrl` | `https://skills.omp.sh` | Registry for `omp skill`. ⚠ skills.omp.sh, only on `omp skill` [8] | — | default |
| `stt.enabled` | `false` | Microphone dictation in the composer. ⚠ model download from huggingface.co [9] | 0330, 0392 | default (`false`): companion owns dictation (0392) |
| `stt.submitTrigger` | `never` | When dictation auto-submits. | 0392 | default |
| `collab.relayUrl` | `wss://my.omp.sh` | Relay for `/collab`. ⚠ wss://my.omp.sh [10] | principle 6 | default; collab stays unused |
| `collab.webUrl` | `""` | Browser UI for `/collab` links; empty derives from relay. | — | default |
| `collab.displayName` | `""` | Name shown to collab peers; empty = OS username. | principle 6 | default |
| `collab.autoStart` | `off` | Hosts every session through the relay. ⚠ wss://my.omp.sh [10] | principle 6 | default (`off`): sessions hold client data |
| `share.serverUrl` | `https://my.omp.sh/s` | `/share` upload base. ⚠ my.omp.sh/s encrypted blob [11] | 0047, principle 6 | default; `/share` only on demand |
| `share.store` | `blob` | `/share` target: encrypted blob or GitHub gist. ⚠ [11] | 0047, principle 6 | default (`blob`) |
| `share.redactSecrets` | `true` | Runs the secret obfuscator before `/share` upload [11]. | 0047 | default |
| `stream.serverUrl` | `https://live.omp.sh` | Server for `omp stream`. ⚠ live.omp.sh [12] | principle 6 | default; never stream client repos |
| `stream.redactPatterns` | `[]` | Extra regexes redacted from every streamed row. | principle 6 | default; client-name patterns only in his local config |

### Appearance (43)

| id | default | what it does | serves | recommendation |
|---|---|---|---|---|
| `theme.dark` | `titanium` | Theme used on dark terminal backgrounds. | 0369, 0395 | default until the UX pass picks the palette (0395) |
| `theme.light` | `light` | Theme used on light terminal backgrounds. | 0369, 0395 | default until the UX pass (0395) |
| `symbolPreset` | `unicode` | Glyph set: Unicode, Nerd Font or ASCII. | 0395 | default |
| `colorBlindMode` | `false` | Blue instead of green for diff additions. | — | default |
| `composer.shape` | `band` | Layout of the input editor and status line. | 0395 | default until the UX pass (0395) |
| `composer.tokenRate` | `false` | Live tokens-per-second readout while generating. | — | default |
| `statusLine.preset` | `default` | Pre-built status line layout. | 0060, 0395 | default until the UX pass (0395) |
| `statusLine.separator` | `powerline-thin` | Separator glyph between status segments. | 0395 | default |
| `statusLine.contextLine` | `embedded` | How the divider line shows context use. | 0174 | default |
| `statusLine.sessionAccent` | `true` | Session-name color on editor border and status gap. | 0395 | default |
| `statusLine.transparent` | `false` | Terminal background instead of theme fill for status line. | 0395 | default |
| `statusLine.compactThinkingLevel` | `true` | Thinking level as an icon on the model name. | — | default |
| `statusLine.showHookStatus` | `true` | Hook status messages under the status line. | — | default |
| `terminal.showImages` | `true` | Renders images inline in the terminal. | 0392 | default |
| `images.autoResize` | `true` | Shrinks images to 2000×2000 max before sending. | — | default |
| `images.blockImages` | `false` | Replaces images with text before provider requests [13]. | principle 6 | default |
| `tui.resizeScrollback` | `rebuild` | How scrollback refreshes after a terminal resize. | 0392 | default |
| `terminal.showProgress` | `false` | Emits OSC 9;4 progress while working. | 0388, 0392 | default: herdr gets state from omp's integration (0388) |
| `terminal.programStatus` | `true` | Reports working/waiting/done/failed over OSC 7501 [17]. | 0388, 0392 | default |
| `tui.textSizing` | `false` | Kitty-only 2× H1 headings (OSC 66). | — | default |
| `tui.renderMermaid` | `true` | Draws Mermaid code blocks as ASCII diagrams. | V6 | default |
| `tui.renderSvg` | `true` | Invites the agent to draw SVG figures shown inline. | V6, 0393 | default until F3, then `false`: drawings go through whiteboard (0393) |
| `tui.autoGraph` | `always` | Charts under numeric tables in answers. | V6 | default |
| `tui.reactions` | `true` | Invites the agent to emoji-react to your message. | — | default |
| `tui.codexResetFireworks` | `false` | Fireworks overlay on Codex weekly usage resets. | — | default |
| `tui.titleState` | `true` | Run-state glyph in the terminal title. | 0378, 0392 | default |
| `tui.titleSpinner` | `braille` | Spinner glyph set in the terminal title. | — | default |
| `tui.hyperlinks` | `auto` | OSC 8 click-to-open paths and URLs. | 0392 | default |
| `tui.mouse` | `false` | Clicks focus subagent cards; native selection needs Shift. | 0392 | default |
| `tui.tight` | `false` | Drops the 1-column side padding. | 0395 | default |
| `display.shimmer` | `classic` | Animation style of working messages. | — | default |
| `display.pinnedAgents` | `collapsed` | Live subagent jump list above the editor. | — | default |
| `display.subagentLivePreview` | `false` | Each pinned subagent's current tool call under its row. | — | default |
| `display.smoothStreaming` | `true` | Reveals streamed text smoothly. | — | default |
| `display.hideToolActivity` | `false` | Hides tool calls and results from the transcript. | V12 | default |
| `display.showTokenUsage` | `false` | Per-turn token usage on assistant messages. | 0173 | default |
| `display.showTurnTime` | `false` | Prompt-to-yield time on usage rows. | 0173 | default |
| `display.cacheMissMarker` | `false` | Divider after a turn that missed the prompt cache. | 0173 | default |
| `display.collapseCompacted` | `true` | Collapses pre-compaction history behind the summary divider. | V9, 0388 | default |
| `showHardwareCursor` | `true` | Shows the terminal cursor for IME input. | — | default |
| `tui.imeSafeCursor` | `false` | Prompt border on its own row for macOS IME. | — | default |
| `task.showResolvedModelBadge` | `false` | Actual model id per subagent in the task widget. | — | default |
| `title.icons` | `nf+emoji` | Icon and short code heading generated session titles. | 0378 | default |

### Sources

Paths are relative to omp-src (tag v18.10.3) unless they start with `herdr-src/` (v0.9.4).

1. `git.enabled`: the status line looks up PRs with `gh pr view --json number,url` (`packages/coding-agent/src/modes/status-line-host.ts:49,61-62`), spawned as the `gh` CLI (`packages/coding-agent/src/utils/github.ts:141`); `gh` talks to the repo's GitHub host. Gated by this setting per its description (`packages/coding-agent/src/modes/settings.ts:42-50`).
2. `spelling.autocomplete`: `auto` resolves to local `ngram` (`packages/coding-agent/src/predict/client.ts:49`; option text "N-gram (nothing to download)", `packages/coding-agent/src/modes/settings.ts:983-985`); `smollm` prefetches SmolLM2-135M weights (`packages/coding-agent/src/predict/client.ts:230`) from `https://huggingface.co` (`packages/coding-agent/src/predict/smollm-weights.ts:39,71`).
3. `composer.predictions`: after a completed turn, runs an ephemeral model turn on the session's model (`packages/coding-agent/src/modes/controllers/composer-prediction-controller.ts:77,84,109-113`); destination = the active model provider.
4. `startup.checkUpdate`, `update.channel`: `checkForNewVersion` (`packages/coding-agent/src/main.ts:239-253`) asks the build service for compiled binaries (`fetchBuild`, default origin `https://build.stencil.so`, override `PI_BUILD_URL`, `packages/coding-agent/src/cli/build-service.ts:44,52-54`) and otherwise the npm registry from his npm/bun config (`packages/coding-agent/src/cli/update-cli.ts:797-806`), default `https://registry.npmjs.org/` (`packages/coding-agent/src/cli/npm-registry.ts:24`). The request names the running version (`packages/coding-agent/src/main.ts:248-251`).
5. `marketplace.autoUpdate`: scheduled at startup (`packages/coding-agent/src/main.ts:2231-2232`); unless `off` it refreshes stale marketplaces and checks plugin updates, `auto` upgrades them (`packages/coding-agent/src/extensibility/plugins/marketplace-auto-update.ts:11-44`); URL marketplaces are fetched with `fetch(source)` (`packages/coding-agent/src/extensibility/plugins/marketplace/fetcher.ts:274`). Destination = each marketplace he has added.
6. `recap.enabled`: after `recap.idleSeconds` idle, runs an ephemeral turn on the session's model and records the reply in the session (`packages/coding-agent/src/modes/controllers/event-controller.ts:2764-2779,2792-2811`); destination = the active model provider.
7. `features.unexpectedStopDetection`: `smart` = "Mechanical + small-model classification of text-only stops" (`packages/coding-agent/src/session/settings.ts:1052-1074`); destination = the provider of the small model.
8. `skills.registryUrl`: default `https://skills.omp.sh` (`packages/wire/src/skillshare.ts:18`), read only by the `omp skill` client (`packages/coding-agent/src/skillshare/client.ts:109-112`).
9. `stt.enabled`: model files come from `https://huggingface.co` (`packages/coding-agent/src/stt/asr-worker.ts:49-50`); default model Parakeet TDT 0.6B v3 (`packages/coding-agent/src/stt/models.ts:91,111`); recognition runs on-device (`packages/coding-agent/src/stt/endpointer.ts:4-5`).
10. `collab.*`: default relay `wss://my.omp.sh` (`packages/wire/src/index.ts:426`), used by `/collab` (`packages/coding-agent/src/collab/controller.ts:207-208`); `autoStart` read at `packages/coding-agent/src/collab/controller.ts:83-85`, default `off` (`packages/coding-agent/src/collab/settings.ts:46-50`).
11. `share.*`: default `https://my.omp.sh/s` (`packages/wire/src/index.ts:428-429`); `/share` uploads with `serverUrl`, `store` (`gist` on `--gist`) and the obfuscator when `redactSecrets` and `secrets` are on (`packages/coding-agent/src/commands/share.ts:66-75`; slash command `packages/coding-agent/src/modes/controllers/command-controller.ts:524-528`, `packages/coding-agent/src/slash-commands/builtin-collaboration.ts:298-303`). Viewer fetches gists from `https://api.github.com/gists/` (`packages/coding-agent/src/export/html/share-loader.js:32`).
12. `stream.*`: default `https://live.omp.sh` (`packages/wire/src/stream.ts:17`), used by `omp stream` (`packages/coding-agent/src/commands/stream.ts:36`) and clip upload (`packages/coding-agent/src/commands/clip.ts:60`).
13. `images.blockImages`: replaces images with "Image reading is disabled." before conversion to provider messages (`packages/coding-agent/src/sdk.ts:4155-4157`). Keeps data on the machine; no ⚠.
14. Approvals: tiers — `always-ask` prompts write/exec, `write` prompts exec (bash, eval, browser, task), `yolo` prompts none; per-tool policies apply in every mode (`packages/coding-agent/src/tools/settings.ts:294-342`). Subagents are spawned with `tools.approvalMode: "yolo"`, user `tools.approval` still applying (`packages/coding-agent/src/task/executor.ts:1172-1173`).
15. Notifications: `completion.notify` sends a terminal notification titled with the session name (`packages/coding-agent/src/modes/controllers/event-controller.ts:2882-2904`) via OSC 9/99 or BEL (`packages/tui/src/terminal-capabilities.ts:156-176`). herdr raises its own toasts: `NeedsAttention` on blocked, `Finished` on working/blocked → idle (`herdr-src/src/app/actions.rs:22-25,87-90`), and a done sound (`herdr-src/src/app/actions.rs:56-60`).
16. `paste.largeMenuThreshold`: herdr's `agent.prompt` writes the text as a bracketed paste (`ESC[200~ … ESC[201~`) then `\r` (`herdr-src/src/app/api/agents.rs:637-639`). That a ≥100-line paste opens omp's menu and stalls the submit is [INFERENCE] from the setting text, not run.
17. `terminal.programStatus`: OSC 7501 root record (`packages/coding-agent/src/utils/run-status.ts:2-6,33,103-112`). herdr has no OSC 7501 reader (no match for `7501` in `herdr-src/src`); it keeps OSC 0/2 titles and OSC 9 progress for detection (`herdr-src/src/pane/osc.rs:485-492`).

### Definitions

Each id is registered with its tab, default and label at:
- `packages/coding-agent/src/modes/settings.ts`: `autoResume` :31, `git.enabled` :43, `theme.dark` :60, `theme.light` :74, `symbolPreset` :88, `colorBlindMode` :111, `composer.shape` :125, `composer.tokenRate` :139, `statusLine.preset` :153, `statusLine.separator` :175, `statusLine.contextLine` :197, `statusLine.sessionAccent` :228, `statusLine.transparent` :240, `statusLine.compactThinkingLevel` :253, `statusLine.showHookStatus` :266, `terminal.showImages` :299, `images.autoResize` :312, `images.blockImages` :324, `tui.resizeScrollback` :362, `terminal.showProgress` :392, `terminal.programStatus` :405, `tui.textSizing` :418, `tui.renderMermaid` :431, `tui.renderSvg` :443, `tui.autoGraph` :456, `tui.reactions` :484, `tui.codexResetFireworks` :496, `tui.titleState` :510, `tui.titleSpinner` :523, `tui.hyperlinks` :543, `tui.mouse` :560, `tui.tight` :575, `display.shimmer` :587, `display.pinnedAgents` :606, `display.subagentLivePreview` :625, `display.smoothStreaming` :637, `display.hideToolActivity` :649, `display.showTokenUsage` :661, `display.showTurnTime` :673, `display.cacheMissMarker` :685, `display.collapseCompacted` :708, `showHardwareCursor` :721, `tui.imeSafeCursor` :733, `steeringMode` :750, `followUpMode` :763, `interruptMode` :776, `tui.vimMode` :789, `tui.vimModeDisplay` :803, `loop.mode` :822, `loop.conditionTimeoutMs` :848, `composer.recallClearedDrafts` :868, `doubleEscapeAction` :882, `input.bareExitOnEmptySession` :897, `input.bareSlashCommands` :910, `treeFilterMode` :923, `autocompleteMaxVisible` :936, `spelling.typoDetection` :956, `spelling.autocomplete` :969, `composer.predictions` :1001, `spelling.autocorrect` :1015, `emojiAutocomplete` :1028, `paste.largeMenuThreshold` :1041, `startup.quiet` :1061, `startup.showSplash` :1073, `startup.setupWizard` :1086, `startup.checkUpdate` :1098, `update.channel` :1110, `marketplace.autoUpdate` :1127, `startup.changelogMode` :1145, `magicKeywords.enabled` :1175, `magicKeywords.ultrathink` :1190, `magicKeywords.orchestrate` :1190, `magicKeywords.workflow` :1190, `magicKeywords.jevify` :1190, `completion.notify` :1201, `error.notify` :1214, `ask.timeout` :1227, `ask.notify` :1257, `recap.enabled` :1270, `recap.idleSeconds` :1282
- `packages/coding-agent/src/session/settings.ts`: `power.sleepPrevention` :39, `features.unexpectedStopDetection` :1053
- `packages/coding-agent/src/tools/settings.ts`: `tools.approval` :295, `tools.approvalMode` :312
- `packages/coding-agent/src/task/settings.ts`: `task.showResolvedModelBadge` :537
- `packages/coding-agent/src/utils/title-settings.ts`: `title.icons` :13
- `packages/coding-agent/src/extensibility/settings.ts`: `skills.registryUrl` :16
- `packages/coding-agent/src/stt/settings.ts`: `stt.enabled` :10, `stt.submitTrigger` :24
- `packages/coding-agent/src/collab/settings.ts`: `collab.relayUrl` :10, `collab.webUrl` :22, `collab.displayName` :35, `collab.autoStart` :47
- `packages/coding-agent/src/commands/settings.ts`: `share.serverUrl` :9, `share.store` :22, `share.redactSecrets` :47
- `packages/coding-agent/src/stream/settings.ts`: `stream.serverUrl` :12, `stream.redactPatterns` :25
- `magicKeywords.ultrathink|orchestrate|workflow|jevify` are generated from `MAGIC_KEYWORDS` in one `register` call (`packages/coding-agent/src/modes/settings.ts:1187-1191`).

## Settings: tools, shell and files tabs

Rows are in `/settings` dump order. `default` is the registered default (omp's settings registry (`config/all-settings.ts`)). Each id's registration is listed under Sources. ⚠ marks a setting that can send data off the machine.

### Approvals: what `tools.approvalMode`, `tools.approval.*` and `bash.patterns` do

`tools.approvalMode` (default `yolo`) and `tools.approval` (default `{}`) sit on the interaction tab. They are explained here because the bash rows below plug into them.

| Mode / policy | Meaning | Source |
|---|---|---|
| `always-ask` | Auto-allows only `read`-tier tools; write and exec tiers prompt | `packages/coding-agent/src/tools/approval.ts:120-124`, `:183-185`, `:307-317` |
| `write` | Auto-allows `read` and `write` tiers; `exec` tier (e.g. unmatched bash, `tools/bash.ts:587`) prompts | `packages/coding-agent/src/tools/approval.ts:120-124`, `:307-317` |
| `yolo` | Allows everything unless a tool or user policy says `deny`/`prompt`; tool "override" prompts are ignored | `packages/coding-agent/src/tools/approval.ts:200-201`, `:255-273` |
| `tools.approval.<tool>: allow\|deny\|prompt` | Per-tool override. `deny` wins in every mode. `allow`/`prompt` replace the mode's answer unless the tool itself declared a policy or override | `packages/coding-agent/src/tools/approval.ts:5`, `:16`, `:245-253`, `:255-272`, `:275-305` |
| Tool's own decision | A tool may return a tier and `allow`/`deny`/`prompt`; tool `deny` wins first; a tool with no declaration is tier `exec` | `packages/coding-agent/src/tools/approval.ts:160`, `:172-181`, `:235-244` |
| No settings in context | Fail-closed `always-ask`; `--auto-approve` forces `yolo` | `packages/coding-agent/src/tools/approval.ts:56-59`, `:67-82` |

What this means for the guard (0391):

- omp's built-in dangerous-command list (`rm -rf /`, `sudo rm`, `mkfs`, `dd … of=/dev/`, `curl … | sh`, `shutdown`, `nc -e`, …) is `CRITICAL_BASH_PATTERNS` at `packages/coding-agent/src/tools/bash.ts:185-230`. A match returns `{ tier: "exec", override: true }` with no policy (`packages/coding-agent/src/tools/bash.ts:541-543`, `:569-573`).
- Under `yolo` such an override has no policy, so it resolves to `allow`: the dangerous-command prompt does **not** fire (`packages/coding-agent/src/tools/approval.ts:200-201`, `:255-272`). Under `write` or `always-ask` it becomes `prompt` (`packages/coding-agent/src/tools/approval.ts:275-284`).
- `bash.patterns` rules carry `policy: "deny"` or `"prompt"` (`packages/coding-agent/src/tools/bash.ts:533-540`, `:579-586`). Those policies are honoured under `yolo` too (`packages/coding-agent/src/tools/approval.ts:235-244`, `:255-265`). An `allow` rule makes the command tier `write` (`packages/coding-agent/src/tools/bash.ts:578`).
- Every pattern in the list targets POSIX shells; none matches PowerShell or cmd commands such as `Remove-Item -Recurse` or `format` (`packages/coding-agent/src/tools/bash.ts:185-230`). [INFERENCE] On Windows, `bash.patterns` deny/prompt rules are the only bash guard that `yolo` keeps.
- Recommendation for the walkthrough: choose `tools.approvalMode: write` to get the dangerous-command prompts back. If he keeps `yolo`, write deny/prompt rules in `bash.patterns`, for example `{ match: "git push --force*", approval: "deny" }`, and turn `bash.gitGuard` on.

### `tools` tab

| id | default | what it does | serves | recommendation |
|---|---|---|---|---|
| `tools.artifactSpillThreshold` | `50` | Output above this many KB spills to an artifact | 0150 artifact spill | default — 50 KB is near 0150's 32 KiB |
| `tools.artifactTailBytes` | `20` | KB of output tail kept inline after a spill | 0150 | default |
| `tools.artifactHeadBytes` | `20` | KB of output head kept inline after a spill | 0150 | default |
| `tools.outputMaxColumns` | `768` | Per-line byte cap for streamed output and read | 0153 core tool limits | default |
| `tools.artifactTailLines` | `500` | Max tail lines kept inline after a spill | 0150 | default |
| `tools.artifactMaxBytes` | `16` | MB cap on one spilled artifact file | 0150 | default |
| `todo.enabled` | `true` | Enable the todo tool | 0386 (todo covered by omp) | default |
| `todo.reminders` | `true` | Remind the agent to finish todos before stopping | 0181 stop detection | default |
| `todo.remindersMax` | `3` | Max todo reminders before giving up | 0181 | default |
| `todo.eager` | `"default"` | How hard to push a todo list after the first message | — | default |
| `tasks.todoClearDelay` | `60` | Seconds before finished todos leave the widget | — | default |
| `glob.enabled` | `true` | Enable the glob tool | 0381 | default |
| `grep.enabled` | `true` | Enable the grep tool | 0381; 0153 | default |
| `grep.contextBefore` | `1` | Context lines before each grep match | 0153 | default |
| `grep.contextAfter` | `3` | Context lines after each grep match | 0153 | default |
| `astGrep.enabled` | `false` | Enable structural AST search | 0290 (ast-grep after v1, 0307) | default |
| `astEdit.enabled` | `true` | Enable structural AST rewrites | 0290 | default |
| `find.enabled` | `"auto"` | Natural-language file/range search judged by the judge role | 0381 | default — auto turns on only when a judge model resolves ⚠ |
| `debug.enabled` | `true` | Enable the DAP debugger tool | 0386 | default |
| `launch.enabled` | `true` | Named bash services under proc:// supervision | 0284 jobs | default |
| `speechgen.enabled` | `false` | Enable the tts speech-file tool | V2; 0392 (voice is the companion's) | default — the companion speaks, not omp ⚠ |
| `generate_image.enabled` | `false` | Enable text-to-image generation and editing | — | default ⚠ |
| `computer.enabled` | `false` | Eval prelude for desktop screenshots, input, accessibility | — | default |
| `ratchet.enabled` | `false` | Eval prelude for ratchet/hillclimb loops | 0167 hillclimbing (Jocasta evals stay in evals/) | default |
| `archive.enabled` | `true` | Read-only eval access to past sessions and prompts | 0389 | default |
| `computer.display` | `"active"` | Which display the computer prelude captures | — | default |
| `computer.maxWidth` | `3840` | Max screenshot width in pixels | — | default |
| `computer.maxHeight` | `2400` | Max screenshot height in pixels | — | default |
| `images.questionTimeoutMs` | `300000` | Timeout for vision-model calls behind read ?q= on images | — | default ⚠ |
| `checkpoint.enabled` | `false` | Enable checkpoint and rewind tools | — | default |
| `fetch.enabled` | `true` | Let read fetch and convert URLs | 0290/0347 web tools | default ⚠ |
| `vault.enabled` | `false` | Enable vault:// for Obsidian vault content | — | default |
| `github.enabled` | `false` | Enable the github tool over the gh CLI | — | default — bash `gh` already works; turn on if he uses it daily ⚠ |
| `github.cache.enabled` | `true` | Cache issue/PR views in a local SQLite file | — | default |
| `github.cache.softTtlSec` | `300` | Seconds cached views are served without refresh | — | default |
| `github.cache.hardTtlSec` | `604800` | Seconds before cached views are dropped | — | default |
| `web_search.enabled` | `true` | Enable the web_search tool | 0290/0347/0365 web tools | default ⚠ |
| `security.enabled` | `false` | Enable omp security scans and security:// resources | — | default |
| `ask.enabled` | `true` | Enable the ask tool for questions to the user | V11 blocked threads; V12 | default |
| `tools.intentTracing` | `true` | Model states each tool call's intent first | — | default |
| `tools.abortOnFabricatedResult` | `true` | Stop the model when it invents a tool result | 0183 fabricated tool results | default |
| `tools.speculativeExecution.enabled` | `false` | Run validated local reads before normal dispatch | 0179 speculative execution (off) | default — 0179 chose off |
| `tools.speculativeExecution.maxInFlight` | `2` | Max speculative reads in flight | 0179 | default |
| `tools.maxTimeout` | `0` | Upper bound on any tool timeout (0 = none) | 0153 | default |
| `async.enabled` | `true` | Async bash and background jobs | 0284 jobs, wait and send | default |
| `tools.xdev` | `true` | Mount rare tools as xd:// devices, not full schemas | 0381; 0184 prompt cache | default |
| `tools.xdevDocs` | `"catalog"` | Which xd:// device docs are inlined in the prompt | 0184 | default |
| `tools.xdevInlineDevices` | `[]` | Globs of devices to inline under Built-ins Only | — | default |
| `dev.autoqa` | `true` | Model may file tool-issue reports to xd://report_issue | — | `false` — reports go to qa.omp.sh after one consent popup ⚠ |
| `dev.autoqaPush.endpoint` | `"https://qa.omp.sh/v1/grievances"` | URL receiving Auto QA reports | — | default — moot with `dev.autoqa: false` ⚠ |
| `extensionHandlers.toolCallTimeoutMs` | `30000` | Timeout for extension tool_call handlers | 0386 (no Jocasta extensions) | default |
| `browser.enabled` | `true` | Puppeteer Chromium prelude in eval | 0289 eval/browser; 0345 | default ⚠ |
| `browser.cdpUrl` | unset | Attach to an existing CDP endpoint instead of launching | 0289 | default |
| `browser.relay` | `false` | Drive his own Chrome tabs through a relay extension | — | default — keeps agents out of his logged-in browser |
| `browser.relayUrl` | unset | Relay endpoint (default 127.0.0.1:9224) | — | default |
| `browser.headless` | `true` | Launch the browser headless | 0289 | default |
| `browser.cmux` | `true` | Use cmux web views when a cmux socket exists | — | default — no cmux on Windows |
| `browser.tern` | `true` | Use Tern pane web views when inside Tern | — | default — herdr, not Tern |
| `browser.freezeOnTurnEnd` | `true` | Freeze idle headless tabs after a turn | — | default |
| `browser.idleCloseSec` | `1800` | Close idle browser tabs after this many seconds | — | default |
| `browser.screenshotDir` | unset | Where screenshots are saved (temp when unset) | — | default |
| `ida.enabled` | `true` | Open executables in IDA Pro; inert without IDA | — | default |
| `ida.python` | `""` | Python that imports ida_domain/idapro | — | default |
| `ida.installDir` | `""` | IDA install dir exported as IDADIR | — | default |
| `ida.maxOpen` | `4` | Max open IDA databases per project | — | default |
| `ida.idleCloseSec` | `900` | Close idle IDA databases after this many seconds | — | default |
| `mcp.enableProjectConfig` | `true` | Load .mcp.json/mcp.json from the project root | 0392 `jocasta mcp`; plan F2 step 6 boss scoping | default — F2 may scope `jocasta mcp --boss` per project |
| `mcp.startupTimeoutMs` | `250` | Wait for initial MCP tool discovery (ms) | 0389 `graph_search` via `jocasta mcp` | default |
| `mcp.renderMarkdownResults` | `true` | Render MCP text results as Markdown | — | default |
| `mcp.notifications` | `false` | Inject MCP resource updates into the conversation | — | default |
| `mcp.notificationDebounceMs` | `500` | Debounce for MCP resource updates (ms) | — | default |

### `shell` tab

| id | default | what it does | serves | recommendation |
|---|---|---|---|---|
| `bash.enabled` | `true` | Enable the bash tool | 0391 | default |
| `bash.allowCompoundCommands` | `false` | Match `&&` chains per command against bash.patterns | 0391 | default — only applies to POSIX shells |
| `bash.autoBackground.enabled` | `true` | Background long bash commands, deliver result later | 0284 | default |
| `bash.gitGuard` | `false` | Refuse stash, hard reset, checkout/switch/restore outside conflicts | 0391 (replaces Jocasta's git-guard corpus) | `true` — protects shared worktree work 0391 no longer guards |
| `bash.patterns` | `[]` | Ordered allow/deny/prompt rules for bash commands (`*` wildcard) | 0391 approvals are the guard | walkthrough — deny/prompt rules hold even under `yolo` |
| `bashInterceptor.enabled` | `false` | Block shell commands that have dedicated tools | 0149 bash interceptor | `true` — 0149 chose it on |
| `bash.direnv` | `"auto"` | Auto-load a repo's .envrc into bash | — | default |
| `bash.direnvLoadTimeoutMs` | `30000` | Max wait for first direnv export | — | default |
| `shellMinimizer.enabled` | `true` | Compress verbose git/npm/cargo output before the model sees it | 0153 | default |
| `shellMinimizer.sourceOutlineLevel` | `"default"` | Outline depth when shell cats source files | — | default |
| `eval.py` | `true` | Python cells via the IPython kernel | 0289 eval/browser | default |
| `eval.js` | `true` | JavaScript cells in the in-process runtime | 0289 | default |
| `eval.autoProvision` | `true` | Create the managed JS package environment on first install | 0289 | default ⚠ installs pull from npm |
| `eval.tools.enabled` | `true` | Eval cells may define tools for subagents | 0286 subagents | default |
| `eval.workpool.freshAgents` | `false` | New subagent per workpool item | 0286 | default |
| `eval.autoBackground.enabled` | `false` | Background long eval cells | 0284 | default |
| `python.kernelMode` | `"session"` | Keep the Python kernel across calls or restart | 0289 one process per thread | default — `session` is 0289's choice |
| `python.interpreter` | `""` | Exact Python executable; skips discovery | 0345 python on host | default |

### `files` tab

| id | default | what it does | serves | recommendation |
|---|---|---|---|---|
| `edit.mode` | `"hashline"` | Which edit tool variant the model gets | 0146 hashline edits; 0178 | default — hashline is 0146's choice |
| `edit.fuzzyMatch` | `true` | Accept near-exact matches differing only in whitespace | 0178 | default |
| `edit.fuzzyThreshold` | `0.95` | Similarity floor for fuzzy matches | 0178 | default |
| `edit.streamingAbort` | `false` | Abort a streaming edit call when its preview fails | — | default |
| `edit.recoverInlineEdits` | `true` | Run edit payloads the model wrote as plain text | — | default |
| `edit.blockAutoGenerated` | `true` | Refuse edits to generated files (protoc, sqlc, swagger) | — | default |
| `edit.enforceSeenLines` | `true` | Reject edits on lines no read or search showed | 0146; 0148 | default — the read-before-edit check 0148 dropped, for free |
| `edit.blackbox.enabled` | `false` | Log full before/after source when an edit breaks parsing | — | default |
| `edit.autoRepair.enabled` | `false` | Ask the smol model to fix edit-caused parse breaks | — | default — hidden extra model calls ⚠ |
| `readLineNumbers` | `false` | Prefix plain line numbers to read output | 0146 | default — hashline anchors already carry line numbers |
| `read.defaultLimit` | `300` | Lines returned by read without a limit | 0153 | default |
| `read.renderMarkdown` | `false` | Show Markdown reads as rendered previews | — | default |
| `read.summarize.enabled` | `true` | Structural code outline when read has no selector | 0153 | default |
| `read.summarize.prose` | `false` | Outline Markdown and text files too | — | default |
| `read.summarize.minBodyLines` | `4` | Shortest body an outline folds | — | default |
| `read.summarize.minCommentLines` | `6` | Shortest block comment an outline folds | — | default |
| `read.summarize.minTotalLines` | `100` | Files shorter than this are read verbatim | — | default |
| `read.summarize.unfoldUntil` | `50` | Unfold outline until at least this many lines | — | default |
| `read.summarize.unfoldLimit` | `100` | Hard ceiling on outline size while unfolding | — | default |
| `read.toolResultPreview` | `false` | Show read results inline in the transcript | — | default |
| `lsp.enabled` | `true` | Enable the lsp tool | 0288 LSP | default |
| `lsp.lazy` | `true` | Start language servers on first use | 0288 | default |
| `lsp.shared` | `true` | Share one language server per project across omp processes | 0288; 0388 many thread panes | default — many herdr panes share servers |
| `lsp.formatOnWrite` | `false` | Format files through LSP after write | — | default |
| `lsp.diagnosticsOnWrite` | `true` | Return diagnostics after write | 0288 | default |
| `lsp.diagnosticsOnEdit` | `false` | Return diagnostics after edit | 0288 LSP | `true` — 0288 wants problems in every edit's result |
| `lsp.diagnosticsDeduplicate` | `true` | Only show new or changed diagnostics after edits | 0288 | default |

### Sources

Data leaving the machine (⚠):

| Setting | Destination | Off switch | Source |
|---|---|---|---|
| `dev.autoqa`, `dev.autoqaPush.endpoint` | Tool-issue reports POSTed to `https://qa.omp.sh/v1/grievances`. A Yes/No consent popup shows before the first report; after Yes, pushes run in the background. | `dev.autoqa: false` or `PI_AUTO_QA=0` | `packages/coding-agent/src/tools/settings.ts:962-985`; `packages/coding-agent/src/tools/report-tool-issue.ts:8-31` |
| `web_search.enabled` | Queries go to search providers: Exa (`api.exa.ai`, or keyless `mcp.exa.ai`), DuckDuckGo, Brave, Jina, Tavily, Perplexity, Parallel. Provider choice is a setting on another tab. | `web_search.enabled: false` | `packages/coding-agent/src/web/search/providers/exa.ts:23-24`, `:446-448`; `duckduckgo.ts:19`; `brave.ts:17`; `jina.ts:17`; `tavily.ts:16`; `perplexity-auth.ts:4-5`; `packages/coding-agent/src/web/parallel.ts:5` |
| `fetch.enabled` | The URL the agent reads. The HTML reader chain is native → trafilatura → lynx → parallel → firecrawl → jina. Parallel and firecrawl need keys; jina (`r.jina.ai/<url>`) runs without a key. | `fetch.enabled: false` | `packages/coding-agent/src/tools/fetch.ts:566-576`, `:631-662` |
| `github.enabled` | GitHub through the `gh` CLI | `github.enabled: false` (default) | `packages/coding-agent/src/utils/github.ts:141` |
| `speechgen.enabled` | Local Kokoro: first use runs `bun install kokoro-js` (npm) and downloads the weights `onnx-community/Kokoro-82M-v1.0-ONNX` (Hugging Face). xAI `grok-tts`: the text goes to xAI. | `speechgen.enabled: false` (default) | `packages/coding-agent/src/tts/runtime.ts:4-5`; `packages/coding-agent/src/tts/models.ts:68-69`; `packages/coding-agent/src/tts/tts-worker.ts:127-164`; `packages/coding-agent/src/priority.json:95-98` |
| `generate_image.enabled` | The image model's provider (xAI, OpenRouter, Google, DeepInfra, …) | `generate_image.enabled: false` (default) | `packages/coding-agent/src/priority.json:75-80` |
| `eval.autoProvision` | Creates the managed JS environment; packages are installed with `bun add` from the npm registry | `eval.autoProvision: false` | `packages/coding-agent/src/eval/js/package-installer.ts:80-86`, `:145-148`; `packages/coding-agent/src/cli/npm-registry.ts:24` |
| `find.enabled`, `images.questionTimeoutMs`, `edit.autoRepair.enabled` | File contents go to the configured judge, vision or smol model's provider. These are providers already in use, not new hosts. | `find.enabled: off`; `edit.autoRepair.enabled: false` (default) | `packages/coding-agent/src/tools/settings.ts:521`, `:668`; `packages/coding-agent/src/edit/settings.ts:167` |
| `browser.enabled` | Any URL the agent navigates to | `browser.enabled: false` | `packages/coding-agent/src/tools/browser/settings.ts:8` |

Registration of every id (`register({ id: … })`):

- `packages/coding-agent/src/edit/settings.ts`: `edit.mode` :37, `edit.fuzzyMatch` :69, `edit.fuzzyThreshold` :82, `edit.streamingAbort` :107, `edit.recoverInlineEdits` :119, `edit.blockAutoGenerated` :131, `edit.enforceSeenLines` :143, `edit.blackbox.enabled` :155, `edit.autoRepair.enabled` :167
- `packages/coding-agent/src/tools/settings.ts`: `tools.artifactSpillThreshold` :13, `tools.artifactTailBytes` :39, `tools.artifactHeadBytes` :61, `tools.outputMaxColumns` :85, `tools.artifactTailLines` :107, `tools.artifactMaxBytes` :128, `readLineNumbers` :149, `read.defaultLimit` :161, `read.renderMarkdown` :180, `read.summarize.enabled` :192, `read.summarize.prose` :204, `read.summarize.minBodyLines` :216, `read.summarize.minCommentLines` :228, `read.summarize.minTotalLines` :240, `read.summarize.unfoldUntil` :252, `read.summarize.unfoldLimit` :265, `read.toolResultPreview` :278, `todo.enabled` :348, `todo.reminders` :360, `todo.remindersMax` :372, `todo.eager` :390, `tasks.todoClearDelay` :412, `glob.enabled` :434, `grep.enabled` :446, `grep.contextBefore` :458, `grep.contextAfter` :477, `astGrep.enabled` :497, `astEdit.enabled` :509, `find.enabled` :521, `debug.enabled` :546, `launch.enabled` :558, `speechgen.enabled` :570, `generate_image.enabled` :582, `computer.enabled` :595, `ratchet.enabled` :607, `archive.enabled` :619, `computer.display` :632, `computer.maxWidth` :644, `computer.maxHeight` :656, `images.questionTimeoutMs` :668, `checkpoint.enabled` :688, `fetch.enabled` :701, `vault.enabled` :713, `github.enabled` :726, `github.cache.enabled` :739, `github.cache.softTtlSec` :751, `github.cache.hardTtlSec` :763, `web_search.enabled` :776, `security.enabled` :788, `ask.enabled` :801, `tools.intentTracing` :814, `tools.abortOnFabricatedResult` :827, `tools.speculativeExecution.enabled` :840, `tools.speculativeExecution.maxInFlight` :853, `tools.maxTimeout` :871, `async.enabled` :893, `tools.xdev` :913, `tools.xdevDocs` :926, `tools.xdevInlineDevices` :949, `dev.autoqa` :962, `dev.autoqaPush.endpoint` :976
- `packages/coding-agent/src/lsp/settings.ts`: `lsp.enabled` :9, `lsp.lazy` :21, `lsp.shared` :34, `lsp.formatOnWrite` :47, `lsp.diagnosticsOnEdit` :71, `lsp.diagnosticsDeduplicate` :83
- `packages/coding-agent/src/config/registry.ts`: `lsp.diagnosticsOnWrite` :13
- `packages/coding-agent/src/exec/settings.ts`: `bash.enabled` :83, `bash.allowCompoundCommands` :95, `bash.autoBackground.enabled` :108, `bash.gitGuard` :121, `bash.patterns` :134, `bashInterceptor.enabled` :148, `bash.direnv` :166, `bash.direnvLoadTimeoutMs` :180, `shellMinimizer.enabled` :194, `shellMinimizer.sourceOutlineLevel` :230
- `packages/coding-agent/src/eval/settings.ts`: `eval.py` :9, `eval.js` :22, `eval.autoProvision` :35, `eval.tools.enabled` :47, `eval.workpool.freshAgents` :60, `eval.autoBackground.enabled` :72, `python.kernelMode` :93, `python.interpreter` :106
- `packages/coding-agent/src/extensibility/settings.ts`: `extensionHandlers.toolCallTimeoutMs` :152
- `packages/coding-agent/src/tools/browser/settings.ts`: `browser.enabled` :8, `browser.cdpUrl` :20, `browser.relay` :33, `browser.relayUrl` :46, `browser.headless` :58, `browser.cmux` :70, `browser.tern` :83, `browser.freezeOnTurnEnd` :96, `browser.idleCloseSec` :109, `browser.screenshotDir` :128
- `packages/coding-agent/src/ida/settings.ts`: `ida.enabled` :9, `ida.python` :23, `ida.installDir` :36, `ida.maxOpen` :50, `ida.idleCloseSec` :70
- `packages/coding-agent/src/mcp/settings.ts`: `mcp.enableProjectConfig` :10, `mcp.startupTimeoutMs` :22, `mcp.renderMarkdownResults` :34, `mcp.notifications` :47, `mcp.notificationDebounceMs` :59

Other cited lines:

- `bash.gitGuard` is passed to the shell executor: `packages/coding-agent/src/tools/bash.ts:874`, `:1534`.
- `bash.allowCompoundCommands` splits chains only for POSIX shells: `packages/coding-agent/src/tools/bash.ts:508-511`.
- `bashInterceptor.patterns` (config-only) defaults to `DEFAULT_BASH_INTERCEPTOR_RULES`: `packages/coding-agent/src/exec/settings.ts:159-163`.
- The JS executor reads `eval.autoProvision`: `packages/coding-agent/src/eval/js/executor.ts:139`; `packages/coding-agent/src/tools/eval.ts:404`.
- `browser.cdpUrl`, `browser.relayUrl` and `browser.screenshotDir` default to `undefined` (shown as unset): `packages/coding-agent/src/tools/browser/settings.ts:22`, `:48`, `:130`.

## Settings: tasks, context and memory tabs

Rows are in `/settings` dump order. Default `(unset)` = no default value. Each id's registration (label, values, default) is cited under its table. ⚠ = can send data off the machine; see Sources.

### Tasks tab

| id | default | what it does | serves | recommendation |
|---|---|---|---|---|
| `task.isolation.enabled` | `false` | Run subagents in an isolated checkout copy, then integrate. | 0286 subagents, 0388 | Keep `false`; the thread already owns a worktree. |
| `isolation.backend` | `"auto"` | Backend for isolation and worktree cloning (ProjFS, block-clone, copy). | 0286 | Keep `auto`. |
| `worktree.clone` | `true` | New worktrees start as copy-on-write clone of checkout. | 0388 | Keep `true`. |
| `worktree.cleanSource` | `false` | `/wt` resets and cleans the source checkout after carry-over. | 0388 | Keep `false`; never discard work in the main checkout. |
| `worktree.onStart` | `"off"` | Fresh interactive sessions start in a new linked worktree. | 0388 herdr worktrees | Keep `off`; herdr's `worktree.create` makes thread worktrees. |
| `worktree.onExit` | `"keep"` | On exit, keep, ask about, or remove session worktrees. | 0388 | Keep `keep`; herdr owns worktree lifecycle. |
| `task.isolation.apply` | `true` | Auto-apply successful isolated task changes to parent. | 0286 | Keep `true` (inert while isolation is off). |
| `task.isolation.merge` | `"patch"` | Integrate isolated changes by patch or branch merge. | 0286 | Keep `patch`. |
| `task.isolation.commits` | `"generic"` | Commit messages for nested repo changes: generic or AI. | 0286 | Keep `generic`. |
| `worktree.base` | (unset) | Base dir for agent-managed worktrees; unset = `~/.omp/wt`. | 0388 | Leave unset; herdr thread worktrees are separate. |
| `task.eager` | `"default"` | How hard the agent is pushed to delegate to subagents. | 0286 | Keep `default`. |
| `task.batch` | `true` | Task tool takes `{context, tasks[]}`; one subagent per item. | 0286 | Keep `true`. |
| `task.structured` | `false` | Subagents return schema-validated results via a yield tool. | 0286 | Keep `false`. |
| `task.speculativeLaunch` | `true` | Start each batch subagent as soon as its item streams. | 0286 | Keep `true`. |
| `task.enableEffort` | `false` | Expose per-spawn effort parameter on task calls. | 0286, principle 2 | Keep `false`; effort pinned per role. |
| `task.maxConcurrency` | `32` | Max concurrent subagents. | 0286 | Keep `32`. |
| `task.enableLsp` | `false` | Let subagents use the `lsp` tool. | 0286, 0288 LSP | Keep `false`. |
| `task.maxRecursionDepth` | `2` | Levels subagents may spawn sub-subagents. | 0286 | Keep `2`. |
| `task.maxRuntimeMs` | `0` | Hard wall-clock limit per subagent; 0 = none. | 0286 | Keep `0`. |
| `task.completionProbe` | `true` | Periodically ask running subagent how complete it is. | 0286 | Keep `true` (interactive only; extra side requests). |
| `task.agentIdleTtlMs` | `420000` | Idle subagent lifetime before parking to disk. | 0286 | Keep `420000`. |
| `task.softRequestBudget` | `200` | Requests per subagent before wrap-up notice; 1.5× force-stops. | 0286 | Keep `200`. |
| `task.softRequestBudgetNotice` | `true` | Inject one wrap-up notice at the soft budget. | 0286 | Keep `true`. |
| `task.maxEffort` | `"max"` | Ceiling for per-spawn effort hints. | 0286 | Keep `max`. |
| `task.prewalk` | `false` | Generic task agent plans, then hands off to smol at first edit. | 0286 | Keep `false`. |
| `plan.enabled` | `true` | Enable read-only plan mode. | 0285/0348 plan mode, 0388 project pane | Keep `true`. |
| `plan.defaultOnStartup` | `false` | Enter plan mode at the start of every new session. | 0388 project pane in plan mode | Keep `false` globally (note 2). |
| `plan.autosave` | `false` | Save approved plans to disk on plan-mode exit. | V9 | Keep `false`; default dir `<project>/.omp/plans/` dirties the repo. |
| `plan.autosaveDir` | (unset) | Plan autosave dir; empty = `<project>/.omp/plans/`. | V9 | Leave unset. |
| `goal.enabled` | `true` | Per-session goal mode and hidden goal tool. | — | Keep `true`. |
| `goal.statusInFooter` | `true` | Show goal token budget in status line. | — | Keep `true`. |
| `goal.continuationModes` | `["interactive"]` | Run modes where goals may auto-continue. | — | Keep `["interactive"]`; print-mode evals stay single-turn. |
| `title.generator` | `"fork"` | Names untitled sessions: fork side-turn or tiny role. | — | Keep `fork`. |
| `title.refreshOnReplan` | `true` | Refresh generated title after todo replans. | — | Keep `true`. |
| `skills.enableSkillCommands` | `true` | Register skills as `/skill:name` commands. | — | Keep `true`. |
| `commands.enableClaudeUser` | `false` | Load commands from `~/.claude/commands/`. | — | Keep `false`. |
| `commands.enableClaudeProject` | `true` | Load commands from repo `.claude/commands/`. | — | Keep `true`. |
| `commands.enableOpencodeUser` | `false` | Load commands from `~/.config/opencode/commands/`. | — | Keep `false`. |
| `commands.enableOpencodeProject` | `true` | Load commands from repo `.opencode/commands/`. | — | Keep `true`. |

Registered at: `packages/coding-agent/src/task/settings.ts` — `task.isolation.enabled` :27, `isolation.backend` :40, `worktree.clone` :77, `worktree.cleanSource` :91, `worktree.onStart` :105, `worktree.onExit` :124, `task.isolation.apply` :148, `task.isolation.merge` :162, `task.isolation.commits` :180, `worktree.base` :198, `task.eager` :219, `task.batch` :242, `task.structured` :256, `task.speculativeLaunch` :269, `task.enableEffort` :282, `task.maxConcurrency` :295, `task.enableLsp` :318, `task.maxRecursionDepth` :331, `task.maxRuntimeMs` :351, `task.completionProbe` :371, `task.agentIdleTtlMs` :384, `task.softRequestBudget` :397, `task.softRequestBudgetNotice` :444, `task.maxEffort` :457, `task.prewalk` :524; `packages/coding-agent/src/plan-mode/settings.ts` — `plan.enabled` :13, `plan.defaultOnStartup` :25, `plan.autosave` :38, `plan.autosaveDir` :51; `packages/coding-agent/src/goals/settings.ts` — `goal.enabled` :4, `goal.statusInFooter` :16, `goal.continuationModes` :30; `packages/coding-agent/src/utils/title-settings.ts` — `title.generator` :35, `title.refreshOnReplan` :57; `packages/coding-agent/src/extensibility/settings.ts` — `skills.enableSkillCommands` :32, `commands.enableClaudeUser` :104, `commands.enableClaudeProject` :116, `commands.enableOpencodeUser` :128, `commands.enableOpencodeProject` :140.

Note 2: `plan.defaultOnStartup` enters plan mode "at the start of every new session" (`packages/coding-agent/src/plan-mode/settings.ts:25`). A profile holding it could start thread panes in plan mode (0388); whether it applies when `omp --continue` reopens an existing session is not verified here.

### Context tab

| id | default | what it does | serves | recommendation |
|---|---|---|---|---|
| `workspace.additionalDirectories` | `[]` | Extra roots every session can read; `/add-dir`, `/remove-dir`. | — | Keep `[]`; threads stay inside their worktree (0388). |
| `contextFiles.extra` | `[]` | Extra instruction filenames loaded beside `AGENTS.md`. | 0387 config home | Keep `[]`; his persona moves to `~/.omp/agent/AGENTS.md` (F1 step 2). |
| `contextPromotion.enabled` | `false` | On overflow, switch to a larger-context model instead of compacting. | 0388 longevity | Keep `false`; compaction carries long threads, model stays fixed (principle 2). |
| `extendedContext` | `false` | Use larger context windows where offered; may cost premium. | — | Keep `false` (cost). |
| `compaction.enabled` | `true` | Auto-compact when context gets too large. | 0388 longevity, V9 compaction | Keep `true`. |
| `compaction.experimentalContextManagement` | `false` | Experimental notes plus searchable raw history across context windows. | V9 | Keep `false`; experimental, and its rollovers write `method`-less entries (`packages/coding-agent/src/session/session-maintenance.ts:1680`). |
| `compaction.midTurnEnabled` | `true` | Check thresholds at safe mid-turn tool-loop boundaries. | 0388 longevity | Keep `true`. |
| `compaction.methodOrder` | `["remote", "snapcompact", "handoff", "shake", "soft"]` | Fallback order of compaction methods tried automatically. | 0388, 0389 watcher | ⚠ Text methods first: `["handoff","soft","remote","shake","snapcompact"]` (note 1). |
| `compaction.thresholdPercent` | `-1` | Percent of window that triggers compaction; -1 = reserve-based. | 0388 | Keep default unless threads compact too late; then `80`. |
| `compaction.thresholdTokens` | `-1` | Fixed token trigger; overrides percent. | 0388 | Keep `-1`. |
| `compaction.handoffSaveToDisk` | `false` | Save auto-handoff docs as markdown in the session artifacts dir. | 0389 watcher, V9 | Keep `false`; the watcher reads session JSONL, not artifacts. |
| `compaction.remoteStreamingV2Enabled` | `true` | Use Responses streaming for remote compaction models. | — | Keep `true`. |
| `compaction.asyncEnabled` | `true` | Summarize in background before the threshold is hit. | 0388 | Keep `true`. |
| `compaction.idleEnabled` | `false` | Compact while idle above a token count. | 0388 | Keep `false`; compaction only when needed. |
| `compaction.idleThresholdTokens` | `200000` | Token count that triggers idle compaction. | — | Keep `200000` (inert while idle compaction is off). |
| `compaction.idleTimeoutSeconds` | `300` | Idle seconds before idle compaction. | — | Keep `300` (inert). |
| `compaction.supersedeReads` | `true` | Prune older read results when a file is re-read. | — | Keep `true`. |
| `compaction.dropUseless` | `true` | Prune consumed empty/timeout tool results. | — | Keep `true`. |
| `snapcompact.systemPrompt` | `"none"` | Experimental: render system prompt as PNG for vision models. | — | Keep `none`; loses prompt caching. |
| `snapcompact.toolResults` | `false` | Experimental: render large tool results as PNG images. | — | Keep `false`. |
| `tools.format` | `"auto"` | How tools reach the model: native calls or a text dialect. | principle 2 | Keep `auto` (native unless the model lacks it); `native` if he wants a loud failure instead. |
| `snapcompact.shape` | `"auto"` | Glyph frame shape used by snapcompact images. | — | Keep `auto`. |
| `branchSummary.enabled` | `false` | Offer a summary when leaving a session branch. | V9 | Keep `false`; `/tree` branching is rare in thread panes. |
| `ttsr.enabled` | `true` | Interrupt mid-stream when output matches rule patterns. | 0287 advisor/TTSR | Keep `true`; his stream rules convert to TTSR rules (F1 step 2). |
| `ttsr.judge` | `"auto"` | Ask judge role about `question` rules after replies. | 0287, principle 2 | ⚠ `off` unless he writes question rules (model-judged, principle 2). |
| `ttsr.contextMode` | `"discard"` | Discard or keep partial output when a rule fires. | 0287 | Keep `discard`. |
| `ttsr.interruptMode` | `"always"` | When to interrupt vs warn after completion. | 0287 | Keep `always`. |
| `ttsr.repeatMode` | `"once"` | Rule fires once per session or after a gap. | 0287 | Keep `once`. |
| `ttsr.repeatGap` | `10` | Messages before a rule may fire again. | 0287 | Keep `10` (inert with `once`). |
| `ttsr.builtinRules` | `true` | Load omp's bundled TTSR rules. | 0287 | Keep `true`; disable single rules by name if noisy. |
| `ttsr.disabledRules` | `[]` | Rule names to ignore. | 0287 | Keep `[]`. |

Registered at: `packages/coding-agent/src/session/context-settings.ts` — `workspace.additionalDirectories` :13, `contextFiles.extra` :49, `contextPromotion.enabled` :77, `extendedContext` :92, `compaction.enabled` :106, `compaction.experimentalContextManagement` :118, `compaction.midTurnEnabled` :130, `compaction.methodOrder` :143, `compaction.thresholdPercent` :158, `compaction.thresholdTokens` :185, `compaction.handoffSaveToDisk` :235, `compaction.remoteStreamingV2Enabled` :247, `compaction.asyncEnabled` :259, `compaction.idleEnabled` :303, `compaction.idleThresholdTokens` :315, `compaction.idleTimeoutSeconds` :338, `compaction.supersedeReads` :358, `compaction.dropUseless` :370, `snapcompact.systemPrompt` :414, `snapcompact.toolResults` :441, `tools.format` :454, `snapcompact.shape` :501, `branchSummary.enabled` :611; `packages/coding-agent/src/export/ttsr-settings.ts` — `ttsr.enabled` :9, `ttsr.judge` :21, `ttsr.contextMode` :44, `ttsr.interruptMode` :57, `ttsr.repeatMode` :76, `ttsr.repeatGap` :89, `ttsr.builtinRules` :108, `ttsr.disabledRules` :120.

Note 1: methods are `remote` (provider-native server compaction: OpenAI Responses compact, Anthropic compaction beta), `snapcompact` ("archive history onto dense bitmap images… no LLM call"), `handoff` (handoff document becomes the summary), `soft` (in-place summary by a compaction model), `shake` (drop heavy content, no LLM) (`packages/coding-agent/src/session/compaction-methods.ts:11-38`); default order `remote, snapcompact, handoff, shake, soft` (`packages/coding-agent/src/session/compaction-methods.ts:44-50`). Every compaction entry carries `summary` and `method` (`packages/coding-agent/src/session/session-entries.ts:125-134`). 0389's watcher wants prose summaries, so the order puts `handoff`/`soft` first; whether `remote` and `snapcompact` entries hold readable prose is not verified here.

`compaction.handoffSaveToDisk` writes auto-triggered handoffs into the session artifacts dir (`packages/coding-agent/src/session/session-handoff.ts:213-216`).

### Memory tab

| id | default | what it does | serves | recommendation |
|---|---|---|---|---|
| `memory.backend` | `"off"` | Selects memory backend: off, local, hindsight, mnemopi, sharpshooter. | 0389 memory = omp SQLite backend + Graphiti | ⚠ `mnemopi`, with `mnemopi.llmMode: none` (comparison below). |
| `sharpshooter.model` | (unset) | Model for sharpshooter extraction/consolidation; empty = smol. | — | Leave unset (backend unused). |
| `autolearn.enabled` | `false` | After stop, nudge agent to save lessons and managed skills. | — | Keep `false`; no model-judged memory writes (0389). |
| `autolearn.autoContinue` | `false` | Auto-run one private capture turn at stop. | — | Keep `false`. |
| `mnemopi.dbPath` | (unset) | SQLite DB path; default `memories/mnemopi/mnemopi.db`. | 0389 | Leave unset (default under `~/.omp/agent`). |
| `mnemopi.bank` | (unset) | Base name for shared bank; per-project banks derive from it. | 0389 | Leave unset. |
| `mnemopi.scoping` | `"per-project"` | Bank per project, one global, or project writes + global recall. | 0389, principle 6 | Keep `per-project`; client memory never leaks across projects. |
| `mnemopi.embeddingVariant` | `"en"` | Local embedding family: English or multilingual. | 0389 | Keep `en`. |
| `mnemopi.autoRecall` | `true` | Recall memories into each session's first turn. | 0389, V9 | Keep `true`. |
| `mnemopi.autoRetain` | `true` | Retain completed turns into Mnemopi. | 0389, V9 nothing gets lost | Keep `true`. |
| `mnemopi.polyphonicRecall` | `false` | Fuse graph, fact, vector and temporal recall. | 0389 | Keep `false`; the graph is Graphiti's job. |
| `mnemopi.enhancedRecall` | `false` | Cache recall results for repeated queries. | — | Keep `false`. |
| `mnemopi.proactiveLinking` | `false` | Link new memories into Mnemopi's episodic graph on store. | 0389 | Keep `false`; Graphiti holds the graph. |
| `mnemopi.noEmbeddings` | `false` | FTS-only recall, no vector embeddings. | 0389 | Keep `false`; embeddings run locally. |
| `mnemopi.embeddingModel` | (unset) | Explicit embedding model id; overrides variant. | — | ⚠ Leave unset; an API model id sends memory text off-machine. |
| `mnemopi.embeddingApiUrl` | (unset) | OpenAI-compatible embedding endpoint. | — | ⚠ Leave unset. |
| `mnemopi.embeddingApiKey` | (unset) | Embedding endpoint API key. | — | Leave unset (credential). |
| `mnemopi.llmMode` | `"smol"` | LLM for extraction: none, memory/tiny role, or remote endpoint. | 0389, principle 6 | ⚠ Set `none`; or keep `smol` only with a local memory-role model. |
| `mnemopi.llmBaseUrl` | (unset) | Endpoint for `remote` LLM mode. | — | ⚠ Leave unset. |
| `mnemopi.llmApiKey` | (unset) | API key for `remote` LLM mode. | — | Leave unset (credential). |
| `mnemopi.llmModel` | (unset) | Model name for `remote` LLM mode. | — | Leave unset. |
| `hindsight.apiUrl` | `"http://localhost:8888"` | Hindsight server URL (cloud or self-hosted). | — | ⚠ Unused; leave default. |
| `hindsight.apiToken` | (unset) | Bearer token for Hindsight server. | — | Leave unset (credential). |
| `hindsight.bankId` | (unset) | Hindsight bank id; default project name. | — | Leave unset. |
| `hindsight.scoping` | `"per-project-tagged"` | Bank scoping: global, per-project, or tagged shared bank. | — | Leave default (unused). |
| `hindsight.autoRecall` | `true` | Recall Hindsight memories on first turn. | — | Leave default (unused). |
| `hindsight.autoRetain` | `true` | Retain transcript every N turns and at boundaries. | — | ⚠ Leave default (unused). |
| `hindsight.retainMode` | `"full-session"` | One document per session, or chunked last turns. | — | Leave default (unused). |
| `hindsight.mentalModelsEnabled` | `true` | Load bank's mental models into instructions at boot. | — | Leave default (unused). |
| `hindsight.mentalModelAutoSeed` | `true` | Create built-in mental models on the bank at start. | — | ⚠ Leave default (unused). |

Registered at: `packages/coding-agent/src/memory-backend/settings.ts` — `memory.backend` :13; `packages/coding-agent/src/sharpshooter/settings.ts` — `sharpshooter.model` :8; `packages/coding-agent/src/autolearn/settings.ts` — `autolearn.enabled` :11, `autolearn.autoContinue` :24; `packages/coding-agent/src/mnemopi/settings.ts` — `mnemopi.dbPath` :10, `mnemopi.bank` :23, `mnemopi.scoping` :36, `mnemopi.embeddingVariant` :76, `mnemopi.autoRecall` :103, `mnemopi.autoRetain` :116, `mnemopi.polyphonicRecall` :129, `mnemopi.enhancedRecall` :143, `mnemopi.proactiveLinking` :157, `mnemopi.noEmbeddings` :171, `mnemopi.embeddingModel` :184, `mnemopi.embeddingApiUrl` :200, `mnemopi.embeddingApiKey` :213, `mnemopi.llmMode` :227, `mnemopi.llmBaseUrl` :251, `mnemopi.llmApiKey` :264, `mnemopi.llmModel` :278; `packages/coding-agent/src/hindsight/settings.ts` — `hindsight.apiUrl` :17, `hindsight.apiToken` :31, `hindsight.bankId` :46, `hindsight.scoping` :62, `hindsight.autoRecall` :109, `hindsight.autoRetain` :123, `hindsight.retainMode` :137, `hindsight.mentalModelsEnabled` :245, `hindsight.mentalModelAutoSeed` :259.

#### `memory.backend` values

Selection: `packages/coding-agent/src/memory-backend/resolve.ts:13-28`. Paths are under `~/.omp/agent` (`memories` dir: `packages/utils/src/dirs.ts:1001-1003`).

| value | where data lives | local SQLite | model calls | network | source |
|---|---|---|---|---|---|
| `off` (default) | nothing | — | none | none | `packages/coding-agent/src/memory-backend/resolve.ts:17,28` |
| `local` | job state in `agent.db` (SQLite); `memories/<project>/MEMORY.md`, `memory_summary.md`, `skills/`, `learned.md` | yes, for job state only; memory itself is markdown; no search | rollout summaries on the default role, consolidation on smol | model provider | `packages/utils/src/dirs.ts:921-923`; `packages/coding-agent/src/memories/storage.ts:67-68`; `packages/coding-agent/src/memories/index.ts:381-384,539-543,1097-1098,1386-1398`; `packages/coding-agent/src/memory-backend/local-backend.ts:14-17,42-44` |
| `mnemopi` | `memories/mnemopi/mnemopi.db`, one bank per project by default | yes | `llmMode: smol` (default) calls the memory role (tiny, then smol) unless that model is `local-inference`; `none` = no LLM; `remote` = configured endpoint | embeddings run locally (`BAAI/bge-base-en-v1.5` via fastembed/onnxruntime child), model downloaded once on first use; otherwise only with `smol`/`remote` LLM or an API embedding model/URL | `packages/coding-agent/src/mnemopi/config.ts:73-87,151-204`; `packages/coding-agent/src/mnemopi/backend.ts:545-606`; `packages/coding-agent/src/config/model-resolver.ts:1113-1119`; `packages/coding-agent/src/mnemopi/embed-worker.ts:1-8`; `packages/coding-agent/src/mnemopi/embed-client.ts:226-229`; `packages/mnemopi/src/core/embeddings.ts:251-265` |
| `sharpshooter` | `memories/sharpshooter/<bank>/` `architecture.md`, `product.md`, `style.md`, queue JSON, `state.json` | no | extraction and consolidation on `sharpshooter.model`, else smol | model provider | `packages/coding-agent/src/sharpshooter/paths.ts:4-31`; `packages/coding-agent/src/sharpshooter/extract.ts:153-170,219`; `packages/coding-agent/src/sharpshooter/consolidate.ts:141,160` |
| `hindsight` | the Hindsight server's bank (storage not in omp-src: not found) | no (remote) | server-side: not found in omp-src | HTTP to `hindsight.apiUrl` (default `http://localhost:8888`); inert if unset | `packages/coding-agent/src/hindsight/settings.ts:17-20`; `packages/coding-agent/src/hindsight/client.ts:569`; `packages/coding-agent/src/hindsight/backend.ts:71-72` |

Recommendation (0389): `memory.backend: mnemopi`, `mnemopi.llmMode: none`, other mnemopi rows at default. It is the only SQLite memory store; with `none` it passes `llm: false` (`packages/coding-agent/src/mnemopi/backend.ts:545-548`) and embeddings stay on the machine after the one-time model download. What Mnemopi's retain/recall loses without an LLM is not verified here. Alternative: keep `smol` and set the memory role to a `local-inference` model (`packages/coding-agent/src/mnemopi/backend.ts:591-597`). Not `local`: its store is markdown and it sends session rollouts to the model provider.

#### Sources (⚠ data leaving the machine)

- `memory.backend: hindsight`, `hindsight.apiUrl`, `hindsight.autoRetain`, `hindsight.mentalModelAutoSeed` → session transcripts and memories go by HTTP to the Hindsight URL (cloud or self-hosted) (`packages/coding-agent/src/hindsight/settings.ts:17-20,123`; `packages/coding-agent/src/hindsight/client.ts:569`).
- `mnemopi.embeddingModel`, `mnemopi.embeddingApiUrl`, env `MNEMOPI_EMBEDDING_API_URL` / `OPENROUTER_BASE_URL` → memory text goes to the embedding endpoint, default host `https://openrouter.ai/api/v1`, when the model is an API model or the URL is not OpenRouter (`packages/mnemopi/src/core/embeddings.ts:223-229,251-265`); an OpenRouter login is passed as the key (`packages/coding-agent/src/mnemopi/backend.ts:520-544`).
- `mnemopi.llmMode: smol` (default) → retained memory text goes to the memory-role model's provider (`packages/coding-agent/src/mnemopi/backend.ts:567-644`; `packages/coding-agent/src/config/model-resolver.ts:1113-1119`).
- `mnemopi.llmMode: remote` + `mnemopi.llmBaseUrl` → memory text goes to that endpoint (`packages/coding-agent/src/mnemopi/backend.ts:552-564`).
- `memory.backend: mnemopi`, first use → embedding model download; host not named in omp-src (not found); fastembed honours `HF_HUB_*` and proxy env (`packages/coding-agent/src/mnemopi/embed-client.ts:67-72,226-229`).
- `memory.backend: local` → session rollouts go to the default-role and smol-role providers for summaries (`packages/coding-agent/src/memories/index.ts:381-384,539-543,880,1023`).
- `memory.backend: sharpshooter`, `sharpshooter.model` → transcripts go to that model or smol (`packages/coding-agent/src/sharpshooter/extract.ts:153-170,219`).
- `compaction.methodOrder` containing `remote` → history goes to the provider's server compaction (OpenAI Responses compact, Anthropic compaction beta) (`packages/coding-agent/src/session/compaction-methods.ts:13-16`); same provider as the session.
- `ttsr.judge` (`auto`/`on`) → replies, reasoning and tool calls go to the judge role's model (`packages/coding-agent/src/export/ttsr-settings.ts:20-39`).

Principle 6 (`docs/plan.md:657`) allows requests to the model provider doing the work; the rows above that reach a second provider or a non-provider host are the ones to keep off.

## Settings: config-file only (no /settings row)

The 136 settings omp 18.10.3 registers without a `/settings` row (`tab: null` in the dump). Set them by hand in `~/.omp/agent/config.yml`; `jocasta setup` copies only his non-default values (0396). `:N` after an id is its `id:` line in the declaring file. `unset` = no default (`undefined`). `serves`: record number, vision line (V*n*) or principle (P*n*, plan §4). ⚠ = data can leave the machine; destinations in **Sources**.


### `auth.*`

Declared in `packages/coding-agent/src/config/model-settings.ts`. ⚠ Credentials leave the machine only if a broker URL is set; destination = that URL (docs/auth-broker-gateway.md:279).

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `auth.broker.url` :43 | unset | `OMP_AUTH_BROKER_URL` | ⚠ Remote credential broker URL; replaces local credential store. | 0387; P6 | default (unset); logins stay local in `~/.omp/agent`. |
| `auth.broker.token` :50 | unset | `OMP_AUTH_BROKER_TOKEN` | ⚠ Bearer token for the auth broker. | 0387 | default (unset); no broker. |
| `auth.accountPolicies` :58 | `[]` | — | Per-account OAuth routing: priority, reserve percent. | 0386 | default; one login per provider. |

### Top-level keys

Declared in `packages/coding-agent/src/config/model-settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `enabledModels` :64 | `[]` | — | Allow-list of models; empty = all available. | V8; P1 | default; roles pick models. |
| `enabledProviders` :71 | `[]` | — | Opt foreign user-level config sources (Claude, Codex…) into discovery. | 0387; P3 | default (empty); only `~/.omp/agent` config counts. |
| `disabledProviders` :78 | `[]` | — | Removes model providers / discovery sources by id. | V8; P1 | Walkthrough: list providers he never uses. |
| `modelRoles` :109 | `{}` | — | Role name → model selector (default, smol, slow, plan…). | 0386; 0390; P2 | Set in `/models`; `graph.main`/`graph.small` map to these (0390). |
| `modelPresets` :113 | `{}` | — | Named role/thinking presets for `/modelpreset`. | V8 | default; add only if he switches setups. |
| `modelTags` :118 | `{}` | — | Custom role/tag metadata; can add chat roles. | V8 | default. |
| `modelProviderOrder` :120 | `[]` | — | Preferred provider when a model id is ambiguous. | P2 | Walkthrough: set if a model exists on two logins. |
| `cycleOrder` :122 | `["smol", "default", "slow"]` | — | Roles cycled by the model switcher. | V8 | default. |

### Top-level keys

Declared in `packages/coding-agent/src/modes/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `setupVersion` :28 | `0` | — | Records completed first-run setup wizard version. | — | default; written by omp, never by hand. |

### `statusLine.*`

Declared in `packages/coding-agent/src/modes/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `statusLine.leftSegments` :278 | `["vim", "model", "mode", "path", "git", "pr"]` | — | Left status-line segments for custom preset. | V12 | default. |
| `statusLine.rightSegments` :285 | `["session_name", "token_total", "cost", "context_pct"]` | — | Right status-line segments for custom preset. | V12 | default; already shows cost and context %. |
| `statusLine.segmentOptions` :292 | `{}` | — | Per-segment options, e.g. `path.abbreviate`. | V12 | default. |

### `tui.*`

Declared in `packages/coding-agent/src/modes/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `tui.maxInlineImageColumns` :336 | `100` | — | Max inline image width in columns; 0 = unlimited. | V6 | default. |
| `tui.maxInlineImageRows` :345 | `20` | — | Max inline image height in rows; 0 = viewport limit. | V6 | default. |
| `tui.maxInlineImages` :354 | `8` | — | Live inline images kept before text placeholders. | V6 | default. |

### `retry.*`

Declared in `packages/coding-agent/src/session/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `retry.enabled` :680 | `true` | — | Retry transient provider errors. | P4 | default. |
| `retry.baseDelayMs` :701 | `500` | — | Initial retry backoff in ms. | — | default. |

### `providers.*`

Declared in `packages/coding-agent/src/session/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `providers.anthropic.slowMode` :905 | `"off"` | — | Anthropic low-priority lane after 5-hour limit (`/slow`). | V11 | default (`off`); `/slow on` when wanted. |

### `thinkingBudgets.*`

Declared in `packages/coding-agent/src/session/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `thinkingBudgets.minimal` :1463 | `1024` | — | Token budget for `minimal` thinking. | P2 | default. |
| `thinkingBudgets.low` :1465 | `2048` | — | Token budget for `low` thinking. | P2 | default. |
| `thinkingBudgets.medium` :1467 | `8192` | — | Token budget for `medium` thinking. | P2 | default. |
| `thinkingBudgets.high` :1469 | `16384` | — | Token budget for `high` thinking. | P2 | default. |
| `thinkingBudgets.xhigh` :1471 | `32768` | — | Token budget for `xhigh` thinking. | P2 | default. |
| `thinkingBudgets.max` :1473 | `32768` | — | Token budget for `max` thinking. | P2 | default. |

### `compaction.*`

Declared in `packages/coding-agent/src/session/context-settings.ts`. `compaction.remoteEndpoint` ⚠: when set, summary generation POSTs `{systemPrompt, prompt, maxTokens}` (or chat-completions) there (docs/compaction.md:332-334).

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `compaction.modelThresholds` :217 | `{}` | — | Per-model compaction trigger (tokens, fixed, or percent). | 0388; V9 | default; edit from `/models` Roles view if needed. |
| `compaction.modelThresholdsEnabled` :229 | `true` | — | Whether `compaction.modelThresholds` applies. | 0388 | default. |
| `compaction.reserveTokens` :276 | unset | — | Absolute reserve floor; unset = max(16384, 15% window). | 0388; V9 | default. |
| `compaction.keepRecentTokens` :282 | `20000` | — | Recent history kept verbatim by summary compaction. | 0388; V9 | default. |
| `compaction.autoContinue` :287 | `true` | — | Continue the turn automatically after compaction. | 0388; V9 | default (`true`); compaction stays seamless. |
| `compaction.remoteEndpoint` :290 | unset | — | ⚠ POST compaction summaries to this endpoint instead of local. | 0388; P6 | default (unset); summaries stay with the provider doing the work. |
| `compaction.v2RetainedMessageBudget` :296 | `64000` | — | User-message token budget kept by OpenAI remote compaction v2. | 0388 | default. |

### `branchSummary.*`

Declared in `packages/coding-agent/src/session/context-settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `branchSummary.reserveTokens` :623 | `16384` | — | Tokens reserved when summarising an abandoned branch. | — | default. |

### `memories.*`

Declared in `packages/coding-agent/src/memories/settings.ts`. Legacy `local` memory backend tuning; inert unless `memory.backend` is `local` (`packages/coding-agent/src/memories/settings.ts:8-9`).

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `memories.enabled` :11 | `false` | — | Legacy local-memory flag; migrated to `memory.backend`. | 0389 | default (`false`); choose `memory.backend` instead. |
| `memories.maxRolloutsPerStartup` :18 | `64` | — | Sessions processed per startup (legacy `local` backend). | 0389 | default; only matters with `local` backend. |
| `memories.maxRolloutAgeDays` :23 | `30` | — | Skip sessions older than this (local backend). | 0389 | default. |
| `memories.minRolloutIdleHours` :26 | `12` | — | Skip sessions active more recently (local backend). | 0389 | default. |
| `memories.threadScanLimit` :31 | `300` | — | Max recent session records scanned at startup. | 0389 | default. |
| `memories.maxRawMemoriesForGlobal` :34 | `200` | — | Max extractions fed to global consolidation. | 0389 | default. |
| `memories.stage1Concurrency` :39 | `8` | — | Concurrent per-session extraction jobs. | 0389 | default. |
| `memories.stage1LeaseSeconds` :42 | `120` | — | Extraction job lease duration. | 0389 | default. |
| `memories.stage1RetryDelaySeconds` :48 | `120` | — | Delay before failed extraction is retried. | 0389 | default. |
| `memories.phase2LeaseSeconds` :54 | `180` | — | Consolidation lease duration. | 0389 | default. |
| `memories.phase2RetryDelaySeconds` :60 | `180` | — | Delay before failed consolidation is retried. | 0389 | default. |
| `memories.phase2HeartbeatSeconds` :66 | `30` | — | Consolidation lease heartbeat interval. | 0389 | default. |
| `memories.rolloutPayloadPercent` :72 | `0.7` | — | Share of model context for rollout payloads. | 0389 | default. |
| `memories.phase1InputTokenLimit` :78 | `4000` | — | Per-session extraction input cap. | 0389 | default. |
| `memories.fallbackTokenLimit` :84 | `16000` | — | Token budget when model has no declared window. | 0389 | default. |
| `memories.summaryInjectionTokenLimit` :90 | `5000` | — | Token cap for memory summary injected into prompt. | 0389 | default. |

### `sharpshooter.*`

Declared in `packages/coding-agent/src/sharpshooter/settings.ts`. Inert unless `memory.backend` is `sharpshooter`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `sharpshooter.intervalMinutes` :20 | `5` | — | Minutes between sharpshooter consolidation runs. | 0389 | default; only with `sharpshooter` backend. |
| `sharpshooter.injectionTokenLimit` :26 | `15000` | — | Token cap for sharpshooter memory injection. | 0389 | default. |

### `autolearn.*`

Declared in `packages/coding-agent/src/autolearn/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `autolearn.minToolCalls` :38 | `5` | — | Min tool calls in a turn before autolearn capture. | 0389 | default. |

### `mnemopi.*`

Declared in `packages/coding-agent/src/mnemopi/settings.ts`. Mnemopi = omp's local SQLite memory backend, the one 0389 expects; values in docs/mnemosyne-memory-backend.md:57-62.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `mnemopi.retainEveryNTurns` :290 | `4` | — | User turns between automatic Mnemopi retain writes. | 0389 | default; tune after a week of use. |
| `mnemopi.recallLimit` :292 | `8` | — | Max recalled memories in the prompt block. | 0389 | default. |
| `mnemopi.recallContextTurns` :294 | `3` | — | Prior turns included in recall queries. | 0389 | default. |
| `mnemopi.recallMaxQueryChars` :297 | `4000` | — | Max composed recall query length. | 0389 | default. |
| `mnemopi.injectionTokenLimit` :303 | `5000` | — | Token budget for Mnemopi prompt injection. | 0389 | default. |
| `mnemopi.debug` :308 | `false` | — | Debug logging for Mnemopi backend failures. | P4 | default. |

### `hindsight.*`

Declared in `packages/coding-agent/src/hindsight/settings.ts`. ⚠ Inert unless `memory.backend` is `hindsight`; then missions, retained turns and recall queries go to `hindsight.apiUrl` (default `http://localhost:8888`, `packages/coding-agent/src/hindsight/settings.ts:16-20`).

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `hindsight.bankIdPrefix` :59 | unset | — | Prefix prepended to Hindsight bank ids. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.bankMission` :96 | unset | `HINDSIGHT_BANK_MISSION` | Mission text set on the Hindsight bank. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.retainMission` :103 | unset | — | Mission guiding what Hindsight retains. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.retainEveryNTurns` :160 | `3` | `HINDSIGHT_RETAIN_EVERY_N_TURNS` | Turns between automatic Hindsight retains. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.retainOverlapTurns` :167 | `2` | — | Turns overlapped between consecutive retains. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.retainContext` :172 | `"omp"` | — | Context label attached to retained items. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.recallBudget` :175 | `"mid"` | `HINDSIGHT_RECALL_BUDGET` | Hindsight recall effort: low, mid, high. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.recallMaxTokens` :183 | `1024` | `HINDSIGHT_RECALL_MAX_TOKENS` | Max tokens returned by recall. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.recallContextTurns` :190 | `1` | `HINDSIGHT_RECALL_CONTEXT_TURNS` | Prior turns included in recall queries. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.recallMaxQueryChars` :197 | `800` | `HINDSIGHT_RECALL_MAX_QUERY_CHARS` | Max recall query length. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.recallTypes` :204 | `["world", "experience"]` | — | Memory types recalled (world, experience). | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.debug` :210 | `false` | `HINDSIGHT_DEBUG` | Debug logging for Hindsight. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.requestTimeoutMs` :217 | `30000` | `HINDSIGHT_REQUEST_TIMEOUT_MS` | Default Hindsight HTTP request timeout. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.reflectTimeoutMs` :224 | `120000` | `HINDSIGHT_REFLECT_TIMEOUT_MS` | Timeout for Hindsight reflect calls. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.recallTimeoutMs` :231 | `30000` | `HINDSIGHT_RECALL_TIMEOUT_MS` | Timeout for Hindsight recall calls. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.retainTimeoutMs` :238 | `60000` | `HINDSIGHT_RETAIN_TIMEOUT_MS` | Timeout for Hindsight retain calls. | 0389 | default; Hindsight backend not chosen (0389). |
| `hindsight.mentalModelMaxRenderChars` :273 | `16000` | — | Max chars of rendered mental models in prompt. | 0389 | default; Hindsight backend not chosen (0389). |

### `edit.*`

Declared in `packages/coding-agent/src/edit/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `edit.modelVariants` :53 | `{}` | — | Model substring → edit mode; first match wins. | 0386; P2 | default. |

### `async.*`

Declared in `packages/coding-agent/src/tools/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `async.maxJobs` :906 | `100` | — | Cap on running background jobs. | — | default. |

### `dev.*`

Declared in `packages/coding-agent/src/tools/settings.ts`. ⚠ Grievances from `report_tool_issue` are POSTed to `dev.autoqaPush.endpoint`, default `https://qa.omp.sh/v1/grievances` (`packages/coding-agent/src/tools/settings.ts:978`), once consent is granted (`packages/coding-agent/src/tools/report-tool-issue.ts:24-26`).

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `dev.autoqaPush.token` :988 | unset | — | ⚠ Bearer token for Auto QA grievance push. | P6 | default (unset). |
| `dev.autoqaConsent` :1008 | `"unset"` | — | ⚠ Consent to record/push `report_tool_issue` grievances. | P6 | `denied`: reports may hold client paths (P6). |

### Top-level keys

Declared in `packages/coding-agent/src/exec/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `shellPath` :80 | unset | — | External shell for services, terminals and `!`. | 0386 | default. |

### `bashInterceptor.*`

Declared in `packages/coding-agent/src/exec/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `bashInterceptor.patterns` :160 | 10 built-in rules (`DEFAULT_BASH_INTERCEPTOR_RULES`): cat/head/tail→read, grep/rg→grep, find/fd→glob, sed/perl/awk in-place→edit, echo/printf redirect→write, nohup/&, dev servers, watch modes→bash | — | Regex rules steering shell commands to dedicated tools. | 0391 | default; active only with `bashInterceptor.enabled`. |

### `shellMinimizer.*`

Declared in `packages/coding-agent/src/exec/settings.ts`. Option meanings from `packages/natives/native/index.d.ts:2592-2629`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `shellMinimizer.settingsPath` :206 | unset | — | TOML file overriding minimizer filter defaults. | — | default. |
| `shellMinimizer.only` :212 | `[]` | — | Allow-list of programs the minimizer filters. | — | default (all built-in filters). |
| `shellMinimizer.except` :218 | `[]` | — | Programs excluded from minimization. | — | default. |
| `shellMinimizer.maxCaptureBytes` :224 | `4194304` | — | Bytes captured before falling back to raw output. | — | default. |
| `shellMinimizer.legacyFilters` :243 | unset | — | Kill-switch to legacy grep/find/pytest filters. | — | default (unset → env decides). |

### `bash.*`

Declared in `packages/coding-agent/src/exec/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `bash.autoBackground.thresholdMs` :263 | `60000` | — | ms before a bash call auto-backgrounds. | V11 | default. |

### `eval.*`

Declared in `packages/coding-agent/src/eval/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `eval.autoBackground.thresholdMs` :85 | `60000` | — | ms before an eval cell auto-backgrounds. | — | default. |

### `task.*`

Declared in `packages/coding-agent/src/task/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `task.agentSoftRequestBudget` :437 | `{}` | — | Per-agent soft request budget overriding the global one. | 0386 | default. |
| `task.disabledAgents` :472 | `[]` | — | Subagent names that fail spawn preflight. | 0386; P1 | default. |
| `task.agentModelOverrides` :479 | `{}` | — | Per-agent model override for task spawns. | 0386; P2 | Walkthrough: pin `reviewer` model if wanted (0394). |
| `task.agentServiceTierOverrides` :486 | `{}` | — | Per-agent service tier (flex, priority…) override. | 0386 | default. |
| `task.agentCompactionThresholdOverrides` :494 | `{}` | — | Per-agent compaction threshold override. | 0388 | default. |
| `task.agentAccountPools` :502 | `{}` | — | Per-agent provider → allowed OAuth accounts. | 0386 | default. |
| `task.agentPrewalk` :510 | `{}` | — | Per-agent prewalk hand-off: on, off, or model. | 0386 | default. |
| `task.agentAdvisor` :517 | `{}` | — | Per-agent advisor: on, off, or model pattern. | 0386 | default (subagents unadvised). |

### Top-level keys

Declared in `packages/coding-agent/src/extensibility/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `extensions` :10 | `[]` | — | Extension paths/packages loaded at startup. | 0386 | default (empty): no Jocasta code inside omp. |
| `disabledExtensions` :12 | `[]` | — | Disabled extension/skill/tool ids, e.g. `skill:<name>`. | 0386; P1 | default. |

### `skills.*`

Declared in `packages/coding-agent/src/extensibility/settings.ts`. Source toggles and filters: docs/skills.md:110-122.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `skills.enabled` :29 | `true` | — | Master switch for skill discovery. | 0386 | default. |
| `skills.enableCodexUser` :43 | `false` | — | Load skills from Codex user dir. | 0387; P3 | default (`false`). |
| `skills.enableClaudeUser` :45 | `false` | — | Load skills from Claude user dir. | 0387; P3 | default (`false`). |
| `skills.enableClaudeProject` :48 | `true` | — | Load skills from project `.claude/skills`. | 0387 | default. |
| `skills.enablePiUser` :53 | `true` | — | Load native omp user skills (legacy name). | 0387 | default. |
| `skills.enablePiProject` :55 | `true` | — | Load native omp project skills (legacy name). | 0387 | default. |
| `skills.enableAgentsUser` :57 | `true` | — | Load skills from user `.agents/skills`. | 0387 | default. |
| `skills.enableAgentsProject` :60 | `true` | — | Load skills from project `.agents/skills`. | 0387 | default. |
| `skills.customDirectories` :66 | `[]` | — | Extra skill dirs scanned non-recursively. | 0394 | default; threads load hunk's skill by path (0394). |
| `skills.ignoredSkills` :72 | `[]` | — | Glob patterns excluding skills. | P1 | default. |
| `skills.includeSkills` :78 | `[]` | — | Glob allow-list of skills; empty = all. | P1 | default. |

### `searxng.*`

Declared in `packages/coding-agent/src/web/settings.ts`. ⚠ Used only when `searxng.endpoint` (a `/settings` row, default unset, `packages/coding-agent/src/web/settings.ts:33-37`) is set; queries and these credentials go there.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `searxng.token` :47 | unset | `SEARXNG_TOKEN` | ⚠ SearXNG bearer token. | — | default (unset). |
| `searxng.basicUsername` :55 | unset | `SEARXNG_BASIC_USERNAME` | ⚠ SearXNG basic-auth username. | — | default (unset). |
| `searxng.basicPassword` :63 | unset | `SEARXNG_BASIC_PASSWORD` | ⚠ SearXNG basic-auth password. | — | default (unset). |
| `searxng.categories` :70 | unset | — | SearXNG categories parameter. | — | default. |
| `searxng.engines` :72 | unset | — | SearXNG engines (comma-separated names/bangs). | — | default. |
| `searxng.language` :74 | unset | — | SearXNG language parameter. | — | default. |
| `searxng.safesearch` :76 | unset | — | SearXNG safesearch level. | — | default. |

### `images.*`

Declared in `packages/coding-agent/src/blob-broker/settings.ts`. ⚠ Used only when `images.urls.enabled` is on (default `false`, `packages/coding-agent/src/blob-broker/settings.ts:25-28`); default backends include `tailscale`, `cloudflared`, `litterbox` (`:8`); litterbox uploads to `https://litterbox.catbox.moe/resources/internals/api.php` (`packages/coding-agent/src/blob-broker/uploaders-anonymous.ts:14`).

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `images.urls.options` :53 | `{}` | — | ⚠ Per-destination options for image URL publishing. | P6 | default; keep `images.urls.enabled` off. |
| `images.urls.credentials` :59 | `{}` | — | ⚠ Per-destination credentials for image URL publishing. | P6 | default (empty). |

### `stt.*`

Declared in `packages/coding-agent/src/stt/settings.ts`.

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `stt.language` :21 | `"en"` | — | Speech-to-text source language hint. | V2 | default (`en`). |

### `commit.*`

Declared in `packages/coding-agent/src/commit/settings.ts`. Used by omp's commit-message generator (`packages/coding-agent/src/commit/conventional/config.ts:88-96`).

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `commit.mapReduceEnabled` :7 | `true` | — | Map-reduce commit message generation for large diffs. | — | default. |
| `commit.mapReduceThreshold` :9 | `5000` | — | Diff tokens above which map-reduce runs. | — | default. |
| `commit.mapBatchTokenBudget` :12 | `16000` | — | Token budget per map batch. | — | default. |
| `commit.cacheEnabled` :17 | `true` | — | Cache commit-message inference results locally. | — | default. |
| `commit.cacheTtlDays` :19 | `14` | — | Commit inference cache lifetime in days. | — | default. |
| `commit.changelogMaxDiffChars` :22 | `120000` | — | Max diff chars fed to changelog generation. | — | default. |

### `gc.*`

Declared in `packages/coding-agent/src/cli/gc-settings.ts`. Used by `omp gc` (`packages/coding-agent/src/cli/gc-cli.ts:242-264`); preview by default, `--apply` acts (docs/session.md:581).

| id | default | env var | what it does | serves | recommendation |
|---|---|---|---|---|---|
| `gc.blobs` :7 | `true` | — | `omp gc` deletes unreferenced blobs. | 0389 | default. |
| `gc.archive` :9 | `true` | — | `omp gc` archives eligible cold sessions. | 0389 | default. |
| `gc.wal` :11 | `true` | — | `omp gc` checkpoints database WALs. | — | default. |
| `gc.coldArchiveAfterDays` :13 | `30` | — | Age after which sessions are cold-archived. | 0388; 0389 | default. |
| `gc.retainNewestGlobal` :15 | `20` | — | Newest sessions kept unarchived globally. | 0388 | default. |
| `gc.retainNewestPerCwd` :17 | `10` | — | Newest sessions kept unarchived per cwd. | 0388 | default. |
| `gc.stale` :21 | `false` | — | `omp gc` also deletes stale reports and collab replicas. | — | default (`false`). |
| `gc.staleRetainNewest` :23 | `20` | — | Stale items kept regardless of age. | — | default. |
| `gc.staleRetainDays` :25 | `30` | — | Stale items newer than this are kept. | — | default. |

### Sources

| setting | destination | off switch | citation |
|---|---|---|---|
| `auth.broker.url`, `auth.broker.token` | the configured broker URL; local credential store bypassed | leave unset (default) and unset `OMP_AUTH_BROKER_URL` | `packages/coding-agent/src/config/model-settings.ts:43,50`; docs/auth-broker-gateway.md:279,293-295,303-304 |
| `compaction.remoteEndpoint` | the configured summarizer URL | leave unset (default) | `packages/coding-agent/src/session/context-settings.ts:290`; docs/compaction.md:329,332-334 |
| `dev.autoqaPush.token`, `dev.autoqaConsent` | `https://qa.omp.sh/v1/grievances` (`dev.autoqaPush.endpoint`) | `dev.autoqaConsent: denied` (every `report_tool_issue` no-ops) | `packages/coding-agent/src/tools/settings.ts:978,988,995-1008`; `packages/coding-agent/src/tools/report-tool-issue.ts:24-26` |
| `images.urls.options`, `images.urls.credentials` | backends `tailscale`, `cloudflared`, `litterbox` (`https://litterbox.catbox.moe/resources/internals/api.php`) | `images.urls.enabled: false` (default) | `packages/coding-agent/src/blob-broker/settings.ts:8,26-28,53,59`; `packages/coding-agent/src/blob-broker/uploaders-anonymous.ts:14` |
| `searxng.*` | `searxng.endpoint` | leave `searxng.endpoint` unset (default) | `packages/coding-agent/src/web/settings.ts:34,47,55,63` |
| `hindsight.*` | `hindsight.apiUrl`, default `http://localhost:8888` | `memory.backend` ≠ `hindsight` | `packages/coding-agent/src/hindsight/settings.ts:17-20` |
| `disabledProviders` | — (removes model providers / discovery sources by id) | — | docs/settings.md:348-355; whether it also skips network model discovery: not found |
