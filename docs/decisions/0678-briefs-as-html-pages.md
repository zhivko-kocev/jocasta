# 0678 Briefs: animated HTML pages served on loopback

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Retires: the lesson `he-posts-pr-comments-himself.md`
- Date: 2026-11-24

## Question
What replaces canvas drawings and the hunk review for showing him a thread, a PR, his morning and his week?

## Facts
- The canvas (whiteboard) outlived Jocasta's Stop by hours (a 41 MB `node.exe`); hunk is a separate daemon and CLI.
- `page_routes.rs:233–265` already gives a launch code and an HttpOnly `SameSite=Strict` cookie for pages the service serves.
- The lesson `he-posts-pr-comments-himself.md` says the agent never posts PR comments.

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep canvas and hunk | Nothing to build | Node process, a second daemon, drawings the agent makes by hand |
| HTML briefs by code from a typed model, vanilla JS/CSS/SVG, the service serves them | No dependency; every card machine-checked; nothing runs after Stop | A page engine to build and harden |
| Terminal-only briefs | No browser | No animation, poor diffs |

## Recommendation
Briefs by code.

## Choice
His answer of 2026-11-24:
- `src/frontier/brief/` holds `model.rs`, `layout.rs`, `store.rs`, `routes.rs` and `page/` (`brief.html`, `brief.js`, `brief.css`, embedded; no framework, no external URL). Briefs are stored at `<data>/briefs/<id>.json`; ids `thread-<tid>`, `pr-<project id>-<n>`, `morning-<yyyy-mm-dd>`, `weekly-<yyyy>-W<ww>`, `show-<n>`.
- A `Brief` has `id, kind, title, subtitle, generated_at_ms` and sections of type `text`, `facts`, `checks`, `flow`, `map`, `diff`, `cards`, `list`. A `Card` has `id, severity (P0|P1|P2), file, line, quote, text, verdict (post|question|skip), proof {sha, ok}, edited, published {url, at_ms}`.
- Routes: `GET /brief/<id>`, `/brief/<id>.json`, `/brief/assets/brief.js|brief.css`, `POST /brief/<id>/act`, under the page cookie rule. Every response carries `content-security-policy: default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`; a `Host` other than `127.0.0.1:<port>` or `localhost:<port>` gets 403; `POST` needs the same `Origin` and `content-type: application/json`. `brief.js` writes text only through `textContent` and sets `href` only for `http:`/`https:` with `rel="noopener noreferrer"`.
- A `flow` lays out by layer and order and animates (nodes fade in by layer, edges draw themselves, `active` pulses, `failed` is red, Play/Prev/Next); motion is CSS and off under `prefers-reduced-motion`; an action's response is the whole brief, no polling.
- Review notes replace hunk: the thread tool `review_note {file, line, quote, severity, text, verdict?}` is accepted only when the lines at `line` in `git show <head>:<file>` contain `quote` (whitespace collapsed); a mismatch errors with the five real lines; notes append to `<thread dir>/review.json`. `review_prompt` tells the reviewer to hand each finding in through `review_note`; `ReviewFlow::found` counts notes and blocking notes (P0, P1) from it. `ReviewProof` and its helpers move to `review_proof.rs`.
- The thread brief is built by code in `ReviewFlow::ready` (facts, flow of phases, checks, cards, map, diff capped at 2,000 lines); its actions `finish` (merge or pr) and `send` go through the existing Allow gate and code paths.
- The PR review brief edits comments in place, with a one-shot `@smol` helper to reword, and publishes comments only (0680); his click on Publish is the post.

## Reason
Every card is machine-checked; his click publishes.

## Consequences
- Retires the lesson `he-posts-pr-comments-himself.md`: Jocasta publishes a comment when he clicks Publish on a checked card; no vote is ever cast.
- Hunk is removed with whiteboard (0679).
- Briefs open at `http://127.0.0.1:<port>/brief/<id>?t=<fresh launch code>` through the `open` callback.
