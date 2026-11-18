//! Eval variants (0375): `evals/variants/<name>.yaml` changes how each trial's omp starts (0386). Every key is
//! optional; `--variant a --variant b` layers b over a.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use jocasta::shared::errors::{Error, Result};
use jocasta::shared::fsx::normalize;
use jocasta::shared::pathkey::resolve;
use jocasta::shared::yaml::{PathSegment, SchemaProblem, parse_yaml_file};
use serde_json::{Map, Value};

use crate::evals::harness::{OMP_KEYS, OmpKey, adapter};
use crate::evals::plan_review::PlanReviewChoice;

type Settings = Map<String, Value>;

/// Deep-merges `delta` over `base` as omp merges config overlays: objects merge, everything else replaces.
pub fn deep_merge(base: &Settings, delta: &Settings) -> Settings {
	let mut merged = base.clone();
	for (key, value) in delta {
		let next = match (merged.get(key), value) {
			(Some(Value::Object(current)), Value::Object(value)) => Value::Object(deep_merge(current, value)),
			_ => value.clone(),
		};
		merged.insert(key.clone(), next);
	}
	merged
}

/// Which headless harness runs the trial (0482 E4, 0537).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
	Omp,
	Claude,
	Opencode,
}

impl Harness {
	/// The harness as a variant file spells it.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Omp => "omp",
			Self::Claude => "claude",
			Self::Opencode => "opencode",
		}
	}
}

impl fmt::Display for Harness {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.as_str())
	}
}

/// A variant file's keys, in the order its schema lists them.
const KEYS: [&str; 15] = [
	"extends",
	"harness",
	"model",
	"plan_first",
	"plan_review",
	"plan_review_role",
	"followups",
	"configs",
	"append_system_prompt",
	"extensions",
	"tools",
	"args",
	"settings",
	"env",
	"layer",
];

/// What a variant file holds (the TypeScript's typebox `VariantSchema`).
#[derive(Debug, Default)]
struct VariantFile {
	/// Variants in this folder layered first, in order (the arms A1–A3 build on each other, 0482).
	extends: Option<Vec<String>>,
	/// Which headless harness runs the trial (0482 E4, 0537): omp (default), Claude Code's `claude -p` or opencode's
	/// `opencode run`; the last one wins.
	harness: Option<Harness>,
	/// The model, passed as `--model`; plan-first needs it (0482 E3). The last one wins.
	model: Option<String>,
	/// Plan first (0482 E3): omp's `--plan-yolo --plan-yolo-into <model>`, so execution stays on the same model.
	plan_first: Option<bool>,
	/// Plan first, answered in omp's interactive plan review (0567): the first turn runs omp interactive in a bench
	/// terminal, starting in plan mode, and an extension answers the review with "Approve and execute" (`execute`) or
	/// "Approve and keep context" (`keep`) (`evals/plan_review.rs`). Not with `plan_first`. The last one wins.
	plan_review: Option<PlanReviewChoice>,
	/// The role the work after an approved plan review runs on (0608): passed as `resolvePlanReview`'s `{ role }`, an
	/// execution tier of omp's review slider (`smol`). Needs `plan_review`. The last one wins.
	plan_review_role: Option<String>,
	/// Follow-up turns (0482 E2), each run after the first as `omp -c -p <prompt>` (`claude -c -p`) in the same repo;
	/// `{base}` in a prompt becomes the fixture commit. Lists concatenate.
	followups: Option<Vec<String>>,
	/// Config files, relative to the variant file, each passed as `--config` before the trial's overlay; lists concatenate.
	configs: Option<Vec<String>>,
	/// A file, relative to the variant file, passed as `--append-system-prompt`; the last one wins.
	append_system_prompt: Option<String>,
	/// Extension paths, relative to the variant file, each passed as `-e`; lists concatenate.
	extensions: Option<Vec<String>>,
	/// Tool names passed as `--tools a,b`; the last one wins.
	tools: Option<Vec<String>>,
	/// Extra harness arguments, verbatim, after the others; lists concatenate.
	args: Option<Vec<String>>,
	/// omp settings deltas in `config.yml` shape, deep-merged into the trial's `--config` overlay.
	settings: Option<Settings>,
	/// Merged into omp's env.
	env: Option<Vec<(String, String)>>,
	/// The harness-OS layer (0532, 0543 A6): report_result, the task file's brief, the cached checks with the `check`
	/// thread tool and the history search tool (0604), built into each trial by Jocasta's layer (`evals/layer.rs`).
	layer: Option<bool>,
}

