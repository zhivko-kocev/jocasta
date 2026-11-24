//! The numbered migrations, in the order they apply. A new one takes the next number; numbers are never reused.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::snapshot::plan_snapshot;
use super::steps::{SetupChange, Step, folder_undo, json, owned_step, shown};
use super::{
	HerdrCheck, RunOutput, RunProgram, SetupDeps, SetupIo, SetupPaths, TOKYO_NIGHT_SCHEME, answered_yes,
	default_herdr_check,
};
use crate::setup::markers::{Marker, Undo, fingerprint, markers_dir};
use crate::setup::mise::MISE_FRAGMENT;
use crate::setup::uninstall::holds_install_marker;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::remove_tree;
use crate::shared::yaml::parse_value;

/// Servers by name.
pub(super) type Servers = Map<String, Value>;

pub(super) struct PlanContext<'a> {
	pub paths: &'a SetupPaths,
	pub deps: &'a SetupDeps,
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

/// Said under what `herdr config check` reported when it found issues.
const IGNORED_KEYS_NOTE: &str = "herdr ignores the keys named above: fix them in the shipped template or report them.";

/// The migrations, in the order they apply. Numbers 1, 3, 4, 6, 9–13, 15 and 17 were the merges into omp's files and
/// the clean-ups of earlier versions: the omp snapshot (19) replaced them, and their markers are ignored.
pub(super) static MIGRATIONS: [Migration; 8] = [
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
		// Graphiti and its containers are gone (0603): Jocasta's compose file marks an install that still has them.
		id: 16,
		slug: "graphiti-removed",
		refresh: None,
		servers: None,
		plan: plan_graphiti_removed,
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
		// Jocasta's omp snapshot (fork plan phase 6): `config/omp/` and herdr's omp hook over omp's own
		// files, each replaced one kept first; `secrets.yml` only when missing.
		id: 19,
		slug: "omp-snapshot",
		refresh: None,
		servers: None,
		plan: plan_snapshot,
	},
	Migration {
		// The `jocasta` MCP server is gone (0675): omp's tools come from the bridge extension, so the entry leaves both
		// `mcp.json` files and every other entry stays.
		id: 20,
		slug: "jocasta-mcp-gone",
		refresh: None,
		servers: None,
		plan: plan_jocasta_mcp_gone,
	},
	Migration {
		// Whiteboard and hunk are gone (0679): the installed folders, and the boss's whiteboard MCP entry.
		id: 21,
		slug: "whiteboard-hunk-gone",
		refresh: None,
		servers: None,
		plan: plan_whiteboard_hunk_gone,
	},
];

/// The folders and file a release shipped for whiteboard and hunk (appendix I), beside `jocasta.exe`.
const WHITEBOARD_HUNK_FOLDERS: [&str; 5] = ["whiteboard", "whiteboard-canvas", "node", "hunk", "editor.worker.js"];

/// The boss's own `mcp.json`, where migration 2 once wrote whiteboard's server.
fn boss_mcp_file(paths: &SetupPaths) -> PathBuf {
	paths.data_dir.join("home").join(".omp").join("mcp.json")
}

fn names_whiteboard(path: &Path) -> bool {
	fs::read_to_string(path)
		.ok()
		.and_then(|text| serde_json::from_str::<Value>(&text).ok())
		.is_some_and(|value| value.get("mcpServers").and_then(|servers| servers.get("whiteboard")).is_some())
}

/// Takes the installed whiteboard, canvas, node and hunk out (0679), only from the folder that holds `install.json`
/// naming itself, never a source run's repo; and whiteboard's entry out of the boss's `mcp.json`.
fn plan_whiteboard_hunk_gone(c: &mut PlanContext) -> Result<Option<Step>> {
	let install = c.paths.assets_dir.parent().filter(|dir| holds_install_marker(dir)).map(Path::to_path_buf);
	let folders: Vec<PathBuf> = install
		.iter()
		.flat_map(|dir| WHITEBOARD_HUNK_FOLDERS.iter().map(|name| dir.join(name)))
		.filter(|path| path.exists())
		.collect();
	let mcp = Some(boss_mcp_file(c.paths)).filter(|path| names_whiteboard(path));
	let Some(first) = folders.first().cloned().or_else(|| mcp.clone()) else { return Ok(None) };
	let mut step = Step::new(&first, fingerprint("whiteboard-hunk-gone"), Vec::new());
	let mut diff: Vec<String> = folders.iter().map(|path| format!("- {}", path.display())).collect();
	if let Some(path) = &mcp {
		diff.push(format!("- mcpServers.whiteboard in {}", path.display()));
	}
	step.change = Some(SetupChange {
		title: "Whiteboard and hunk are gone: their installed files and the boss's whiteboard server leave".to_owned(),
		path: shown(&first),
		diff,
		kept: Vec::new(),
		apply: Box::new(move |_io| {
			Box::pin(async move {
				for path in &folders {
					remove_tree(path)?;
				}
				if let Some(path) = mcp {
					let mut value: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
					if let Some(servers) = value.get_mut("mcpServers").and_then(Value::as_object_mut) {
						servers.remove("whiteboard");
					}
					fs::write(&path, json(&value))?;
				}
				Ok(())
			})
		}),
	});
	Ok(Some(step))
}

/// The shipped file `relative` under `config/`.
pub(super) fn asset(paths: &SetupPaths, relative: &[&str]) -> Result<String> {
	let mut path = paths.assets_dir.clone();
	path.extend(relative);
	Ok(fs::read_to_string(path)?)
}

