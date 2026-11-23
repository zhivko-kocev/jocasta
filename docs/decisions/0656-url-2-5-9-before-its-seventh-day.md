# 0656 url 2.5.9 enters the lock before its seventh day

- Status: v1
- Kind: process
- Phase/slice: fork build, review
- Date: 2026-11-23

## Question
RUSTSEC-2026-0592 (`url` 2.5.8: `Url::origin()` recurses on nested `blob:` URLs) is ignored by ID in `deny.toml` until `url` 2.5.9, published 2026-11-20, is 7 days old (`fork-build.md` choice 104, For him 26). Wait, or take it now?

## Facts
Verified 2026-11-23: `cargo update -p url --precise 2.5.9` is refused as too new; with `CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow` it moves `url` 2.5.8 → 2.5.9 and `form_urlencoded` 1.2.2 → 1.2.3, which 2.5.9 requires; both are from the `servo/rust-url` repository and were published 2026-11-20. Jocasta calls `origin()` only on an `http` URL on 127.0.0.1 that it has checked (`read_discovery`, `src/frontier/server/whiteboard.rs`).

## Options
- **Wait until 2026-11-27.** Pros: 0627's rule holds. Cons: the advisory stays ignored four more days.
- **Take it now.** Pros: the fix is in at once and the ignore line goes. Cons: waives the 7-day rule for one release.

## Recommendation
Wait.

## Choice
His answer of 2026-11-23: now. `url` 2.5.9 and the `form_urlencoded` 1.2.3 it requires enter `Cargo.lock` with the resolver's age check allowed for that one update; `deny.toml`'s RUSTSEC-2026-0592 line is removed. `.cargo/config.toml`'s 7-day rule is unchanged.

## Reason
His answer in the review form.

## Consequences
- A one-time exception to 0627 for these two releases; every other bump still waits 7 days.
- `fork-build.md` For him 26 is done.
