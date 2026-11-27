# 0712 Submit with a verdict; one page for threads and pull requests

- Status: v1 (service, page order and the read-only verdict section built 2026-11-27; the window's verdict buttons and `Show its code` are not built yet)
- Kind: product
- Phase/slice: ultron 0.3 iteration, M2 R6
- Date: 2026-11-26
- Amends: 0680 (a review ends in one review with a verdict; Publish stays per card), 0678 (the thread and pull-request pages share their sections and order), 0721 (the verdict box, `Show its code`)

## Question
A pull-request page publishes card by card and never casts a vote, and a thread's page is built differently from a pull request's. How does a review end, and what does each page show?

## Facts
- GitHub takes one review with an event, a body and line comments in one call (`POST repos/<o>/<r>/pulls/<n>/reviews`); `REQUEST_CHANGES` and `COMMENT` need a body. A comment on a line outside the diff makes the whole call fail (422).
- Azure DevOps has no review object: comments are threads, the vote is `az repos pr set-vote` (`approve`, `wait-for-author`, …).
- Both pages already have facts, works, the system view (0710), the cards, the changed files and the diff; each also draws a "Where it stands" flow of its phases.

## Options
| Option | Pros | Cons |
|---|---|---|
| A final Submit with Approve / Request changes / Comment, carrying his summary and every kept card; the same sections on both pages | The author gets one review with a verdict; he learns one page | Per-platform code for the verdict |
| Per-card Publish only | Built | No verdict, no summary; many notifications for the author |

## Recommendation
Submit with a verdict, one page.

## Choice
His answers of 2026-11-26 and the approved plan:
- New action `submit {verdict: approve | request_changes | comment}`. GitHub: `gh api repos/<o>/<r>/pulls/<n>/reviews` with `event` (`APPROVE`, `REQUEST_CHANGES`, `COMMENT`), `commit_id` the head, `body` his summary and `comments` every kept (verdict not `skip`), unpublished card with a file and a line (`path`, `line`, `side: RIGHT`, its text); a kept card without a file joins the body. Azure DevOps: the existing per-card publish for each such card, the summary as a comment on the pull request, then `az repos pr set-vote` with `approve` or `wait-for-author` (none for Comment). Per-card Publish stays.
- Built details: a review is submitted once (`submit: this review was submitted already.`); Request changes and Comment need a summary or a card to send (`submit: write a summary first; GitHub needs one for this verdict.`); the cards it sent count as published with the review's link; the page shows `Submitted: <verdict>` with the link instead of the buttons. A GitHub 422 says the CLI's last line and that a comment outside the diff can be published on its own (Publish falls back to a pull-request comment); nothing is stored.
- Thread pages (`thread_brief.rs`) and pull-request pages are built from the same sections in this order: facts, works, system, cards, then (pull requests) the verdict box with his summary and the three buttons, the changed files (map), and the diff. The "Where it stands" flows go: the facts already say where the thread or the review stands. Merge / Open PR / Send back stay on thread pages.
- The diff stays closed behind `Show code (<n> files, +a −b)`. Each card whose file and line fall in a hunk of the diff gets `Show its code`, which opens that hunk under the card, drawn as the diff section draws it.

## Reason
The author gets his verdict and his words in one review; he reads both kinds of page the same way, top to bottom, code last.

## Consequences
- `pr_actions.rs`: `submit` (`submit_github`, `submit_azure`); `review.json` gains `submitted {verdict, url, at_ms}`.
- `model.rs`: `Section::Verdict {summary, submitted}`; `pr_review.rs` and `thread_brief.rs` lose their flows and share the order.
- `view/`: the verdict box, `Show its code`.
