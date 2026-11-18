# herdr's suite before the trim (2026-11-18)

`cargo nextest run -p herdr -p ghostty-vt --no-fail-fast -j 4` at the fork's first commit (upstream v0.9.5 with the
workspace's lock), Rust 1.100.0, Zig 0.16.0. Later gates allow no new failures; the trimmed parts' tests leave the
count with them.

| Where | Run | Passed | Failed | Skipped |
|---|---|---|---|---|
| Windows 11 (his PC) | 3434 | 3430 | 4 | 12 |
| Ubuntu in WSL 2 | 4030 | 4028 | 2 | 14 |

## The failures

Each also fails on upstream's own lock and toolchain pin overridden to 1.100.0, run alone, except the one marked.

- Windows: `detect::manifest_update::tests::auto_update_reloads_manifest_cache_after_remote_commit`,
  `auto_update_does_not_reload_agents_whose_check_failed` and
  `auto_update_reloads_manifest_cache_when_remote_is_already_current`: no update event arrives (left 0, right 1).
  The manifest updater goes in the trim.
- Windows: `pane::terminal::tests::render_keeps_halfwidth_katakana_voiced_tail_empty`, from the workspace's
  ratatui 0.30.2 (upstream's lock holds 0.30.0, where it passes). The fork's test now checks what the host terminal
  is sent; see `FORK.md`.
- WSL: `server_headless::server_survives_hangup_and_logs_why_it_stops` (the log never says "server shutdown
  initiated") and `client_mode::sigwinch_refreshes_host_palette_without_resizing`: both send a Unix signal to a
  spawned process and fail on every run under WSL 2, upstream's lock too.

## After the trim (phase 2)

| Where | Run | Passed | Failed | Skipped |
|---|---|---|---|---|
| Windows 11 (his PC) | 3123 | 3123 | 0 | 10 |
| Ubuntu in WSL 2 | 3590 | 3589 | 1 | 12 |

WSL's one failure is `sigwinch_refreshes_host_palette_without_resizing` above; `server_survives_hangup_and_logs_why_it_stops`
passed on that run. CI leaves both out.
