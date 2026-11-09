# 0569 Finished threads chime with herdr's done sound

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V11
- Date: 2026-11-09

## Question
0554 had finished threads ping silently. herdr 0.9.5 can't mute only omp's done sound. Keep the chime or silence it?

## Facts
Found 2026-11-09 in Phase B's live run.
- `[ui.sound.agents]` accepts `pi`, `claude` and `droid` in herdr 0.9.5; `ui.sound.agents.omp` is reported as an unknown key and every attached client shows "herdr.toml has unknown keys".
- `[ui.sound] done_path` can point at any mp3; a silent one would mute finished threads.

## Options
| Option | Pros | Cons |
|---|---|---|
| Silent, as 0554 (a silent `done_path` mp3) | Only "needs you" sounds | A shipped mp3 just to stay quiet |
| Keep herdr's done sound | Nothing to ship; he hears a thread finish | Amends 0554 |

## Recommendation
Silent, as 0554.

## Choice
Finished threads chime with herdr's own done sound; "needs you" keeps the request sound.

## Reason
"Keep herdr's done sound".

## Consequences
- Differs from the recommendation; amends 0554.
- `ui.sound.agents.omp` is removed from `config/herdr.toml`.
