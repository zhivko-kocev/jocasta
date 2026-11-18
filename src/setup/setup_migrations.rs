//! The numbered migrations, in the order they apply. A new one takes the next number; numbers are never reused.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::steps::{
	MergedFile, SetupChange, Step, apply_sync, folder_undo, json, keeps_only, mcp_step, merged_step, new_file,
	owned_step, read_text, shown, text_step, write_if_unchanged,
};
use super::{
	HERDR_OMP_EXTENSION, HerdrCheck, RunProgram, SetupDeps, SetupIo, SetupPaths, TOKYO_NIGHT_SCHEME, answered_yes,
	default_herdr_check,
};
use crate::frontier::server::ptt_hold::{STT_TOGGLE_KEY, STT_TOGGLE_SETTING};
use crate::frontier::server::whiteboard::WHITEBOARD_ENV;
use crate::setup::config_file::parse_json_object;
use crate::setup::diff::line_diff;
use crate::setup::markers::{Marker, Undo, fingerprint, markers_dir, read_marker};
use crate::setup::mise::MISE_FRAGMENT;
use crate::setup::omp_settings::{deep_equals, omp_settings_text};
use crate::setup::run_value::{RUN_KEY, RUN_VALUE, starts_companion};
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::{as_object, remove_tree};
use crate::shared::yaml::parse_value;

/// Servers by name.
type Servers = Map<String, Value>;

pub(super) struct PlanContext<'a> {
	pub paths: &'a SetupPaths,
	pub deps: &'a SetupDeps,
	pub notes: &'a mut Vec<String>,
	/// This migration's marker, when it has one.
	pub marker: Option<Marker>,
}

/// A numbered migration (0466); `plan` returns `None` when it doesn't apply here or can't run now (a note says why).
pub(super) struct Migration {
	pub id: u32,
	pub slug: &'static str,
	/// The file name `--refresh` resets it by (0467).
	pub refresh: Option<&'static str>,
	/// The MCP servers it writes, by name: uninstall compares his entries with them (0468).
	pub servers: Option<fn(&SetupPaths) -> Servers>,
	/// The servers an earlier version of it wrote, for a marker from then: uninstall compares his entries with these.
	pub legacy_servers: Option<fn(&SetupPaths) -> Servers>,
	pub plan: fn(&mut PlanContext) -> Result<Option<Step>>,
}

/// A migration's marker name, `<NNNN>-<slug>`.
pub(super) fn marker_name(migration: &Migration) -> String {
	format!("{:04}-{}", migration.id, migration.slug)
}

/// Said before the diff and in place of "Done" while herdr is missing: the plugin can't be linked (0558, 0584).
pub(super) const HERDR_MISSING: &str =
	"herdr isn't installed: install it from https://herdr.dev, then run `jocasta setup` again.";

/// omp's request timeout for the thread's MCP server (0577): a thread's `check` may run for 30 minutes, plus a minute;
/// omp's own default of 30 s would cut it.
const MCP_TIMEOUT_MS: u64 = 1_860_000;
/// omp's request timeout for the boss's MCP server (0484, 0577): a call may outlast its 30-minute bound with his Allow.
const BOSS_MCP_TIMEOUT_MS: u64 = 2_100_000;

/// Said under what `herdr config check` reported when it found issues.
const IGNORED_KEYS_NOTE: &str = "herdr ignores the keys named above: fix them in the shipped template or report them.";

