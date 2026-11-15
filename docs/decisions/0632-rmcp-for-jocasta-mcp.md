# 0632 rmcp serves `jocasta mcp`

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
Is the Rust `jocasta mcp` hand-written over stdio, as today, or built on the official SDK?

## Facts
Verified 2026-11-15.
- **Today:** 492 lines of hand-written JSON-RPC over stdio, covering initialize, tools/list and tools/call.
- **rmcp 3.10.0** is the official Rust SDK. With `server` and `transport-io` it brings 68 transitive crates, among them tokio and schemars.
- **rmcp's API moves fast.** Majors 1 to 3 came between March and July 2026, then about 20 releases since August.
- **Four rmcp advisories in 2026,** all in its HTTP and OAuth transports and all fixed by 3.10.0.
- **Protocol:** it implements the 2026-07-28 version of the spec.

## Options
| Option | For | Against |
|---|---|---|
| Hand-written over stdio with serde_json, as today | no SDK churn; a small protocol surface | new protocol versions are added by hand |
| rmcp with `server` and `transport-io` | follows the spec | 68 crates, a fast-moving API, schemars |

## Recommendation
Hand-written.

## Choice
`jocasta mcp` (and `--boss`) is built on rmcp with `server` and `transport-io` (stdio).

## Reason
His answer: `rmcp`.

## Consequences
- This differs from the recommendation.
- **tokio comes with rmcp,** and it is the runtime of 0631.
- **Versions.** The 7-day rule (0627) takes the newest rmcp release that is at least 7 days old. An rmcp bump re-checks the MCP seam in plan section 5.4.
