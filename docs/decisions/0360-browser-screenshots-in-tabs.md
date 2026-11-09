# 0360 Browser screenshots in the thread's TUI

- Status: decided
- Kind: product
- Phase/slice: 6/6.8
- Date: 2026-10-09

## Question
The agents' browser runs headless (0289, 0348), so he sees nothing of it. How does he see it?

## Facts
Checked 2026-10-09.
- Pi's TUI draws images when the terminal supports the kitty or iTerm2 image protocol; `PI_IMAGE_PROTOCOL=iterm2|kitty|none` overrides detection (`@earendil-works/pi-tui` `terminal-image.js`).
- `@xterm/addon-image` 0.9.0 (MIT) draws sixel and iTerm2 inline images in xterm.js.
- ConPTY re-encodes the child's output (Bun's docs); whether it passes the iTerm2 image sequence (OSC 1337) through is unverified.

## Options
| Option | Pros | Cons |
|---|---|---|
| Headless; screenshots shown inline in the thread's TUI | Nothing steals focus; he sees what the agent saw | One more install; depends on ConPTY passing the sequence |
| A visible browser window while an agent uses it | He watches it live | Steals focus |
| Headless only | No change | He sees nothing |

## Recommendation
Headless; screenshots inline in the TUI.

## Choice
Now (measured at build, 2026-10-09): no images in the tabs. ConPTY passed the iTerm2 image sequence through intact (byte-identical at 266 KB), but Pi's fullscreen TUI turns iTerm2 images off itself, and only kitty images survive its alternate screen, which `@xterm/addon-image` doesn't draw. Showing them would have meant running threads in Pi's regular TUI mode. He asked "do we need images in tui and why do we need them". Told that the model sees screenshots either way and inline images serve only his watching, he chose no images: the fullscreen TUI stays, screenshots show Pi's `[Image: …]` placeholder, and `@xterm/addon-image` is not installed.

Originally: Headless, with the browser's screenshots shown inline in the thread's TUI tab. Installs `@xterm/addon-image` 0.9.0; Frontier's host starts each harness with the image protocol xterm.js draws.

## Reason
Not given.

## Consequences
- Build-time rule: measure whether ConPTY passes the iTerm2 image sequence through `Bun.Terminal`. If it does, images go through the terminal. If it doesn't, the question comes back to him with what was measured.
- Builds on 0357.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
