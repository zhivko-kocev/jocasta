# 0647 cargo-deny ignores chrono's two "unmaintained" notices by ID

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-18

## Question
The cutover's gate fails `cargo deny check` on two new RustSec notices: RUSTSEC-2026-0543 (chrono is unmaintained) and RUSTSEC-2026-0555 (chrono-tz is unmaintained). Neither is a vulnerability, and no fixed version exists. 0639's policy fails the check on any advisory. What does the check do?

## Facts
Verified 2026-11-18.
- Both notices cover every version, and both say "No safe upgrade is available". RustSec names jiff and time as alternatives.
- Jocasta uses chrono directly in 61 files. rmcp 3.10.0 (through schemars) and icalendar 0.17.17 (through chrono-tz and rrule) also depend on it, so moving Jocasta's own code to another crate would still leave chrono in the build.
- `deny.toml` has no ignore list. Its bans, licences and sources pass.

## Options
- Ignore the two IDs by name, each with its reason. Pros: any other advisory, including a vulnerability in chrono itself, still fails the check. Cons: the list has to be revisited when rmcp or icalendar move off chrono.
- `unmaintained = "none"`. Pros: no upkeep. Cons: a future abandoned crate passes unseen.
- Move off chrono. Cons: a large rewrite and a new crate, and the check still fails through rmcp and icalendar.

## Recommendation
Ignore the two IDs by name.

## Choice
Ignore RUSTSEC-2026-0543 and RUSTSEC-2026-0555 in `deny.toml`'s `[advisories]`, each with its reason. Every other advisory still fails the check.

## Reason
His answer: the recommendation.

## Consequences
- Amends 0639: cargo-deny's advisories have an ignore list of two IDs.
- The ignore goes when rmcp and icalendar no longer bring chrono in, or when RustSec withdraws the notices.
