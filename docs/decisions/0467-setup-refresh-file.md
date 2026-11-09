# 0467 `jarvis setup --refresh <file>`

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
How does he reset one Jarvis-owned file to the shipped version, without losing what was there?

## Facts
Verified 2026-10-23.
- Today herdr.toml is replaced after `y` without a backup (`src/setup/setup.ts:267-280`), and nothing reloads herdr afterwards (no `reload-config` call in `src/`).
- Omarchy's `omarchy-refresh-config` keeps a `.bak.<time>` backup and shows the diff (https://github.com/omacom/omarchy/blob/quattro/bin/omarchy-refresh-config, lines 20-43).
- `omarchy-refresh-herdr` reloads herdr through `herdr server reload-config` (https://github.com/omacom/omarchy/blob/quattro/bin/omarchy-refresh-herdr, https://github.com/omacom/omarchy/blob/quattro/bin/omarchy-restart-herdr).
- Whether herdr 0.9.5 on Windows has `server reload-config` is not verified.

## Options
| Option | Pros | Cons |
|---|---|---|
| `--refresh <file>` with diff, backup and herdr reload | Explicit, per file; the old file stays beside it | One more flag |
| No refresh | Nothing to build | Resetting a file means deleting it by hand |

## Recommendation
`--refresh <file>` with diff, backup and herdr reload.

## Choice
`jarvis setup --refresh <file>` resets one Jarvis-owned file (herdr.toml, the Terminal fragment, Jarvis's MCP entries) to the shipped version after showing the diff, keeps the old one as `<name>.bak.<YYYYMMDD-HHMMSS>` beside it, and reloads herdr after a herdr.toml refresh.

## Reason
"`jarvis setup --refresh <file>` resets one file, keeps a backup".

## Consequences
- Amends 0440 (herdr.toml's replacement gets a backup and a reload when refreshed).
- The builder checks `herdr server reload-config` on herdr 0.9.5 on Windows; if it is missing, the command says to restart herdr instead.
