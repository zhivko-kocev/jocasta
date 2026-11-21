# 0649 The herdr fork: one binary, one server

- Status: v1 (built 2026-11-18, unattended; for his review)
- Kind: product
- Phase/slice: fork build, phases 1–5
- Date: 2026-11-18

## Question
Jocasta ran as a herdr plugin beside a separate service (0552, 0553). He asked whether Jocasta could be herdr itself: a fork with Jocasta built in, trimmed to what Jocasta uses.

## Facts
Verified 2026-11-18 (`docs/research/two-plugins.md`, `docs/research/fork-plan.md`).
- herdr v0.9.5 is about 11.8 MB of Rust plus libghostty-vt in Zig. Releases come every one to two weeks, and upstream takes no outside pull requests.
- Its server and client are one binary, its `run_server` reads argv by position and exits the process on a busy socket, and it already carries an append-only `EndpointControl {kind, data}` message.
- herdr's plugins can't run a long-lived program, bind keys or set herdr's config, and live handoff is Unix only.

## Options
- **The fork:** a full fork trimmed to what Jocasta uses or may use; the core only; or stay a plugin.
- **History:** upstream's history in a subtree, or one squashed commit.
- **Processes:** herdr's server and Jocasta's service as one process, or two.

## Recommendation
A full fork, trimmed; one process; one binary named `jocasta`.

## Choice
His answers of 2026-11-18, quoted in `docs/research/two-plugins.md` ("His answers so far"), built as listed in `docs/research/fork-build.md` (choices 1–48, 57, 58, 61–63, 75, 77):
- herdr v0.9.5 lives in this repo at `crates/herdr` as a library, in one Cargo workspace. It came in as one squashed subtree commit (choice 1; upstream's authors would fail the pre-push identity rule).
- The trim drops remote and machine access, the self-updater, 21 of 22 agent manifests and the other agents' integrations, and the render benchmark. It keeps kitty graphics, the plugin system and live handoff (`crates/herdr/FORK.md`).
- There is one binary, `jocasta`: herdr's verbs join Jocasta's at the top level, and `worktree` is one merged command, with Jocasta's leftover list at `jocasta worktree leftover`.
- herdr's files go under Jocasta's own dirs, and its config is `~/.jocasta/agent/herdr.toml`.
- There is one server: bare `jocasta` starts herdr's server with Jocasta's service inside it (`src/frontier/server/embedded.rs`), and his terminal is its client.
- `today` is a dock, and questions are a popup, both drawn by herdr's own client from the service's messages. His thread keys are intercepted in-process.
- The refresh wakes on herdr's events.

## Reason
His words: "full fork trimmed, i love it, we take all that we use or can be useful in future for jocasta and drop other things and make herdr lightweight also"; "if we can reuse something from herdr background server then we dont need the jocasta service part".

## Consequences
- An upstream release comes in with `git subtree pull --squash`, read and merged deliberately (`crates/herdr/FORK.md`).
- Amends 0392, 0518, 0544, 0552, 0553 and 0582. The herdr plugin of 0553, its programs, and 0552's separate `jocasta service` are gone: the service lives in herdr's server process (0392). There is no separate herdr server to revive or spare (0518, 0582). `jocasta worktree list` is herdr's, and Jocasta's list is `jocasta worktree leftover` (0544).
- The lock takes herdr's crates, and CI builds Zig's library on three OSes with a 90-minute limit.
