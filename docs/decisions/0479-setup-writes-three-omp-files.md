# 0479 Setup writes the three omp files after his y

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
0451 left three files in his omp folder for him to make by hand: `keybindings.yml` (`app.live.toggle: []`), `WATCHDOG.md` (the advisor's brief) and `lsp.json` (`idleTimeoutMs: 300000`). Since 0466, setup already merges `config.yml` key by key after a diff and his `y`, and installs its mise skill there. Does setup write these three the same way?

## Facts
Verified 2026-10-23.
- omp reads key remaps from `~/.omp/agent/keybindings.yml` (a remap replaces the action's whole default list; omp needs a restart), the advisor's brief from `~/.omp/agent/WATCHDOG.md`, and the language-server settings from `~/.omp/agent/lsp.json` (`local://omp-docs-DocsInteractiveUI.md`, `local://omp-docs-DocsAgentsReview.md`, `local://omp-docs-DocsToolsWeb.md`).
- None of the three exists in his omp folder today.
- Setup's migrations show a diff, write after `y`, keep a marker per migration and record their undo for `jarvis uninstall` (0466, 0468).

## Options
| Option | Pros | Cons |
|---|---|---|
| All three, each as a diff after `y` | Nothing left by hand; same path as `config.yml` | Setup touches three more of his files |
| Only `WATCHDOG.md` | The privacy brief arrives; bindings and LSP stay his | Two by hand |
| None, by hand | As 0451 | Easy to forget |

## Recommendation
All three, each as a diff after `y`.

## Choice
`jarvis setup` gains three migrations, each shown as a diff and written only after his `y`: `~/.omp/agent/keybindings.yml` gets `app.live.toggle: []` (his other keys kept), `~/.omp/agent/WATCHDOG.md` comes from `config/WATCHDOG.md` (an existing file he changed is shown and kept, never overwritten silently), and `~/.omp/agent/lsp.json` gets `idleTimeoutMs: 300000` (his other keys kept). `jarvis uninstall` takes out only what setup added (its keys; `WATCHDOG.md` only while it is still Jarvis's text).

## Reason
"Yes, all three".

## Consequences
- Amends 0451 (the three files are no longer by hand) and 0466 (three more migrations).
- README's config section and plan appendix I's setup paragraph list them.
- Amended by 0594: setup also writes `secrets.yml` from `config/secrets.yml` when it's missing; uninstall removes it only while it is still Jocasta's text.
