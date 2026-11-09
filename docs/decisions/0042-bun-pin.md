# 0042 Bun pin

- Status: decided
- Kind: product
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
Which Bun version builds and runs Jarvis?

## Facts
2026-10-04: Bun's latest release is 1.4.2 (`npm view bun version`); this machine had 1.4.0.

## Options
| Option | Pros | Cons |
|---|---|---|
| 1.4.2 in CI and `package.json`; upgrade local Bun | Reproducible compile step | Local upgrade needed |
| 1.4.0, as installed | No install | Older; a bump comes later |
| Latest, unpinned | No bump work | Unreproducible builds |

## Recommendation
1.4.2, with local Bun upgraded.

## Choice
Bun 1.4.2 in CI and in `package.json`; Zhivko approved `bun upgrade` on this machine.

## Reason
Not given.

## Consequences
`package.json` declares `packageManager: bun@1.4.2`; CI installs exactly that version.

- Amended by 0435: Bun 1.4.3 in `package.json`, CI and the release.
