# 0654 His review of the fork build: what stays as built

- Status: v1
- Kind: process, product
- Phase/slice: fork build, review
- Date: 2026-11-23

## Question
The fork build ran unattended from 2026-11-18 and left 26 items for his review (`docs/research/fork-build.md`, "For him at the review"), each built on the plan's default. Which stay as built?

## Facts
Verified 2026-11-23: HEAD `8be82986` passes CI on Windows, Ubuntu and macOS (run 45057445868); the items' facts are in the ledger's choices they cite. Items 17, 19, 20, 23 and 26 changed something and have records of their own (0655–0657); his addition to item 20 is 0658.

## Options
For each item: keep as built, or change it. The tick-list (0089) held the low-stakes ones, each ticked to keep.

## Recommendation
Keep every item below as built.

## Choice
His answers of 2026-11-23:
- **Kept as built:** 1 (herdr as a squashed subtree), 2 (herdr's crates and cargo-deny's four exceptions), 4 (`jocasta worktree leftover`), 5 (the release notes, announcements and onboarding kept), 6 (the dock's key and place), 7 (the HUD's segment), 8 (`herdr plugin unlink jocasta` for an old link), 9 (the question's reader rule and its stated limit), 10 (the plan poll), 11 (translate-c vendored), 12 (what the first setup takes from his omp files), 13 (`tui.renderEmbeds: false`, `local/phonon-31m`), 14 (no live handoff on macOS), 16 (the Windows upgrade's restart), 18 (the VM's removal), 22 (the daemon spared; U11 upstream), 24 (omp 19's new settings at their defaults), 25 (composer predictions off; `tool_approval_requested` noted, not built).
- **21, the guard on tool-command text:** accepted. A deny or prompt pattern may fire on a tool argument that only contains its text; how often it fires is counted in the next trials.
- **The private name in `ead47a10`:** left in the history; no rewrite, no force-push. The current file has the placeholder.
- Item 3 (`mise trust`, `mise lock`) is his to run.

## Reason
His answers in the review form: every tick kept, "explain only the 25" (explained), and the defaults for 1, 21 and the history.

## Consequences
- The ledger's items 1–26 are answered; the ones with changes are built under 0655–0658.
- Item 25's `tool_approval_requested` reader stays an idea for after v1 (0018).
