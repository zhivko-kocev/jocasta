//! The settings trials (fork plan section 4, phase 7): `jocasta-bench evals settings <step> --dir <run>`.
//!
//! - `enumerate` (step A, no model): every setting of the omp on PATH, from its own `omp config list --json` (and, for
//!   an enum, the values its `config set` names), each in one class by the fixed rule of [`classify`], with the values
//!   its arms try, into `<run>/settings.jsonl`.
//! - `replay` (step B, no spend): `evals/replay.rs`.
//! - `screen`, `confirm` (steps C and D, real trials): `evals/ladder.rs`.
//!
//! The run folder holds every step's output, so each step can be run again on its own.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use jocasta::frontier::server::overlay::overlay_text;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::paths::data_dir;
use jocasta::shared::process::{RunOptions, run_program};
use jocasta::shared::yaml::parse_value;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::run::{report_error, variants_dir};
use super::runner::{base_overlay, process_env};
use super::variants::{Settings, Variant, deep_merge, load_variants};

/// A setting's class (fork plan section 4, step A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Class {
	/// Sends data out, weakens the guard or approvals, or acts on his omp home or machine beyond the trial (his
	/// session store, his memory, his desktop, his worktrees): checked at the pinned value only.
	P,
	/// Set per pane by an overlay under a record: checked at the decided value.
	J,
	/// No enumerable values: checked at the default and the shipped value.
	F,
	/// A quality trade: trialled and reported, not adopted on these cases.
	Q,
	/// The rest: every value is an arm.
	C,
}

/// One setting of omp's registry with its class and the values its arms try.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setting {
	pub key: String,
	#[serde(rename = "type")]
	pub kind: String,
	/// omp's default; absent when omp gives none.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub default: Option<Value>,
	/// An enum's values, as its `config set` names them.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub values: Vec<String>,
	pub tab: String,
	pub class: Class,
	/// The values an arm tries (C and Q only): the other boolean, every other enum value, a number at ×0.5 and ×2.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub arms: Vec<Value>,
}

/// The settings keys a YAML file sets, as dotted paths down to each leaf (a list is a leaf).
pub fn set_keys(text: &str) -> Vec<String> {
	fn walk(value: &Value, prefix: &str, out: &mut Vec<String>) {
		match value {
			Value::Object(map) if !map.is_empty() => {
				for (key, inner) in map {
					let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
					walk(inner, &path, out);
				}
			}
			_ if !prefix.is_empty() => out.push(prefix.to_owned()),
			_ => {}
		}
	}
	let mut out = Vec::new();
	if let Ok(value) = parse_value(text) {
		walk(&value, "", &mut out);
	}
	out
}

/// Whether one of `paths` is `key` or lies under it (a record's own keys).
fn sets(paths: &[String], key: &str) -> bool {
	paths.iter().any(|path| path == key || path.strip_prefix(key).is_some_and(|rest| rest.starts_with('.')))
}

/// Keys whose value sends data out or decides approvals, beyond those the shipped files pin (0398, 0451, 0593), among
/// them the tools that only add traffic out when turned on (the browser relay, image and speech generation), and keys
/// that act on what outlives a trial: the session store's cleanup, memory across sessions, the desktop, a worktree's
/// source. The trials run on his own omp home (a fresh `--profile` has no login), so none of them is tried.
const OUTWARD: [&str; 21] = [
	"auth.",
	"telemetry",
	"secrets.",
	"tools.approvalMode",
	"tools.restrictedFolders",
	"bash.",
	"providers.",
	"enabledProviders",
	"disabledProviders",
	"share",
	"marketplace.",
	"update.",
	"gc.",
	"memories.",
	"mnemopi.",
	"hindsight.",
	"computer.",
	"worktree.",
	"browser.relay",
	"generate_image.",
	"speechgen.",
];

/// The quality trades the trials report without adopting (0619, 0620).
fn quality(key: &str) -> bool {
	key == "defaultThinkingLevel"
		|| key == "compaction.methodOrder"
		|| ["modelRoles", "advisor.", "read.summarize", "snapcompact."].iter().any(|prefix| key.starts_with(prefix))
}

