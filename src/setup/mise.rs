//! Jocasta's mise machine file, `~/.config/mise/conf.d/jocasta.toml` (0476), written by `jocasta setup` when mise is
//! installed. conf.d files load before his own `config.toml`, so his settings and pins win. Exact pins only, never
//! `latest` or auto-update: a bump is an edit here plus `mise lock --global`, re-checked per 0383. herdr isn't among
//! them: it is built into `jocasta` (fork plan phase 4).

pub const MISE_FRAGMENT: &str =
	"# Jocasta's mise pins and settings (0476), written by `jocasta setup`; your own config.toml wins on conflicts.
# Changed only through `jocasta setup` (it shows the diff first); `jocasta uninstall` removes it.
min_version = \"2026.10.12\"

[settings]
# Nothing about your tools leaves the machine: no versions host, no install tracking.
use_versions_host = false
use_versions_host_track = false
# Shims and tasks never install a tool by themselves.
auto_install = false
# No config is trusted without you, not even under omp's CI=true (0473).
paranoid = [\"config\"]
# A replaced version stays until you remove it: running panes use it.
upgrade.auto_prune = false

[tools]
gh = \"2.92.0\"
";

#[cfg(test)]
mod tests {
	use super::*;

	/// The TypeScript's check, `Bun.TOML.parse(MISE_FRAGMENT)` equal to the expected tables, as its lines (no TOML crate).
	#[test]
	fn the_fragment_holds_exact_pins_and_the_private_paranoid_settings() {
		let code: Vec<&str> = MISE_FRAGMENT.lines().filter(|line| !line.starts_with('#') && !line.is_empty()).collect();
		assert_eq!(
			code,
			[
				"min_version = \"2026.10.12\"",
				"[settings]",
				"use_versions_host = false",
				"use_versions_host_track = false",
				"auto_install = false",
				"paranoid = [\"config\"]",
				"upgrade.auto_prune = false",
				"[tools]",
				"gh = \"2.92.0\"",
			]
		);
		assert!(MISE_FRAGMENT.starts_with("# Jocasta's mise pins and settings (0476), written by `jocasta setup`;"));
		assert!(MISE_FRAGMENT.ends_with("gh = \"2.92.0\"\n"));
	}
}