/// The migrations, in the order they apply.
pub(super) static MIGRATIONS: [Migration; 17] = [
	Migration {
		// Everyone's `history_search` (0389, 0603). whiteboard's tools are the boss's only (0607): its entry is in
		// migration 2's file, and migration 17 removes the one this migration wrote before.
		id: 1,
		slug: "mcp",
		refresh: Some("mcp.json"),
		servers: Some(global_servers),
		legacy_servers: Some(user_servers_before_0607),
		plan: plan_mcp,
	},
	Migration {
		// The boss's tools only in its own folder (0392: project-level MCP config), with omp's request timeout past
		// their own bounds (0577); whiteboard's tools for drawing (0393, 0607) on the service's whiteboard state,
		// telemetry off.
		id: 2,
		slug: "boss-mcp",
		refresh: Some("mcp.json"),
		servers: Some(boss_servers),
		legacy_servers: None,
		plan: plan_boss_mcp,
	},
	Migration {
		// The omp settings Jocasta ships (0396–0400, 0447–0465, 0592–0594): the whole file when he has none, else merged
		// key by key; on Windows its fence also names Windows' own folders (0594).
		id: 3,
		slug: "omp-settings",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_omp_settings,
	},
	Migration {
		// herdr's omp integration reports each pane's agent state (0388). One that was there before setup (another
		// install's, his own) isn't setup's to undo (0468).
		id: 4,
		slug: "herdr-integration",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_herdr_integration,
	},
	Migration {
		// Jocasta's own herdr config follows the shipped template (0440). herdr then checks what was written and names
		// each key it ignores (0608).
		id: 5,
		slug: "herdr-config",
		refresh: Some("herdr.toml"),
		servers: None,
		legacy_servers: None,
		plan: plan_herdr_config,
	},
	Migration {
		// Pi's settings.json `frontier`/`pollers` keys moved to jocasta.yaml once (0387).
		id: 6,
		slug: "jocasta-yaml",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_jocasta_yaml,
	},
	Migration {
		// The Tokyo Night scheme for Windows Terminal (0401, 0474).
		id: 7,
		slug: "terminal-scheme",
		refresh: Some("jocasta.json"),
		servers: None,
		legacy_servers: None,
		plan: plan_terminal_scheme,
	},
	Migration {
		// mise's machine file (0476), only when mise is installed.
		id: 8,
		slug: "mise-config",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_mise_config,
	},
	Migration {
		// The mise skill for agents (0476), in omp's user skills.
		id: 9,
		slug: "mise-skill",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_mise_skill,
	},
	Migration {
		// Ctrl+L no longer starts omp's live voice (0451, 0479): `app.live.toggle` unbound; F9, which omp and herdr leave
		// unused, toggles omp's dictation (0556): Right Ctrl sends it to the boss pane through herdr. Both in the file omp
		// reads (`keybindings.yml`, else his `keybindings.yaml`; pi-tui `app-keybindings.ts`).
		id: 10,
		slug: "omp-keybindings",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_keybindings,
	},
	Migration {
		// The advisor's brief (0451, 0479): written when missing, following Jocasta's text while it is still Jocasta's;
		// his own is shown and kept.
		id: 11,
		slug: "omp-watchdog",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_watchdog,
	},
	Migration {
		// Idle language servers stop after 5 minutes (0451, 0479); each pane holds its own.
		id: 12,
		slug: "omp-lsp",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_lsp,
	},
	Migration {
		// No autostart (0557): the logon Run value the companion wrote (0367) goes; he starts `jocasta` himself. Only a
		// value that starts Jocasta's companion launcher: another program's value of that name stays.
		id: 13,
		slug: "autostart-off",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_autostart_off,
	},
	Migration {
		// His own strings omp hides before a request leaves (0593, 0594): the format and an empty list, written when
		// missing and following Jocasta's text while it is still Jocasta's. His own is kept and never shown: it holds his
		// strings.
		id: 15,
		slug: "omp-secrets",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_secrets,
	},
	Migration {
		// Graphiti and its containers are gone (0603): Jocasta's compose file marks an install that still has them.
		id: 16,
		slug: "graphiti-removed",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_graphiti_removed,
	},
	Migration {
		// whiteboard's tools are the boss's only (0607): the entry migration 1 wrote into the user-level `mcp.json` goes,
		// while it still is what setup wrote; the boss's file got its entry from migration 2. One he changed stays and
		// is named.
		id: 17,
		slug: "whiteboard-boss-only",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_whiteboard_boss_only,
	},
	Migration {
		// The `jocasta` herdr plugin is gone (fork plan phase 5): herdr's client draws `today` and the questions itself.
		// Its folder goes, and migration 14's marker with it, so uninstall never looks for the plugin again.
		id: 18,
		slug: "herdr-plugin-gone",
		refresh: None,
		servers: None,
		legacy_servers: None,
		plan: plan_herdr_plugin_gone,
	},
];

/// A stdio MCP server running this Jocasta with `args`.
fn jocasta_server(paths: &SetupPaths, args: &[&str]) -> Map<String, Value> {
	let mut command = paths.jocasta_command.iter().map(|part| part.display().to_string());
	let program = command.next().unwrap_or_else(|| "jocasta".to_owned());
	let args: Vec<String> = command.chain(args.iter().map(|arg| (*arg).to_owned())).collect();
	match json!({"type": "stdio", "command": program, "args": args}) {
		Value::Object(server) => server,
		_ => Map::new(),
	}
}

