# 0136 Past windows

- Status: decided
- Kind: product
- Phase/slice: 1/1.5
- Date: 2026-10-07

## Question
How are a thread's earlier windows reached?

## Facts
- Plan 1.5: earlier windows stay readable and searchable inside their thread (3.5); there is never a list of old sessions.
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Readable and searchable inside the thread; no session list | V9 | — |
| A session list | Familiar | Sessions come back |

## Recommendation
Inside the thread only.

## Choice
Earlier windows stay readable and searchable inside their thread (built in 3.5); there is never a list of old sessions.

## Reason
Not given.

## Consequences
- 3.5 builds history search.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