fn invalid(path: Vec<PathSegment>, message: &str) -> SchemaProblem {
	SchemaProblem::Invalid { path, message: message.to_owned() }
}

/// A variant file's top-level fields as typebox checks them: each reads its value or names the first problem.
struct Fields<'a> {
	map: &'a Map<String, Value>,
}

impl Fields<'_> {
	fn key(key: &str) -> Vec<PathSegment> {
		vec![PathSegment::from(key)]
	}

	fn string(&self, key: &str) -> std::result::Result<Option<String>, SchemaProblem> {
		match self.map.get(key) {
			None => Ok(None),
			Some(value) => string_of(value, Self::key(key)).map(Some),
		}
	}

	fn boolean(&self, key: &str) -> std::result::Result<Option<bool>, SchemaProblem> {
		match self.map.get(key) {
			None => Ok(None),
			Some(Value::Bool(flag)) => Ok(Some(*flag)),
			Some(_) => Err(invalid(Self::key(key), "must be boolean")),
		}
	}

	/// A union of string literals: a value that isn't a string is told so, as typebox does for the first literal.
	fn literal<'v>(&self, key: &str, allowed: &[&'v str]) -> std::result::Result<Option<&'v str>, SchemaProblem> {
		match self.map.get(key) {
			None => Ok(None),
			Some(Value::String(text)) => allowed
				.iter()
				.find(|name| **name == text)
				.copied()
				.map(Some)
				.ok_or_else(|| invalid(Self::key(key), "must be equal to constant")),
			Some(_) => Err(invalid(Self::key(key), "must be string")),
		}
	}

	/// An array of non-empty strings; `min_items` as `Type.Array`'s.
	fn strings(&self, key: &str, min_items: usize) -> std::result::Result<Option<Vec<String>>, SchemaProblem> {
		let Some(value) = self.map.get(key) else { return Ok(None) };
		let Value::Array(items) = value else { return Err(invalid(Self::key(key), "must be array")) };
		if items.len() < min_items {
			return Err(invalid(Self::key(key), &format!("must not have fewer than {min_items} items")));
		}
		items
			.iter()
			.enumerate()
			.map(|(i, item)| string_of(item, vec![PathSegment::from(key), PathSegment::from(i)]))
			.collect::<std::result::Result<Vec<_>, _>>()
			.map(Some)
	}

	fn object(&self, key: &str) -> std::result::Result<Option<&Map<String, Value>>, SchemaProblem> {
		match self.map.get(key) {
			None => Ok(None),
			Some(Value::Object(object)) => Ok(Some(object)),
			Some(_) => Err(invalid(Self::key(key), "must be object")),
		}
	}

	/// `Type.Record(Type.String(), Type.String())`.
	fn string_map(&self, key: &str) -> std::result::Result<Option<Vec<(String, String)>>, SchemaProblem> {
		let Some(object) = self.object(key)? else { return Ok(None) };
		object
			.iter()
			.map(|(name, value)| match value {
				Value::String(text) => Ok((name.clone(), text.clone())),
				_ => Err(invalid(vec![PathSegment::from(key), PathSegment::from(name.as_str())], "must be string")),
			})
			.collect::<std::result::Result<Vec<_>, _>>()
			.map(Some)
	}
}

/// A string of at least one character.
fn string_of(value: &Value, path: Vec<PathSegment>) -> std::result::Result<String, SchemaProblem> {
	match value {
		Value::String(text) if text.is_empty() => Err(invalid(path, "must not have fewer than 1 characters")),
		Value::String(text) => Ok(text.clone()),
		_ => Err(invalid(path, "must be string")),
	}
}

