# 0669 Quota for every logged-in account

- Status: v1
- Kind: product, ux
- Phase/slice: HUD and orchestration round, after the fork build
- Date: 2026-11-23

## Question
With 0668's dock he added: "also how much is left for all logged in accs maybe". How does the dock show quota across his accounts?

## Facts
Verified 2026-11-23 on omp 19.1.1: `omp usage --json` reports each logged-in account with its `anthropic:5h`, `anthropic:7d` and `anthropic:7d:fable` windows and their resets, and a `capacity` block per window (5h: 2 accounts, 0.54 used, 1.46 left). The dock shows only the most used window across accounts (0456, `src/frontier/server/quota.rs`).

## Options
- **One line per account plus the total left.** Pros: shows which login to use. Cons: a line per account.
- **Only the total left per window.** Pros: one line. Cons: hides which account is near its limit.

## Recommendation
One line per account.

## Choice
His answer of 2026-11-23: one line per account, its name shortened as `omp usage --redact` shortens it, with its 5h and 7d use and the next reset, and one line with what is left across the accounts per window, from omp's `capacity`.

## Reason
He works across two logins; which one has room is the decision the line serves.

## Consequences
- Amends 0456 (the HUD's quota).
- The warning toast past `QUOTA_WARN` goes out per account.
