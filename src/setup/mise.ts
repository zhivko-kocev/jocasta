/**
 * Jocasta's mise machine file, `~/.config/mise/conf.d/jocasta.toml` (0476), written by `jocasta setup` when mise is
 * installed. conf.d files load before his own `config.toml`, so his settings and pins win. Exact pins only, never
 * `latest` or auto-update: a bump is an edit here plus `mise lock --global`, re-checked per 0383 (herdr and hunk also
 * pinned in `package.json` and the herdr checks).
 */
export const MISE_FRAGMENT = `# Jocasta's mise pins and settings (0476), written by \`jocasta setup\`; your own config.toml wins on conflicts.
# Changed only through \`jocasta setup\` (it shows the diff first); \`jocasta uninstall\` removes it.
min_version = "2026.10.12"

[settings]
# Nothing about your tools leaves the machine: no versions host, no install tracking.
use_versions_host = false
use_versions_host_track = false
# Shims and tasks never install a tool by themselves.
auto_install = false
# No config is trusted without you, not even under omp's CI=true (0473).
paranoid = ["config"]
# A replaced version stays until you remove it: running herdr panes use it.
upgrade.auto_prune = false

[tools]
herdr = "0.9.5"
hunk = "0.23.0"
gh = "2.92.0"
# The code index behind graph_search (0542, 0562); Jocasta starts only its stdio server.
"github:DeusData/codebase-memory-mcp" = "0.12.1"
`;
