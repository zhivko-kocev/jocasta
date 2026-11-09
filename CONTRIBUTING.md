# Contributing

Jocasta is built slice by slice from `docs/plan.md`.

1. **Grill.** Before a slice is built, every decision on its list is asked in a grilling session (0001, 0020). Since the v1 beta, details that carry out a decided record (texts, labels, small defaults, fixes restoring decided behavior) are the builders' and listed as built; only what goes beyond the plan is asked (0363).
2. **Record.** Each answer becomes a record in `docs/decisions/` with an index row, before the slice's code is written (0002, 0031). Decided records are carried into the plan text in the same change.
3. **Build.** The slice is implemented exactly as its records say. Anything unclear stops the work and opens a grilling session (0020).
4. **Check.** `bun run typecheck`, `bun run lint`, `bunx tsc -p src/app` (after a Windows build) and `bun test` pass after each code slice (`mise run check` runs all four, 0476). Page tests drive headless Microsoft Edge and skip without it; the smokes that need a real herdr, Docker, his omp or the dictation model run only with `JOCASTA_HERDR_SMOKE=1`, `JOCASTA_GRAPHITI_SMOKE=1`, `JOCASTA_OMP_GATEWAY_SMOKE=1` (0463) or `JOCASTA_DICTATION_SMOKE=1`.
5. **Review.** Zhivko reviews the working tree and commits whenever he chooses (0312). Agents never commit, stage, branch or push.

Changed decisions are edited in place (0014); the reason for the change goes in the phase-end commit message. Requests after v1 are GitHub issues that link the feature's record (0018).
