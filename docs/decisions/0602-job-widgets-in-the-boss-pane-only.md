# 0602 Background-job widgets stay in the boss pane only

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-14

## Question
omp writes a widget and a caption for each background job through a model call. Do Jocasta's panes keep them?

## Facts
- `async.widgets` (default true, unset in Jocasta's shipped config): the `widget` role (default `smol`) writes one widget spec and at most one caption per background job (omp 18.20.0's setting description; `docs/research/state-of-the-art.md` section 2).
- Measured in the window 2026-11-10 to 2026-11-14 (`docs/research/state-of-the-art.md` section 2): 249 widget calls costing $0.15 and 205 caption calls costing $0.02.
- The widget shows a job's status in omp's own UI. No reader of it was found in Jocasta's service ([INFERENCE] from absence in `src/`).
- The boss's one session in the window had 42 turns and $2.44 of own spend; the calls sit almost entirely in project and thread panes.

## Options
| Option | Pros | Cons |
|---|---|---|
| Boss pane only | The boss keeps the job view; project and thread panes lose a model call per job | The boss's calls are small but not zero |
| Off everywhere | Fewest model steps | No job view in the boss either |
| On everywhere | omp unchanged | A model call per background job in every pane |

## Recommendation
Off everywhere: the widget is decoration in a view Jocasta's service doesn't read.

## Choice
Boss pane only. Jocasta's pane overlay sets `async.widgets: false` for project and thread panes; the boss keeps omp's default. His own `config.yml` is not changed by this record.

## Reason
His answers: "maybe widgets only in the main tab?", then "boss-only".

## Consequences
- Built with 0601 in the same overlay change, with the overlay's test text.
- The window's widget and caption lines are the before measurement.
