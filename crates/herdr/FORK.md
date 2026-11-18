# This fork

herdr v0.9.5 (`github.com/herdrdev/herdr`, tag `v0.9.5`, commit `2ecf19a9`), Apache-2.0, brought into Jocasta's repo with
`git subtree add --prefix=crates/herdr … v0.9.5 --squash` on 2026-11-18. Jocasta builds it into its one `jocasta`
binary; it is not shipped as `herdr`. Changed files are listed below, as Apache-2.0 §4(b) asks; `LICENSE` is herdr's.

## Removed

- The repo's agent and contributor dressing: `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `.agents/`, `.pi/`, `.zed/`,
  `.githooks/`, `.github/`, and `build.rs` (it only printed the contributor policy and named three build variables).
- Its own toolchain and lock, which the workspace root holds: `rust-toolchain.toml`, `.cargo/`, `Cargo.lock`; the Nix
  files (`flake.*`, `nix/`), `justfile`, `SPONSORS.md`, `README.zh-CN.md`, and the README's images under `assets/`
  (the sounds stay).

## Changed

- `Cargo.toml`: no `[workspace]`, `[patch.crates-io]` or `build`; the root's workspace lists this crate and
  `crates/ghostty-vt`, and patches `portable-pty` with `vendor/portable-pty`. The MSVC static C runtime is set in the
  root's `.cargo/config.toml`.
- `src/pane/terminal.rs`, the test `render_keeps_halfwidth_katakana_voiced_tail_empty`: it checks what the host
  terminal is sent (no write over the wide tail) rather than the test backend's copy, because the workspace's ratatui
  0.30.2 counts a halfwidth voiced mark's cell and so skips the tail that 0.30.0 wrote as empty.

## Taking an upstream release

`git subtree pull --prefix=crates/herdr https://github.com/herdrdev/herdr.git vX.Y.Z --squash`, then `git rm` every
file this fork removed that the release changed (a modify/delete conflict), and re-apply the seams listed above.
herdr's own suite (`cargo nextest run -p herdr`) and Jocasta's run before the merge is committed.
