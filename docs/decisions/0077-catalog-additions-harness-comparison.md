# 0077 Catalog additions from the harness comparison

- Status: decided
- Kind: product
- Phase/slice: 0/appendix A
- Vision: V8
- Date: 2026-10-07

## Question
Which findings from the eight-part comparison of Pi, omp, opencode and Claude Code enter the plan, and how?

## Facts
Verified 2026-10-07 against pinned sources: Pi v1.0.1 (`a7229dd`), omp v18.6.0 (`89d2610`), opencode v1.18.22 (`47b6b6f`), Codex rust-v0.160.1 (`d27764b`), and the Claude Code docs at https://code.claude.com/docs/en/ .
- Appendix A: "Zhivko can add rows at any grilling session (0020). A new row names the slice that decides it." `AGENTS.md`: the plan changes only to carry a decided record.
- Rule precedence. Claude Code (docs `permissions`): "Rules are evaluated in order: deny, then ask, then allow. The first match in that order determines the outcome"; hook decisions don't bypass deny or ask rules. opencode evaluates the last matching rule (`packages/opencode/src/permission/index.ts:28-37`, `findLast`), so a later allow overrides an earlier deny.
- Capability tiers. omp tools declare a tier (`read`, `write`, `exec`); modes `always-ask`, `write` and `yolo` (the default) auto-approve by tier; user policies are `allow`, `deny` or `prompt`; bash critical patterns prompt even in yolo (`docs/approval-mode.md:10-72`).
- Windows sandbox. Claude Code runs commands unsandboxed on native Windows (docs `sandboxing`). Codex sandboxes on native Windows with a restricted token (`codex-rs/windows-sandbox-rs/src/token.rs:499-514`) and a Job Object (`codex-rs/utils/pty/src/win/job.rs:44-60`).
- Context epochs. opencode's `CONTEXT.md` defines context that changes during a conversation without rewriting the cached prefix.
- Credential refresh. omp coordinates OAuth refresh across processes with a 15-second lease (`packages/ai/src/auth/refresh.ts:41-44`). Jarvis keeps Pi's `auth.json` (0063), which several tabs will share from phase 4.
- Command loading. omp imports its runtime only after argv is accepted (`packages/coding-agent/src/commands/launch.ts`); opencode imports every command module up front (`packages/opencode/src/index.ts:2-30`). `--version` from source on his machine: omp about 0.1 s, opencode about 1.8 s.
- Repeat-call guard. opencode asks for permission when the same tool is called with the same input 3 times in a row (`packages/opencode/src/session/processor.ts:29,356-372`). The catalog already lists omp's loop guard.
- Steering. omp's live steering can interrupt a turn while the model is still streaming (`packages/agent/src/live-steering.ts`); Pi and Claude Code deliver steering between turns.

## Options
| Option | Pros | Cons |
|---|---|---|
| Appendix A rows only | Matches the catalog rule; nothing is decided early; each row is grilled in its slice | Evidence lives in this record, not next to the slice |
| Rows plus fact notes under slices 1.8, 2.1, 2.6, 3.3, 3.4, 6.3 | Evidence sits where the slice is read | Pre-empts each phase's fact check; facts age before phases 2–6 |
| Nothing in the plan | No plan churn | Candidates are easy to forget |

Candidates offered: rule precedence (2.1), capability tiers (2.1), Codex on the OS-sandbox row (2.1), context epochs (3.3), the compaction re-injection contract (3.4), crash-safe tool calls (4.10), Job Object process-tree kill (6.3), credential refresh lease (4.1), lazy subcommands (1.6), opencode on the loop-guard row (1.8), omp on the steering row (4.7).

## Recommendation
Appendix A rows only, with every candidate.

## Choice
Appendix A rows only. Added: rule precedence (2.1), capability tiers (2.1), credential refresh lease (4.1), context epochs (3.3), lazy subcommands (1.6). Updated: the OS-sandbox row adds Codex, the loop-guard row adds opencode, the steering row adds omp.

## Reason
Not given.

## Consequences
- Plan appendix A carries the rows; each is a grilling question in the slice named.
- Not added: the compaction re-injection contract (3.4), crash-safe tool calls (4.10) and Job Object process-tree kill (6.3). They can be raised again at those slices.
