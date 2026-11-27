# Contributing to Jocasta

Thanks for looking. Jocasta is small enough that one person decides its shape, and open so that anyone can use it, read why it works the way it does, and help.

## Propose something

- **A bug:** open an issue with the [bug form](https://github.com/zhivko-kocev/jocasta/issues/new?template=bug.yml). It asks for `jocasta --version`, your OS and terminal, and what you saw.
- **An idea:** open an issue with the [idea form](https://github.com/zhivko-kocev/jocasta/issues/new?template=idea.yml). Say what you were trying to do and where Jocasta got in the way; a proposed design is welcome but not needed. Accepted ideas become a decision record (below), and the issue links it (0018).
- **A fix:** small pull requests are welcome directly: one change, its test, and the docs it touches. For anything larger, or anything you can see or keep (a command, a flag, an output text, a file format, a default, a key), open an issue first, so the decision is made before you write the code.

## Set up

You need [mise](https://mise.jdx.dev). It installs the pinned toolchain (Rust with clippy and rustfmt, Bun, cargo-deny, cargo-machete, cargo-nextest) from `mise.toml` and `mise.lock`.

```sh
mise trust mise.toml                         # once per checkout, and again after mise.toml changes
mise install --locked
mise exec -- bun install --frozen-lockfile   # the packages of the Bun checks
mise run check                               # everything CI runs (below)
mise run build                               # release build into dist/
```

`mise run check` runs the TypeScript checks (typecheck, lint, knip and the Bun tests of the docs, config and install scripts) and the Rust ones (rustfmt, clippy with the ceilings in `clippy.toml`, the tests through cargo-nextest, cargo-deny and cargo-machete). `mise run quick` is the seconds-long formatting and type check. To try your build, set `JOCASTA_INSTALL_FROM` to `dist` and run `scripts/install.sh` (Linux, macOS) or `scripts/install.ps1` (Windows).

When you run your build by hand, point it at sandbox folders so it never touches your own Jocasta and omp: set `JOCASTA_DATA_DIR` and `JOCASTA_CODING_AGENT_DIR` to empty folders first. The [manual's development section](https://github.com/zhivko-kocev/jocasta/blob/main/docs/manual.md#development) has the opt-in smokes, the evals and the build layout.

Where the code lives is mapped in [AGENTS.md](AGENTS.md) (section "Package map"); unit tests sit beside their module as `<module>_tests.rs`.

## How decisions are made

Every behaviour you can see or keep was decided on purpose, and the reason is written down in [`docs/decisions/`](docs/decisions/): one record per decision, numbered, with an index in [`docs/decisions/README.md`](docs/decisions/README.md). [AGENTS.md](AGENTS.md) holds the full rules; in short:

1. **Ask first.** Features, commands, flags, output text, errors, file formats, paths, environment variables, libraries, defaults and key bindings are the maintainer's decisions (0005). Fixes that restore decided behaviour, and details that carry out a decided record (texts, labels, small defaults), are the builder's (0363).
2. **Record before code.** Each decision becomes a record before the code that carries it is written (0002), in the template of [record 0031](docs/decisions/0031-record-template-and-index.md): Question, Facts, Options with pros and cons, Recommendation, Choice, Reason, Consequences. Add its row to the index.
3. **Build exactly that.** The change does what its records say, nothing from a later slice "while you're at it". If something is unclear, stop and ask in the issue rather than guess (0020).
4. **Change a decision in place.** A record that changes is edited, not replaced (0014); say why in the commit message, and add the new record's number to the old one's "Amended by" column in the index.

A pull request that changes behaviour without a record will be asked for one; a pure fix needs none.

## Rules every change keeps

- **Placeholder names only.** The repo is public (0035): examples, tests, fixtures and docs use `acme`, `client-a`, `example-org`, never a real client, employer or person. Before every push the maintainer runs a private-names scan over the whole tree; a match blocks the push.
- **Windows keeps working.** Jocasta's daily use is on Windows 11, and CI runs `mise run check` on Windows, Ubuntu and macOS for every push and pull request. Think about paths (`\` and drive letters), `pwsh` instead of `sh`, CRLF files and process handling; if you can't test on Windows, say so in the pull request and let CI tell.
- **Docs change with code.** A change you can see updates `README.md` and `docs/manual.md`; every change that lands gets a line in `CHANGELOG.md` under the unreleased version.
- **No new dependency without asking.** Crates must be at least 7 days old (`.cargo/config.toml`) and pass `cargo deny`; Bun packages follow the same rule (`bunfig.toml`).
- **No secrets.** No tokens, keys or personal paths in code, tests or logs you paste; Jocasta's own secrets stay out of logs and toasts, and a change must keep it that way.

## Releases

The version is `version` in `Cargo.toml`. A maintainer pushes a tag `v<version>` once CI is green on `main`; `.github/workflows/release.yml` then tests and builds each target on its own OS, publishes `jocasta-linux-x64.tar.gz`, `jocasta-darwin-arm64.tar.gz` and `jocasta-windows-x64.zip` with their `.sha256` files, attests them with GitHub's build provenance, and pushes a `checksums-v<version>` tag that the install scripts check downloads against.

Users get a release with `mise use -g github:zhivko-kocev/jocasta` on Linux and macOS (mise verifies the attestation) or the PowerShell line in the README on Windows. To check an archive by hand: `gh attestation verify jocasta-linux-x64.tar.gz -R zhivko-kocev/jocasta`.

## License

By contributing you agree that your work is released under the [MIT License](LICENSE) of this repository.