/// A server with omp's request `timeout` added.
fn with_timeout(mut server: Map<String, Value>, timeout: u64) -> Value {
	server.insert("timeout".to_owned(), json!(timeout));
	Value::Object(server)
}

/// The thread's `jocasta` server in omp's global `mcp.json` (migration 1).
fn global_servers(paths: &SetupPaths) -> Servers {
	let mut servers = Servers::new();
	servers.insert("jocasta".to_owned(), with_timeout(jocasta_server(paths, &["mcp"]), MCP_TIMEOUT_MS));
	servers
}

/// whiteboard's `mcp` server, `None` while whiteboard isn't built. It finds the service's whiteboard server (its
/// Desktop server, instance `stable`) through DEV_REVIEW_HOME.
fn whiteboard_server(paths: &SetupPaths) -> Option<Value> {
	let whiteboard = paths.whiteboard.as_ref()?;
	let mut env = Map::new();
	env.insert("DEV_REVIEW_HOME".to_owned(), json!(paths.data_dir.join("whiteboard").display().to_string()));
	for (name, value) in WHITEBOARD_ENV {
		env.insert(name.to_owned(), json!(value));
	}
	Some(json!({
		"type": "stdio",
		"command": whiteboard.node.display().to_string(),
		"args": [whiteboard.cli.display().to_string(), "mcp"],
		"env": env,
	}))
}

/// The boss's `jocasta --boss` server and whiteboard's (0607), in its folder's `mcp.json` (migration 2).
fn boss_servers(paths: &SetupPaths) -> Servers {
	let mut servers = Servers::new();
	servers.insert("jocasta".to_owned(), with_timeout(jocasta_server(paths, &["mcp", "--boss"]), BOSS_MCP_TIMEOUT_MS));
	if let Some(whiteboard) = whiteboard_server(paths) {
		servers.insert("whiteboard".to_owned(), whiteboard);
	}
	servers
}

/// What migration 1 wrote into the user-level `mcp.json` before 0607: `jocasta` and whiteboard's server. Uninstall
/// and migration 17 recognise an entry of an install that never ran 17 by it.
fn user_servers_before_0607(paths: &SetupPaths) -> Servers {
	let mut servers = global_servers(paths);
	if let Some(whiteboard) = whiteboard_server(paths) {
		servers.insert("whiteboard".to_owned(), whiteboard);
	}
	servers
}

fn asset(paths: &SetupPaths, relative: &[&str]) -> Result<String> {
	let mut path = paths.assets_dir.clone();
	path.extend(relative);
	Ok(fs::read_to_string(path)?)
}

fn plan_mcp(c: &mut PlanContext) -> Result<Option<Step>> {
	let path = c.paths.omp_agent_dir.join("mcp.json");
	mcp_step(&path, &global_servers(c.paths), vec!["jocasta".to_owned()]).map(Some)
}

fn plan_boss_mcp(c: &mut PlanContext) -> Result<Option<Step>> {
	if c.paths.whiteboard.is_none() {
		c.notes.push(
			"whiteboard isn't built: run `mise run build` (or install a release), then `jocasta setup` again."
				.to_owned(),
		);
	}
	let servers = boss_servers(c.paths);
	let path = c.paths.data_dir.join("home").join(".omp").join("mcp.json");
	mcp_step(&path, &servers, servers.keys().cloned().collect()).map(Some)
}

fn plan_omp_settings(c: &mut PlanContext) -> Result<Option<Step>> {
	let text = omp_settings_text(&asset(c.paths, &["omp-config.yml"])?, c.paths.system_folders.as_deref())?;
	let shipped = match parse_value(&text).map_err(Error::other)? {
		Value::Object(object) => object,
		_ => Map::new(),
	};
	merged_step(MergedFile {
		title: "omp settings Jocasta ships",
		new_title: Some("omp settings"),
		kept_title: "omp settings Jocasta ships (yours kept where you changed them)".to_owned(),
		path: &c.paths.omp_agent_dir.join("config.yml"),
		shipped,
		shipped_text: Some(text),
		marker: c.marker.as_ref(),
		keys: false,
	})
	.map(Some)
}