/// Runs `herdr config check` on `path` (`HERDR_CONFIG_PATH`) and prints what it reports, each unknown-key line (0608).
/// A check that can't run, or finds issues, never stops setup: the file is already written.
async fn print_herdr_check(deps: &SetupDeps, path: &Path, io: &dyn SetupIo) {
	let result: Result<HerdrCheck> = match &deps.check_herdr_config {
		Some(check) => check(deps.herdr.clone(), path.to_path_buf()).await,
		None => default_herdr_check(&deps.herdr, path).await,
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
				print_herdr_check(&deps, &path, io).await;
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

/// The question before the old graph's images and volume go.
const GRAPH_IMAGES_QUESTION: &str = "Also remove the old graph's images and volume (Graphiti and FalkorDB)? [y/N] ";

fn plan_graphiti_removed(c: &mut PlanContext) -> Result<Option<Step>> {
	let file = c.paths.jocasta_config_dir.join("compose.yaml");
	if !file.exists() {
		return Ok(None);
	}
	let containers = c.paths.jocasta_config_dir.join("containers");
	let docker = c.deps.docker.clone();
	let mut step = Step::new(&file, fingerprint("graphiti-removed"), Vec::new());
	step.change = Some(SetupChange {
		title: "Graphiti is gone: its containers come down and their compose file is removed".to_owned(),
		path: shown(&file),
		diff: vec![format!("- {}", file.display()), format!("- {}", containers.display())],
		kept: Vec::new(),
		apply: Box::new(move |io| Box::pin(remove_graphiti(file, containers, docker, io))),
	});
	Ok(Some(step))
}

/// Takes the containers of the Compose project `file` names down while Docker's engine runs, then removes the file and
/// the build context beside it (0603). Whether the old graph's images and volume go too is asked once; whatever stays is
/// named with the one command that removes it later, which is the whole answer when Docker isn't running.
async fn remove_graphiti(
	file: PathBuf,
	containers: PathBuf,
	docker: Option<RunProgram>,
	io: &dyn SetupIo,
) -> Result<()> {
	let project = parse_value(&fs::read_to_string(&file)?)
		.ok()
		.and_then(|value| value.get("name")?.as_str().map(str::to_owned))
		.unwrap_or_else(|| "jocasta".to_owned());
	let mut everything_gone = false;
	if let Some(docker) = &docker
		&& engine_runs(docker).await
	{
		compose(docker, &file, &["down"]).await?;
		if answered_yes(io.ask(GRAPH_IMAGES_QUESTION).await.as_deref()) {
			compose(docker, &file, &["down", "--rmi", "all", "--volumes"]).await?;
			everything_gone = true;
		}
	}
	remove_tree(&file)?;
	remove_tree(&containers)?;
	if !everything_gone {
		io.print(&format!(
			"The old graph's images and volume stay; remove them with: docker compose -p {project} down --rmi all --volumes"
		));
	}
	Ok(())
}

/// Whether Docker's engine runs now: `docker info` succeeds only then.
async fn engine_runs(docker: &RunProgram) -> bool {
	matches!(docker(vec!["docker".to_owned(), "info".to_owned()]).await, Ok(RunOutput { code: 0, .. }))
}

/// `docker compose -f <file> <args>`; a failure stops setup with what Docker said.
async fn compose(docker: &RunProgram, file: &Path, args: &[&str]) -> Result<()> {
	let argv = ["docker", "compose", "-f"]
		.into_iter()
		.map(str::to_owned)
		.chain(std::iter::once(file.display().to_string()))
		.chain(args.iter().map(|arg| (*arg).to_owned()))
		.collect();
	let out = docker(argv).await?;
	if out.code != 0 {
		return Err(Error::other(format!(
			"docker compose {} exited {}: {}",
			args.join(" "),
			out.code,
			out.output.trim()
		)));
	}
	Ok(())
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

/// The `mcp.json` files that still name a `jocasta` server: omp's global one and the boss's own.
fn mcp_files_with_jocasta(paths: &SetupPaths) -> Vec<PathBuf> {
	let candidates = [paths.omp_agent_dir.join("mcp.json"), paths.data_dir.join("home").join(".omp").join("mcp.json")];
	candidates
		.into_iter()
		.filter(|path| {
			fs::read_to_string(path)
				.ok()
				.and_then(|text| serde_json::from_str::<Value>(&text).ok())
				.is_some_and(|value| value.get("mcpServers").and_then(|servers| servers.get("jocasta")).is_some())
		})
		.collect()
}

/// Takes the `jocasta` entry out of each `mcp.json` that has one (0675), leaving his other entries as they are.
fn plan_jocasta_mcp_gone(c: &mut PlanContext) -> Result<Option<Step>> {
	let files = mcp_files_with_jocasta(c.paths);
	let Some(first) = files.first().cloned() else { return Ok(None) };
	let mut step = Step::new(&first, fingerprint("jocasta-mcp-gone"), Vec::new());
	step.change = Some(SetupChange {
		title: "The jocasta MCP server is gone: its entry leaves mcp.json".to_owned(),
		path: shown(&first),
		diff: files.iter().map(|path| format!("- mcpServers.jocasta in {}", path.display())).collect(),
		kept: Vec::new(),
		apply: Box::new(move |_io| {
			Box::pin(async move {
				for path in files {
					let mut value: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
					if let Some(servers) = value.get_mut("mcpServers").and_then(Value::as_object_mut) {
						servers.remove("jocasta");
					}
					fs::write(&path, json(&value))?;
				}
				Ok(())
			})
		}),
	});
	Ok(Some(step))
}
