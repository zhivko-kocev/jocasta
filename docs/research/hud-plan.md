# HUD round build plan: records 0667–0673

Spec: `docs/decisions/0667-*.md` … `0673-*.md`. Research: `docs/research/hud-research.md`. This file only says how to build it. Everything marked [INFERENCE] was not verified in code; the builder checks it first and reports the result.

## 0. Open questions for Zhivko

None block the build. Non-blocking notes, reported as built:

1. **Token location.** `secrets.yml` (0594) is omp's own file and nothing in `src` reads it, so the ntfy token lives in `jocasta.yaml` (`ntfy.token`), with `ntfy.server` and the own-server flag `ntfy.ownServer`; topics are generated. Nothing to decide.
2. **"7-day average" does not start cold.** It is backfilled once at service start from omp session files of the last 8 days (§3, 0668 item 8), then kept from recorded daily totals.
3. **Possible Cargo.toml feature flag (0671, Windows).** Main approved adding the windows-sys feature `Win32_System_ProcessStatus` if the `NtQueryInformationProcess` route fails under `PROCESS_QUERY_LIMITED_INFORMATION`. D writes the exact `Cargo.toml` line in its result note and Main places it; D does not edit `Cargo.toml`.
4. **One small fork change (0668).** The dock gets a mouse-wheel seam in `crates/herdr` (§3, 0668). It is recorded in `crates/herdr/FORK.md`.
5. **No new crates.** `reqwest` 0.13, `axum`, `sha2`, `getrandom` and `chrono` are already dependencies.

## 1. Builders and file ownership

Five builders, disjoint files. Nobody builds, tests or commits mid-flight. Main runs `mise run check` once at the end.

A lands protocol.rs and settings.rs first, as the first thing it does. B, C, D and E code against the signatures in §2 verbatim and can start at once. A owns the shared files, so every other builder hands its needs to A as signatures in this file, not by editing A's files.

