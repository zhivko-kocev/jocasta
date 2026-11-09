# 0353 The calendar parser

- Status: decided
- Kind: product
- Phase/slice: 8/8.1
- Date: 2026-10-09

## Question
What parses the Google Calendar iCal feed (0301)?

## Facts
Verified 2026-10-09 on npm.
- Recurring meetings are stored once with a repeat rule (RRULE), plus time zones and exceptions.
- `node-ical` 0.27.3 (Apache-2.0, 0.4 MB) uses `rrule` for repeats; `ical.js` 2.2.1 (MPL-2.0, 1.2 MB) is Mozilla's parser.

## Options
| Option | Pros | Cons |
|---|---|---|
| `node-ical` | Repeats, time zones and exceptions handled; small | An install |
| `ical.js` | Very complete | MPL's file-level copyleft; larger |
| Own minimal parser | No install | Repeats and exceptions easy to get wrong |

## Recommendation
`node-ical`.

## Choice
`node-ical` 0.27.3, pinned exactly; installed with his approval.

## Reason
Chosen from the options.

## Consequences
- None beyond the build.
