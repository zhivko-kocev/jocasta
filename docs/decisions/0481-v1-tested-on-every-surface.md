# 0481 v1 is tested on every surface before it is called done

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-24

## Question
He wants every aspect and window tested before v1 is done ("security beta smoke idk what else there is"). What does the test campaign cover, and where does the clean-machine run happen?

## Facts
Verified 2026-10-24.
- The surface map (`local://test-surface-map.md`) lists every surface (CLI, HTTP, MCP, companion views, herdr surfaces, setup migrations, pollers, voice and dictation, files, integrations) with its tests; no automated test covers audio devices (playback, capture, recognition) or Windows integration (Credential Manager, autostart registry, the Right Ctrl hook, WebView2 navigation).
- Live herdr, Docker (Graphiti), real omp and dictation runs exist only as opt-in smoke tests (`JARVIS_HERDR_SMOKE`, `JARVIS_GRAPHITI_SMOKE`, `JARVIS_OMP_GATEWAY_SMOKE`, `JARVIS_DICTATION_SMOKE`).
- The full suite passed after 0478/0479: 448 pass, 7 opt-in skips, 0 fail; the extra omp log during that run came from omp's tiny-model worker of a session started before it.
- Hyper-V feature `Microsoft-Hyper-V-All` and `Containers` are Disabled (InstallState 2); the VirtualMachinePlatform feature is on; the `vmms` service and `New-VM` are absent.
- 31.5 GB RAM, 14 logical CPUs; D: has 340 GB free; Windows 11 Pro build 26300.

## Options
The campaign's scope is his request and was not offered as a choice. The clean-machine run was chosen from:

| Option | Pros | Cons |
|---|---|---|
| Windows Sandbox | Built into Windows 11 Pro; starts clean every time; one feature to enable | Gone on close, so no restart or log-off inside it; no snapshots |
| A throwaway user profile | No new feature; real Windows session | Shares the machine-wide installs, registry and Docker, so it isn't clean |
| A Hyper-V VM | A full clean Windows with restarts, snapshots and a before/after diff | Hyper-V to enable, an evaluation image of about 6 GB, more setup time |

## Recommendation
Windows Sandbox.

## Choice
- v1 is done when every surface listed in the surface map (`docs/testing.md`, made from `local://test-surface-map.md` with each surface's verdict) passes these checks.
- **Automated tests:** unit, temp-dir, PTY, herdr sandbox session and CDP/WebView2 tests where they fit.
- **Live sandbox walkthroughs (0441, 0446)** of every window and state: rail, canvas, dashboard on the second display, capsule, tray, dialogs, toasts and HUD.
- **Voice and dictation** with the real speakers and microphone, in one session where he speaks.
- **Graphiti** with Docker.
- **A clean-machine run in a Hyper-V VM** (Windows 11 Enterprise evaluation image): `install.ps1` from a local build, setup, first run, a thread, uninstall, and a before/after diff of files and registry.
- **Linux and macOS:** the builds compile; their runtime stays untested and is written down as such.
- **Performance:** startup, RAM and idle CPU.
- **Reliability:** kill herdr, the companion and a pane mid-turn, and log off.
- **Privacy egress:** every network destination observed must be in appendix I's list.
- **Security:** every 0484/0485 fix is re-tested.
- **Beta scenarios:** agents play him through a full day in the sandbox (start, plan, approve, review, finish, conflicts, quota, quiet hours), with model calls on his subscriptions.
- **Approved installs:** Windows' Hyper-V and Containers features (one admin prompt and a restart he chooses) and the Windows 11 Enterprise evaluation ISO (about 6 GB, on D:).
- **Reporting:** each surface's verdict goes in CHANGELOG and `docs/status.md`.

## Reason
His request, and "A Hyper-V VM".

## Consequences
- Differs from the recommendation: a Hyper-V VM instead of Windows Sandbox.
- Plan section 4 and AGENTS.md's approved installs add Windows' Hyper-V and Containers features and the Windows 11 Enterprise evaluation ISO.
- 0487's install waits for this campaign to pass.
