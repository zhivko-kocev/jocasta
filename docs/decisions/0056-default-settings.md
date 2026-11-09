# 0056 Default settings

- Status: decided
- Kind: product
- Phase/slice: 0/0.3
- Date: 2026-10-04

## Question
Does Jarvis ship default settings, how do they get in, and which values?

## Facts
- Pi has no layer for shipped defaults; it reads only the global and project `settings.json`.
- An extension can set model and thinking level at session start, overriding the file every time.
- Pi 1.0.1 defaults: provider and model automatic, `defaultThinkingLevel` `medium`, `theme` `system`.
- Usage (plan appendix A): models opus-5 338, sonnet-5 151, opus-5-5 139; thinking max 287, medium 58, low 55, high 31.
- Pi's model catalog is fetched at runtime; the exact model id is confirmed at the phase 0 sign-in smoke.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's defaults, set by Zhivko in his `settings.json` | No shipped knobs; one owner | Fresh machines start on Pi's defaults |
| Jarvis ships defaults | Same start everywhere | Needs a write or forced overrides |

Mechanism, asked after the choice:

| Option | Pros | Cons |
|---|---|---|
| Seed on first run: write `settings.json` only if it doesn't exist | One owner afterwards | New defaults don't reach existing files |
| Forced at every start by an extension | Always the same | A second owner |
| A defaults file next to the exe, applied for missing keys | New defaults reach old installs | Layering code |

Values:

| Option | Pros | Cons |
|---|---|---|
| opus-5, high, dark | Most-used model | — |
| opus-5, max, dark | Matches usage | Costliest every turn |
| opus-5-5, high, dark | Newest Opus | Less used |

## Recommendation
Pi's defaults (the planning choice was shipped defaults; changed because Pi has no defaults layer). For the mechanism, seeding; for values, opus-5 at high.

## Choice
Now (0377, 0378): the seed names `github-copilot` and `claude-opus-5.5`, lists each role Copilot first, then Anthropic (0377), and sets `quietStartup: "header"` (0378); the rest stands.

Originally: Jarvis ships defaults, seeded on first run: if `<config dir>/settings.json` doesn't exist, `src/entry.ts` writes it before Pi starts, and never touches it again.

```json
{
	"defaultProvider": "anthropic",
	"defaultModel": "claude-opus-5-5",
	"defaultThinkingLevel": "high",
	"theme": "dark",
	"enableInstallTelemetry": false
}
```

## Reason
Not given.

## Consequences
- Differs from the recommendation (Pi's defaults; and opus-5 for the model).
- Bends principle 3 at seed time only; afterwards the file is Zhivko's.
- `enableInstallTelemetry` comes from 0048 and 0058.
- A default added in a later release doesn't reach an existing file.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
- Amended by 0396: omp's settings come from his walkthrough and `jarvis setup` writes them.
