---
name: leftover-worktrees
description: Thread folders left on disk after their workspace closed. Use when projects() lists "left on disk" or he asks to clean up old thread folders.
---

# Leftover thread folders

A thread he closed in herdr keeps its folder and branch (`projects()` lists them as "left on disk: <folder> [<branch>]
N ahead, clean or M changed").

- Tell him about them only when he asks or is cleaning up; never on your own.
- `worktree_remove(path)` removes one folder, only on his word for that folder. Its branch stays.
- A folder with uncommitted changes is refused: tell him; never discard the work some other way.
- A folder whose branch is ahead of trunk holds unlanded commits: say so before he decides.