fn plan_herdr_integration(c: &mut PlanContext) -> Result<Option<Step>> {
	let path = c.paths.omp_agent_dir.join("extensions").join(HERDR_OMP_EXTENSION);
	let path_text = shown(&path);
	let claimed = c
		.marker
		.as_ref()
		.is_some_and(|m| m.undo.iter().any(|u| matches!(u, Undo::HerdrIntegration { path } if *path == path_text)));
	let mut step =
		Step::new(&path, "herdr integration install omp", vec![Undo::HerdrIntegration { path: path_text.clone() }]);
	if path.exists() {
		if !claimed {
			step.undo = Vec::new();
		}
		return Ok(Some(step));
	}
	let Some(herdr) = c.deps.herdr.clone() else {
		c.notes.push(HERDR_MISSING.to_owned());
		return Ok(None);
	};
	let run = c.deps.run.clone();
	step.change = Some(SetupChange {
		title: "herdr's omp integration (`herdr integration install omp`)".to_owned(),
		path: path_text.clone(),
		diff: vec![format!("+ {path_text} (written by herdr)")],
		kept: Vec::new(),
		keeps_only: false,
		apply: Box::new(move |_| {
			Box::pin(async move {
				let argv = vec![herdr.display().to_string(), "integration".into(), "install".into(), "omp".into()];
				let done = run(argv).await?;
				if done.code != 0 {
					return Err(Error::user_with_code(
						format!("herdr integration install omp exited {}: {}", done.code, done.output.trim()),
						1,
					));
				}
				Ok(())
			})
		}),
	});
	Ok(Some(step))
}

/// Runs `herdr config check` on `path` (`HERDR_CONFIG_PATH`) and prints what it reports, each unknown-key line (0608).
/// A check that can't run, or finds issues, never stops setup: the file is already written.
async fn print_herdr_check(deps: &SetupDeps, herdr: &Path, path: &Path, io: &dyn SetupIo) {
	let result: Result<HerdrCheck> = match &deps.check_herdr_config {
		Some(check) => check(herdr.to_path_buf(), path.to_path_buf()).await,
		None => default_herdr_check(herdr, path).await,
	};
	let result = match result {
		Ok(result) => result,
		Err(error) => {
			io.print(&format!("herdr config check didn't run: {error}"));
			return;
		}
	};
	io.print(&format!("herdr config check ({}):", path.display()));
	for line in result.output.split('\n') {
		if !line.trim().is_empty() {
			io.print(&format!("  {}", line.trim()));
		}
	}
	if result.code != Some(0) {
		io.print(&format!("  {IGNORED_KEYS_NOTE}"));
	}
}

fn plan_herdr_config(c: &mut PlanContext) -> Result<Option<Step>> {
	let path = c.paths.jocasta_config_dir.join("herdr.toml");
	let title = if path.exists() {
		"Jocasta's herdr config, to the shipped template"
	} else {
		"herdr config for session `jocasta`"
	};
	let wanted = asset(c.paths, &["herdr.toml"])?;
	let mut step = owned_step(title, &path, &wanted, vec![Undo::File { path: shown(&path) }])?;
	let Some(change) = step.change.take() else { return Ok(Some(step)) };
	let Some(herdr) = c.deps.herdr.clone() else {
		c.notes.push(HERDR_MISSING.to_owned());
		step.change = Some(change);
		return Ok(Some(step));
	};
	let deps = c.deps.clone();
	let SetupChange { title, path: shown_path, diff, kept, keeps_only, apply } = change;
	step.change = Some(SetupChange {
		title,
		path: shown_path,
		diff,
		kept,
		keeps_only,
		apply: Box::new(move |io: &dyn SetupIo| {
			Box::pin(async move {
				apply(io).await?;
				print_herdr_check(&deps, &herdr, &path, io).await;
				Ok(())
			})
		}),
	});
	Ok(Some(step))
}

