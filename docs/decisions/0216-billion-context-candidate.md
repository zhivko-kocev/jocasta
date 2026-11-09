# 0216 billion-context as a 3.4 and 3.5 candidate

- Status: decided
- Kind: product
- Phase/slice: 3/3.4, 3.5 (raised during phase 2)
- Date: 2026-10-08

## Question
Zhivko found billion-context and asked whether it can simplify Jarvis. How does it enter the plan?

## Facts
Inspected 2026-10-08: npm `billion-context` 0.1.188 (https://pi.dev/packages/billion-context), repo ranxianglei/billion-context at commit 96be7a6.
- A local proxy between the agent and the model API. It injects four tools: `compress` folds a message range into a summary the model writes in its normal turn, `decompress` restores it, `search_context` searches folded and visible messages, and `acp_status` reports usage. Originals are kept on disk, so folds are reversible (README "Context-management tools"; `src/compress-tool.ts:414-418`).
- As a Pi extension it starts a child Node proxy (`dist/index.js start`, Node ≥20, `package.json:85`), patches `globalThis.fetch` to route model requests through it (`src/agent/pi-native.ts:1-12`), and cancels Pi's threshold and overflow compactions (`src/agent/pi.ts:671-688`). Inside a non-Node host such as `jarvis.exe` it finds Node on PATH or through `BILLION_CONTEXT_NODE` (`src/launcher.ts:3817-3899`); node v24.13.0 is installed on this machine.
- It checks npm every 3 minutes and installs newer versions, and separately force-installs "advisory" versions; both are on by default and turned off by `ACP_AUTO_UPDATE=0` and `BILI_ADVISORY_CHECK=0` (`CONFIGURATION.md:579,1688`). Exact pins (0041) need both off.
- Developed against `@earendil-works/pi-coding-agent` 0.83.0 (`package.json:61-62`); Jarvis pins 1.1.0 (0192).
- README status: "Early. Protocol handling and compression work against mock tests (500+ passing). Real-model integration testing is the next milestone." (`README.md:541`). Its paper reports 174,327 model calls and 18.76B input tokens with no window overflow, and a 95–97% prompt-cache hit rate in healthy sessions.
- About 80,000 lines of non-test TypeScript under `src/`; the npm package is 119.3 MB; one runtime dependency (`zod` 4.1.8).
- License: MIT plus an attribution term: a product end users see must credit billion-context with a link, on its home page, docs or About page (`LICENSE:25-43`).
- CI runs on `ubuntu-latest` and `windows-latest` (`.github/workflows/ci.yml:28`); on Windows, antivirus can lock its session files and needs an exclusion (README).

## Options
| Option | Pros | Cons |
|---|---|---|
| A fifth 3.4 mechanism and a 3.5 search option, decided at phase 3 with a spike | Decided with data | Waits for phase 3 |
| Adopt it now as Jarvis's compaction, dropping 3.4's own design | V9's hardest part done by someone else | A Node sidecar and a fetch patch in the exe; untested on Pi 1.1.0; early status |
| A design reference only | No sidecar | Jarvis builds it |
| Not used | — | — |

## Recommendation
A 3.4 and 3.5 option with a spike.

## Choice
A fifth mechanism in 3.4 and an option in 3.5, decided in phase 3's grilling with a spike rule written then: run it pinned, with self-update and advisories off, on Pi 1.1.0 over real threads, and measure prompt-cache hit rate, window overflows and cost against Jarvis's own rollover.

## Reason
Not given.

## Consequences
- Plan 3.4 and 3.5 list it; appendix A gains a row.
- Installing it for the spike asks him first (no installs without asking); the attribution term applies if it ships.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
