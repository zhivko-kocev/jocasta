# 0383 Fork only what needs a patch

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-13

## Question
How do the upstreams (omp, herdr, whiteboard, hunk) live in Jarvis?

## Facts
Verified 2026-10-13.
- whiteboard (`devdotfast/whiteboard`) v0.2.4 (2026-10-09), MIT, a pnpm 11 + Node 24 monorepo with an Electron + Code-OSS desktop. Its canvas `@dev.fast/review-canvas` is private (`packages/review/app`, build `dist/desktop`, `mountReviewCanvas(container, content, ui)`, host bridge `ReviewCanvasBridge` with `inlineEditors` and `diffView`); its canvas is a private workspace, not on npm, so using it needs its source.
- omp's `main()` takes no extension factories; hunk works unchanged from npm; herdr is usable unchanged unless its panes must render outside its own client.
- Every fork is a rebase stream: omp 2–4 releases a day, whiteboard daily previews, herdr and hunk weekly.
- Zhivko creates, commits and pushes forks himself (0003).

## Options
| Option | Pros | Cons |
|---|---|---|
| Fork only what needs a patch, pinned | Forks only where Jarvis must patch; small rebases; the Jarvis repo stays the integration repo | Each patched project is a rebase stream |
| One merged monorepo | One tree, one build | The repo grows from ~50K to over 1.5M lines; Rust, Node 24/pnpm 11 and Bun in one build; each upstream sync is a large merge |
| Fork all five regardless | Everything under his control | Five rebase streams, most carrying no patch |

## Recommendation
Fork only what needs a patch, pinned.

## Choice
Upstreams are consumed as released. A GitHub fork (his account, branch `jarvis` = upstream tag + small patches, git submodule `forks/<name>`) is made only when Jarvis needs a change that their settings and seams can't make. A project built from source without a patch is a submodule pinned to the upstream tag. He forks, tags, adds submodules and commits; agents never write git.

## Reason
"Fork only what needs a patch, pinned".

## Consequences
- Plan 5.5 becomes this policy plus the bump re-check list (plan 5.4).
- The package map gains `forks/`.
- Today no upstream needs a patch: whiteboard is built from source at its tag (0393); herdr, hunk and omp are used as released.
- Amended by 0417: whiteboard is a plain clone in forks/whiteboard until the repository has git again.
- Amended by 0564: git is back; the agent made `forks/whiteboard` a submodule at `v0.2.4`.