/// `frontier`/`pollers` from Pi's `settings.json` as `jocasta.yaml` (0387); `None` when there's nothing to move.
fn jocasta_yaml(settings_path: &Path) -> Result<Option<String>> {
	let Some(text) = read_text(settings_path)? else { return Ok(None) };
	let settings = parse_json_object(settings_path, &text)?;
	let moved: Map<String, Value> = ["frontier", "pollers"]
		.into_iter()
		.filter_map(|key| settings.get(key).map(|value| (key.to_owned(), value.clone())))
		.collect();
	if moved.is_empty() {
		return Ok(None);
	}
	let yaml = crate::setup::omp_settings::omp_yaml(&Value::Object(moved));
	let yaml = yaml.split('\n').map(str::trim_end).collect::<Vec<_>>().join("\n");
	let yaml = format!("{}\n", yaml.trim_end_matches('\n'));
	Ok(Some(format!(
		"# Jocasta-only settings (0387), moved from settings.json's frontier and pollers keys by `jocasta setup`.\n{yaml}"
	)))
}

fn plan_jocasta_yaml(c: &mut PlanContext) -> Result<Option<Step>> {
	let path = c.paths.jocasta_config_dir.join("jocasta.yaml");
	let mut step = Step::new(&path, "settings.json", Vec::new());
	if path.exists() {
		return Ok(Some(step));
	}
	if let Some(yaml) = jocasta_yaml(&c.paths.jocasta_config_dir.join("settings.json"))? {
		step.change = Some(new_file("Jocasta settings", &path, &yaml));
	}
	Ok(Some(step))
}

fn plan_terminal_scheme(c: &mut PlanContext) -> Result<Option<Step>> {
	let Some(fragments) = &c.paths.terminal_fragments_dir else { return Ok(None) };
	let dir = fragments.join("jocasta");
	let undo = folder_undo(&dir, "jocasta.json", c.marker.as_ref());
	let text = json(&json!({"schemes": [&*TOKYO_NIGHT_SCHEME]}));
	owned_step("Windows Terminal scheme Tokyo Night", &dir.join("jocasta.json"), &text, vec![undo]).map(Some)
}

fn plan_mise_config(c: &mut PlanContext) -> Result<Option<Step>> {
	if c.deps.mise.is_none() {
		return Ok(None);
	}
	let path = c.paths.mise_config_dir.join("conf.d").join("jocasta.toml");
	owned_step(
		"mise's pins and settings for Jocasta's tools",
		&path,
		MISE_FRAGMENT,
		vec![Undo::File { path: shown(&path) }],
	)
	.map(Some)
}

fn plan_mise_skill(c: &mut PlanContext) -> Result<Option<Step>> {
	let dir = c.paths.omp_agent_dir.join("skills").join("mise");
	let undo = folder_undo(&dir, "SKILL.md", c.marker.as_ref());
	owned_step("omp skill mise", &dir.join("SKILL.md"), &asset(c.paths, &["skills", "mise", "SKILL.md"])?, vec![undo])
		.map(Some)
}

fn plan_keybindings(c: &mut PlanContext) -> Result<Option<Step>> {
	let dir = &c.paths.omp_agent_dir;
	let (yml, yaml) = (dir.join("keybindings.yml"), dir.join("keybindings.yaml"));
	if !yml.exists() && !yaml.exists() && dir.join("keybindings.json").exists() {
		c.notes.push(
			"omp's keybindings.json is in its old format: start omp once (it moves it to keybindings.yml), then run `jocasta setup` again."
				.to_owned(),
		);
		return Ok(None);
	}
	let path = if !yml.exists() && yaml.exists() { yaml } else { yml };
	let title = "omp key bindings: Ctrl+L no longer starts live voice; F9 toggles dictation";
	let mut shipped = Map::new();
	shipped.insert("app.live.toggle".to_owned(), json!([]));
	shipped.insert(STT_TOGGLE_SETTING.to_owned(), json!(STT_TOGGLE_KEY));
	merged_step(MergedFile {
		title,
		new_title: None,
		kept_title: format!("{title} (yours kept)"),
		path: &path,
		shipped,
		marker: c.marker.as_ref(),
		shipped_text: None,
		keys: true,
	})
	.map(Some)
}

fn plan_watchdog(c: &mut PlanContext) -> Result<Option<Step>> {
	let path = c.paths.omp_agent_dir.join("WATCHDOG.md");
	let wanted = asset(c.paths, &["WATCHDOG.md"])?;
	let title = "the advisor's brief";
	let shown_to_him = |has: &str| {
		keeps_only(
			format!("{title} (yours kept; Jocasta's differs as shown)"),
			&path,
			line_diff(has, &wanted),
			vec!["yours stays as it is, never overwritten (Jocasta's text is config/WATCHDOG.md)".to_owned()],
		)
	};
	text_step(title, &path, &wanted, c.marker.as_ref(), Some(&shown_to_him)).map(Some)
}

