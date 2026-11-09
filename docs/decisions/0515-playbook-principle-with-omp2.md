# 0515 The playbook's five rules become a principle with the omp² move

- Status: after v1
- Kind: process
- Phase/slice: after v1
- Date: 2026-11-01

## Question
He wants the Harness Playbook's philosophy "as part of the thesis and jocasta". The thesis uses its five consequences as a checklist (0511). How does it enter Jocasta, and when?

## Facts
Verified 2026-11-01.
- The playbook draws five consequences from four ways a harness is used: "One authoritative session… A trusted control plane… Bounded work… Explicit compatibility… Views are projections." (https://stencil.so/blog/harness-playbook; explained simply in `D:/thesis/sources/playbook-explained.md`).
- omp² is built on them, and Jocasta moves to omp² once it's released (0514).
- The plan's principles are §4's nine (0025, 0087); a principle bends every later recommendation that touches it.

## Options
| Option | Pros | Cons |
|---|---|---|
| Principle 10 in the plan, now | Every after-v1 decision is weighed against it | Jocasta on omp 18 already breaks one rule by design (several authorities) |
| With the omp² move | Enters when the harness under Jocasta is built on it | Waits for omp² |
| Thesis only | No plan change | The philosophy stays outside Jocasta |

## Recommendation
Principle 10 in the plan, now.

## Choice
With the omp² move, the plan gains a tenth principle, in this wording unless the move's own record changes it:

"One authority, a trusted host, bounded work, written-down compatibility, views that only show (from Stencil's Harness Playbook). One authority: each piece of state has one source of truth that everything else is rebuilt from. A trusted host: policy and decisions stay with the service and his Allow/Deny; agents get bounded jobs, never the keys. Bounded work: every job has a limit and can be stopped, its whole process tree with it. Written-down compatibility: what Jocasta relies on in each upstream, and what a bump re-checks, is in plan 5.4 and 5.5. Views only show: herdr's panes, the companion and the rail render state; they never own it."

## Reason
"With the omp² move".

## Consequences
- Differs from the recommendation: it waits for omp².
- Until then the five rules are the thesis's checklist (0511), and the omp² move's record (0514) brings the principle in.
