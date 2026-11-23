# 0672 Phone check-in through ntfy: content-free on a public server, Allow only on his own

- Status: v1
- Kind: product
- Phase/slice: HUD and orchestration round, after the fork build
- Date: 2026-11-23

## Question
He ticked "Phone check-in: NEEDS YOU pushed through ntfy, with Allow/Deny from the phone" (omp #436, 23 reactions). Which service, and may the phone answer an Allow?

## Facts
Verified 2026-11-23 (docs.ntfy.sh): ntfy publishes by an HTTP POST to `<server>/<topic>` with no sign-up; the topic is the password; its free app runs on Android and iOS; a notification may carry `http` action buttons; a server can require an access token to read or write a topic. A button that posts to a second topic, which the service reads as a JSON stream, answers without opening a port. On the public ntfy.sh server the text passes through ntfy's servers; ntfy can be self-hosted. No new crate: `reqwest` is in the tree.

Corrected the same day, before the build: a button's URL and token ride inside the notification, so whoever can read the main topic (the server's operator, or anyone who learns the topic name) can answer an Allow; a separate reply topic adds no protection. NEEDS YOU rows carry thread titles and project names, client projects' among them.

## Options
- **ntfy with Allow from the phone:** NEEDS YOU rows and finished threads are pushed; an Allow's buttons carry a one-time token for that question. Off until he sets a topic.
- **ntfy, notifications only.**
- **A Telegram bot.**

## Recommendation
ntfy with Allow from the phone.

## Choice
His first answer of 2026-11-23 was ntfy with full text and Allow on ntfy.sh. After the correction above, his answer the same day:
- **Off until configured.** His `jocasta.yaml` names the server; Jocasta generates the topic names (random, never typed) and shows them for his phone app.
- **On a public server (ntfy.sh by default):** content-free pushes only, the kind and count ("Jocasta: 2 need you", "Jocasta: a thread finished"), no titles, projects or commands, and no Allow buttons.
- **On a server he names as his own, with an ntfy access token:** the full text, and Allow and Deny buttons that post a random one-time token to the reply topic; the service accepts a token once, only while that question is open, and only from the reply topic. The terminal's typed code still works.
- New NEEDS YOU rows and finished threads are pushed, once each. Quiet hours (0298) hold the pushes but not an Allow.
- The manual states the trust boundary: whoever reads the topic sees the pushes, and on his own server can answer an Allow.

## Reason
He works away from the laptop while threads run; an Allow waiting for him at the desk stalls them.

## Consequences
- Amends 0558 (an Allow is answered by the typed code or, on his own server, the phone's one-time token) and 0552 (notifications also reach the phone).
- Nothing about a thread leaves the machine unless he names his own server.