/// What decides a setting's class: the keys the guard and the shipped `config.yml` pin, and the keys the per-pane
/// overlays (`boss.yml`, `heavy.yml`, `headless.yml`, the project overlay) set.
pub struct Pins {
	pub pinned: Vec<String>,
	pub per_pane: Vec<String>,
}

impl Pins {
	/// The pins as the repo's shipped files give them.
	pub fn shipped(config_dir: &Path) -> Result<Self> {
		let read = |name: &str| fs::read_to_string(config_dir.join(name)).map_err(Error::from);
		let mut pinned = set_keys(&read("guard.yml")?);
		pinned.extend(
			set_keys(&read("omp/config.yml")?).into_iter().filter(|key| !quality(key) && !key.starts_with("retry.")),
		);
		let mut per_pane = set_keys(&overlay_text("p0", Some(47_499)));
		for name in ["boss.yml", "heavy.yml", "headless.yml"] {
			per_pane.extend(set_keys(&read(name)?));
		}
		Ok(Self { pinned, per_pane })
	}
}

/// The fixed rule of step A: per pane before pinned (an overlay's decided value is checked as decided), a quality
/// trade, then free-form for what has no enumerable values; every other key is a candidate.
pub fn classify(key: &str, kind: &str, pins: &Pins) -> Class {
	if sets(&pins.per_pane, key) || ["memory.backend", "plan.", "collab."].iter().any(|prefix| key.starts_with(prefix))
	{
		Class::J
	} else if sets(&pins.pinned, key) || OUTWARD.iter().any(|prefix| key.starts_with(prefix)) {
		Class::P
	} else if quality(key) {
		Class::Q
	} else if !matches!(kind, "boolean" | "enum" | "number") {
		Class::F
	} else {
		Class::C
	}
}

/// The values an arm tries: the other boolean, every enum value but the default, a number at ×0.5 and ×2 (whole when
/// the default is). None for a number at 0 or without a default.
pub fn arm_values(kind: &str, default: Option<&Value>, values: &[String]) -> Vec<Value> {
	match (kind, default) {
		("boolean", Some(Value::Bool(on))) => vec![json!(!on)],
		("enum", _) => values
			.iter()
			.filter(|value| default.and_then(Value::as_str) != Some(value.as_str()))
			.map(|value| json!(value))
			.collect(),
		("number", Some(Value::Number(number))) => {
			let whole = number.is_i64() || number.is_u64();
			let Some(n) = number.as_f64().filter(|n| *n != 0.0) else { return Vec::new() };
			[n * 0.5, n * 2.0]
				.into_iter()
				.map(|scaled| if whole { json!(scaled.round() as i64) } else { json!(scaled) })
				.filter(|scaled| Some(scaled) != default)
				.collect()
		}
		_ => Vec::new(),
	}
}

/// The values omp names when `config set` refuses a value: `Error: Invalid value: x. Valid values: a, b, c`.
pub fn valid_values(said: &str) -> Vec<String> {
	said.lines()
		.find_map(|line| line.split_once("Valid values:").map(|(_, list)| list))
		.map(|list| {
			list.split(',')
				.map(|value| value.trim().trim_end_matches('.').to_owned())
				.filter(|v| !v.is_empty())
				.collect()
		})
		.unwrap_or_default()
}

/// An omp run in the scratch folder: its own agent folder and home, so neither his settings nor his skills reach it.
pub struct Scratch {
	pub root: PathBuf,
}

impl Scratch {
	/// Makes the folder's `agent` and `home` folders under `root`.
	pub fn new(root: PathBuf) -> Result<Self> {
		fs::create_dir_all(root.join("agent"))?;
		fs::create_dir_all(root.join("home"))?;
		Ok(Self { root })
	}

	/// omp's agent folder here (`PI_CODING_AGENT_DIR`).
	pub fn agent(&self) -> PathBuf {
		self.root.join("agent")
	}

