# 0640 The port's helper crates; the first lock takes the newest versions

- Status: v1
- Kind: product, process
- Phase/slice: program
- Date: 2026-11-15

## Question
Two questions: which helper crates may the port use directly, beyond those its records name? And how does the 7-day rule (0627) apply to the first `Cargo.lock`?

## Facts
Verified 2026-11-15, on cargo 1.100.0 with the 7-day rule.
- **What the helpers replace.** The TypeScript uses each of these today:
  - regular expressions in `guard.ts`, `yaml.ts`, `hunk.ts` and many literal patterns;
  - local dates and times;
  - sha256 and random tokens, through `node:crypto`;
  - a websocket client, used by `today` and `ask` to read the state feed;
  - one glob, for the projects' ignore patterns, through `Bun.Glob`.
- **Size of the chosen set:** 244 crates on Windows. regex, chrono, sha2, getrandom, uuid, tokio-tungstenite, futures-util, url and thiserror are already among them.
- **globset** adds 2 crates. A hand-written matcher would be about 40 lines.
- **What the rule holds back today:**
  - rusqlite 0.42.0, 4 days old. 0.41.1 would be used instead; it carries libsqlite3-sys 0.39.1, which has the CVE fix.
  - rmcp 3.10.0, 3 days old (3.9.0 instead).
  - saphyr 0.3.4, 4 days old (0.3.3 instead).

## Options
Helpers, a tick-list:
- the crates already in the tree, used directly;
- globset.

His note on that form, "you can use the newest versions also", led to a second question:

| Option | For | Against |
|---|---|---|
| Keep the 7-day rule; "newest" means the newest it allows | the protection he chose in 0627 | a few days behind |
| The first lock takes the newest versions; the rule holds after that | current versions from day one | three packages under a week old enter the lock |
| Drop the rule for crates | always the newest | no wait for crates |

## Recommendation
Both helper ticks, and keep the rule as it is.

## Choice
- **Helpers used directly:** regex, chrono, sha2, getrandom, uuid, tokio-tungstenite, futures-util, url and thiserror. None adds a crate. globset is used as well.
- **The first `Cargo.lock` takes the newest version of every crate.** From then on, the 7-day rule in `.cargo/config.toml` holds for every crate that is added or bumped.

## Reason
His answers:
- On the helpers: both ticked, with "you can use the newest versions also".
- On the rule: "we keep the rule just for the start we start with newest".

## Consequences
- This record amends 0627 (the first lock is taken outside the rule) and 0639 (rusqlite 0.42 enters the first lock now).
- **How the first lock is made:** once, with `CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow`. Later resolves run with the rule.
