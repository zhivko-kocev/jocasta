# 0043 Build targets in v1

- Status: decided
- Kind: product
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
Which platforms does v1 build for?

## Facts
- Pi `v1.0.1` builds six targets (darwin-arm64, darwin-x64, linux-x64, linux-arm64, windows-x64, windows-arm64).
- The npm `pi-tui` 1.0.1 tarball ships native prebuilds for all six.

## Options
| Option | Pros | Cons |
|---|---|---|
| windows-x64, linux-x64, darwin-arm64 | His machine plus section 7's CI OSes | Three builds per release |
| All six | Matches Pi | Mostly unused |
| windows-x64 only | Fastest | No ubuntu or macos builds |

## Recommendation
The three.

## Choice
windows-x64, linux-x64 and darwin-arm64.

## Reason
Not given.

## Consequences
`scripts/build.ts` accepts `--target` with these three names.
