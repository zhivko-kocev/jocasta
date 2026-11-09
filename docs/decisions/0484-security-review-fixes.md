# 0484 The security review's fixes

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
The security review (`local://security-review.md`) found that a prompt-injected agent is the real risk. Which of its fixes go into v1?

## Facts
Verified 2026-10-24.
- F2: boss tools are gated on `caller.paneId === this.boss.paneId` (`src/frontier/server/service.ts:1936-1941`), and the pane id comes from `process.env.HERDR_PANE_ID` (`src/entry.ts:85`), so any pane can claim the boss's id; herdr's socket takes `agent.prompt {target, text}` from any client (`src/frontier/server/herdr.ts:743-745`).
- F3: every brief ends with the lessons line (`service.ts:353-354`, appended at `service.ts:2341`); `lesson` writes the body verbatim (`service.ts` `lesson()`, around lines 2205-2245); `graph_search` returns facts unquoted (`service.ts:2108`) and takes `cwd` from the request body (`server.ts:465-470`).
- F4: both companion windows are created without `sandbox` (`src/app/main.ts:251-258`, `:335-343`), so Electrobun's internal bridge is injected; `src/frontier/ui/whiteboard/bridge.ts:331` uses `innerHTML`.
- F5: `rmSync(path, { recursive: true })` on a thread worktree (`service.ts:488`) and uninstall's `removePath` (`src/setup/uninstall.ts:127-131`) don't guard against junctions or symlinks.
- F6: `scripts/install.ps1:62-66` and `scripts/install.sh:19-29` download the asset and its `.sha256` from the same `releases/latest` URL, so a replaced release replaces both.
- F7: data and config folders are created with the default umask (`src/shared/paths.ts:20-29`, `src/shared/fsx.ts:12`); only `frontier.json` gets `0o600`.
- F8: uninstall deletes `dir`/`appData` taken from `install.json` (`uninstall.ts:44-58`, deleted at `:317`, `:389`), and setup records whole-folder undos for folders it may not have created (`setup.ts:516`, `:547`).
- F9: `service.ts:804-807` merges `{ ...env, ...mise }`, so mise's env can override `HERDR_*`, `JARVIS_*` and `PI_*` variables in panes.
- F10: `src/setup/diagnose.ts:2-6` says "The log stays on this machine (P6)", but `:120-156` attaches `recent.log` and `state.md` to an omp pane's first message.
- F11: `docker/graphiti/Dockerfile` has no `USER`, and the compose service has default network egress (`src/shared/containers/resources.ts:56-80`).
- Checked and safe: service auth (per-launch token, `HttpOnly; SameSite=Strict` cookie, bearer-protected endpoints), loopback binding, the Host allowlist, the WebSocket Origin check, path handling, argv command construction, outside text quoted in «», secrets at rest, pinned model downloads, navigation rules, the supply chain and autostart.

## Options
Shown as a multi-select; all four recommended.

| Option | Pros | Cons |
|---|---|---|
| Boss secret + confirm risky boss actions | A pane can no longer act as the boss; merges, removals and cross-project starts need his click | One more prompt in the companion; not a boundary without OS isolation |
| Lessons and graph facts | Injected text can't become a lasting lesson; graph facts arrive marked as data, from the right project only | A confirmation per lesson |
| Hardening fixes | Closes the low findings (webview sandbox, junctions, permissions, uninstall, Graphiti, mise env, diagnose wording) | Several small changes to check |
| Installers check hashes from the tagged repo | A replaced release asset is caught | The release workflow has to publish the hash into the repo at the tag |

## Recommendation
All four.

## Choice
1. **Boss secret and confirmations.**
   - Boss-only MCP tools require a per-run secret that only the boss pane's environment receives, instead of trusting `HERDR_PANE_ID`.
   - The companion asks him (Allow / Deny; no answer in 2 minutes = Deny) before:
     - a `thread_finish` that merges or opens a PR;
     - `worktree_remove`;
     - a `thread_start` for a project other than the one of the last pane he focused.
2. **Lessons and graph facts.**
   - `lesson` saves only after he confirms its text in the companion.
   - `graph_search` returns each fact quoted («…») with its source and date.
   - It searches the project of the calling pane's recorded worktree, not the caller's `cwd`; the boss names the project explicitly.
3. **Hardening.**
   - The companion's webviews run with Electrobun's `sandbox: true` if the page keeps working (else stop and report, 0020); `bridge.ts:331` uses `textContent`.
   - Recursive deletes never follow junctions or symlinks: the link is removed, not its target.
   - Data and config folders are 0700 on Linux and macOS.
   - Uninstall deletes a recorded folder only when it still holds Jocasta's marker files and isn't a drive root or home folder, and removes whole-folder undos only for folders setup created.
   - Graphiti's container runs as a non-root user, with network access only to the host's model gateway, after checking that nothing it needs downloads at run time.
   - mise env can't override `HERDR_*`, `JOCASTA_*` or `PI_*` in panes.
   - `jocasta diagnose` says that the log is sent to the model provider (the code comment and the pane's first line), instead of claiming it stays local.
4. **Installer integrity.** The installers take the expected SHA-256 from the repository at the release's tag (or a file the release workflow commits), never from the release itself, so a replaced release asset is caught; the builder picks the shape that fits the release workflow and documents tag protection.

## Reason
The four labels he picked.

## Consequences
- Amends 0391 (omp's approvals are no longer the only gate: risky boss actions also wait for his Allow in the companion), 0425 (a reviewed thread's `thread_finish` that merges or opens a PR waits for his Allow), 0459 (a lesson is saved only after he confirms its text in the companion), 0430 (boss tools take a per-run secret; graph facts arrive quoted like `[event]` text; Graphiti runs non-root with network only to the model gateway), 0472 (the log goes to the model provider, said plainly) and the installers (0049: the expected hash comes from the tagged repo).
- 0481's campaign re-tests every fix.
- Amended by 0489: Graphiti's network limit to the model gateway comes after v1.
- Amended by 0492: the cross-project check compares with the last focused pane that belongs to a project.
- Amended by 0519: the Allow box is Jocasta's own window; its answer counts only with that window's one-time code, and no answer in 2 minutes is Deny.
