# Design rubric

The rules a program cannot check (0609, 0611). From Ousterhout's CS190 notes, Clean Code, Refactoring, Design Patterns and Designing Data-Intensive Applications; where they disagree, Ousterhout's reading holds. Lint, the compiler and the tests already check length, complexity, dead code and formatting: leave those alone.

A design finding names the rule, the place in the diff, and the smaller design. Never ask for a new layer, option, interface or abstraction unless it removes a smell you can point at in this diff. Say which code the change makes deletable. No finding is a fine result.

## Modules and interfaces
- A module gives much behaviour through a small interface. Size is not the measure; the interface is.
- A design decision (a format, an order, a timing, a protocol) lives in one module. The others call it.
- Code that shares knowledge or does one job sits together. Code with nothing in common is split.
- A part receives the collaborators it uses. No shared bag of everything.
- A method that only passes its arguments on is a layer that hides nothing: remove it.
- Composition and function-typed seams before inheritance.

## Functions and names
- A function either changes state or answers a question, and its name says any side effect.
- A function is split only where the pieces share no locals and each name says more than its code.
- A name says what the thing is in the project's own words. One word per concept, everywhere.
- A literal with a meaning is a named constant with its unit. A value nobody tunes is not a setting.

## Comments
- Each exported function, class and type has a comment for its caller: what it does, units, edge cases, errors.
- An inside comment says why, never what the next line does. A comment that names a thing that is gone is a defect.

## Errors
- Shape the interface so the common failure is an ordinary result, not an exception.
- Failures handled the same way are handled in one place.
- No throw and catch, or repeated failing attempts, as control flow: test the condition first.
- Input from outside (files, JSON, another program's output) is parsed once at the edge into a typed value.

## State and data
- A fact is stored once. What can be computed is computed.
- A record has no field that exists only in some phases: one type per phase.
- A closed set is a union handled in one place. A second switch on the same value calls for a table or a handler map.

## Duplication and change
- Each piece of knowledge has one home. Two readers of one format are a duplicate even when their code differs.
- One reason to change touches one module. If this diff had to edit several places for one decision, say where it should live.
- When the task shows the structure is wrong, the structure changes. No special case around it.
- Nothing is built for a need the task does not have.

## Patterns
- A pattern is used only to remove a smell, and a one-line comment names it.
- One module per foreign program turns its wire shapes into the project's own types.
- A state machine is a table of allowed moves and one function that applies them.
- A policy such as a timeout, a retry or warn-once is one helper around the calls.

## Stored data and background work
- One system of record per fact. An index, cache or summary can be dropped and rebuilt, and is never the only copy.
- A write is safe to repeat. A reader keeps its position and tolerates a replay.
- A file is replaced whole through a temporary file and a rename. An append store survives a torn last record.
- Schema changes are numbered, forward-only migrations. Readers ignore fields they do not know.
- Each file or table has one writer, or a lock. Each append-only store has a bound.
- A poller never overlaps itself, backs off on failure, and measures durations on a monotonic clock.

## Tests
- A test checks one behaviour, and its name states the unit and the condition.
- The boundary cases are covered, and the cases next to a fixed bug.
- A test does not depend on wall time, sleeps or order.
