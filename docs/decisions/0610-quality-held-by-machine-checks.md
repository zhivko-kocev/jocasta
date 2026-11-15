# 0610 Quality is held by machine checks first

- Status: v1
- Kind: product, process
- Phase/slice: program
- Date: 2026-11-14

## Question
47 of the standard's 60 rules (0609) can be checked by a program at no token cost. Which checks does Jocasta build, and where do they run?

## Facts
Verified 2026-11-14 (the code-quality section of `docs/research/state-of-the-art.md`).
- **What corrects code today, per dollar:** the check tool and the diagnostics shown at an edit cost no tokens. Then come agents' own check runs (about $8 for a fail, a fix and a pass), the advisor (about $4.50 per note acted on) and reviewer runs ($5.10 each). Plan mode showed no correction in the window.
- **Could a program have caught 0573's 242 findings?** A static check 58, a test 29, a reviewer model 144, only he 11.
- **Lint:** Biome 2.5.15 runs its recommended rules. Seven more rules have no hit in `src` today: `noForEach`, `noEnum`, `noUselessElse`, `noShoutyConstants`, `useExhaustiveSwitchCases`, `noFloatingPromises`, `noExcessiveNestedCallbacks`. Four more have 13 hits between them: `noParameterAssign` 2, `noImportCycles` 2 (a real cycle in the canvas), `useCollapsedIf` 4, `useDefaultSwitchClause` 5. Three of the eleven are nursery rules, which may change between Biome versions; Biome is pinned.
- **Compiler:** `noUnusedLocals`, `noImplicitReturns` and `allowUnreachableCode: false` need three one-line fixes.
- **Ceilings, by threshold:** cognitive complexity 10/20/25/40 is passed by 172/57/36/11 functions; lines per function 30/80/120/150 by 95/15/4/2; lines per file 500/800/1,000 by 10/5/3; 2 functions take more than 5 parameters.
- **Tests in a review:** the review's proof (0453) counts the diff's size, the test files it changed and the skips it added. It is shown, and the review's verdict does not read it. Removed tests and removed assertions are not looked for. In a published study Claude models that cheated on a failing task did so mostly by editing the tests (ImpossibleBench, over 79%).
- **The check on idle** (0513, 0542): after a thread's turn ends with edits, Jocasta runs the project's mise `verify` task (else `test`) and the task's `done_when` commands on that tree, cached by git tree. A pass says nothing; a failing gate is typed into the pane. It runs in threads only.
- **Where code is written:** in project panes and their subagents, $704 of the window's $977. Of 4 project-pane sessions that edited, none ran a check after its last edit.
- **Jocasta's own repo** has the mise tasks `check`, `build`, `evals` and `smoke`. The check on idle looks for `verify` or `test`, so Jocasta's own threads have no gate.
- **What Biome cannot see:** flag parameters, forwarding methods and exported items without a comment. Scripts counted them over the syntax tree with the TypeScript 5.9.3 parser: 21, 64, and 14 functions with 19 classes. The repo's `typescript` is 7.0.2, which has no parser a script can call. Such checks give false positives.

## Options
| Question | Options | Recommended |
|---|---|---|
| Lint and compiler | The eleven rules and three options; the seven with no hit; none | All |
| Ceilings | 40, 120, 5 and 1,000; 25 for complexity; none | 40, 120, 5 and 1,000 |
| A diff that weakens tests | Fails the review; shown only | Fails the review |
| The check on idle in project panes | Yes; threads only | Yes |
| A `verify` task in Jocasta's repo | Yes; no | Yes |
| A scorecard in the weekly review | From Biome's counts; none | From Biome's counts |
| Syntax-tree checks | Build them, with the TypeScript 5 parser as a dev dependency; wait for the scorecard to show drift | Wait |
| Mutation testing, duplicate-code and dead-code tools | Install StrykerJS, jscpd and knip; not now | Not now |

## Recommendation
As in the table's last column.

## Choice
- **Lint and compiler:** the eleven Biome rules and the three compiler options go on, with their 13 and 3 fixes.
- **Ceilings:** cognitive complexity 40, 120 lines a function, 5 parameters, 1,000 lines a file. Each of today's 11, 4, 2 and 3 offenders is fixed or marked as an exception with its reason.
- **A review cannot pass with weakened tests:** a removed test, fewer assertions in a changed test file, or an added skip fails the review.
- **The check on idle also runs in project panes**, by the threads' rule: after a turn that left edits the project's check runs, silent on a pass, the failing gate typed into the pane.
- **Jocasta's own repo gets a `verify` task** that runs what `mise run check` runs.
- **A scorecard in the weekly review** from Biome's own counts, week on week. It fails nothing.
- **Syntax-tree checks** for flag parameters, forwarding methods and undocumented exports, with the TypeScript 5 parser as a dev dependency. They fail on new cases only: today's are listed as the baseline.
- **Mutation testing and the duplicate-code and dead-code tools** are not decided here: he asked for them to be explained first, together with the security tools he named.

## Reason
His ticks. The syntax-tree checks were offered unticked and he ticked them.

## Consequences
- Amends 0036 and 0037 (Biome's rules and the compiler's options), 0453 (the proof now decides), and 0513 and 0542 (the check on idle beyond threads).
- A turn in a project pane that stops on purpose with failing tests gets the failure typed back.
- A change that rewrites tests for a good reason fails its review on the proof; he sees the reason beside the Allow, as with any failed review.
- The TypeScript 5 parser is a new dev dependency (0005: approved by his tick). It parses; `tsc` stays 7.0.2.
- The scorecard's first values are the before measurement.
