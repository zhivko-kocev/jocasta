# 0358 Frontier's fixed port

- Status: decided
- Kind: product
- Phase/slice: 4/4.1
- Date: 2026-10-09

## Question
Frontier takes a free port each launch (0254), so its page's origin changes every time. How is that fixed?

## Facts
- Found in the v1 beta (2026-10-09): Edge keys the microphone permission, the notification permission and `localStorage` (voice mute, `frontier.voiceMuted`) by origin, so each launch forgets them. The first hold of Space after a launch does nothing while Edge asks for the mic.
- Graphiti uses 127.0.0.1:47380 (0340).

## Options
| Option | Pros | Cons |
|---|---|---|
| A fixed port, `frontier.port`, default 47400 | Same origin every launch; permissions and settings stick | A port can be taken |
| Free ports plus a "mic permission needed" hint | No port clash | Re-asks every launch |

## Recommendation
A fixed port, `frontier.port`, default 47400.

## Choice
A fixed port: `frontier.port` in `settings.json`, default 47400, on 127.0.0.1. If it is busy, Frontier says so and uses a free port for that launch.

## Reason
Not given.

## Consequences
- Amends 0254 (a free port).
- Appendix I gains the settings key `frontier.port`.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
