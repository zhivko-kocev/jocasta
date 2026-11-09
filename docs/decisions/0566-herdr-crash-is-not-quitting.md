# 0566 A herdr crash the service healed is no longer read as him quitting

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V3
- Date: 2026-11-09

## Question
0534's second fix: after a herdr server crash, the companion started herdr again and, 92 ms later, `jocasta`'s check after its client exited took the exit for him quitting and stopped everything (2026-11-02, H3; `herdrBack` in `src/shared/cli.ts`). How does `jocasta` tell a healed crash from him quitting?

## Facts
Built and tested 2026-11-09 in Phase A.
- The service restarts a dead herdr server (0518) and now counts each restart (`herdrRevivals()` in `herdr.ts`), reported in `/health`.

## Options
No choice for him: a fix restoring decided behaviour (0363, 0534).

## Recommendation
As the choice.

## Choice
- `jocasta` reads the service's revival count before each client attach. When the client exits and herdr answers, the exit is a crash only if the count grew; `jocasta` then attaches again. An unchanged count means he quit.
- When herdr doesn't answer, `jocasta` waits up to 30 s for the service's restart, as before.

## Reason
0534.

## Consequences
- Carries out 0534; amends 0518.
- Regression test in `test/shared/cli.test.ts` ("a herdr crash the service healed before the client looked…").
- `/health` gains `herdrRevivals` (plan appendix I).