/// The file's shape: an unknown key first (typebox reports `additionalProperties` ahead of the properties), then each
/// property in the schema's order; the first problem is the one told.
fn validate(value: &Value) -> std::result::Result<VariantFile, SchemaProblem> {
	let Value::Object(map) = value else { return Err(invalid(Vec::new(), "must be object")) };
	if let Some(key) = map.keys().find(|key| !KEYS.contains(&key.as_str())) {
		return Err(SchemaProblem::UnknownKey { path: Vec::new(), key: key.clone() });
	}
	let fields = Fields { map };
	let extends = fields.strings("extends", 0)?;
	let harness = fields.literal("harness", &["omp", "claude", "opencode"])?.map(|name| match name {
		"claude" => Harness::Claude,
		"opencode" => Harness::Opencode,
		_ => Harness::Omp,
	});
	let model = fields.string("model")?;
	let plan_first = fields.boolean("plan_first")?;
	let plan_review = fields.literal("plan_review", &["execute", "keep"])?.map(|name| match name {
		"keep" => PlanReviewChoice::Keep,
		_ => PlanReviewChoice::Execute,
	});
	Ok(VariantFile {
		extends,
		harness,
		model,
		plan_first,
		plan_review,
		plan_review_role: fields.string("plan_review_role")?,
		followups: fields.strings("followups", 0)?,
		configs: fields.strings("configs", 0)?,
		append_system_prompt: fields.string("append_system_prompt")?,
		extensions: fields.strings("extensions", 0)?,
		tools: fields.strings("tools", 1)?,
		args: fields.strings("args", 0)?,
		settings: fields.object("settings")?.cloned(),
		env: fields.string_map("env")?,
		layer: fields.boolean("layer")?,
	})
}

/// The variants of one run, layered in flag order.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
	/// The names joined with "+", or "default" without `--variant`.
	pub label: String,
	pub harness: Harness,
	pub model: Option<String>,
	pub plan_first: bool,
	/// Answer omp's interactive plan review with this choice (0567); unset: no interactive plan review.
	pub plan_review: Option<PlanReviewChoice>,
	/// The role of omp's plan review slider the approved work runs on (0608); needs `plan_review`.
	pub plan_review_role: Option<String>,
	pub followups: Vec<String>,
	pub configs: Vec<String>,
	pub append_system_prompt: Option<String>,
	pub extensions: Vec<String>,
	pub tools: Option<Vec<String>>,
	pub args: Vec<String>,
	pub settings: Option<Settings>,
	pub env: HashMap<String, String>,
	/// Whether the trial runs with the harness-OS layer.
	pub layer: bool,
}

/// The variant of a run without `--variant` (`DEFAULT_VARIANT`).
pub fn default_variant() -> Variant {
	Variant {
		label: "default".to_owned(),
		harness: Harness::Omp,
		model: None,
		plan_first: false,
		plan_review: None,
		plan_review_role: None,
		followups: Vec::new(),
		configs: Vec::new(),
		append_system_prompt: None,
		extensions: Vec::new(),
		tools: None,
		args: Vec::new(),
		settings: None,
		env: HashMap::new(),
		layer: false,
	}
}