fn plan_lsp(c: &mut PlanContext) -> Result<Option<Step>> {
	let title = "omp language servers: idle ones stop after 5 minutes";
	let mut shipped = Map::new();
	shipped.insert("idleTimeoutMs".to_owned(), json!(300_000));
	merged_step(MergedFile {
		title,
		new_title: None,
		kept_title: format!("{title} (yours kept)"),
		path: &c.paths.omp_agent_dir.join("lsp.json"),
		shipped,
		marker: c.marker.as_ref(),
		shipped_text: None,
		keys: true,
	})
	.map(Some)
}

fn plan_autostart_off(c: &mut PlanContext) -> Result<Option<Step>> {
	let Some(store) = c.deps.run_value.clone() else { return Ok(None) };
	let Some(data) = store.read(RUN_VALUE).filter(|data| starts_companion(data)) else { return Ok(None) };
	let path = format!("{RUN_KEY}\\{RUN_VALUE}");
	Ok(Some(Step {
		path: path.clone(),
		fingerprint: fingerprint(&path),
		undo: Vec::new(),
		shipped: None,
		change: Some(SetupChange {
			title: "Jocasta no longer starts at logon: its Run value goes".to_owned(),
			path: path.clone(),
			diff: vec![format!("- {RUN_VALUE}  {data}")],
			kept: Vec::new(),
			keeps_only: false,
			apply: apply_sync(move || {
				if store.remove(RUN_VALUE) { Ok(()) } else { Err(Error::other(format!("{path} wasn't removed"))) }
			}),
		}),
	}))
}

fn plan_herdr_plugin_gone(c: &mut PlanContext) -> Result<Option<Step>> {
	let dir = c.paths.jocasta_config_dir.join("herdr-plugin");
	let marker = markers_dir(&c.paths.jocasta_config_dir).join("0014-herdr-plugin.json");
	let mut step = Step::new(&dir, fingerprint("herdr-plugin-gone"), Vec::new());
	if !dir.exists() && !marker.exists() {
		return Ok(Some(step));
	}
	let gone = [dir.clone(), marker.clone()];
	step.change = Some(SetupChange {
		title: "the `jocasta` herdr plugin is gone: Jocasta draws `today` and the questions in herdr's own client".to_owned(),
		path: shown(&dir),
		diff: gone.iter().filter(|path| path.exists()).map(|path| format!("- {}", path.display())).collect(),
		kept: vec![
			"a herdr installed apart from Jocasta still lists the plugin it linked; `herdr plugin unlink jocasta` there removes it"
				.to_owned(),
		],
		keeps_only: false,
		apply: Box::new(move |_| {
			Box::pin(async move {
				for path in &gone {
					remove_tree(path)?;
				}
				Ok(())
			})
		}),
	});
	Ok(Some(step))
}

fn plan_secrets(c: &mut PlanContext) -> Result<Option<Step>> {
	text_step(
		"omp's secrets.yml: your own strings to hide, an empty list to fill",
		&c.paths.omp_agent_dir.join("secrets.yml"),
		&asset(c.paths, &["secrets.yml"])?,
		c.marker.as_ref(),
		None,
	)
	.map(Some)
}