	/// This process's environment with omp's folders and the home pointed into the scratch folder and no provider key.
	pub fn env(&self) -> HashMap<String, String> {
		let mut env = process_env();
		env.retain(|name, _| !name.to_ascii_uppercase().starts_with("ANTHROPIC_") && !name.starts_with("PI_"));
		let home = self.root.join("home").display().to_string();
		env.insert("PI_CODING_AGENT_DIR".to_owned(), self.agent().display().to_string());
		for name in ["HOME", "USERPROFILE"] {
			env.insert(name.to_owned(), home.clone());
		}
		env
	}

	async fn omp(&self, args: &[&str]) -> Result<(Option<i32>, String, String)> {
		let argv: Vec<String> = std::iter::once("omp").chain(args.iter().copied()).map(str::to_owned).collect();
		let options = RunOptions {
			cwd: Some(self.root.clone()),
			env: Some(self.env()),
			timeout_ms: Some(60_000),
			..RunOptions::default()
		};
		let ran = run_program(&argv, options).await?;
		Ok((ran.code, ran.stdout, ran.stderr))
	}
}

/// Step A: omp's registry, classified, with each enum's values and each arm, written to `<run>/settings.jsonl`.
pub async fn enumerate(run: &Path) -> Result<Vec<Setting>> {
	let scratch = Scratch::new(run.join("scratch-enumerate"))?;
	let (code, listed, said) = scratch.omp(&["config", "list", "--json"]).await?;
	if code != Some(0) {
		return Err(Error::other(format!("omp config list --json exited {code:?}: {said}")));
	}
	let registry: Map<String, Value> = serde_json::from_str(&listed)?;
	let pins = Pins::shipped(&Path::new(jocasta::shared::install::REPO).join("config"))?;
	let mut settings = Vec::with_capacity(registry.len());
	for (key, entry) in &registry {
		let kind = entry["type"].as_str().unwrap_or("").to_owned();
		let default = entry.get("value").cloned();
		let values = if kind == "enum" {
			let (_, out, err) = scratch.omp(&["config", "set", key, "__jocasta_probe__"]).await?;
			valid_values(&format!("{out}\n{err}"))
		} else {
			Vec::new()
		};
		let class = classify(key, &kind, &pins);
		let arms = if matches!(class, Class::C | Class::Q) {
			arm_values(&kind, default.as_ref(), &values)
		} else {
			Vec::new()
		};
		let tab = key.split('.').next().unwrap_or(key).to_owned();
		settings.push(Setting { key: key.clone(), kind, default, values, tab, class, arms });
	}
	let lines: String = settings
		.iter()
		.map(|setting| serde_json::to_string(setting).map(|line| line + "\n"))
		.collect::<serde_json::Result<_>>()?;
	fs::write(run.join("settings.jsonl"), lines)?;
	Ok(settings)
}

/// `<run>/settings.jsonl` as step A wrote it.
pub fn read_settings(run: &Path) -> Result<Vec<Setting>> {
	let text = fs::read_to_string(run.join("settings.jsonl"))?;
	text.lines()
		.filter(|line| !line.trim().is_empty())
		.map(|line| serde_json::from_str(line).map_err(Error::from))
		.collect()
}

/// The base every arm changes one setting of (fork plan section 4, step C): A1, so A0 with the pane config, the
/// advisor off.
pub fn base_variant() -> Result<Variant> {
	load_variants(&variants_dir(), &["a1".to_owned()])
}

/// One arm as settings: `key`'s dotted path set to `value`.
pub fn arm_delta(key: &str, value: &Value) -> Settings {
	let mut delta = value.clone();
	for part in key.split('.').rev() {
		let mut level = Map::new();
		level.insert(part.to_owned(), delta);
		delta = Value::Object(level);
	}
	match delta {
		Value::Object(map) => map,
		_ => Map::new(),
	}
}

/// An arm: the settings it sets over the base, each a dotted key and its value; empty for the base itself.
pub type Arm = Vec<(String, Value)>;

/// The `--config` overlay of a run of `variant` with `arm` over it: the runner's own (memory off), the variant's
/// settings, the arm.
pub fn overlay_of(variant: &Variant, arm: &[(String, Value)]) -> Settings {
	let mut settings = deep_merge(&base_overlay(), &variant.settings.clone().unwrap_or_default());
	for (key, value) in arm {
		settings = deep_merge(&settings, &arm_delta(key, value));
	}
	settings
}

