# 0480 The product is Jocasta

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
"Jarvis" is too default, and Marvel holds JARVIS for voice-assistant software. He wants a lesser-known, underdog Stark AI. Which name does the product take?

## Facts
Verified 2026-10-24.
- npm `jocasta` is free (404); there is no winget, scoop or Windows command, alias or prefix clash; the biggest GitHub repo named jocasta has 6★ (raddad772/jocasta-emu); PyPI `jocasta` 0.3.2 is a sensor library ("Fetch sensor data.").
- VisionQuest (Disney+, 13 Oct – 10 Nov 2026) features Jocasta, and her Wikipedia views rose (`Jocasta (Marvel Comics)`: 42,909 views in 12 months, 19,470 of them in Sep 2026).
- Marvel's JARVIS trademark (reg. 4737881, serial 86294162) for "software for use as a voice controlled personal digital assistant" was renewed 2025-05-09 (https://uspto.report/TM/86294162); Jarvis.ai renamed itself Jasper after a Marvel cease-and-desist.
- An automated USPTO wordmark search on 2026-10-24 returned "No results found" even for the control query JARVIS, so Jocasta's trademark status is unverified; he checks it by hand on https://tmsearch.uspto.gov.

## Options
| Option | Pros | Cons |
|---|---|---|
| Jocasta | The cleanest namespace of the eight checked (npm free, no CLI binary on any registry, GitHub max 6★, no Windows clash); the best underdog story: Tony passed over the J.O.C.A.S.T.A. chip for F.R.I.D.A.Y., and in the comics she defied what she was built for | Less unknown while VisionQuest airs; an enemy of Vision in the MCU; the Oedipus association; 7 letters; trademark risk unverified (medium) |
| EDITH | 5 letters; canonical backronym and the `edit` pun; reads well in a title | Not an underdog; same-category collisions (EDITH-Harness, edith-cli, npm `edith-cli` ships a bin `edith`); `edi`+Tab reaches Windows' `edit.exe`; high trademark exposure |
| Veronica | Obscure; Tony's dispatcher of armor ↔ a boss that dispatches threads; a common given name, low trademark risk; same in English and Macedonian | 8 letters; npm taken; weak link to Stark; `ve`+Tab clashes with `verifier.exe`; a module, not an AI, in the film |
| DUM-E | The purest underdog; 4 letters; npm `dume` free; no Windows clash | PyPI `dume` is already a CLI; "Dummy" undercuts a thesis defense and a tool trusted with merges; no obvious pronunciation |

## Recommendation
Jocasta.

## Choice
Jocasta everywhere, in one cutover:
- Command `jocasta` (`jocasta setup | uninstall | diagnose | mcp | project`) and the package name `jocasta`.
- Env vars: every `JARVIS_*` becomes `JOCASTA_*`, same suffix.
- Paths: `~/.jocasta/agent/` (config), `%LOCALAPPDATA%\jocasta\` and the POSIX equivalents (data), install folder `%LOCALAPPDATA%\Programs\jocasta`.
- App identifier `local.jocasta.frontier`; autostart value `Jocasta`.
- herdr session `jocasta`; the boss workspace "Jocasta"; omp MCP server `jocasta` (tools `mcp__jocasta_*`).
- Compose project `jocasta`; Credential Manager target `jocasta/calendar-ical`; cookie `jocasta_frontier`.
- mise: env `jocasta-<project id>`, files `conf.d/jocasta.toml` and `config.jocasta-<project id>.override.toml`.
- Release assets `jocasta-<platform>.zip`.
- The boss and the companion call themselves Jocasta; the voice (kokoro `af_heart`) stays; "frontier" stays as an internal module name.
- Living docs use the new name: `docs/plan.md`, README, DESIGN, PRODUCT, AGENTS.md, CONTRIBUTING, `docs/status.md`, `docs/research/`, `config/`, `evals/`.
- History keeps "Jarvis": decision records 0001–0479 and the CHANGELOG sections before the rename. The plan gains one line: "Jarvis was the working name until 0480".
- No migration code from Jarvis's paths: there are no other installs, and his machine is cleaned and installed fresh (0487).
- He renames the repo folder `D:/jarvis` and the GitHub repository himself when he pushes; nothing in the code depends on the folder name.

## Reason
"Jocasta".

## Consequences
- Plan appendix I's claimed names and env var table change to the new names.
- AGENTS.md's package map and rules change to the new names.
- Every later record says Jocasta.
- Amended by 0564: the agent created the public repository `zhivko-kocev/jocasta`; the folder `D:/jarvis` is renamed to `D:/jocasta` after the harness-OS build by a script he runs.
