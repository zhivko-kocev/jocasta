# 0035 Repo visibility

- Status: decided
- Kind: product
- Phase/slice: 0/0.1
- Date: 2026-10-04

## Question
Is the Jarvis repo public or private?

## Facts
- New on 2026-10-04: `docs/plan.md` holds the final plan verbatim, including real names (appendix F's check pattern lists them, and 0.6 and 4.4 name his own folders). A public repo would need the plan scrubbed to placeholders.
- Public makes GitHub issues (0018) public too.

## Options
| Option | Pros | Cons |
|---|---|---|
| Private | The plan stays verbatim; client names allowed in records and issues | Nothing can be shared |
| Public, with the placeholder rule | Shareable | The plan is scrubbed now and kept scrubbed; every record, test and issue uses placeholders |

## Recommendation
Private, because of the verbatim plan. The planning lean was public with placeholders. Re-asked on 2026-10-04 after the releases answer: public, with the pattern in the config dir.

## Choice
Public, with the placeholder rule. The client-name pattern for plan appendix F check 3 lives outside the repo, in `~/.jarvis/agent/private-names.txt`, and the check runs `git grep -f` against it.

## Reason
"then the repo will be public" (answering 0051, releases on GitHub Actions).

## Consequences
- The placeholder rule applies to tests, docs, records and issues (`acme`, `client-a`, `example-org`).
- The only real names in the repo on 2026-10-04 were the check's own pattern in `docs/plan.md`; it moved to the config dir.
- Install scripts download over HTTPS without auth (0049).
- Principle 6 holds: personal identity, scopes, watched roots and memory live in the config and data dirs, never in the repo (0.6, 1.1).
- Differs from the recommendation (private).