| Owner | Records | Files (exclusive) |
|---|---|---|
| **A** shared UI/docs | UI of all seven | `src/frontier/protocol.rs` (+ its tests), `src/frontier/today/view.rs`, `today/view_tests.rs`, `src/frontier/shell.rs`, `shell_tests.rs`, `src/shared/settings.rs` (+ tests), `crates/herdr/src/embed.rs`, `crates/herdr/src/client/shell/extension.rs`, `crates/herdr/src/client/shell/tests/boss_guard.rs`, `crates/herdr/FORK.md`, `docs/manual.md`, `CHANGELOG.md`, `docs/plan.md` (appendix I), `docs/status.md` |
| **B** boss row, leftovers, service plumbing | 0667, 0670, wiring for 0668/0669/0671/0672/0673 | `server/native.rs`, `native_tests.rs`, `server/status.rs` (+ its tests), `server/tools.rs`, `server/wording.rs`, `server/service.rs`, `server/context.rs`, `server/fake_herdr.rs`, `config/herdr.toml` if touched |
| **C** Day side | 0668, 0669 (Day side), 0673 (tick) | `server/day.rs`, `day_view.rs`, `day_poll.rs`, `day_tests.rs`, `day_threads_tests.rs`, new `server/thread_facts.rs` + `thread_facts_tests.rs`, `server/finish.rs`, `server/review_flow.rs`, `server/sessions.rs` (only the `with_horizon` constructor), new `shared/spend_days.rs` + tests, `server/mod.rs` (the `mod` lines for C, D and E's new files; C is the only editor) |
| **D** quota + check admission | 0669 (parser), 0671 | `server/quota.rs` (+ tests), `server/checks.rs` (+ its tests), `shared/proctree.rs`, `proctree_windows.rs`, `proctree_unix.rs` (+ tests), `shared/pollers/machine.rs` if touched, new `server/check_costs.rs` + tests |
| **E** phone + schedules | 0672, 0673 | new `server/phone.rs` + `phone_tests.rs`, `server/popups.rs`, `server/asker.rs`, `server/server.rs`, `src/setup/diagnose.rs` + `diagnose_tests.rs` (the `phone` line), new `shared/schedules.rs` + `schedules_tests.rs`, `server/thread_start.rs`, `shared/mod.rs` (the `schedules` and `spend_days` `mod` lines; E is the only editor) |

Overlap rules:
- `server/mod.rs`: C adds `pub mod thread_facts;`, `pub mod check_costs;` and `pub mod phone;` (D's and E's modules) in one edit. D and E tell C nothing more than the names. Names are fixed here.
- `shared/mod.rs`: E adds `pub mod schedules; pub mod spend_days;`.
- `Cargo.toml`: nobody edits it. See §0, note 3.
- If a builder needs a line in a file it doesn't own, it writes the exact text in its result note and Main places it. It does not edit the file.

Order:
1. A first lands the types in §2 (protocol.rs, settings.rs).
2. B, C, D and E work in parallel from §2.
3. A's view and shell work, plus the docs, are written against the same §2 types.
4. Main runs `mise run check` last.

## 2. Contracts: types and signatures

### 2.1 protocol.rs (A)
All new fields use `#[serde(default, skip_serializing_if = "...")]` so old wire JSON still parses. The protocol.rs wire tests, which compare JSON, are updated by A.

```rust
// protocol.rs, near RailItem (215) / LandedItem (238) / NeedsYouKind (298) / Hud (543)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ThreadDiff { pub added: u32, pub removed: u32 }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PrCi { Pending, Passing, Failing, Merged, Closed }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PrFacts { pub number: u64, pub ci: PrCi }

// RailItem gets:
pub diff: Option<ThreadDiff>,
pub check: Option<bool>,          // Some(true) = last check passed, Some(false) = failed, None = none yet
pub stuck_for: Option<i64>,       // ms with no write to the session file, only when stuck
// LandedItem gets:
pub pr: Option<PrFacts>,
// NeedsYouKind gets a new variant `Stuck` (glyph "!").  Add it at EVERY match:
//   day_view.rs:319/381/392/480 (C) and today/view.rs:327–351 (A).
// Today gets:
pub scheduled: Vec<ScheduledItem>,   // soonest first, at most 3

pub struct ScheduledItem { pub project: String, pub brief: String, pub at_ms: i64 }
pub struct QuotaAccount { pub name: String, pub five_h: Option<QuotaWindow>, pub seven_d: Option<QuotaWindow> }
pub struct QuotaCapacity { pub window: QuotaSpan, pub remaining: u32, pub accounts: u32 }
pub struct ChecksLoad { pub running: u32, pub waiting: u32 }
pub struct TopThread { pub title: String, pub cost: f64 }
// Hud gets:
pub spend_avg_7d: Option<f64>,
pub top_thread: Option<TopThread>,
pub accounts: Vec<QuotaAccount>,
pub capacity: Vec<QuotaCapacity>,
pub checks: Option<ChecksLoad>,
// ClientMessage gets (641), sent on SHELL_FEED:
LeftoverRemove { folder: PathBuf },
```
`QuotaWindow` is unchanged: `{window, used, resets_at: Option<String>}`. `Hud.quota` stays: it is still the most-used window of each span, for the header.

### 2.2 DayThread and Day (C provides; B and A consume)
```rust
// day.rs:131 DayThread gets (filled by status.day_thread, owned by B, from ThreadFacts):
pub diff: Option<ThreadDiff>, pub check: Option<bool>, pub last_write_ms: Option<i64>,
// Day:
pub fn quota_accounts(&self) -> (Vec<QuotaAccount>, Vec<QuotaCapacity>);   // last good read
pub fn spend_avg_7d(&self) -> Option<f64>;
pub fn scheduled_next(&self, projects: &[Project]) -> Vec<ScheduledItem>;
// DayHooks (day.rs ~150–177) gets (implemented by B in service.rs ServiceHooks):
fn start_scheduled(&self, project: &str, schedule: &Schedule) -> BoxFuture<'static, Result<ScheduleStart>>;
fn pr_facts(&self, url: &str) -> Option<PrFacts>;          // sync cache read
```
`Schedule` is from `shared/schedules.rs` (E, §3.0673). `ScheduleStart` is `enum {Started, Skipped(String)}`, defined in `day.rs` by C.

### 2.3 Thread facts (C provides; B calls)
```rust
// server/thread_facts.rs
pub struct ThreadFacts { /* Mutex<HashMap<PathBuf, Facts>> */ }
pub struct Facts { pub diff: Option<ThreadDiff>, pub check: Option<bool>, pub last_write_ms: Option<i64> }
impl ThreadFacts {
    pub fn new() -> Self;
    pub fn get(&self, worktree: &Path) -> Facts;                               // sync, cheap
    pub fn note_check(&self, worktree: &Path, passed: bool, at_ms: i64);       // called by review_flow::run_checks
    pub async fn refresh(&self, c: &ServiceContext);                           // diff + mtime for open threads
}
```
B holds `ThreadFacts` in `ServiceContext` and calls `refresh` on the refresh tick. C writes `refresh` against `c.threads.all()`, `c.places.trunk`, and `worktrees::git_out`.

### 2.4 Quota parsing (D provides; C calls)
```rust
// server/quota.rs, additive. Existing omp_usage/usage_reports/read_quota/quota_warning stay (evals/pacer.rs uses them).
pub async fn omp_usage_redacted() -> Option<String>;     // `omp usage --json --redact`
pub struct AccountRead { pub accounts: Vec<QuotaAccount>, pub capacity: Vec<QuotaCapacity> }
pub fn read_accounts(text: &str) -> Option<AccountRead>;
pub fn account_warning(account: &str, w: &QuotaWindow) -> String;   // "Subscription quota (name): 5h at 84%, resets at 16:30"
```

### 2.5 Check queue (D provides; B wires)
```rust
// checks.rs
pub struct Admission { /* free-memory fn, cores fn, reserve, costs: CheckCosts */ }
impl Admission {
    pub fn from_settings() -> Self;                       // reads checks.memoryReserveGb via read_settings()
    pub fn with(free_bytes: Arc<dyn Fn()->u64+Send+Sync>, cores: usize, reserve_bytes: u64, costs: CheckCosts) -> Self; // tests
}
// CheckRunnerOptions gets: pub admission: Option<Admission>,   // None = no queue (layer/bench.rs:171 keeps default)
impl CheckRunner { pub fn load(&self) -> ChecksLoad; }
```
B's `context.rs::ServiceContext::new` (~232–236) passes `admission: Some(Admission::from_settings())` when `options.checks` is `None`, and `status.rs::hud_for` sets `Hud.checks = Some(load)` only when `running + waiting > 0`.

### 2.6 Phone (E provides; B wires through service.rs)
```rust
// server/phone.rs
pub struct PhoneFacts { pub needs: Vec<NeedsYou>, pub finished: Vec<LandedItem>, pub quiet: bool }
pub enum PhoneMode { Public, Own }
pub struct PhoneOptions { pub server: String, pub own_server: bool, pub token: Option<String>, pub popups: Arc<Popups>, pub facts: Arc<dyn Fn() -> PhoneFacts + Send + Sync>, pub data_dir: PathBuf }
pub struct Phone;
impl Phone {
    pub fn start(options: PhoneOptions) -> Option<Arc<Phone>>;    // None when `ntfy.server` is unset; mode chosen inside from own_server and token
    pub async fn stop(&self);
}
```
B adds `JocastaService::phone_facts(&self) -> PhoneFacts` (calls `day.today()` and `day.quiet()`, public after C). E's `server.rs` builds the `Phone` after the service and stops it in `stopper_of`. E reads `service.phone_facts()` from the `service` already in `Rig`.

### 2.7 Settings (A; `settings.rs` `validate` ~230, `rule_of` ~63)
```rust
pub struct NtfySettings { pub server: Option<String>, pub own_server: bool, pub token: Option<String> }   // server None = off
pub fn read_ntfy() -> NtfySettings;         // via read_settings(); invalid values are reported by check_settings and read as unset
pub fn read_check_reserve_gb() -> f64;      // checks.memoryReserveGb, default 2, 0..=256
```
A new top-level group `checks` and a group `ntfy` in `validate`/`rule_of`. `ntfy.server` must be an `http(s)://` URL. `ntfy.ownServer` is a boolean. `ntfy.token` is a non-empty string without whitespace. The topics are not settings. `check_settings` warns once when `ntfy.ownServer` is true without a token.

## 3. Slices

### 0667: the boss row pinned first (B; test beside `native_tests.rs`)
**Testable:**
1. The agent view's filter keeps the boss pane but still drops every other pane of the boss workspace.
2. The view's sort puts `{"token":"pin"}` first.
3. The boss pane carries token `pin="1"` while Jocasta runs, with the existing TTL/renew in `status.rs::report_once`, and `pin` is cleared at `clear()`.
4. A relaunched boss pane (new pane id) gets the view set again.

**Facts and anchors:**
- `native.rs::agent_view(boss_ws)` (48–64) becomes `agent_view(boss_ws, boss_pane)`.
  - Filter: `{"op":"any","filters":[{"op":"not","filter":{"op":"eq","field":"workspace_id","value":boss_ws}},{"op":"eq","field":"pane_id","value":boss_pane}]}`.
  - Sort: `{"field":{"token":"pin"},"order":"asc"}` first, then the existing keys. An agent without the token sorts after one with it (`crates/herdr/src/agent_view_eval.rs`, `compare_optional_values`).
- `Native::heal` (164–184) keys `view_for` on `(workspace, pane)`.
- `status.rs::report()` (~270) reports pane metadata on the boss pane with token `pin`. Use the pane key `pane:<id>` in `report_once`. Add the boss pane to the `live` set so `forget_closed` keeps it. `clear()` (~436–470) clears `pin`.
- The row's label comes from the shipped sidebar template in `config/herdr.toml`: workspace "Jocasta" with the state icon. No fork change. Click or Enter on the row is `pane.focus`, which switches the workspace (`mouse.rs` ~1600). Alt+1, Alt+G and Ctrl+B w stay.
- [INFERENCE] The boss pane (omp via pi.toml) is an agent entry in herdr, so the Agents view lists it. B checks with a `FakeHerdr` test and notes if live behaviour differs.
- `FakeHerdr` already records `setAgentView` (`fake_herdr.rs:748`) and `reportPaneMetadata` (771). Update the tests at `native_tests.rs:240–270`.

**Builder-chosen (list when done):** the token name `pin` and value `"1"`.
**Docs:** manual "Daily use" (how to get back to the boss from a thread). CHANGELOG. Appendix I (the seam). A writes these from B's result note.
**Risks:** herdr's own agent-view filter on pane id needs the field name `pane_id` to be valid. B reads `agent_view_eval.rs` to confirm. If a different name applies, report it.

### 0668: news box, thread facts, spend trend (C service, A view)
**Testable (view, A):**
1. NEWS is a 5-line box, newest first across all feeds (`Headline.at` desc, unparseable last, stable).
2. It shrinks at small heights and hides under 3 lines. Layout in `today/view.rs::render` (503–563): NEWS reserves up to 7 lines (blank, heading, 5); NEXT gets leftovers only.
3. The wheel and Up/Down move `news_top`.
4. UNDERWAY rows show, as the row narrows, state, check ✓/✗, diff (`+84` green, `−12` red), age, cost, context, dropped in that order from the right.
5. A stuck thread shows the state "stuck" in red.
6. LANDED rows show `PR #n` and its CI state.
7. The header shows `avg` and the costliest thread through `Part.yields` (header, view.rs 200–256).

**Testable (service, C):**
1. A Working thread whose session file has not been written for 10 min (`STUCK_MS = 600_000`) is stuck: a `NeedsYou` row of kind `Stuck`, counted in `Hud.awaiting` through `today.needs_you.len()`. Glyph "!".
2. The diff comes from `git diff --shortstat <merge-base trunk>` in the thread's worktree.
3. The check result comes from `review_flow.rs::run_checks` (~680–700, the only path to `c.checks.run`). After a restart it falls back to `Thread.review.result.checks` (`CheckExit{command,exit}`).
4. A PR thread appears as LANDED: the finish removes its pane and worktree at `finish.rs::pr_opened` (~530–560). Pass the PR url through `Day::landed(...)` (day_view.rs:398) into `NewEvent.url`, and read it back in `finished_since` (409).
5. PR facts come from `gh pr view <url> --json number,state,statusCheckRollup` through the existing gh seam (`ServiceOptions.gh`, `gh_timeout_ms`, the `finish.rs gh()` helper). `gh pr view --json` supports exactly these fields (checked with gh 2.92).
6. Caching: TTL 2 min while pending, 10 min once settled, no polling after merged/closed, error back-off 2→30 min, log once, keep the last good. At most about 10 PRs.
7. A red pipeline keeps its NEEDS YOU row. The existing `pipeline` event kind from `pollers/github.rs` already does this; C adds the test.
8. Daily spend totals persist in `<data>/spend-days.json` as `{local-day: usd}`, last 14 days kept, written at most once a minute and at day rollover. `spend_avg_7d` is the mean of the recorded prior days (up to 7), `None` with none recorded. **No cold start:** at service start, once, `spend_days::backfill` fills the missing prior days from omp session files modified in the last 8 days, summed per local day by the same ledger code `spend_today` uses (`sessions.rs` `SpendLedger`, `spent_on`). Recorded totals are never overwritten by the backfill; today is excluded (live). The shared `SessionIndex` can't be used: its ledger forgets days before yesterday (`roll_over`, `sessions.rs:1307–1315`). So C adds to `sessions.rs` a constructor for a throwaway index with a longer horizon, `SessionIndex::with_horizon(sessions_dir, days_back)`, lists with `list(day_start(today, -8))`, reads each file (`read(file, true)`), sums `spent_on(day)` for each prior day and drops the index. It runs off the event loop (a spawned task; `warm` steps as `spend_settled` does) and never blocks start. The costliest thread is the highest `cost` among open threads (a simplification; list it).
   - Signature (C): `pub async fn backfill(sessions_dir: &Path, now_ms: i64, store: &SpendDays)`. B spawns it from `service.rs` at start with `ServiceOptions.sessions_dir` (default `~/.omp/agent/sessions`) and `Day`'s clock (`DayOptions.now`).
   - Test: a temp sessions dir with calls on three earlier local days gives those days' totals; a file older than 8 days is ignored; a recorded day is not overwritten.

**Fork change (A, `crates/herdr`):** the dock gets only left clicks today (`ShellExtension::dock_mouse`, `embed.rs:330`, called from `extension_takes_mouse`, `extension.rs:185–205`). Add a trait method with a default body so the test impl in `boss_guard.rs:26` still compiles: `fn dock_scroll(&mut self, column: u16, row: u16, down: bool) -> Reply { Reply::default() }`. Add `shell_dock_scroll` in `embed.rs` and handle `MouseEventKind::ScrollUp/Down` in `extension_takes_mouse`. `src/frontier/shell.rs` implements it in `impl ShellExtension for JocastaShell` (~242–318). Update FORK.md "Seams for Jocasta".

**A's view:**
- `TodayState.news_top: usize`. `const NEWS_LINES: usize = 5`.
- `Pick` gains `news: bool`. `JocastaShell::step` (shell.rs:141) scrolls `news_top` when the picked news row is at an edge. `Frame` exposes the news box line range for the wheel.
- `underway_rows` (389–419) gets the facts. `landed_rows` (422) gets the PR. `needs_you_rows` (343) gets `Stuck`.

**C's files:**
- `thread_facts.rs` (§2.3).
- `day_view.rs`: `rail` (111) fills `diff`, `check`, `stuck_for` from `DayThread`. `threads_awaiting` (377) emits `Stuck` rows. The clock is `Day::now_ms()`.
- `day.rs`: `DayThread` fields. `DayHooks::pr_facts`.
- `finish.rs`: pass the PR url to `landed`.
- `review_flow.rs`: call `thread_facts.note_check` in `run_checks`.
- `spend_days.rs`: pure read, update and prune, atomic write through `shared/fsx`.

**B's part:** `status.rs::day_thread` (596) fills the new `DayThread` fields from `ThreadFacts`; `hud_for` (~480) fills `spend_avg_7d` and `top_thread`. `ServiceHooks::pr_facts` delegates to C's cache.

**Builder-chosen:** `STUCK_MS`; `NEWS_LINES`; file name `spend-days.json`; the PR cache TTLs above; the costliest-thread rule; the backfill's 8-day window.
**Docs:** manual Daily use: news box, thread facts, stuck, PR, and that `avg` is backfilled from the last 8 days of omp sessions at start. CHANGELOG. Appendix I rows for `thread_facts.rs`, `spend_days.rs`, the `spend-days.json` file, the `dock_scroll` seam.
**Risks:** `gh` rate limits (the caching above, ≤ about 10 PRs, no polling when settled). `git diff --shortstat` on a large worktree runs off the event loop (PERF-1) and its result is cached. Dock height: see §4.

### 0669: quota for every account (D parser, C Day side, A view, B hud)
**Testable:**
1. Each logged-in account shows its 5h and 7d use with the next reset, plus a "left" line per window from `capacity`.
2. One toast per account and window when it passes 80%, again only after it went back under.
3. A failing `omp usage` is quiet.

**Facts:**
- `omp usage --json` as of omp 19.1.1: top-level `reports[]` with `limits[]` (`id` `anthropic:5h|7d|7d:fable`, `window{id,resetsAt}`, `amount{usedFraction}`) and `metadata{email,accountId,orgId,subscriptionPlan,credentialId}`; `capacity.anthropic[]` = `{window, meter?, accounts, usedAccounts, remainingAccounts}`.
- `--redact` shortens the email to a prefix (`"zi*"`) and the account id (`"98*"`), so full emails never reach the protocol. Use it for the per-account read.
- Account name = `metadata.email`, else `accountId`, else `#<credentialId>`.
- Fable-tier limits (`meter` present) are skipped for the 5h/7d lines; capacity entries with `meter` are skipped (builder choice).
- The existing `usage_reports`, `read_quota` and `omp_usage` stay untouched; `evals/pacer.rs:14,63,131` uses them.

**D:** `omp_usage_redacted`, `read_accounts`, `account_warning` (§2.4) with tests beside `quota.rs` (a 2-account fixture, a missing `metadata`, no `capacity`, `not json`).
**C:** `day_poll.rs::read_quota` (200–260) calls `omp_usage_redacted` + `read_accounts`, stores accounts and capacity, still computes `Hud.quota` from the most-used window across accounts, and tracks warnings by `(account, QuotaSpan)` (`day.rs` State `quota_warned`). Update `day_threads_tests.rs:220` and add a 2-account case.
**A:** a QUOTA section in the dock after CALENDAR: one line per account (`name 5h 42% · 7d 18% · resets 16:30`), then one `left` line per window. Capacity lines are dropped first at small heights.
**B:** `hud_for` takes `day.quota_accounts()` into `Hud.accounts` and `Hud.capacity`.
**Builder-chosen:** the line format; the skipping of fable limits.
**Docs:** manual Daily use; CHANGELOG.
**Risks:** `omp usage` is slow or absent: the poll is already a Job with a failure that is reported once. If `--redact` isn't supported by a given omp, the read yields `None` and the HUD keeps its last good value [INFERENCE: omp 19.1.1 was observed to support it].

### 0670: leftovers in the dock, removed behind his Allow (A view, B service)
**Testable:**
1. A LEFTOVERS section shows when `Dashboard.leftovers` is non-empty: one line `project/folder`, pickable.
2. Key `x` on a picked leftover sends `ClientMessage::LeftoverRemove{folder}`.
3. The service only acts on a folder that is in `status.leftovers()`, else it ignores it.
4. It asks Allow through `Tools::remove_leftover(path, Door::Key)` (tools.rs:689).
5. The asker's wording for a key-opened question says "You asked to delete …", not "The boss".
6. The result comes back to the dock as `ServerMessage::Action{text, is_error}`, on success as well as on error.

**A:** `Pick.leftover: Option<String>`, `x` in `shell.rs::dock_key` (~295), `KEYS` (view.rs:62) and `KEY_LINE` (64), where `x` is lower priority than the existing keys and drops first. Do not add a `PluginAction`; the existing 5 are bound to herdr keys through `config/herdr.toml`.
**B:** `service.rs::handle_one` (~835–845), like `link.open`. `wording.rs::asker(door, command)` (136) gets Door::Key wording. The handler replies with `ServerMessage::Action` for both outcomes (it currently replies only on error).
**Tests (B):** an unknown folder is ignored; an allowed one is removed and replies; a denied one stays and replies; wording per door.
**Builder-chosen:** the wording text; the `x` key; the section's place in the dock.
**Docs:** manual Daily use and keys table. CHANGELOG.
**Risks:** the dock's key table is small: `x` must not collide with any existing dock key (A checks `dock_key`). A folder path from the wire is never used unless it matches a listed leftover.

### 0671: the check queue learns what each check costs (D; B wires)
**Testable (pure arithmetic with injected free memory and cores; no real OS calls):**
1. A check with a record is admitted when its recorded peak plus the sum of the running checks' peaks fits in `available_memory_bytes()` minus the reserve, and the sum of cores plus its own fits in `available_parallelism`. Otherwise it waits, FIFO.
2. A check with no record runs alone once: it waits for the running ones to drain and blocks others while it runs; its measurement becomes the record.
3. Waiting does not count toward its own timeout. The timeout starts at `Inner::execute`, after admission, before `self.start`.
4. `stop()` releases waiters.
5. Records live in `<data>/checks/costs.json`, key = project dir + command: `peak_rss_bytes`, `avg_cores`, `runs`, `at`.
6. The dock shows `checks: <n> running · <m> waiting` only while any run or wait.

**Facts and anchors:**
- Hook: `CheckRunner` (`checks.rs` 456–580), `Inner::execute` (~884) before `start` (852); `finish` (863) sleeps from the start. The lock file is already held while waiting; in-process callers join through the `in_flight` Flight. A cross-process waiter's deadline is `timeout + LOCK_GRACE` and may expire while queued. All Jocasta checks are in-process, so this is a documented risk, not handled.
- `layer/bench.rs:171` builds a default runner: admission is off (`None`).
- `available_memory_bytes()` is in `src/shared/pollers/machine.rs:33` (exists).
- Measurement in `proctree*.rs`: descendants of the check's root pid (`ChildWrapper::id()`) through `parents()`/`descendants(root, parents, skip)` (`proctree.rs:412`). Sample about every 500 ms. Peak = the maximum over samples of the summed RSS of the tree. `avg_cores` = the tree's CPU-time delta over wall time.
  - Windows: the existing Toolhelp `snapshot()` plus `NtQueryInformationProcess` class 3 (`ProcessVmCounters`); the `Wdk_System_Threading` binding is already enabled. [INFERENCE] it works with `PROCESS_QUERY_LIMITED_INFORMATION`. D verifies; if not, §0 note 3.
  - Linux: `/proc/<pid>/status` `VmRSS` and `/proc/<pid>/stat` utime+stime.
  - macOS: `/bin/ps -o pid=,rss=,time=` (the `ps` constant is in `proctree_unix.rs`); no libc crate.
  - The Job Object handle is not exposed by process-wrap's `ChildWrapper`, so `QueryInformationJobObject` is not usable.
  - A process that exits between samples loses its CPU time. This is accepted and documented.
- Setting `checks.memoryReserveGb`, default 2 (A, §2.7).

**Tests:** admission arithmetic, FIFO, the lone first run, stop-release, record round-trip and corrupt-file tolerance (an unreadable `costs.json` is treated as no records); an own-process RSS > 0 test per platform with `#[cfg]`.
**Builder-chosen:** the setting name; the file `costs.json`; the 500 ms sample; the record key; where a wait is logged.
**Docs:** manual Config (`checks.memoryReserveGb`, a note that the first run of each check runs alone). CHANGELOG. Appendix I: `check_costs.rs`, `costs.json`, the setting.
**Risks:**
- Windows, Linux and macOS read memory differently; peak is a sample maximum and can miss a spike shorter than 500 ms.
- A wrong record makes the queue too tight or too loose; each run updates it (`runs`, a rolling maximum with decay is the builder's choice; list it).
- A check recorded alone may be an outlier.

### 0672: phone check-in through ntfy: content-free on a public server, Allow only on his own (E; A settings and docs; B wiring)
Amended record: re-read 0672 "Choice". The mode is decided once at start from settings.

**Settings (A, §2.7), builder-chosen names:** `ntfy.server` (a URL; unset = off), `ntfy.ownServer` (bool, default false: marks the server as his own), `ntfy.token` (his ntfy access token). `secrets.yml` (0594) is omp's own file (`config/omp/secrets.yml`, shipped as `[]`, written only when missing by `setup_snapshot.rs`); nothing in `src` reads it, and it belongs to omp, not Jocasta. So the token lives in `jocasta.yaml`, under `ntfy.token`. The manual says the token is a secret in that file. It is never logged, never in a toast, never in `jocasta diagnose` output.

| Mode | When | Pushed | Buttons | Reply stream |
|---|---|---|---|---|
| **off** | `ntfy.server` unset | nothing | none | none |
| **public** | `ntfy.server` set, and `ntfy.ownServer` is not true or `ntfy.token` is empty | content-free: kind and count only | none | none |
| **own** | `ntfy.server` set, `ntfy.ownServer: true`, `ntfy.token` set | full text | Allow and Deny one-time tokens | subscribed, `Authorization: Bearer <token>` |

In public mode the token is never sent anywhere, even if set. `check_settings` warns once when `ntfy.ownServer` is true without a token ("pushes stay content-free"). The old 16-character topic safeguard is dropped: topics are generated.

**Topics:** Jocasta generates them, never typed: two names, main and reply, each 32 random bytes in hex (256 bits, at least the 128 required), through `getrandom` as `popups.rs random_hex` (~43). They are stored once in `<data>/ntfy-topics.json` and reused. To see them for the phone app, builder-chosen: `jocasta diagnose` (`src/setup/diagnose.rs`, owned by E for this one line) prints a `phone` line with the server, the mode and the main topic (the reply topic is not needed on the phone); before the first start it says the topics are made when Jocasta starts. The manual says the same. No new protocol message or toast. The main topic is the only thing diagnose shows that is secret: the manual says diagnose output with the phone on shouldn't be pasted.

**Testable (a local axum fake ntfy server, `ntfy.server` pointed at it):**
1. Off until `ntfy.server` is set: no request is made.
2. **Public mode never sends text or buttons.** A push is `Jocasta: <n> need you` or `Jocasta: a thread finished` (plural, builder wording). Title, body, headers and `Actions` never contain a thread title, project name, command, path, `message` or `detail`; the fake sees no `Actions` header. A test scans every request for sentinel text. An Allow popup pushes `Jocasta: an Allow is waiting`, with no buttons.
3. **No reply stream without own server and token.** In off and public mode (including `ownServer: true` with no token) the fake sees no `GET .../json`.
4. Own mode: a new Allow popup (choices `[ALLOW, DENY]`, not opened by his key) publishes full text, the question's `message` only, never its `detail`, cut by `native.rs::note_body` (240), with Allow and Deny buttons. Every publish and the subscription carry `Authorization: Bearer <token>`.
5. A token is accepted only when it came through the reply stream, its question is still open, and it was not used before. Allow and Deny have separate tokens. A token for a closed question, a used one, or an unknown one is refused.
6. Allow from a token answers through `Popups::answer(code, {choice:"Allow", typed:<popup.view().code>})`; Deny is `{choice:"Deny"}`. The typed code still works.
7. The reply stream reconnects with back-off 1 s→60 s, treats 90 s without bytes as dead, and resumes with `?since=<last id>`; the first connect uses `since=<now unix>` so old tokens aren't replayed.
8. NEEDS YOU rows and finished threads are pushed once each, in both modes: the sent set is kept in `<data>/ntfy-sent.json`, keys pruned after 7 days. Quiet hours (`Day::quiet()`) hold these pushes (not marked sent while quiet); an Allow is not held. In public mode, count = the rows new since the last push, one push per tick, not one per row (builder-chosen).
9. Topics are generated once, are at least 64 hex characters, differ from each other, and are reused after a restart.

**Facts:**
- Publish: `POST <server>/<topic>` with `Title`, `Priority`, and in own mode `Actions` and `Authorization`. Buttons: `http, Allow, <server>/<replyTopic>, method=POST, body=<token>, clear=true`, plus the button's `headers.Authorization=Bearer <token>` parameter because the phone posts to a protected topic [INFERENCE: ntfy http actions take `headers.*` parameters; E checks the ntfy docs and reports].
- Subscribe (own only): `GET <server>/<replyTopic>/json`, newline-delimited; handle `event:"message"`, ignore `open` and `keepalive`. `reqwest::Response::chunk()` streams it: no new crate.
- Tokens: 32 random bytes in hex, single use; the registry drops a question's tokens when it ends and is cleared on stop.
- Hook point: a `Popups` observer in `popups.rs` (`Popups::open`; `asker.rs::asker` is the single open point). Key-opened popups (`from_key`) are not pushed.
- Wiring: start in `server.rs::start_rest` after the service; stop in `stopper_of` (`StopParts`).
- Trust boundary (the manual says it): in own mode whoever can read the main topic with his token, or the server's operator, sees the pushes and can answer an Allow, since the one-time token rides in the notification. On a public server only a kind and a count leave the machine.

**Tests:** the nine points above; no sleeps (inject back-off and dead-stream times in `PhoneOptions`, an internal field, no env var).
**Docs (A):** manual Config: `ntfy.server`, `ntfy.ownServer`, `ntfy.token`, the three modes, how to see the topic (`jocasta diagnose`), and the trust boundary in plain words. CHANGELOG. Appendix I: `phone.rs`, `ntfy-sent.json`, `ntfy-topics.json`, the settings.
**Risks:**
- A long-lived stream through proxies: the 90 s dead-stream rule handles stalls, not auth.
- `ntfy.ownServer: true` on a server that isn't his is his claim; the token requirement is the only other gate. The manual says so.
- The token is a secret in `jocasta.yaml`.
- [INFERENCE] `ntfy.sh` stream messages carry an `id` and the server supports `since=<id|unix>`; E checks with the fake and the ntfy docs.

### 0673: scheduled threads (E parse/start, C tick, B hook, A NEXT)
**Testable (pure, `schedules_tests.rs`):** slot math: `latest_slot(now)` and `next_run(now)` over `days` and `time` in local time, with DST gaps and ambiguity resolved through `.earliest()`; parse errors as `file:line: reason`.
**Testable (Day level, injected clock `DayOptions.now`):**
1. A missed run starts once, however many slots passed.
2. A new schedule's baseline is its first-seen time, so past slots don't fire.
3. The same slot never repeats.
4. A clock moved backwards does not re-run a slot.
5. A start error is reported once and not retried for that slot.
6. A schedule whose `done_when` the guard would answer 'prompt' (`Ok(true)`) or deny (`Err`) is skipped with a notification, because an unattended Key door would otherwise run it unasked.
7. A thread already open in the project with the same title is skipped, with a notification.
8. A bad schedule is reported once and skipped.
9. NEXT shows the soonest 3 as `Mon 09:00 acme-api: dependency bumps`, before his notes.

**Facts:**
- File `<data>/projects/<pid>/schedules.yml` (`paths::project_dir(project_id)`, `paths.rs:100`): a list of `{brief, days:[mon..sun], time:"HH:MM" local, done_when?:[..], heavy?:bool}`. Parse with `shared/yaml.rs` (`parse_yaml_file` / `parse_value`).
- State `<data>/projects/<pid>/schedules.state.json`: `{id: {since, last}}`. Schedule id = the first 12 hex of sha256 over `brief|days|time`, so editing a schedule resets it. Baseline = `last ?? since`; due when `latest_slot(now) > baseline`; on start `last = now`.
- Tick: `Day::tick` (day.rs:760, each minute) gets `self.guard("schedules", …)` beside the weekly-review guard (~790). It uses `Day::now_ms()` (the injectable `DayOptions.now`), not `clock::now_ms()`.
- Start: `ThreadStarts::start_with(project_name, title, brief, StartOptions{door: Door::Key, done_when, heavy})` (`thread_start.rs:228`). The title is `keys.rs::brief_title(brief)`. Door::Key skips the boss's Allow and opens in plan mode, so he approves the plan.
- E changes `thread_start.rs::vet_done_when` (~320) from private to `pub(super)`, and calls it before starting (item 6).

**Division:**
- E: `schedules.rs` (`Schedule`, `parse_schedules`, `latest_slot`, `next_run`, `ScheduleState` read/write) and `thread_start.rs`: `ThreadStarts::start_scheduled(project, &Schedule) -> Result<ScheduleStart>` doing the vet, the same-title check and the start.
- C: the tick guard in `day.rs`, the baseline/last handling, `DayHooks::start_scheduled`, `Day::scheduled_next`.
- B: `ServiceHooks::start_scheduled` in `service.rs` calls E's function.
- A: `Today.scheduled` is drawn in NEXT before his notes, with `shared::day::weekday`/`hhmm`. The dock's NEXT is only drawn when spare ≥ 2.

**Builder-chosen:** file names; the id hash; the notification wording; where NEXT puts the line.
**Docs:** manual: a new "Scheduled threads" section under Daily use (file location, format, missed-run rule, the plan-mode approval). CHANGELOG. Appendix I: `schedules.rs`, the two files.
**Risks:**
- DST: a time in a gap (`.earliest()` gives none) shifts to the next valid minute [INFERENCE]; E decides and lists it.
- A machine asleep for days yields one run, by design.
- A tick that overlaps a long start: the `guard` serialises.

## 4. Cross-cutting

**Dock height budget (A).** Section priority from most to least important at small heights: NEEDS YOU, UNDERWAY, LANDED, CALENDAR, QUOTA, LEFTOVERS, NEWS, NEXT. NEWS reserves up to 7 lines and drops under 3; QUOTA lines per account drop capacity first; LEFTOVERS shows nothing when empty. A adds `view_tests.rs` cases at heights 10, 20 and 40 for the new combinations. No section may push NEEDS YOU or UNDERWAY off the screen.

**Docs (A writes all, from the builders' result notes):**
- `docs/manual.md`: Daily use (~57): boss row, news box, thread facts, quota accounts, leftovers, scheduled threads. Config (~102, `jocasta.yaml` keys ~109): `ntfy.*`, `checks.memoryReserveGb`.
- `CHANGELOG.md`: one entry per record.
- `docs/plan.md` appendix I (~2194): rows for each new file, setting and seam (0027).
- `crates/herdr/FORK.md`: the `dock_scroll` seam.
- `docs/status.md`: HUD round progress.
- No new `JOCASTA_*` variable. Test seams are `ServiceOptions` and `DayOptions` fields (`test/docs/process-rules.test.ts` enforces appendix I for any env var).
- The decisions index already lists 0667–0673.

**Every builder's result note must list:** its builder-chosen details (as listed per record), the files it touched, the [INFERENCE] items it verified, and anything it could not match to the plan.

**Verification (Main, once):** `mise run check`; then, by hand, a live look at the boss row (0667), the news-box wheel and a leftover removal (0668, 0670), and the ntfy flow against a private topic (0672). Quota and check queue are covered by unit tests.
