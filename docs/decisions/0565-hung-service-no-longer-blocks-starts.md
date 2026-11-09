# 0565 A hung service no longer blocks every start

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V3
- Date: 2026-11-09

## Question
0534's first fix: a companion that stopped answering while it held the single-instance lock made every later start quit (2026-11-02, H5; `docs/testing.md`). How does the plain Bun service (0552) take over from a hung one?

## Facts
Built and tested 2026-11-09 in Phase A.
- The service's lock is `<data>/service.lock` holding `{pid, start}`, created only if absent; a holder is identified by pid and start time, so a reused pid isn't mistaken for it.
- `/health` answers while the service runs.

## Options
No choice for him: a fix restoring decided behaviour (0363, 0534).

## Recommendation
As the choice.

## Choice
- A start that finds the lock held by a dead process, or by a pid now running something else, removes the lock and starts.
- A holder that answers `/health` is the running Jocasta: the new start leaves (exit 0) and `jocasta` attaches to it.
- A holder that is closing is waited for up to 60 s.
- A holder that is alive but doesn't answer `/health` for 15 s is hung: its whole process tree is killed and the lock taken over.
- Bare `jocasta`, finding the service busy or down, starts `jocasta service` and waits up to 90 s for it.

## Reason
0534.

## Consequences
- Carries out 0534; amends 0518's lock handling for the service.
- Regression test: `test/frontier/server/instance.test.ts` (a stand-in process that never answers).
- Plan appendix I gains `<data>/service.lock` and the `jocasta service` role.
