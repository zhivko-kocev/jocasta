# 0648 CI's `bun audit` ignores picomatch's ReDoS advisory by ID

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-18

## Question
The cutover's CI run fails `bun audit --audit-level=high` on one new advisory, GHSA-fc5q-39xc-vjpf: picomatch through 4.0.7 is open to a ReDoS through single-branch repeated extglobs. knip pulls it in. What does the audit do?

## Facts
Verified 2026-11-18.
- The advisory covers picomatch through 4.0.7. 4.0.7 (2026-08-24) is the newest version on npm, so no fixed version exists.
- picomatch reaches the repo only through knip 6.44.2 (`^4.0.7`) and tinyglobby 0.2.17. knip, a dev dependency (0614), matches the repo's own globs against the repo's own files.
- Bun 1.4.3's `bun audit` takes `--ignore=<GHSA>` and can repeat it.

## Options
- Ignore that GHSA in CI's audit, with its reason, until a fixed picomatch ships. Pros: every other advisory still fails CI. Cons: one more ignore to remember.
- Keep the audit step but don't let it fail CI. Cons: the next real advisory goes unnoticed.

## Recommendation
Ignore the GHSA by ID.

## Choice
CI's audit step runs `bun audit --audit-level=high --ignore=GHSA-fc5q-39xc-vjpf`, with a comment giving the reason. Every other high or critical advisory still fails CI.

## Reason
His answer: the recommendation.

## Consequences
- Amends 0614: CI's `bun audit` has an ignore list of one ID.
- The ignore goes once a picomatch release fixes the advisory and is 7 days old.
