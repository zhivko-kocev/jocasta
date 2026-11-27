# 0716 `jocasta brief open`

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, M2 R8
- Date: 2026-11-27
- Amends: 0721 (without a running Jocasta, `brief open` stores its own `show` brief)

## Question
Any agent (a thread, a script, ultron's tools) should be able to show a picture the way the boss's `show` does. The plan's `jocasta brief render` wrote a self-contained HTML page; the amendment of 2026-11-27 drops HTML. What does the command take, where does the brief go, and what happens without a running Jocasta?

## Facts
- `show` stores a brief as `show-<n>` and starts `jocasta view show-<n>` detached (0721); a `show` brief has no buttons.
- The bridge already takes CLI commands from a peer with the token (`brief act`, `brief live`, `thread …`, 0675, 0721); a reference section (0714) needs the service's threads.
- `jocasta view` reads only stored briefs; the store's ids are checked (`store::valid_id`).
- Exit codes so far: 0 done, 1 a fault, 2 a refusal or bad usage (0140), 3 Jocasta isn't running (0544).

## Options
| Option | Pros | Cons |
|---|---|---|
| Check the spec here; the running service shows it as `show` does; without it, store `show-<n>` here and open the window, refusing only reference sections | Same reader, same errors, same brief and retention as the boss's; works without the service for anything that needs no thread | Two writers of the brief folder (the second only while the first isn't running) |
| A temporary brief file passed to `jocasta view --file` | Nothing stored | A new view flag; nothing to open again; retention doesn't apply |
| Only through the service | One writer | Useless without a running Jocasta |

## Recommendation
Check here, show through the service, store here without it.

## Choice
His 2026-11-27 amendment and the approved plan:
- `jocasta brief open <spec.json | ->` reads the spec from the file or standard input and checks it with `spec::read` (0714). Then it asks the running service over the bridge (`brief open`, the `show` tool's own code: reference sections filled, stored as `show-<n>`, its window opened) and prints `Shown: <title>`.
- Without a running Jocasta it stores the same `show-<n>` brief itself and starts `jocasta view show-<n>` detached: the window opens read-only, which a `show` brief is anyway. A spec with a reference section is refused then: `show: section <n> is filled from the thread <name>, and Jocasta isn't running to fill it.`
- Every refusal prints `jocasta: show: <error>` and exits 2: a spec `show` refuses, a file it can't read (`show: can't read <path>: <error>`), text that isn't a JSON object, a refusal from the service. A window that can't be started exits 1 (`jocasta: the window didn't open: <error>`). clap's usage errors exit 2 as every command's.
- A stored brief, not a temporary file: it opens again with `jocasta view show-<n>`, and 0715's 30 days clean it up.

## Reason
One spec reader and one kind of brief for the boss and for any agent; the picture still opens when Jocasta isn't running.

## Consequences
- `Route::BriefOpen`, `view::open_spec`, `client::brief_open` and the bridge CLI command `brief open`. Plan appendix I lists the command and its exit codes.
- ultron's skill can tell any agent to run `jocasta brief open` (the ultron side, R9).
- The tests check the refusals end to end; a spec that passes is not opened by the tests (it would open a window on the desktop they run on).
