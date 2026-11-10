# 0593 His /settings pass: the values in his config and in Jocasta's

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V8, V13
- Date: 2026-11-10

## Question
0420 left 0398's omp values for him to set by hand, and 0466 has setup merge them after his `y`. Setup has never run on his PC, so his own omp runs without most of them, and omp 18.18.6 has keys that came after 0451. Which values does he take, and where do they go?

## Facts
Verified 2026-11-10 against omp 18.18.6's settings registry (604 settings, the same in 18.18.2) and his `~/.omp/agent/config.yml`. The report is `D:/jocasta-spikes/settings/settings.md`, outside the repo; his sessions as aggregates only.
- **Auto QA:**
  - His config had `dev.autoqaConsent: granted`. His plain sessions sent 17 reports to `https://qa.omp.sh/v1/grievances`, from 2026-08-26 to 2026-11-09.
  - Each report carries his install id, platform, model and omp version. Its text is the model's own and can quote commands and paths; omp replaces only the home folder and login name.
- **Web search:** his `modelRoles.web` was `web/parallel`, so Parallel got each query with the session id and model name. DuckDuckGo gets only the query. His fallback chain was `[duckduckgo, startpage, public]`.
- **Claude Code plugins** load into his plain omp sessions (the `claude-plugins` provider); Jocasta's panes turn them off (0478).
- **Memory:**
  - `local` (his plain sessions): at a session's start the `default` model, at low effort, summarises older sessions in that folder, and `smol` merges the summaries.
  - mnemopi (Jocasta's panes, 0447): the `memory` role extracts facts every 4 of his messages; the search index is built locally.
  - Switching leaves `local`'s notes (about 360 KB, 12 project folders) unused, not deleted.
- **The security scanner:** `security.enabled: true` is his own setting (default off). It adds `security_scan` and `/security`. A local scan sends the code it reads to his model provider; cloud actions use OpenAI's Codex Security through his ChatGPT login.
- **Spend warning:** `session.warnCost` is unset. It counts USD over the whole session tree and warns once; `--warn-cost` overrides it for one run (`session/cost-gate.ts`, `session/settings.ts:1578-1584`). Threads start with `--warn-cost 50` (0541).
- **Fenced folders:**
  - `tools.restrictedFolders` (default empty) makes every read, write, edit, search or command in a listed folder ask, whatever the approval mode or grant (`tools/approval.ts:566-585`).
  - A command is matched by the folder it runs in, and a search started above a listed folder counts too. A thread's subagents can't answer.
  - On his PC `~/.ssh`, `~/.aws`, `~/.azure` and `~/.docker` exist. Windows refuses writes to its system folders without an admin prompt.
- **Own secrets:** `secrets.enabled: true`, but `~/.omp/agent/secrets.yml` is missing, so only built-in token shapes and secret-looking environment variables are replaced. The file is a YAML list of `{type: plain|regex, content, mode?, replacement?, flags?, friendlyName?}` (`secrets/index.ts:166-389`). A project may add `.omp/secrets.yml`.
- **Dead key:** `auth.subscriptionOrder` is not an omp setting, in 18.18.2 or 18.18.6, so it was ignored. `auth.accountPolicies` ranks accounts by email or account id, with a priority and a reserve.
- **New keys since 0451:**
  - `providers.openai-codex.daybreak` defaults to true: "OpenAI may use content from Daybreak Blue sessions to improve its models".
  - `spotlight.experiment.enabled` defaults to false; on, it would send tool output to `cloud.stencil.so`.
- **The rest:** telemetry, the wizard and changelog, the theme, `find`, `github`, diagnostics on edit, the paste menu, recaps and cache warming were missing from his config. 0398 and 0451 had decided them. With `find.enabled: auto`, `find` stays off while `judge` is a chat model.

## Options
| Question | Options | Recommended |
|---|---|---|
| Where the decided values go | His config now and Jocasta's shipped settings; his config only | Both |
| Auto QA | Off everywhere, consent denied; keep it on in plain sessions | Off everywhere |
| First web engine | DuckDuckGo with the chain kept; DuckDuckGo only; keep Parallel | DuckDuckGo, chain kept |
| Claude Code plugins in omp | Off everywhere; keep them in plain sessions | Off everywhere |
| Memory in plain sessions | mnemopi everywhere; keep `local` | mnemopi |
| The security scanner | Keep it on; off | Keep it on |
| The scanner in Jocasta's settings | His PC only; ship it on | His PC only |
| Spend warning | Warn at $50, as threads; none; a hard stop | Warn at $50 |
| Fenced folders | None; his key folders; key and OS folders | None, then his key folders once he asked about OS folders |
| `secrets.yml` | An empty one to fill; skip | An empty one to fill |
| `auth.subscriptionOrder` | Remove it; a real order by email | Remove it |
| The rest | A tick-list of 0398's and 0451's values, the two pins and F9 | All |

## Recommendation
As in the table's last column.

## Choice
- **Scope:** each value goes into his `~/.omp/agent/config.yml` now and into Jocasta's shipped settings, except the two below that are his PC's only.
- `dev.autoqa: false`, `dev.autoqaConsent: denied`.
- `modelRoles.web: web/duckduckgo`; his chain `[duckduckgo, startpage, public]` stays.
- `disabledProviders: [claude-plugins]`.
- `memory.backend: mnemopi` in his plain sessions too.
- `security.enabled: true` stays, on his PC only.
- `session.warnCost: 50`.
- `tools.restrictedFolders`: `~/.ssh`, `~/.aws`, `~/.azure`, `~/.docker`, plus Windows' system folders (`C:\WINDOWS` and `C:\Program Files` on his PC). How setup ships them is 0594.
- `~/.omp/agent/secrets.yml`: an empty one for him to fill (0594).
- `auth.subscriptionOrder` is removed from his config, his PC only.
- **Every tick-list item:**
  - `telemetry.otlpExportEnabled: false`;
  - `startup.setupWizard: false`, `startup.changelogMode: hidden`;
  - `theme.dark: dark-tokyo-night`;
  - `find.enabled: "off"`, `github.enabled: true`;
  - `lsp.diagnosticsOnEdit: true`, `paste.largeMenuThreshold: 0`;
  - `recap.enabled: false`, `providers.cacheWarming: streaming`;
  - `providers.openai-codex.daybreak: false`, and `spotlight.experiment.enabled: false` pinned;
  - `compaction.experimentalContextManagement` left at omp's false;
  - F9 for dictation: `app.stt.toggle: f9` in his `keybindings.yml`.

## Reason
On scope: "recomended also in jocasta everywhere". He picked the recommended option on Auto QA, web search, plugins, memory, the scanner, the spend warning, `secrets.yml` and the dead key, and ticked every item in the list.
- On the scanner he first asked "what does the security scan does", and after the explanation picked "Keep it on", then "Your PC only".
- On fences he asked "maybe the OS folders of the system idk does that make sense or will it block something?", then picked "Key folders and OS folders".
- On the tick-list he asked "would find:off help?". It changes nothing today; it keeps `find` off if a dedicated judge model is set later.

## Consequences
- Amends 0398 and 0451: the reference gains `session.warnCost`, `tools.restrictedFolders`, `providers.openai-codex.daybreak` and `spotlight.experiment.enabled`.
- Amends 0420: after his answers the agent wrote his config, on 2026-11-10; he didn't set the values by hand.
- The fenced OS folders differ from the recommendation.
- Plain sessions lose the Claude Code plugins' skills and servers, the Microsoft Learn docs among them, and start with an empty mnemopi memory.
- README's config section and plan appendix I's omp row list the new keys.
