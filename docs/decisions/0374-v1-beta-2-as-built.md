# 0374 v1 beta 2: what was fixed as built

- Status: decided
- Kind: product
- Phase/slice: 2/2.2, 4/4.3–4.4, 6/6.1, 7/7.3
- Date: 2026-10-10

## Question
Which fixes made during the second beta (2026-10-10) carry out decided records, and are recorded as built under 0363?

## Facts
Found on 2026-10-10 in the source build and in headless Edge, on fixture projects, with his real model (GitHub Copilot, Claude Opus 5.5) for the boss and two threads:
- The git-guard let through commands that its decided rules cover in another spelling: git's long-option prefixes (`git reset --har`, `git clean --forc`), an alias defined with `-c` (`git -c alias.p='push --force' p`), `wsl` as a wrapper, `su -c`, whole-tree `git restore .`, `git checkout -f` and `git switch -f` (which discard uncommitted work like `checkout -- .`), `find … -exec rm {} +`, `Get-ChildItem -Recurse | Remove-Item`, `$PWD/..`, and `.env` read as `git show HEAD:.env`, in an interpreter's inline code or by PowerShell's `[IO.File]::ReadAllText('.env')`.
- The boss chat showed the boss's markdown raw (`**Fix refunds**`, `- ` bullets), while thread tabs render it.
- A plan to approve filled its card as one run-on paragraph with the answers below the fold; a step's sub-points never reached the approval question.
- A boss-started thread's title was cut mid-clause ("…returns CSV text: a") and spoken that way, code marks included.
- A six-step flow on the canvas was clipped at both ends: every diagram frame was 320 px, and fitting centred what didn't fit at the legible zoom floor.
- The chat log didn't follow its last line when the dock grew under it.
- The boss told a ship thread not to commit, against 0359; it drew a project's plan on its own scratchpad.
- The boss's messages in a thread's TUI looked like his own: the host passed `from: "boss"`, and the harness dropped it (0270 says the tab marks them).
- In a source run (`bun src/entry.ts`) Pi reads `~/.pi/agent` while Jarvis reads `~/.jarvis/agent`, so a role model fails with "no model in roles.default has credentials".

## Options
A tick-list of the fixes as built.

## Recommendation
Accept them as built.

## Choice
Built under 0363, each restoring a decided record:
- Git-guard (0209–0214): every spelling above is caught by the rule it belongs to (`checkout-discard` now reads "checkout -- ., restore . and switch -f discard uncommitted work"); an alias that runs a shell command (`!…`) asks as unparsed. The corpus gains 29 cases (166).
- Boss chat (0369): a finished message is rendered as markdown with the canvas's renderer (raw HTML stays text); while it streams it is plain text. The spoken line is marked in either form.
- Cards (0269): a question renders as markdown; a long one scrolls inside its card, so its answers stay in view. Plan approval lists each step's sub-points.
- Thread names (0265, 0294): a boss-started thread is titled from its brief's first line, cut at the last clause's end within 60 characters, else at a word; a spoken name drops code marks (`toCsv(entries)` is said "toCsv"). Crew pills cut a long name at 28 characters, with the whole name in the tooltip.
- Canvas (0371): a diagram's frame grows to show it whole at the legible floor, up to 640 px; one still too big starts at its top or first lane.
- The chat log keeps its last line in view when the dock under it grows.
- Boss manual (0331, 0359, 0372): a ship thread commits on its own branch and reports; a drawing about one project goes on that project's scratchpad.
- Thread TUIs (0270): a message the boss sent reads "**boss ›** …"; the model gets it as sent. Messages from before the harness started show unmarked.
- Speech (0373): when he sends a message, the server stops reading the rest of the boss's current answer, as the page stops playing it.
- README: a source run needs `PI_CODING_AGENT_DIR` and `JARVIS_CODING_AGENT_DIR` pointed at one folder.

## Reason
Each restores behavior a record already decided (0363).

## Consequences
- New rules the beta suggested (stash, reflog, `gh repo delete`, `curl | sh` and others) are not built: they go beyond the plan and wait for his word. Now (0382): decided; stash, reflog and pipe-to-shell ask, `gh repo delete` is denied.
- Amended by 0393: whiteboard's canvas, built from source, replaces Jarvis's canvas and `draw`.
