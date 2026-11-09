---
name: save-lesson
description: Offering and saving a project lesson. Use after an [event] ending "offer once to save a lesson", when he says "make that a lesson", or when he asks to turn test-first (TDD) on for a project.
---

# Saving a lesson

A lesson is a rule a project's threads read before planning and its reviewers check against (`skill://lessons` in
that project's panes). It lives with Jocasta's data for the project, never in the repo.

## Offering one

After an `[event]` that ends with `offer once to save a lesson` (he sent a thread's plan or its reviewed work back, or
committed by hand on its branch), ask him once, in one line: "Save what we learned as a lesson for <project>?" Don't
ask again for that event, and never save on your own judgement.

## Saving one

Only on his yes, or when he says "make that a lesson". Save it only when it is non-obvious, durable and material; if it
isn't, say so in one line and save nothing. Call `lesson` with:
- `project`: the project's name;
- `title`: a few words;
- `applies_when`: when a thread should follow it, concrete enough to match a task;
- `rule`: the rule itself, in his words where he gave them, with the why in one sentence;
- `source`: what it came from (the thread and what happened);
- `retire_when`: only when he names when it stops applying.

## Test-first for a project

When he asks to turn TDD or test-first on for a project, read `skill://save-lesson/tdd.md` and call `lesson` with its
`title`, `applies_when` and `source` and its body as `rule`. Test-first is off for every project until he asks.
