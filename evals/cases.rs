//! The cases of a run (`case.yaml` files, 0164, 0375, 0482): read, checked and reported with file and line.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::evals::runner::{Check, EvalCase};
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::projects::locale_compare;
use jocasta::shared::yaml::{PathSegment, SchemaProblem, YamlDocument, describe, parse_yaml_file};

fn invalid(path: Vec<PathSegment>, message: &str) -> SchemaProblem {
	SchemaProblem::Invalid { path, message: message.to_owned() }
}

fn key_path(parts: &[PathSegment]) -> Vec<PathSegment> {
	parts.to_vec()
}

/// A string of at least one character, or the problem at `path`.
fn text_of(value: &Value, path: &[PathSegment], min_length: usize) -> std::result::Result<String, SchemaProblem> {
	match value {
		Value::String(text) if text.chars().count() < min_length => {
			Err(invalid(key_path(path), &format!("must not have fewer than {min_length} characters")))
		}
		Value::String(text) => Ok(text.clone()),
		_ => Err(invalid(key_path(path), "must be string")),
	}
}

fn field_path(key: &str) -> Vec<PathSegment> {
	vec![PathSegment::from(key)]
}

/// An end-state check as typebox's `CheckSchema` reads it: an unknown key first, then each property in order.
fn check_of(value: &Value, path: &[PathSegment]) -> std::result::Result<Check, SchemaProblem> {
	let Value::Object(map) = value else { return Err(invalid(key_path(path), "must be object")) };
	const KEYS: [&str; 5] = ["command", "file", "contains", "absent", "git_clean"];
	if let Some(key) = map.keys().find(|key| !KEYS.contains(&key.as_str())) {
		return Err(SchemaProblem::UnknownKey { path: key_path(path), key: key.clone() });
	}
	let at = |key: &str| {
		let mut full = key_path(path);
		full.push(PathSegment::from(key));
		full
	};
	let optional = |key: &str, min: usize| map.get(key).map(|value| text_of(value, &at(key), min)).transpose();
	let absent = match map.get("absent") {
		None => None,
		Some(Value::Bool(true)) => Some(true),
		Some(_) => return Err(invalid(at("absent"), "must be equal to constant")),
	};
	let git_clean = match map.get("git_clean") {
		None => None,
		Some(Value::Bool(flag)) => Some(*flag),
		Some(_) => return Err(invalid(at("git_clean"), "must be boolean")),
	};
	Ok(Check {
		command: optional("command", 1)?,
		file: optional("file", 1)?,
		contains: optional("contains", 0)?,
		absent,
		git_clean,
	})
}

/// The checks under `key`, at least `min_items` of them.
fn checks_of(
	map: &Map<String, Value>,
	key: &str,
	min_items: usize,
) -> std::result::Result<Option<Vec<Check>>, SchemaProblem> {
	let Some(value) = map.get(key) else { return Ok(None) };
	let Value::Array(items) = value else { return Err(invalid(field_path(key), "must be array")) };
	if items.len() < min_items {
		return Err(invalid(field_path(key), &format!("must not have fewer than {min_items} items")));
	}
	items
		.iter()
		.enumerate()
		.map(|(i, item)| check_of(item, &[PathSegment::from(key), PathSegment::from(i)]))
		.collect::<std::result::Result<Vec<_>, _>>()
		.map(Some)
}

/// `case.yaml`: the prompt, tags for splits and grouping, the end-state checks, and an optional setup command.
struct CaseFile {
	prompt: String,
	tags: Vec<String>,
	/// A shell command run in the trial repo after the fixture is copied, before the fixture commit (0375).
	setup: Option<String>,
	checks: Vec<Check>,
	/// Checks the agent never sees (0482 E1): run after it exits, on a clean clone of its result with `setup` run again
	/// and the case's `hidden/` folder copied in.
	hidden_checks: Option<Vec<Check>>,
}

