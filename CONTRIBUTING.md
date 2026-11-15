# Contributing

Jocasta is built slice by slice from `docs/plan.md`.

1. **Grill.** Before a slice is built, every decision on its list is asked in a grilling session (0001, 0020). Since the v1 beta, details that carry out a decided record (texts, labels, small defaults, fixes restoring decided behavior) are the builders' and listed as built; only what goes beyond the plan is asked (0363).
2. **Record.** Each answer becomes a record in `docs/decisions/` with an index row, before the slice's code is written (0002, 0031). Decided records are carried into the plan text in the same change.
3. **Build.** The slice is implemented exactly as its records say. Anything unclear stops the work and opens a grilling session (0020).
4. **Check.** `bun run typecheck`, `bun run lint` and `bun run test` pass after each code slice (`mise run check` runs all three, 0476; the tests run four workers at a time, 0616). Page tests drive headless Microsoft Edge and skip without it; the smoke that needs a real herdr runs only with `JOCASTA_HERDR_SMOKE=1` (`mise run smoke`).
5. **Review.** The agent commits each finished phase once its gates pass, under the repo's own identity, and Zhivko reviews the commits (0564). No branch or force-push unless he asks; before any push the private-names scan prints nothing.

Changed decisions are edited in place (0014); the reason for the change goes in the phase-end commit message. Requests after v1 are GitHub issues that link the feature's record (0018).
