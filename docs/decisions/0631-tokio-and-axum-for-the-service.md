# 0631 tokio and axum for the service's HTTP and websockets

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
Which async runtime and HTTP server does the Rust service use, for its routes, the state feed and the relay's websocket rooms?

## Facts
Verified 2026-11-15. Crate counts are transitive, on Windows, measured with `cargo tree` (`docs/research/rust-port.md`).
- **Requests the service holds open:**
  - the pane check, up to 15 minutes;
  - questions, until he answers.
- **Cancel.** The pane check's cancel (0617) needs the server to see the client disconnect:
  - hyper and axum drop a handler when its client goes away;
  - tiny_http can't see a disconnect.
- **tokio is in either way:** rmcp (0632) runs only on tokio.
- **What each layer gives:**
  - hyper is the HTTP engine. Routing, body reading and the websocket upgrade are our own code. That is today's shape: `Bun.serve({ fetch })` in `routes.ts` does a host check, then takes the first route that matches.
  - axum is a framework on hyper, by the tokio team. It adds a router, typed request bodies and websockets.
- **Crate counts:**
  - tokio, rmcp, serde_json, hyper, tokio-tungstenite and ureq: 106.
  - With axum (and its `ws` feature) in place of hyper: 118.
  - With reqwest as well: 145.
- **axum's API** is pre-1.0 and has broken about yearly: 0.7 in 2023, 0.8 in 2025. atuin uses axum.

## Options
First form:

| Option | For | Against |
|---|---|---|
| tokio and hyper, routes matched by hand | the fewest crates that see a disconnect | more code of our own |
| tokio and axum with `ws` | routing, extractors and websockets ready-made | four times the crates, pre-1.0 |
| std threads and tiny_http | no async | a thread per held request, no disconnect detection, last released 2022 |

He asked what tokio and axum are. The second form explained both, with code for each, and offered hyper (106) or axum (118, +12).

## Recommendation
hyper.

## Choice
- **tokio** is the runtime.
- **axum 0.8 with `ws`** serves the HTTP routes, the state-feed websocket and the relay's websocket rooms on their fixed port.

## Reason
His answers:
- First form: "i dont understand what does the first and the second option offer what is tokio axum".
- After the explanation: `axum`.

## Consequences
- This differs from the recommendation (hyper).
- **axum's next breaking release** is a bump, re-checked like any pinned version.
