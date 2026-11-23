# 0665 Jocasta owns the workspaces

- Status: v1
- Kind: product, ux
- Phase/slice: fork build, after his first day on it
- Date: 2026-11-23

## Question
He asked to "prune everything that interferes with my idea of the perfect orchestration engine". herdr's client still makes, renames and closes workspaces on his keys and menus. Which of those stay?

## Facts
Verified 2026-11-23 in the fork (`crates/herdr/src/client/shell/`):
- Jocasta makes every project and thread workspace through herdr's API (`src/frontier/server/herdr.rs`: `workspace.create`, `tab.create`, `layout.apply`, `pane.split`, `workspace.close`) and finds the boss by its workspace's label, "Jocasta" (`BOSS_WORKSPACE`, `boss.rs`).
- herdr's client still offers: a new workspace (Ctrl+B Shift+N, the sidebar's `new`), git worktrees (new, open, delete checkout: Ctrl+B Shift+G and the workspace right-click menu), workspace rename (Ctrl+B Shift+W and the menu), workspace close (Ctrl+B Shift+D and the menu), new tabs (Ctrl+B c, the tab bar's `+`), splits and pane close, all on any workspace, the boss's included.
- A workspace or worktree made this way is one Jocasta doesn't track; a renamed "Jocasta" loses the boss; a closed one ends it.

## Options
- **Jocasta owns workspaces.** herdr's new workspace, worktree commands and workspace rename go everywhere; the boss's workspace can't be closed, split or given tabs; other workspaces keep tabs, splits and close. Pros: nothing he does can break the boss or make an untracked workspace; a shell beside a thread stays possible. Cons: no ad-hoc workspace outside Jocasta.
- **Jocasta owns every pane.** The above, and no tabs or splits anywhere. Pros: strictest. Cons: no extra shell beside a thread.
- **Protect only the boss's workspace.** Pros: least change. Cons: untracked workspaces and worktrees stay possible.

## Recommendation
Jocasta owns workspaces.

## Choice
His answer of 2026-11-23: Jocasta owns workspaces. herdr's client loses new workspace, the worktree commands (new, open, delete checkout) and workspace rename, by key, menu and button. On the boss's workspace, his client's close workspace, new tab, split and close pane do nothing. Other workspaces keep tabs, splits, pane close and workspace close. Jocasta's own calls through herdr's API are unchanged.

## Reason
His goal: nothing in the surface works against the orchestration; Jocasta makes the places, he works in them.

## Consequences
- Amends 0649 (what the fork keeps of herdr's client); `crates/herdr/FORK.md` lists the removals.
- Key bindings for the removed actions in a `herdr.toml` are read and ignored, so an old config still loads.
