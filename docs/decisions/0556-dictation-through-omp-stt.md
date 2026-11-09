# 0556 Dictation goes through omp's own speech-to-text in the boss pane, started from anywhere with Right Ctrl

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V2
- Date: 2026-11-09

## Question
0552 keeps dictation through a native audio helper chosen by a spike. The settings review found omp records and transcribes on its own. How does dictation work without the app?

## Facts
Verified 2026-11-09 against omp 18.18.2's docs (`omp://settings.md`, `omp://keybindings.md`, `omp://local-models.md`) and his config.
- `stt.enabled` (default false) turns on microphone speech-to-text; `modelRoles.dictation` picks the engine, and `local/parakeet-tdt-0.6b-v3` ("Default; fast multilingual Parakeet TDT v3", sherpa-onnx int8, ~680 MB, on CPU, "Audio stays on your machine") is the model Jocasta's dictation uses (0330). `retry.fallbackChains.dictation: []` keeps it local.
- `stt.submitTrigger`: `never` (default), `release`, `release-complete` or `say-submit`.
- Keys: `app.stt.pushToTalk` (Space, held, in the focused omp pane) and `app.stt.toggle` (unbound; each press starts or stops recording).
- His `~/.omp/agent/config.yml` already has `stt.enabled: true` and the Parakeet dictation role.
- Jocasta's Right Ctrl hook is a Bun worker (`src/app/ptt-worker.ts`); herdr's `pane.send_keys` sends a key to a pane.

## Options
| Item | Options (pros / cons) |
|---|---|
| Dictation | omp's STT with Right Ctrl anywhere (no helper; a key sent through herdr) · omp's STT in the focused pane only (no global key) · The audio-helper spike (Jocasta records itself) |
| After he stops | Sent at once (`release`, as 0368) · Stays in the input (`never`) |

## Recommendation
omp's STT with Right Ctrl anywhere; sent at once.

## Choice
- The boss pane's `boss.yml` sets `stt.enabled: true`, `modelRoles.dictation: local/parakeet-tdt-0.6b-v3`, `retry.fallbackChains.dictation: []` and `stt.submitTrigger: release`.
- `app.stt.toggle` is bound to a key omp and herdr leave unused (setup writes it into `keybindings.yml`); Jocasta's Right Ctrl hook, in the service, sends that key to the boss pane through herdr on press and again on release. Holding Space in a focused omp pane stays omp's own.
- Jocasta's own recorder, recognizer, model download and the audio-helper spike go.

## Reason
"omp's own STT, Right Ctrl anywhere"; "Sent at once".

## Consequences
- Amends 0552 (no audio helper; five spikes remain with 0553's plugin), 0330 (the speech model is omp's, downloaded by omp on first use or `omp setup speech`), 0368 and 0273 (the global key sends omp's toggle key).
- `sherpa-onnx-node` and its platform packages leave the repo with `src/frontier/dictation/`.
- From omp 18.18.2's source: `app.stt.toggle` calls `STTController.toggle` without `manualSubmit` (`input-controller.ts:764`, `interactive-mode.ts:9352`), so `stt.submitTrigger` applies to it; only the composer's mic button and RPC keep a draft (`manualSubmit: true`). Capture is omp's native `AudioCapture` from `@oh-my-pi/pi-natives`. The build checks live on this PC that it records on Windows and that the toggle key reaches an unfocused boss pane through herdr's `pane.send_keys`; if `release` doesn't send after a toggled stop, the hook sends Enter after the second toggle.