/// Between an arm's settings in its name.
const ARM_JOIN: &str = " & ";

/// An arm's name in the run's files: `base`, or each `key=value` with the value as JSON, joined by ` & `.
pub fn arm_name(arm: &[(String, Value)]) -> String {
	if arm.is_empty() {
		return "base".to_owned();
	}
	arm.iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>().join(ARM_JOIN)
}

/// The arm a name stands for.
pub fn parse_arm(name: &str) -> Result<Arm> {
	if name == "base" {
		return Ok(Vec::new());
	}
	name.split(ARM_JOIN)
		.map(|part| {
			let (key, value) = part.split_once('=').ok_or_else(|| Error::user(format!("not an arm: {name}")))?;
			Ok((key.to_owned(), serde_json::from_str(value)?))
		})
		.collect()
}

/// `--dir <run>`, or a new run folder under Jocasta's data folder.
fn run_dir(argv: &[String]) -> Result<PathBuf> {
	match argv.iter().position(|arg| arg == "--dir") {
		// Absolute: omp finds its native addon under the scratch home only by a full path.
		Some(at) => {
			let dir = argv.get(at + 1).map(PathBuf::from).ok_or_else(|| Error::user("--dir needs a folder"))?;
			Ok(std::path::absolute(dir)?)
		}
		None => {
			Ok(data_dir().join("evals").join("settings").join(chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()))
		}
	}
}

/// `jocasta-bench evals settings <enumerate|replay|screen|confirm|report> [--dir <run>]`.
pub async fn main(argv: Vec<String>) -> ExitCode {
	match run(&argv).await {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => report_error(&error),
	}
}

async fn run(argv: &[String]) -> Result<()> {
	let step = argv.first().map(String::as_str).unwrap_or("");
	let rest = argv.get(1..).unwrap_or_default();
	if step == "trial" {
		return super::ladder::trial(rest).await;
	}
	let dir = run_dir(rest)?;
	fs::create_dir_all(&dir)?;
	println!("run folder: {}", dir.display());
	if step != "report" {
		hold_omp_version(&dir)?;
	}
	match step {
		"enumerate" => {
			let settings = enumerate(&dir).await?;
			let count = |class: Class| settings.iter().filter(|s| s.class == class).count();
			let arms: usize = settings.iter().map(|s| s.arms.len()).sum();
			println!(
				"{} settings: P {}, J {}, F {}, Q {}, C {}; {arms} arms",
				settings.len(),
				count(Class::P),
				count(Class::J),
				count(Class::F),
				count(Class::Q),
				count(Class::C)
			);
			Ok(())
		}
		"replay" => super::replay::main(&dir).await,
		"screen" => super::ladder::screen(&dir).await,
		"report" => {
			println!("{}", serde_json::to_string_pretty(&super::ladder::counts(&dir)?)?);
			Ok(())
		}
		"confirm" => super::ladder::confirm(&dir).await,
		"models" => super::ladder::models(&dir).await,
		_ => Err(Error::user(
			"usage: jocasta-bench evals settings <enumerate|replay|screen|confirm|models|report> [--dir <run>]",
		)),
	}
}

/// One run, one omp: the version on PATH is written to `<run>/omp-version.txt` by the run's first step, and any later
/// step or trial refuses another (an update mid-run stops the ladder after three trials), so no arm is ever compared
/// with a base an older omp ran.
pub(super) fn hold_omp_version(dir: &Path) -> Result<()> {
	let now = crate::lib::output::read_version(&["omp"])?;
	let file = dir.join("omp-version.txt");
	match fs::read_to_string(&file) {
		Ok(held) if held.trim() == now => Ok(()),
		Ok(held) => Err(Error::user(format!(
			"this run was made on {}, and omp is now {now}: start a new run folder",
			held.trim()
		))),
		Err(_) => Ok(fs::write(&file, format!("{now}\n"))?),
	}
}
