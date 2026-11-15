# 0629 One Cargo package, with each Rust module beside the TypeScript file it replaces

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
How is the Rust code laid out?

## Facts
Verified 2026-11-15.
- **One binary, parts that change together.** Jocasta is one `jocasta` binary. Its service, MCP server and CLI share paths, config, storage and the herdr client.
- **herdr's layout** (its `Cargo.toml`, read today): the whole product is one package with many modules under `src/`. Its only second crate is the vendored terminal emulator, `crates/ghostty-vt`, a separate library.
- **What a workspace buys:** caching crates that rarely change. Here few would stay unchanged.
- **AGENTS.md's package map** follows `src/`.

## Options
| Option | For | Against |
|---|---|---|
| One package, one binary; `Cargo.toml` at the root, each Rust module in `src/` beside the TypeScript file it replaces | the package map stays true; each module is reviewed against the file it replaces | two languages in one tree until the cutover |
| A workspace with one crate per part under `crates/` | boundaries the compiler enforces, smaller compile units | five manifests, a new package map, little compile gain |
| One package in `rust/`, moved to the root at the cutover | the trees stay apart | one large move at the end |

## Recommendation
One package beside the TypeScript files. He asked which was better; the answer was this option, with the facts above.

## Choice
- **One Cargo package named `jocasta`.** `Cargo.toml` and `Cargo.lock` are at the repo root.
- **Each Rust module sits in `src/` beside the TypeScript file it replaces,** for example `src/frontier/server/herdr.rs` next to `herdr.ts`.
- **After the cutover,** the only TypeScript left in `src/` is the canvas page.

## Reason
His question: "what is better here the second one or?". After the answer: `one-package-beside`.

## Consequences
- This record amends 0038 (the repo's layout gains `Cargo.toml` at the root and Rust modules in `src/`).
- **Second binary.** 0636 adds the bench's binary to the same package.
- **`target/` is ignored by git.**
- **AGENTS.md's package map** gains `Cargo.toml` when the first slice lands.
