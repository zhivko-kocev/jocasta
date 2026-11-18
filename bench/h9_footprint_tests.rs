//! Ported from the H9 case of `test/bench/scripts.test.ts` and `test/bench/build-and-threads.test.ts`.

use std::fs;

use jocasta::shared::testing::temp_dir;

use super::*;

fn strings(items: &[&str]) -> Vec<String> {
	items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn reg_export_lines_continuations_diffs_and_named_leftovers() {
	let text = "\u{feff}Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\Acme]\r\n\"Path\"=\"C:\\\\acme\"\r\n\"List\"=hex:01,02,\\\r\n  03\r\n";
	assert_eq!(
		parse_reg_export(text),
		strings(&[
			"[HKEY_CURRENT_USER\\Software\\Acme]",
			"[HKEY_CURRENT_USER\\Software\\Acme]\\\"Path\"=\"C:\\\\acme\"",
			"[HKEY_CURRENT_USER\\Software\\Acme]\\\"List\"=hex:01,02,03",
		])
	);
	assert_eq!(
		diff_sets(&strings(&["a", "b"]), &strings(&["b", "c"])),
		Diff { added: strings(&["c"]), removed: strings(&["a"]) }
	);
	assert_eq!(
		named(&strings(&["C:\\x\\jocasta\\a", "C:\\y\\other", "HKCU\\Run\\\"Jocasta\""])),
		strings(&["C:\\x\\jocasta\\a", "HKCU\\Run\\\"Jocasta\""])
	);
	// Setup's mise files name mise; a word that only contains it doesn't (0468).
	assert_eq!(
		named(&strings(&[
			"C:\\Users\\t\\.config\\mise\\conf.d\\jocasta.toml",
			"C:\\Users\\t\\.config\\mise\\config.p-1.override.toml",
			"C:\\Users\\t\\AppData\\Local\\acme\\promise\\x.js",
		])),
		strings(&[
			"C:\\Users\\t\\.config\\mise\\conf.d\\jocasta.toml",
			"C:\\Users\\t\\.config\\mise\\config.p-1.override.toml",
		])
	);
}

#[test]
fn the_jocasta_on_path_else_the_install_folders_jocasta_exe_else_none() {
	let on_path = temp_dir("jocasta-bench-build");
	let install_dir = temp_dir("jocasta-bench-build");
	let local = temp_dir("jocasta-bench-build");
	let exe = if cfg!(windows) { "jocasta.exe" } else { "jocasta" };
	let text = |path: &Path| path.to_string_lossy().into_owned();
	let vars = |pairs: &[(&str, String)]| -> HashMap<String, String> {
		pairs.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect()
	};
	// PATH predates the install: JOCASTA_INSTALL_DIR's exe, else %LOCALAPPDATA%\Programs\jocasta's.
	fs::write(install_dir.path().join("jocasta.exe"), "").unwrap();
	assert_eq!(
		installed_jocasta(&vars(&[
			("PATH", text(on_path.path())),
			("JOCASTA_INSTALL_DIR", text(install_dir.path())),
			("LOCALAPPDATA", text(local.path())),
		])),
		Some(install_dir.path().join("jocasta.exe"))
	);
	assert_eq!(installed_jocasta(&vars(&[("PATH", text(on_path.path())), ("LOCALAPPDATA", text(local.path()))])), None);
	fs::create_dir_all(local.path().join("Programs").join("jocasta")).unwrap();
	fs::write(local.path().join("Programs").join("jocasta").join("jocasta.exe"), "").unwrap();
	assert_eq!(
		installed_jocasta(&vars(&[("PATH", text(on_path.path())), ("LOCALAPPDATA", text(local.path()))])),
		Some(local.path().join("Programs").join("jocasta").join("jocasta.exe"))
	);
	// On PATH, that one wins.
	fs::write(on_path.path().join(exe), "").unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(on_path.path().join(exe), fs::Permissions::from_mode(0o755)).unwrap();
	}
	// The extension comes from PATHEXT (`.EXE`), so the name matches without case on Windows.
	let found = installed_jocasta(&vars(&[
		("PATH", text(on_path.path())),
		("JOCASTA_INSTALL_DIR", text(install_dir.path())),
		("LOCALAPPDATA", text(local.path())),
	]))
	.map(|path| text(&path));
	assert!(found.is_some_and(|found| found.eq_ignore_ascii_case(&text(&on_path.path().join(exe)))));
}
