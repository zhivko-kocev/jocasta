# 0048 Telemetry

- Status: decided
- Kind: product
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
Does Jarvis report anything about its use? (Non-goal confirmed here, 0032.)

## Facts
Pi `docs/settings.md` (`v1.0.1`): `enableInstallTelemetry` defaults to `true` ("anonymous install/update reporting and selected provider attribution headers"); `enableAnalytics` defaults to `false`. Update checks are separate.

## Options
| Option | Pros | Cons |
|---|---|---|
| No telemetry: `enableInstallTelemetry` pinned off, `enableAnalytics` stays off | Matches the non-goal | One shipped default (a principle 3 exception, marked pinned) |
| Pi's defaults | No delta | Jarvis reports installs to Pi's endpoint |

## Recommendation
No telemetry. Bends principle 3 as an identity invariant.

## Choice
No telemetry: `enableInstallTelemetry` is pinned off and `enableAnalytics` stays off.

## Reason
Not given.

## Consequences
The mechanism is the seeded setting (0058). Update checks are not telemetry and are decided with settings in 0.3.
