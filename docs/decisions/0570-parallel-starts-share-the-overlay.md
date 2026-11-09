# 0570 Parallel thread starts in one project no longer race on its omp.yml

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V10
- Date: 2026-11-09

## Question
0534's third fix: 8 `thread_start`s in one project at once ended with `EPERM: operation not permitted, rename '…omp.yml.<pid>.tmp' -> '…omp.yml'` for one of them (2026-11-02, H8; `docs/testing.md`). How do parallel starts share the project's overlay?

## Facts
Reproduced and fixed 2026-11-09 in Phase C.
- Windows refuses a rename over a file another process holds open without delete-sharing, which is how omp reads its `--config` files: EPERM.
- Every start in a project wrote the same overlay text.

## Options
No choice for him: a fix restoring decided behaviour (0363, 0534).

## Recommendation
As the choice.

## Choice
- One overlay write per project at a time: starts that arrive meanwhile wait for that write.
- The file is read first and not rewritten when its text is already the same, so parallel starts never replace a file other omps are reading.
- A rename refused with EPERM, EBUSY or EACCES is retried 10 times, 100 ms apart, without blocking the service; on failure the temporary file is removed.

## Reason
0534.

## Consequences
- Carries out 0534.
- Regression test: on Windows, 4 starts against an overlay held open by another process, then 4 against a current one held open throughout.
