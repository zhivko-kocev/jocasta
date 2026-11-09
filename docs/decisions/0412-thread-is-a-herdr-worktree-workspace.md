# 0412 A thread is a herdr worktree workspace

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
herdr opens every worktree as its own workspace; what shape does a thread take?

## Facts
Verified 2026-10-13.
- Built and run live on herdr 0.9.4 (2026-10-13): `worktree.create` always opens the checkout as a new workspace grouped under the source workspace in the sidebar (`herdr:docs/next/website/src/content/docs/socket-api.mdx:344`; `cli-reference.mdx:182`); worktrees go under `worktrees.directory` (default `~/.herdr/worktrees`).
- `tab.create` and `pane.split` start only the default shell; only `layout.apply` takes a command (`herdr:src/pane.rs:1854-1870`).
- herdr's API answers one request per connection; only `events.subscribe` stays open (`herdr:src/api/server.rs:328-376`).

## Options
| Option | Pros | Cons |
|---|---|---|
| herdr's way: a workspace per thread, grouped under its project | herdr lists, opens and removes the worktrees itself; threads nest under their project in the sidebar; Alt+digits reach them | Amends the tab-per-thread layout |
| Tab per thread | As 0388/0392 wrote | Jarvis runs `git worktree add` itself; herdr's worktree commands don't know them |

## Recommendation
herdr's way.

## Choice
A workspace thread is a herdr workspace made by `worktree.create`, grouped under its project's workspace, running omp in the worktree; the project workspace holds the project pane. Removing a thread is herdr's worktree remove.

## Reason
"herdr's way: workspace per thread, grouped".

## Consequences
- Amends 0388 and 0392 (tab per thread) and the tab wording in 0399.
- The herdr module opens a fresh connection per request.
- Amended by 0445: a thread's worktree goes beside its project, in `<project folder>.wt/<title slug>`, not herdr's default folder.
- Amended by 0457: before omp starts, a project's `.worktreeinclude` files are copied in and thread panes get per-thread variables.
