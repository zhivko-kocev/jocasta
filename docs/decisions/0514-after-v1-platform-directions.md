# 0514 After v1, platform: omp², herdr's split, Rust and the companion

- Status: decided
- Kind: product
- Phase/slice: after v1
- Date: 2026-11-01

## Question
omp is being rewritten as omp², herdr split into a core and an app, and the companion runs on WebView2. What does Jocasta do about each, and what is written down now?

## Facts
Verified 2026-11-01 (`D:/thesis/sources/proposals-2026-11/stencil-playbook.md`, `rust-ui-herdr.md`, `tools-and-pi.md`; `D:/thesis/sources/playbook-explained.md`).
- omp² is a Rust rewrite, not released: npm's only tag is `latest: 18.15.0`; "There is no public release date or ETA for OMP 2 yet" (issue #9820); its public branch says "expect renames and breaking changes without notice". It would break Jocasta's YAML `--config` overlays (cfg scripts there) and its `.jsonl` session readers (an `.oms` journal there), likely `APPEND_SYSTEM.md` and herdr's TypeScript integration [INFERENCE]; it adds `--plan-mode`, plan mode as a composable "Director", and a session subscription.
- herdr's makers started `herdr-core` (`hc` 0.1.16, "Core draws nothing", status "preview") and `herdr-app` (0.1.11, GPUI); `hc` has no workspace, tab, worktree or `agent.prompt` methods; herdr 0.9.5 "keeps working" but doesn't share agents with them; herdr-app's mods "can't: use HTML, CSS, a web view".
- 0009 chose TypeScript on Bun ("No Go rewrite"); Rust was never weighed. Jocasta's seams to omp and herdr are commands, files and sockets.
- Electrobun 2.0.2 never sets WebView2's `AllowSingleSignOnUsingOSPrimaryAccount`, whose default is false; 0496's after-v1 follow-up planned to switch it off there. No measure covers the companion or WebView2 alone (`docs/testing.md`).
- The boss's MCP tools hold the service's token, the boss key and his Allow/Deny; 0485 accepts that scripts bypass the guard until OS isolation.
- Pi "skips features like sub-agents and plan mode" (https://pi.dev); 0386 moved Jocasta to whole omp.

## Options
| omp² | Pros | Cons |
|---|---|---|
| Break list now, move when released | Known cost; the playbook's design | Large port when it comes |
| Stay on omp 18 | No port | 18.x may stop getting fixes |

| herdr-core | Pros | Cons |
|---|---|---|
| A spike after v1 | Cost known early | Work before it's needed |
| Watch only | No work until it matters | Cost unknown |
| Neither | | Surprised later |

| Rust | Pros | Cons |
|---|---|---|
| TypeScript, stronger checks | Nothing breaks | |
| TypeScript plus Rust for new isolated parts | Rust where it pays | Two toolchains |
| A full rewrite after v1 | One language with herdr-core and omp² | Months |
| Decide after omp² ships | Decides with omp²'s facts | Waits |

Also asked on their own: measure the companion first; the boss tools as a proxy boundary; an inventory of the seams on Pi.

## Recommendation
omp²'s break list now; herdr-core watched; TypeScript plus Rust for new isolated parts; the companion measured first; the proxy boundary written down; the inventory written.

## Choice
- **omp²:** its break list goes into plan §5.5 now; Jocasta moves to omp² once it's released (that move gets its own record, re-checking every seam), taking omp²'s live session feed instead of tailing files and its plan-mode building block instead of the settle timer.
- **herdr-core:** watched only: re-checked when it leaves "preview" or herdr 0.9.x stops getting fixes.
- **Rust:** decided after omp² ships.
- **The companion:** measured before any rewrite: H6 split into the Bun service, the WebView2 processes and the whiteboard server; the time to first paint in H5; and a bare WebView2 host's egress, which settles whether the Outlook/Substrate peer is the runtime's own (0496).
- **The boss tools as a proxy boundary:** appendix I sorts the 17 tools into boundary and view; OS isolation's acceptance test is that no script a thread writes reaches the service's token or `/api/mcp/*`.
- **Seams on Pi:** appendix I's seams gain a "holds on Pi?" column, Pi's facts checked against its source first.

## Reason
"omp² break list now", "once omp2 is out we use that"; "Watch only"; "Decide after omp² ships"; "Measure the companion first"; "Boss tools as a proxy boundary"; "Write the inventory".

## Consequences
- Differs from the recommendation: Rust waits for omp².
- 0500 still holds omp at 18.13.1 until A5 is done, and 0383's re-check applies to every bump before omp².
- 0496's follow-up becomes the bare WebView2 egress check; there is nothing to switch off in Electrobun.
- The herdr-core spike is not chosen.
