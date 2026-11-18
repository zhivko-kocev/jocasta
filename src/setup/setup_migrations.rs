//! The numbered migrations, in the order they apply. A new one takes the next number; numbers are never reused.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value, json};

use super::snapshot::plan_snapshot;
use super::steps::{SetupChange, Step, folder_undo, json, mcp_step, owned_step, shown};
use super::{HerdrCheck, SetupDeps, SetupIo, SetupPaths, TOKYO_NIGHT_SCHEME, default_herdr_check};
use crate::frontier::server::whiteboard::WHITEBOARD_ENV;
use crate::setup::markers::{Marker, Undo, fingerprint, markers_dir};
use crate::setup::mise::MISE_FRAGMENT;
use crate::shared::errors::Result;
use crate::shared::fsx::remove_tree;

/// Servers by name.
pub(super) type Servers = Map<String, Value>;

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
	pub plan: fn(&mut PlanContext) -> Result<Option<Step>>,
}

/// A migration's marker name, `<NNNN>-<slug>`.
pub(super) fn marker_name(migration: &Migration) -> String {
	format!("{:04}-{}", migration.id, migration.slug)
}

/// Said before the diff and in place of "Done" while herdr is missing (0558, 0584).
pub(super) const HERDR_MISSING: &str =
	"herdr isn't installed: install it from https://herdr.dev, then run `jocasta setup` again.";

/// omp's request timeout for the thread's MCP server (0577): a thread's `check` may run for 30 minutes, plus a minute;
/// omp's own default of 30 s would cut it.
pub(super) const MCP_TIMEOUT_MS: u64 = 1_860_000;
/// omp's request timeout for the boss's MCP server (0484, 0577): a call may outlast its 30-minute bound with his Allow.
const BOSS_MCP_TIMEOUT_MS: u64 = 2_100_000;

/// Said under what `herdr config check` reported when it found issues.
const IGNORED_KEYS_NOTE: &str = "herdr ignores the keys named above: fix them in the shipped template or report them.";

/// The migrations, in the order they apply. Numbers 1, 3, 4, 6, 9–13 and 15–17 were the merges into omp's files and
/// the clean-ups of earlier versions: the omp snapshot (19) replaced them, and their markers are ignored.
pub(super) static MIGRATIONS: [Migration; 6] = [
	Migration {
		// The boss's tools only in its own folder (0392: project-level MCP config), with omp's request timeout past
		// their own bounds (0577); whiteboard's tools for drawing (0393, 0607) on the service's whiteboard state,
		// telemetry off.
		id: 2,
		slug: "boss-mcp",
		refresh: Some("mcp.json"),
		servers: Some(boss_servers),
		plan: plan_boss_mcp,
	},
	Migration {
		// Jocasta's own herdr config follows the shipped template (0440). herdr then checks what was written and names
		// each key it ignores (0608).
		id: 5,
		slug: "herdr-config",
		refresh: Some("herdr.toml"),
		servers: None,
		plan: plan_herdr_config,
	},
	Migration {
		// The Tokyo Night scheme for Windows Terminal (0401, 0474).
		id: 7,
		slug: "terminal-scheme",
		refresh: Some("jocasta.json"),
		servers: None,
		plan: plan_terminal_scheme,
	},
	Migration {
		// mise's machine file (0476), only when mise is installed.
		id: 8,
		slug: "mise-config",
		refresh: None,
		servers: None,
		plan: plan_mise_config,
	},
	Migration {
		// The `jocasta` herdr plugin is gone (fork plan phase 5): herdr's client draws `today` and the questions itself.
		// Its folder goes, and migration 14's marker with it, so uninstall never looks for the plugin again.
		id: 18,
		slug: "herdr-plugin-gone",
		refresh: None,
		servers: None,
		plan: plan_herdr_plugin_gone,
	},
	Migration {
		// Jocasta's omp snapshot (fork plan phase 6): `config/omp/`, `mcp.json` and herdr's omp hook over omp's own
		// files, each replaced one kept first; `secrets.yml` only when missing.
		id: 19,
		slug: "omp-snapshot",
		refresh: None,
		servers: None,
		plan: plan_snapshot,
	},
];

/// A stdio MCP server running this Jocasta with `args`.
pub(super) fn jocasta_server(paths: &SetupPaths, args: &[&str]) -> Map<String, Value> {
	let mut command = paths.jocasta_command.iter().map(|part| part.display().to_string());
	let program = command.next().unwrap_or_else(|| "jocasta".to_owned());
	let args: Vec<String> = command.chain(args.iter().map(|arg| (*arg).to_owned())).collect();
	match json!({"type": "stdio", "command": program, "args": args}) {
		Value::Object(server) => server,
		_ => Map::new(),
	}
}

/// A server with omp's request `timeout` added.
pub(super) fn with_timeout(mut server: Map<String, Value>, timeout: u64) -> Value {
	server.insert("timeout".to_owned(), json!(timeout));
	Value::Object(server)
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

/// The shipped file `relative` under `config/`.
pub(super) fn asset(paths: &SetupPaths, relative: &[&str]) -> Result<String> {
	let mut path = paths.assets_dir.clone();
	path.extend(relative);
	Ok(fs::read_to_string(path)?)
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
	let SetupChange { title, path: shown_path, diff, kept, apply } = change;
	step.change = Some(SetupChange {
		title,
		path: shown_path,
		diff,
		kept,
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