fn is_name(name: &str) -> bool {
	let mut chars = name.chars();
	chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
		&& chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `relative` against the variant file's folder; it must exist.
fn existing(file: &Path, relative: &str) -> Result<String> {
	let absolute = resolve(&file.parent().unwrap_or(Path::new("")).join(relative));
	if !absolute.exists() {
		return Err(Error::user(format!("{}: {relative} does not exist", file.display())));
	}
	Ok(absolute.display().to_string())
}

/// Layers `<dir>/<name>.yaml` over `variant`, the files it extends first.
fn layer(dir: &Path, name: &str, chain: &[String], variant: &mut Variant) -> Result<()> {
	// A name is a file stem in the variants folder, never a path.
	let file: PathBuf = normalize(&dir.join(format!("{name}.yaml")));
	if !is_name(name) || !file.exists() {
		return Err(Error::user(format!("unknown variant: {name}")));
	}
	if chain.iter().any(|link| link == name) {
		let mut path = chain.to_vec();
		path.push(name.to_owned());
		return Err(Error::user(format!("variant {name} extends itself: {}", path.join(" → "))));
	}
	let source = String::from_utf8_lossy(&fs::read(&file)?).into_owned();
	let doc = parse_yaml_file(&source, &file.display().to_string(), validate)
		.map_err(|error| Error::user(error.to_string()))?
		.value;
	let mut below = chain.to_vec();
	below.push(name.to_owned());
	for base in doc.extends.iter().flatten() {
		layer(dir, base, &below, variant)?;
	}
	apply(&file, doc, variant)
}

/// What one variant file sets, over what is layered so far.
fn apply(file: &Path, doc: VariantFile, variant: &mut Variant) -> Result<()> {
	if let Some(harness) = doc.harness {
		variant.harness = harness;
	}
	if doc.model.is_some() {
		variant.model = doc.model;
	}
	if let Some(plan_first) = doc.plan_first {
		variant.plan_first = plan_first;
	}
	if doc.plan_review.is_some() {
		variant.plan_review = doc.plan_review;
	}
	if doc.plan_review_role.is_some() {
		variant.plan_review_role = doc.plan_review_role;
	}
	variant.followups.extend(doc.followups.into_iter().flatten());
	for config in doc.configs.iter().flatten() {
		variant.configs.push(existing(file, config)?);
	}
	if let Some(prompt) = &doc.append_system_prompt {
		variant.append_system_prompt = Some(existing(file, prompt)?);
	}
	for extension in doc.extensions.iter().flatten() {
		variant.extensions.push(existing(file, extension)?);
	}
	if doc.tools.is_some() {
		variant.tools = doc.tools;
	}
	variant.args.extend(doc.args.into_iter().flatten());
	if let Some(settings) = doc.settings {
		variant.settings = Some(deep_merge(&variant.settings.take().unwrap_or_default(), &settings));
	}
	variant.env.extend(doc.env.into_iter().flatten());
	if let Some(layer) = doc.layer {
		variant.layer = layer;
	}
	Ok(())
}

/// Whether `variant` sets `key`.
fn is_set(key: OmpKey, variant: &Variant) -> bool {
	match key {
		OmpKey::PlanFirst => variant.plan_first,
		OmpKey::PlanReview => variant.plan_review.is_some(),
		OmpKey::PlanReviewRole => variant.plan_review_role.is_some(),
		OmpKey::Configs => !variant.configs.is_empty(),
		OmpKey::Extensions => !variant.extensions.is_empty(),
		OmpKey::Tools => variant.tools.is_some(),
		OmpKey::Settings => variant.settings.is_some(),
		OmpKey::Layer => variant.layer,
		OmpKey::AppendSystemPrompt => variant.append_system_prompt.is_some(),
	}
}

/// The refusals of a layered variant as a whole.
fn check(variant: &Variant) -> Result<()> {
	let label = &variant.label;
	if variant.plan_first && variant.model.is_none() {
		return Err(Error::user(format!(
			"{label}: plan_first needs a model, so execution stays on the same model (0482)"
		)));
	}
	if variant.plan_first && variant.plan_review.is_some() {
		return Err(Error::user(format!(
			"{label}: plan_first (--plan-yolo) and plan_review are two ways to plan first; set plan_first: false"
		)));
	}
	if variant.plan_review_role.is_some() && variant.plan_review.is_none() {
		return Err(Error::user(format!("{label}: plan_review_role answers omp's plan review; set plan_review too")));
	}
	let accepts = adapter(variant.harness).accepts;
	let omp_only: Vec<&str> =
		OMP_KEYS.iter().filter(|key| is_set(**key, variant) && !accepts.contains(key)).map(|key| key.name()).collect();
	if !omp_only.is_empty() {
		return Err(Error::user(format!(
			"{label}: {} apply to omp only, not {}",
			omp_only.join(", "),
			variant.harness
		)));
	}
	Ok(())
}

/// Reads and layers `<dir>/<name>.yaml` for each name. An unknown or malformed variant is an `Error::User` (exit 2).
pub fn load_variants(dir: &Path, names: &[String]) -> Result<Variant> {
	if names.is_empty() {
		return Ok(default_variant());
	}
	let mut variant = Variant { label: names.join("+"), ..default_variant() };
	for name in names {
		layer(dir, name, &[], &mut variant)?;
	}
	check(&variant)?;
	Ok(variant)
}

/// The harness flags a variant adds; omp's `settings` go into the trial's `--config` overlay instead.
pub fn variant_args(variant: &Variant) -> Vec<String> {
	let mut args: Vec<String> = Vec::new();
	if let Some(model) = &variant.model {
		args.extend(["--model".to_owned(), model.clone()]);
		if variant.plan_first {
			args.extend(["--plan-yolo".to_owned(), "--plan-yolo-into".to_owned(), model.clone()]);
		}
	}
	for config in &variant.configs {
		args.extend(["--config".to_owned(), config.clone()]);
	}
	if let Some(prompt) = &variant.append_system_prompt {
		args.extend(["--append-system-prompt".to_owned(), prompt.clone()]);
	}
	for extension in &variant.extensions {
		args.extend(["-e".to_owned(), extension.clone()]);
	}
	if let Some(tools) = &variant.tools {
		args.extend(["--tools".to_owned(), tools.join(",")]);
	}
	args.extend(variant.args.iter().cloned());
	args
}
