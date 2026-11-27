# 0691 Releases start at v0.1.0; mise installs them

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J10
- Date: 2026-11-26
- Amends: 0051 (a release carries build attestations) and 0049 (Linux and macOS install with mise)

## Question
Jocasta has no tag yet, and ultron builds it from source with an omp pin it reads out of the README. How are releases cut, and how do Linux and macOS users, ultron among them, install one?

## Facts
- `Cargo.toml` and `package.json` say `0.1.0`; `git tag` is empty. `.github/workflows/release.yml` builds `jocasta-linux-x64.tar.gz`, `jocasta-darwin-arm64.tar.gz` and `jocasta-windows-x64.zip` (each with its `.sha256`) on a pushed `v*` tag, commits their checksums at the tag `checksums-<tag>` for the install scripts (0484) and creates the release.
- The archive holds `jocasta`, `config/`, `README.md` and `CHANGELOG.md` at its top; the binary finds `config/` beside itself (`src/shared/install.rs`).
- `mise use -g github:OWNER/REPO` installs the release asset whose name carries the system's OS and CPU words, takes the tag without `v` as the version, and verifies GitHub artifact attestations when the release has them; `mise upgrade` moves to a newer release.
- `actions/attest-build-provenance` (v4.2.2, commit `4d10147`) attests files from a workflow with the `id-token: write` and `attestations: write` permissions; `gh attestation verify <file> -R <repo>` checks one.

## Options
| Option | Pros | Cons |
|---|---|---|
| Tags `v<Cargo version>` pushed by the agent after a green check, attested archives, `mise use -g github:zhivko-kocev/jocasta` on Linux and macOS | One line installs and upgrades; provenance checked by mise; ultron's source build and omp pin go | mise becomes the Unix install's prerequisite |
| Keep the install script as the Unix install | No mise needed | ultron keeps building from source; no attestation check |

## Recommendation
Tags, attestations and mise.

## Choice
His answers of 2026-11-26 and the approved plan:
- A release is the tag `v<Cargo.toml version>`, pushed by the agent once CI is green on `main`; the first is `v0.1.0`. `CHANGELOG.md`'s section for it is headed with that version.
- `release.yml`'s publish job attests the three archives with `actions/attest-build-provenance`, pinned by commit like the other actions.
- The README's and manual's Linux and macOS install is `mise use -g github:zhivko-kocev/jocasta`; upgrades are `mise upgrade`.
- `scripts/install.sh`, `scripts/install.ps1` and the `checksums-<tag>` tag stay (Windows' install, and the Unix install without mise) until the ponytail audit of the next milestone decides on them.

## Reason
A tagged, attested release that mise installs is what ultron and a newcomer both need, with no build on their machine.

## Consequences
- `release.yml`: the publish job gains `id-token: write`, `attestations: write` and the attestation step over `dist/*.zip` and `dist/*.tar.gz`, before the release is created.
- README and manual: the mise line first, the install script as the alternative.
- ultron installs Jocasta through its mise tool list and drops its source build and omp pin.
