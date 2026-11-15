# 0638 icalendar reads the calendar feeds

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
What reads the ICS feeds of the day poller, which node-ical reads today?

## Facts
Verified 2026-11-15. Crate counts are transitive, on Windows, on top of the port's base.
- **icalendar 0.17.17,** released 20 days ago. With its `recurrence` feature (rrule 0.14) it adds 13 crates. It parses and expands RRULE, RDATE and EXDATE.
- **ical 0.11** is a parser only, last released 977 days ago. Its licence is non-standard, and its README says it checks no fields.
- **Not yet tested:** expansion across daylight-saving changes.

## Options
| Option | For | Against |
|---|---|---|
| icalendar with `recurrence` | maintained; parses and expands | DST expansion still untested |
| ical with rrule | a smaller parser | stale, odd licence, no field checks |

## Recommendation
icalendar.

## Choice
icalendar 0.17 with `recurrence`.

## Reason
His answer: `icalendar`.

## Consequences
- **Tests.** The port's tests cover recurrence across a daylight-saving change.
- **node-ical** leaves `package.json` at the cutover.
