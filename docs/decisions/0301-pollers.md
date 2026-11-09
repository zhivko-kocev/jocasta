# 0301 Pollers

- Status: decided
- Kind: product
- Phase/slice: 8/8.1
- Date: 2026-10-08

## Question
How do the GitHub, Azure DevOps and Google Calendar pollers work, and where do their settings live?

## Facts
Checked 2026-10-08: `gh` 2.92.0 is signed in to his GitHub account; `az` 2.87.0 has the `azure-devops` 1.0.7 extension.
- Google Calendar: the Calendar API needs an OAuth app and consent (scopes include `calendar.events.readonly`, https://developers.google.com/workspace/calendar/api/auth). Each calendar also has a read-only "Secret address in iCal format", which an admin can block on work accounts (https://support.google.com/calendar/answer/37111).
- Credentials stay in Pi's `auth.json` (0063); redaction hides secret-named env vars, `.env*` and `auth.json` (0234).

## Options
| Option | Pros | Cons |
|---|---|---|
| Pollers in Frontier's server via `gh`, `az` and the secret iCal URL; settings under `pollers` in `settings.json`; the URL in `JARVIS_CALENDAR_ICAL_SECRET` | No new auth; redaction covers the URL | — |
| Calendar through the OAuth API | Attendees and meeting links | A Google Cloud app |
| Everything in `settings.json` | One place | A secret URL in a plain file |

## Recommendation
As described, with the URL in an env var.

## Choice
Pollers run inside Frontier's server: GitHub every 2 minutes (`gh api notifications`, his PRs and check runs); Azure DevOps every 5 minutes (`az repos pr list`, `az pipelines runs list`) for the orgs in config; Google Calendar every 10 minutes from the secret iCal URL. Settings live under `pollers` in `settings.json`; the URL comes only from `JARVIS_CALENDAR_ICAL_SECRET`, and Jarvis never writes it to a file.

```json
"pollers": {
  "github": { "everyMinutes": 2 },
  "azureDevOps": { "orgs": ["https://dev.azure.com/example-org"], "everyMinutes": 5 },
  "calendar": { "everyMinutes": 10 }
}
```

## Reason
Not given.

## Consequences
- Appendix I gains `JARVIS_CALENDAR_ICAL_SECRET`.
- Amended by 0430: the calendar secret lives in Windows Credential Manager.