/// Takes the containers of the Compose project `file` names down while Docker's engine runs, then removes the file and
/// the build context beside it (0603). Whether the old graph's images and volume go too is asked once; whatever stays is
/// named with the one command that removes it later, which is the whole answer when Docker isn't running.
async fn remove_graphiti(file: PathBuf, run: RunProgram, io: &dyn SetupIo) -> Result<()> {
	let named = match parse_value(&fs::read_to_string(&file)?).map_err(Error::other)? {
		Value::Object(object) => object.get("name").cloned(),
		_ => None,
	};
	let project = match named {
		Some(Value::String(name)) => name,
		Some(Value::Number(number)) => number.to_string(),
		Some(Value::Bool(flag)) => flag.to_string(),
		_ => "jocasta".to_owned(),
	};
	let file_text = file.display().to_string();
	let compose = |args: &[&str]| {
		let mut argv: Vec<String> =
			["docker", "compose", "-f", &file_text].iter().map(|part| (*part).to_owned()).collect();
		argv.extend(args.iter().map(|arg| (*arg).to_owned()));
		let (run, args) = (run.clone(), args.join(" "));
		async move {
			let done = run(argv).await?;
			if done.code != 0 {
				return Err(Error::other(format!(
					"docker compose {args} exited {}: {}",
					done.code,
					done.output.trim()
				)));
			}
			Ok(())
		}
	};
	// `docker info` succeeds only while the engine runs.
	let engine_runs = run(vec!["docker".to_owned(), "info".to_owned()]).await.is_ok_and(|done| done.code == 0);
	let mut everything_gone = false;
	if engine_runs {
		compose(&["down"]).await?;
		let question = "Also remove the old graph's images and volume (Graphiti and FalkorDB)? [y/N] ";
		if answered_yes(io.ask(question).await.as_deref()) {
			compose(&["down", "--rmi", "all", "--volumes"]).await?;
			everything_gone = true;
		}
	}
	remove_tree(&file)?;
	if let Some(dir) = file.parent() {
		remove_tree(&dir.join("containers"))?;
	}
	if !everything_gone {
		io.print(&format!(
			"The old graph's images and volume stay; remove them with: docker compose -p {project} down --rmi all --volumes"
		));
	}
	Ok(())
}

fn plan_graphiti_removed(c: &mut PlanContext) -> Result<Option<Step>> {
	let file = c.paths.jocasta_config_dir.join("compose.yaml");
	if !file.exists() {
		return Ok(None);
	}
	let run = c.deps.run.clone();
	let mut step = Step::new(&file, fingerprint("graphiti-removed"), Vec::new());
	let target = file.clone();
	step.change = Some(SetupChange {
		title: "Graphiti is gone: its containers come down and their compose file is removed".to_owned(),
		path: shown(&file),
		diff: vec![
			format!("- {}", file.display()),
			format!("- {}", c.paths.jocasta_config_dir.join("containers").display()),
		],
		kept: Vec::new(),
		keeps_only: false,
		apply: Box::new(move |io: &dyn SetupIo| Box::pin(remove_graphiti(target, run, io))),
	});
	Ok(Some(step))
}

fn plan_whiteboard_boss_only(c: &mut PlanContext) -> Result<Option<Step>> {
	let paths = c.paths;
	let path = paths.omp_agent_dir.join("mcp.json");
	let mut step = Step::new(&path, fingerprint("whiteboard-boss-only"), Vec::new());
	let Some(text) = read_text(&path)? else { return Ok(Some(step)) };
	let mut config = parse_json_object(&path, &text)?;
	let Some(servers) = config.get("mcpServers").and_then(as_object).cloned() else { return Ok(Some(step)) };
	let Some(has) = servers.get("whiteboard") else { return Ok(Some(step)) };
	// What setup wrote: today's entry, or, with the others there, what migration 1's marker fingerprinted.
	let wrote = read_marker(&paths.jocasta_config_dir, &marker_name(&MIGRATIONS[0]));
	let present: Map<String, Value> = user_servers_before_0607(paths)
		.keys()
		.filter_map(|name| servers.get(name).map(|server| (name.clone(), server.clone())))
		.collect();
	let as_written = whiteboard_server(paths).is_some_and(|server| deep_equals(has, &server))
		|| wrote.is_some_and(|wrote| fingerprint(&Value::Object(present).to_string()) == wrote.fingerprint);
	if !as_written {
		step.change = Some(keeps_only(
			"omp MCP server \"whiteboard\" in your own MCP config stays: it isn't what Jocasta wrote, and only the boss mounts whiteboard now".to_owned(),
			&path,
			Vec::new(),
			vec![format!(
				"\"whiteboard\" in {} is yours; remove it by hand if you don't want it in every omp session",
				path.display()
			)],
		));
		return Ok(Some(step));
	}
	let mut rest = servers;
	rest.shift_remove("whiteboard");
	config.insert("mcpServers".to_owned(), Value::Object(rest));
	let after = json(&Value::Object(config));
	step.change = Some(SetupChange {
		title: "whiteboard's tools are the boss's only: its omp MCP server \"whiteboard\" leaves your MCP config"
			.to_owned(),
		path: shown(&path),
		diff: line_diff(&text, &after),
		kept: Vec::new(),
		keeps_only: false,
		apply: apply_sync(move || write_if_unchanged(&path, Some(&text), &after)),
	});
	Ok(Some(step))
}
