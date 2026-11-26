# 0698 Palette file

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J7
- Date: 2026-11-26
- Amends: DESIGN.md "one look" (the "Don't add a theme setting" rule) and 0678 (the brief pages' `:root` gains a block served after it)

## Question
ultron themes the whole desktop from one palette. Jocasta draws `today`, the question box, the shell's chrome and the brief pages in a fixed Tokyo Night. Does Jocasta follow the desktop's colours, and how does it learn them?

## Facts
- The terminal surfaces take their colours from one `Palette` in `src/frontier/tui.rs` (`TOKYO_NIGHT`); the brief pages from the `:root` custom properties of `src/frontier/brief/page/brief.css`, served by the brief routes.
- ultron's `ultron apply` knows the theme's colours (`BG`, `BG_ALT`, `BG_SEL`, `FG`, `FG_DIM`, `MUTED`, `C9`–`C14`, `ACCENT_ALT`) and can write a file beside Jocasta's config.
- omp 19.2.1's built-in themes (its embedded `defaults.json`) include `dark-terminal`, a fixed neon palette that does not take the terminal's own colours, and no `light-terminal`; `omp config set theme.dark <name>` accepts any name without checking it.

## Options
| Option | Pros | Cons |
|---|---|---|
| An optional palette file ultron writes, read at start | No setting to learn; no file = today's look; ultron owns the theme | One more file format |
| A theme setting in `jocasta.yaml` | Visible to Jocasta's users | A second place to keep in step with the desktop |
| Keep one look | Nothing to build | Jocasta clashes with every desktop theme but Tokyo Night |

## Recommendation
The palette file.

## Choice
His answers of 2026-11-26 and the approved plan:
- Optional `~/.jocasta/agent/palette.json` (`JOCASTA_CODING_AGENT_DIR` moves it with the folder): `{"colors": {"<name>": "#rrggbb", …}}` with the names `bg-0`, `bg-1`, `bg-2`, `bg-3`, `fg`, `fg-2`, `fg-3`, `comment`, `blue`, `cyan`, `yellow`, `red`, `green`, `purple`, `orange`.
- Read once when Jocasta starts. Each valid colour replaces the built-in Tokyo Night value of that name in the terminal palette (`today`, the question box, the shell's chrome); an unknown name, a value that isn't `#rrggbb`, a malformed file or no file keeps the built-in values. A theme change shows after Jocasta restarts.
- The brief pages get the same colours as a `:root{…}` block appended to the served `brief.css`: `--bg` from `bg-0`, `--panel` from `bg-2`, `--line` from `bg-3`, `--fg` from `fg`, `--mut` from `fg-3`, `--blue`, `--green`, `--yellow`, `--red`, `--purple` from their names, and `color-scheme: light` when `bg-0` is light.
- `ompTheme` is ignored: omp has no `light-terminal`, and its `dark-terminal` doesn't follow the terminal's colours (the plan's contingency). omp's panes stay on the snapshot's theme, and ultron leaves `ompTheme` out.
- The Windows Terminal scheme `jocasta setup` writes stays Tokyo Night.

## Reason
Jocasta looks like the desktop it runs on, without a setting, and looks as before everywhere else.

## Consequences
- `tui.rs`: `load_palette` (called first by `run_embedded`), `palette()` for every terminal surface, `palette_css()` for the brief routes; `TOKYO_NIGHT` stays the built-in and setup's scheme.
- DESIGN.md: the palette is Tokyo Night unless the palette file names other colours; the "one look" rule says so.
