---
title: Test first
applies_when: a change in behaviour that a test can pin at a seam the plan names (a function, a module's interface, a command's output)
date: 2026-10-23
source: Jocasta's shipped TDD lesson (0461)
---

Write the test first, at the seam the plan names, and run it: it must fail, for the reason you expect. Then write the
least code that makes it pass, and run it again. Refactor only with the tests passing. Test through the seam, not
internals; one behaviour per test. Where a test can't come first (layout, a spike, glue to an outside service), say so
in the plan and add the test after.
