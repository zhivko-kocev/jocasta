# 0646 A README for newcomers, the reference in the manual

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-17

## Question
He asked for a README that someone else who wants to use Jocasta can understand. How is it laid out?

## Facts
Verified 2026-11-17.
- The README (178 lines) is a full reference manual. Nearly every sentence cites records, it uses the boss, threads, the layer and the guard without introducing them, and it opens with "Zhivko's work surface". It never says in plain words what Jocasta is, what it needs, or how to get to a first thread.
- `scripts/build.ts` copies `README.md` into every release, beside `jocasta.exe`. A relative link from it to another doc in the repo would break in the installed copy.
- No test reads the README's text.
- 0039 chose the repo's docs: README, AGENTS.md, CONTRIBUTING.md, an MIT LICENSE and a CHANGELOG.

## Options
- **Structure.**
  - Split: the README becomes a newcomer's guide, and the detailed reference moves to a new `docs/manual.md` that the README links to. Pros: a short front door, and nothing is lost. Cons: two files to keep in step.
  - One file: the README guide first, with the reference kept below. Pros: one place. Cons: about 250 lines.
  - Trim: the guide only, with the detail dropped. Cons: the reference is gone from the docs.
- **Record numbers in user-facing text.** Out of the README but kept in the manual; out of both; or kept everywhere.
- **Voice.** "You", with no personal names; or his own setup, explained for others.

## Recommendation
Split; record numbers in the manual only; "you".

## Choice
- **Split.** `README.md` is a newcomer's guide: what Jocasta is, how a thread runs, its features, requirements, install, first run, adding projects, a first thread, keys, commands, where files live, safety and its limits, uninstall, development and credits. `docs/manual.md` holds the full reference that the README held: setup's every write, daily use in detail, every command, config keys, the guard and its limits, `/share`, and Linux and macOS.
- **Record numbers** stay in the manual, so each fact still traces to its record. The README has none.
- **Voice:** second person, with no personal names. Jocasta is described as a tool that anyone with omp and herdr can run.
- The README links to the manual by its GitHub address, so the link also works in the copy a release ships.

## Reason
His answer: the three recommendations.

## Consequences
- Amends 0039: the repo's docs add `docs/manual.md`.
- The release still ships only `README.md` (no change to `scripts/build.ts`).
- A change that alters what a user sees updates the manual. It updates the README too when it changes the guide's level of detail: a requirement, a step of the first run, a key or a command.
