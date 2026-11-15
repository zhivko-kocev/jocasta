# 0633 reqwest for HTTP calls

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
Which HTTP client makes Jocasta's outgoing calls? These are the pollers (GitHub, the news, weather, calendars) and the panes' calls to the service.

## Facts
Verified 2026-11-15. Crate counts are transitive, on Windows.
- **ureq 3.5** is blocking. On tokio it would run on the blocking threads, and a running request can't be cancelled; it ends at its timeout.
- **reqwest 0.13** is async, so dropping a request cancels it.
- **Crate counts:** with rustls and json, reqwest adds 32 crates over ureq on the hyper base (138 against 106), and 27 on the axum base (145 against 118).

## Options
| Option | For | Against |
|---|---|---|
| ureq | small and simple | not cancellable midway |
| reqwest with rustls | async, cancels on drop | about 30 more crates |

## Recommendation
ureq.

## Choice
reqwest 0.13, without default features, with `rustls` and `json`.

## Reason
His answer: `reqwest`.

## Consequences
- This differs from the recommendation.
