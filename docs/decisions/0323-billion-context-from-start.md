# 0323 billion-context from the start

- Status: decided
- Kind: product
- Phase/slice: 3/3.4, 3.5
- Date: 2026-10-08

## Question
Which compaction does v1 ship, and how is billion-context taken in?

## Facts
- 0242 ruled a spike between Pi's compaction (A), billion-context (B) and a Jarvis rollover (C). Running it needs real-model runs, an install and building C first.
- Checked 2026-10-08 (npm and the package's README): `billion-context` 0.1.188, MIT plus one term requiring a credit with a link in the product's docs, dependency `zod` only. It works as a proxy: on Pi its plugin spawns a local proxy in the process that rewrites model requests and injects `compress`, `decompress`, `search_context` and `acp_status`, which the model calls itself. It self-updates from npm every 3 minutes (`ACP_AUTO_UPDATE=0` turns it off) and force-installs critical-advisory versions (`BILI_ADVISORY_CHECK=0` turns that off); self-update can't change code compiled into `jarvis.exe`. Its README calls it early, with real-model integration testing next. On Windows, antivirus can lock its sessions dir (`%USERPROFILE%\.local\share\billion-context\`).
- Redaction (0233) runs on tool results before requests are built, so the proxy sees redacted content.
- His answers: "i would use the billion context plugin from the start", "use latest billion context + we steal omp best stuff"; he first chose self-update, then, after the token risk was laid out, a pinned copy.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pinned in `jarvis.exe` (0.1.188), updates and advisories off; a bump command later | Reviewed versions only | Bumps by hand |
| Global install, self-updating | Always latest | Unreviewed code sees his sign-in token |
| Global install, self-update off | — | A second install to manage |

## Recommendation
Pinned in jarvis.exe.

## Choice
Now (0325): billion-context ships next to `jarvis.exe`, and Jarvis runs its proxy with `jarvis.exe` in Bun mode.

Originally: billion-context 0.1.188 is a pinned dependency compiled into `jarvis.exe`, loaded through its Pi plugin, with `ACP_AUTO_UPDATE=0` and `BILI_ADVISORY_CHECK=0` set by Jarvis. It replaces Pi's compaction (no double compression). Its `search_context` replaces `history_search` (0245). A `jarvis` command to bump it is decided when built. If it fails on Pi 1.1.0, the build falls back to Pi's compaction and says so.

## Reason
"i would use the billion context plugin from the start but also can you list me omp compaction methods"; "use latest billion context + we steal omp best stuff"

## Consequences
- Replaces 0242's spike: B is chosen without it.
- 3.1 and 3.3 are not built (0241). 3.5's `history_search` is not built (0245).
- README and docs credit billion-context with a link (its license term).
- Differs from the recommendation, which was to ship Pi's compaction now and run the spike after phase 6.
- Amended by 0325: shipping set by 0325.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
