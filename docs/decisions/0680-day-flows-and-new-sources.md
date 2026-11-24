# 0680 Day flows and three new read-only sources

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Date: 2026-11-24

## Question
Which processes of his day get a deterministic brief, and which new data does Jocasta read?

## Facts
- `weekly_review.rs:161` runs a one-shot omp helper; `day_poll.rs:72` has `run_round`; the CLI pollers resolve events that vanish; `credentials.rs:128` takes calendar secrets from the environment.
- `async-imap` 0.12.0 (2026-09-30) passes the 7-day rule; `tokio-rustls` and `rustls-platform-verifier` are already locked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep today's flows; read new sources through the agent | No code | Private data would reach the model; tokens per read |
| Six flows by code and three private read-only sources | No model turn; privacy by type (0681) | More pollers |
| Only the PR flows | Smaller | Leaves mail, Jira, Azure and mornings manual |

## Recommendation
Six flows, three new sources.

## Choice
His answer of 2026-11-24. This record is the install approval for `async-imap` 0.12.0 (`default-features = false`, `runtime-tokio`; `tokio-rustls` and `rustls-platform-verifier` become direct dependencies at their locked versions).
- Flows: PR inbox, PR review brief, thread brief, morning brief, red CI, weekly brief.
- PR review: key `r` on a `review` row of `today` (`R` if bound) starts a review thread without a boss turn, from `config/pr-review.md` plus facts gathered by code, in phase Review; its brief is `pr-<project id>-<n>`. Brief actions: `edit`, `reword` (`omp -p --model @smol --no-tools --no-session --no-extensions --no-skills --no-rules --system-prompt "Rewrite the review comment as instructed. Keep every technical claim. Answer with the comment text only."`, 60 s, input only the instruction, the card text, `file:line` and quote; the suggestion is stored only on his Accept), `publish` (refused unless `proof.ok`, `verdict != "skip"` and unpublished; GitHub `gh api repos/<o>/<r>/pulls/<n>/comments …`, with a `gh pr comment` fallback on 422; Azure DevOps `az devops invoke … pullRequestThreads …`). No vote is ever cast.
- Red CI: key `f` on a `pipeline` row starts `Fix CI: <pipeline> on <branch>` with the failing log tail filled by code and `done_when` empty.
- Sources (read-only): Gmail over IMAP (`pollers.gmail {address, everyMinutes: 5}`; secret credential target `jocasta/gmail-app-password`, env `JOCASTA_GMAIL_APP_PASSWORD`; `imap.gmail.com:993`, `EXAMINE INBOX`, unseen, newest 20); Jira REST (`pollers.jira {site, email, everyMinutes: 10}`; `jocasta/jira-token`, env `JOCASTA_JIRA_TOKEN`; `/rest/api/3/search/jql`); Azure failed operations through `az` (`pollers.azurePortal {everyMinutes: 15}`; `az monitor activity-log list --status Failed --offset 24h`). `EventSource` gains `Mail`, `Jira`, `AzurePortal`; `today` gets MAIL, JIRA and AZURE sections after CALENDAR, hidden when unconfigured; `jocasta setup` asks for each secret and `jocasta uninstall` deletes both credential targets.
- Morning brief `morning-<date>` built on the first refresh tick after 06:00 local and on key `b`; weekly brief `weekly-<yyyy>-W<ww>` when the weekly review is due (numbers from the journal, 0688). A dock row opens each; nothing opens by itself.

## Reason
One brief per process of his day.

## Consequences
- Gmail's app-password IMAP login and the `#inbox/<thrid hex>` link are not checked live; on a refused login MAIL shows `mail: sign-in refused; run jocasta setup`.
- Jira's and Azure DevOps' payloads are written from the documented APIs; the first live poll and publish are part of the phase 5 live check.
- Existing sources keep reaching the boss as today; only the three new ones are private (0681).