fn validate_case(value: &Value) -> std::result::Result<CaseFile, SchemaProblem> {
	const KEYS: [&str; 5] = ["prompt", "tags", "setup", "checks", "hidden_checks"];
	let Value::Object(map) = value else { return Err(invalid(Vec::new(), "must be object")) };
	let missing: Vec<&str> = ["prompt", "checks"].into_iter().filter(|key| !map.contains_key(*key)).collect();
	if !missing.is_empty() {
		return Err(invalid(Vec::new(), &format!("must have required properties {}", missing.join(", "))));
	}
	if let Some(key) = map.keys().find(|key| !KEYS.contains(&key.as_str())) {
		return Err(SchemaProblem::UnknownKey { path: Vec::new(), key: key.clone() });
	}
	let prompt = text_of(&map["prompt"], &field_path("prompt"), 1)?;
	let tags = match map.get("tags") {
		None => Vec::new(),
		Some(Value::Array(items)) => items
			.iter()
			.enumerate()
			.map(|(i, tag)| text_of(tag, &[PathSegment::from("tags"), PathSegment::from(i)], 1))
			.collect::<std::result::Result<Vec<_>, _>>()?,
		Some(_) => return Err(invalid(field_path("tags"), "must be array")),
	};
	let setup = map.get("setup").map(|value| text_of(value, &field_path("setup"), 1)).transpose()?;
	let checks = checks_of(map, "checks", 1)?.unwrap_or_default();
	let hidden_checks = checks_of(map, "hidden_checks", 1)?;
	Ok(CaseFile { prompt, tags, setup, checks, hidden_checks })
}

fn case_error(error: impl std::fmt::Display) -> Error {
	Error::other(error.to_string())
}

fn validate_checks(checks: &[Check], key: &str, file: &str, doc: &YamlDocument<CaseFile>) -> Result<()> {
	for (i, check) in checks.iter().enumerate() {
		let kinds = [check.command.is_some(), check.file.is_some(), check.git_clean.is_some()]
			.iter()
			.filter(|kind| **kind)
			.count();
		let file_only = check.file.is_none() && (check.contains.is_some() || check.absent.is_some());
		if kinds != 1 || file_only || (check.contains.is_some() && check.absent.is_some()) {
			let path = [PathSegment::from(key), PathSegment::from(i)];
			return Err(case_error(format!(
				"{file}:{}: {} needs one of `command`, `file` (with `contains` or `absent`) or `git_clean`",
				doc.line_of(&path),
				describe(&path)
			)));
		}
	}
	Ok(())
}

pub fn load_cases(cases_dir: &Path) -> Result<Vec<EvalCase>> {
	if !cases_dir.exists() {
		return Ok(Vec::new());
	}
	let mut names: Vec<String> = Vec::new();
	for entry in fs::read_dir(cases_dir)? {
		let entry = entry?;
		if entry.path().is_dir() && entry.path().join("case.yaml").exists() {
			names.push(entry.file_name().to_string_lossy().into_owned());
		}
	}
	// Name order on every OS: Linux and macOS list a folder in no fixed order.
	names.sort_by(|a, b| locale_compare(a, b));
	names.iter().map(|name| load_case(cases_dir, name)).collect()
}

fn load_case(cases_dir: &Path, name: &str) -> Result<EvalCase> {
	let dir = cases_dir.join(name);
	let file = dir.join("case.yaml");
	let shown = file.display().to_string();
	let source = String::from_utf8_lossy(&fs::read(&file)?).into_owned();
	let doc = parse_yaml_file(&source, &shown, validate_case).map_err(case_error)?;
	validate_checks(&doc.value.checks, "checks", &shown, &doc)?;
	validate_checks(doc.value.hidden_checks.as_deref().unwrap_or_default(), "hidden_checks", &shown, &doc)?;
	let (fixture, hidden) = (dir.join("fixture"), dir.join("hidden"));
	if hidden.exists() && doc.value.hidden_checks.is_none() {
		return Err(case_error(format!("{shown}:1: the case has a hidden/ folder but no hidden_checks")));
	}
	let CaseFile { prompt, tags, setup, checks, hidden_checks } = doc.value;
	Ok(EvalCase {
		id: name.to_owned(),
		dir,
		prompt,
		tags,
		checks,
		hidden_checks: hidden_checks.unwrap_or_default(),
		setup,
		fixture: fixture.exists().then_some(fixture),
		hidden: hidden.exists().then_some(hidden),
		..EvalCase::default()
	})
}

/// Every case in `dirs`, in order; a case id in two folders or no case at all fails.
pub fn load_all_cases(dirs: &[PathBuf]) -> Result<Vec<EvalCase>> {
	let shown = dirs.iter().map(|dir| dir.display().to_string()).collect::<Vec<_>>().join(", ");
	let mut cases = Vec::new();
	for dir in dirs {
		cases.extend(load_cases(dir)?);
	}
	let ids: Vec<&String> = cases.iter().map(|case| &case.id).collect();
	if let Some(twice) = ids.iter().enumerate().find(|(i, id)| ids.iter().position(|other| other == *id) != Some(*i)) {
		return Err(Error::other(format!("eval case {} is in more than one of {shown}", twice.1)));
	}
	if cases.is_empty() {
		return Err(Error::other(format!("no eval cases in {shown}")));
	}
	Ok(cases)
}
