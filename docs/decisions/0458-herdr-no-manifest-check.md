# 0458 herdr fetches no detection files

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
herdr fetches agent-detection files from herdr.dev under a pinned binary. Does Jarvis's herdr config turn that off?

## Facts
Verified 2026-10-23.
- `manifest_check` exists and defaults on in herdr 0.9.5 (`herdr --default-config`; agents.mdx "Detection manifests").
- omp's state reaches herdr through its integration, not detection ("OMP · state requires the integration", agents.mdx).
- `config/herdr.toml` sets only `version_check = false` under `[update]` (`herdr.toml:17-19`).

## Options
| Option | Pros | Cons |
|---|---|---|
| `manifest_check = false` | No fetch from herdr.dev; behaviour stays pinned (0383) | Detection of other agents isn't refreshed |
| Keep herdr's default | No change | Detection rules change live under a pinned binary |

## Recommendation
`manifest_check = false`.

## Choice
`config/herdr.toml` sets `manifest_check = false`.

## Reason
"`manifest_check = false`".

## Consequences
- Amends 0401 (Jarvis's `herdr.toml`).
- `jarvis setup` carries it into his installed `herdr.toml` (0440).
