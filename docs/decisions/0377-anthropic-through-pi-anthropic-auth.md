# 0377 Anthropic through pi-anthropic-auth, Copilot first

- Status: decided
- Kind: product
- Phase/slice: 5/5.9, 6/6.2
- Date: 2026-10-10

## Question
After Anthropic refused Jarvis's own subscription shaping, how does Jarvis reach Anthropic, and which provider comes first?

## Facts
Verified 2026-10-10.
- In February 2026 Anthropic barred subscription OAuth outside Claude Code and claude.ai; since April 2026 third-party harnesses bill to extra usage. Claude Code's terms reserve OAuth for Anthropic's own apps (0062).
- Jarvis's own shaping (0337, 0338) was refused on 2026-10-09: "Third-party apps now draw from extra usage, not plan limits".
- `@gotgenes/pi-anthropic-auth` 3.4.2: MIT, no runtime dependencies, peers `@earendil-works/pi-ai` and `pi-coding-agent` ≥0.86.0, entry `./src/index.ts` (TypeScript source). It unregisters `anthropic` and registers it again with a `streamSimple` wrapper. Only `sk-ant-oat` requests are shaped: a billing-header system block goes first; Pi's default preamble (not Jarvis's), `<docs>` section and "In addition to the tools above" paragraph are dropped. It retries once on `claude_code_version_too_old`, logs in through Pi's built-in `anthropicOAuth` (one account), and adds `/anthropic-auth:status`. Its maintainer measured that the billing header decides between 200 and "You're out of extra usage".
- billion-context's proxy adds `x-bili-*` headers, a ~10.7 KB rules block and `<acp …>` tags after shaping; its only passthrough is proxy-wide.
- Account rotation (0346) is Anthropic-only.
- Graphiti's models default to the newest Anthropic Sonnet and Haiku with auth, so a Copilot-only login gets none.
- The seeded roles are Anthropic-only. Provider `github-copilot` serves `claude-opus-5.5`, `claude-sonnet-5.5`, `claude-sonnet-5` and `claude-haiku-4.5`.

## Options
| Option | Pros | Cons |
|---|---|---|
| The package, compiled in, Copilot first | A maintained shaper with measured results; Copilot works without the terms risk | The terms risk stays for Anthropic; the package can break when Anthropic changes its checks |
| Jarvis's own shaping (0337) | Under Jarvis's control | Refused in real use |
| Copilot only, no shaping | No terms risk | Anthropic bills extra usage |

## Recommendation
The package, Copilot first.

## Choice
- `@gotgenes/pi-anthropic-auth` 3.4.2 is pinned exactly and compiled in, in `anthropic-auth`'s slot, through a typed boundary (`src/harness/anthropic-auth/index.js` and `index.d.ts`), so `tsc` never checks its source. Jarvis's own shaping, the finisher hop and the Claude Code identity are deleted.
- Account rotation is dropped: Pi's `/login anthropic` holds one account. The in-flight cap stays.
- billion-context's proxy stays on Anthropic subscription requests; if Anthropic refuses them, the work stops and he decides.
- Seeds: `defaultProvider: github-copilot`, `defaultModel: claude-opus-5.5`; each role lists Copilot first, then Anthropic.
- Graphiti's models fall back to Copilot's newest Sonnet and Haiku when no Anthropic model has auth; dotted ids are read as versions.

## Reason
"i would add the extension for anthropic in the harness"; rotation dropped; seeded roles Copilot first, then Anthropic; if Anthropic refuses: "Stop and ask me".

## Consequences
- Amends 0056, 0062, 0283, 0311, 0313, 0336, 0337, 0338, 0339, 0341, 0343, 0344, 0346 and 0350.
- Closes the open item on subscription shaping (AGENTS.md).
- The omp drift check becomes a read of the package's npm metadata; its peer range is checked on each Pi bump.
- `JARVIS_CLAUDE_CODE_VERSION`, `install-id` and `accounts.json` are gone; the package reads its own `PI_ANTHROPIC_AUTH_CLAUDE_CODE_VERSION`.
- The strict profile no longer sets `providers.rotateAccounts`.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
- Amended by 0390: the companion serves `/llm/v1` through omp's model registry and logins.
