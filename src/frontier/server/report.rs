//! `report_result` (0539, K4): what a thread hands in when its task ends, one schema that `jocasta mcp` lists as the
//! tool's input and the service validates before storing it with the thread. Its `tests` are the thread's claim, never
//! the review's proof (0453).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const REPORT_RESULT: &str = "report_result";

/// How a task ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
	Done,
	Blocked,
	Failed,
}

impl Outcome {
	fn as_str(self) -> &'static str {
		match self {
			Self::Done => "done",
			Self::Blocked => "blocked",
			Self::Failed => "failed",
		}
	}
}

/// A test the thread ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportedTest {
	pub command: String,
	pub passed: bool,
}

/// A choice the thread settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportedDecision {
	pub question: String,
	pub choice: String,
	pub why: String,
}

/// The report: what `report_result`'s schema describes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThreadReport {
	pub outcome: Outcome,
	pub summary: String,
	pub files: Vec<String>,
	pub tests: Vec<ReportedTest>,
	pub decisions: Vec<ReportedDecision>,
	pub questions: Vec<String>,
	pub follow_ups: Vec<String>,
}

/// The tool's description as thread panes see it.
pub const REPORT_DESCRIPTION: &str = "Hand in this task's report once its work is committed.\nGive the outcome, a summary of at most 200 characters, the files changed, the tests you ran, the decisions you settled, open questions for him and follow-ups. Jocasta stores it with the thread and its review reads it; a test you mark passed is your claim, the review runs its own.";

/// The schema's shape, one tree that both `report_schema` and the checker read.
enum Shape {
	Text { min: usize, max: Option<usize>, description: &'static str },
	Flag { description: &'static str },
	List { items: Box<Shape>, description: &'static str },
	Record { fields: Vec<(&'static str, Shape)>, description: Option<&'static str> },
	Outcome { description: &'static str },
}

fn text(description: &'static str) -> Shape {
	Shape::Text { min: 1, max: None, description }
}

fn list_of(items: Shape, description: &'static str) -> Shape {
	Shape::List { items: Box::new(items), description }
}

fn report_shape() -> Shape {
	let test = Shape::Record {
		fields: vec![
			("command", text("The test command as you ran it")),
			("passed", Shape::Flag { description: "Whether it passed" }),
		],
		description: None,
	};
	let decision = Shape::Record {
		fields: vec![
			("question", text("What had to be decided")),
			("choice", text("What you chose")),
			("why", text("Why")),
		],
		description: None,
	};
	Shape::Record {
		fields: vec![
			("outcome", Shape::Outcome { description: "done, blocked or failed" }),
			(
				"summary",
				Shape::Text { min: 1, max: Some(200), description: "What the task did, at most 200 characters" },
			),
			("files", list_of(text("A file path"), "The files the task changed")),
			("tests", list_of(test, "The tests you ran and whether each passed")),
			("decisions", list_of(decision, "Choices you settled that later work shouldn't reopen")),
			("questions", list_of(text("A question"), "Open questions for him")),
			("followUps", list_of(text("A follow-up"), "Work left for later")),
		],
		description: None,
	}
}

impl Shape {
	/// The shape as JSON Schema, the way typebox writes it.
	fn schema(&self) -> Value {
		match self {
			Self::Text { min, max, description } => {
				let mut schema = json!({"type": "string", "minLength": min});
				if let Some(max) = max {
					schema["maxLength"] = json!(max);
				}
				schema["description"] = json!(description);
				schema
			}
			Self::Flag { description } => json!({"type": "boolean", "description": description}),
			Self::List { items, description } => {
				json!({"type": "array", "items": items.schema(), "description": description})
			}
			Self::Outcome { description } => {
				let any_of: Vec<Value> = [Outcome::Done, Outcome::Blocked, Outcome::Failed]
					.into_iter()
					.map(|outcome| json!({"type": "string", "const": outcome.as_str()}))
					.collect();
				json!({"anyOf": any_of, "description": description})
			}
			Self::Record { fields, description } => {
				let required: Vec<&str> = fields.iter().map(|(name, _)| *name).collect();
				let properties: Map<String, Value> =
					fields.iter().map(|(name, shape)| ((*name).to_owned(), shape.schema())).collect();
				let mut schema = json!({
					"type": "object",
					"required": required,
					"properties": properties,
					"additionalProperties": false,
				});
				if let Some(description) = description {
					schema["description"] = json!(description);
				}
				schema
			}
		}
	}
}

/// The JSON Schema of `report_result`'s input: what `jocasta mcp` lists as the tool's input (typebox's `ReportSchema`).
pub fn report_schema() -> Value {
	report_shape().schema()
}

/// What is wrong at `path` and how: typebox's first error for it.
enum Mismatch {
	Unknown(String),
	Message(String),
}

type Found = (Vec<String>, Mismatch);

fn message(path: &[String], text: &str) -> Option<Found> {
	Some((path.to_vec(), Mismatch::Message(text.to_owned())))
}

fn type_name(shape: &Shape) -> &'static str {
	match shape {
		Shape::Text { .. } | Shape::Outcome { .. } => "string",
		Shape::Flag { .. } => "boolean",
		Shape::List { .. } => "array",
		Shape::Record { .. } => "object",
	}
}

/// The first error typebox lists for `value` against `shape`, in its order: an object's missing properties, its
/// unknown ones, then each property in the schema's order.
fn first_error(shape: &Shape, value: &Value, path: &mut Vec<String>) -> Option<Found> {
	match (shape, value) {
		(Shape::Text { min, max, .. }, Value::String(text)) => {
			let length = text.chars().count();
			if length < *min {
				return message(path, &format!("must not have fewer than {min} characters"));
			}
			max.filter(|max| length > *max)
				.and_then(|max| message(path, &format!("must not have more than {max} characters")))
		}
		(Shape::Flag { .. }, Value::Bool(_)) => None,
		(Shape::Outcome { .. }, Value::String(text)) => (text != Outcome::Done.as_str())
			.then(|| (path.clone(), Mismatch::Message("must be equal to constant".to_owned()))),
		(Shape::List { items, .. }, Value::Array(values)) => values.iter().enumerate().find_map(|(i, item)| {
			path.push(i.to_string());
			let found = first_error(items, item, path);
			path.pop();
			found
		}),
		(Shape::Record { fields, .. }, Value::Object(object)) => record_error(fields, object, path),
		_ => message(path, &format!("must be {}", type_name(shape))),
	}
}

fn record_error(
	fields: &[(&'static str, Shape)],
	object: &Map<String, Value>,
	path: &mut Vec<String>,
) -> Option<Found> {
	let missing: Vec<&str> = fields.iter().map(|(name, _)| *name).filter(|name| !object.contains_key(*name)).collect();
	if !missing.is_empty() {
		return message(path, &format!("must have required properties {}", missing.join(", ")));
	}
	if let Some(unknown) = object.keys().find(|key| !fields.iter().any(|(name, _)| name == key)) {
		return Some((path.clone(), Mismatch::Unknown(unknown.clone())));
	}
	fields.iter().find_map(|(name, shape)| {
		path.push((*name).to_owned());
		let found = first_error(shape, &object[*name], path);
		path.pop();
		found
	})
}

/// The report, or the first reason it doesn't fit the schema (`report_result: <path> <message>`).
pub fn parse_report(value: &Value) -> Result<ThreadReport, String> {
	let Some((path, mismatch)) = first_error(&report_shape(), value, &mut Vec::new()) else {
		return serde_json::from_value(value.clone())
			.map_err(|_| format!("{REPORT_RESULT}: the report doesn't fit its schema"));
	};
	let at = if path.is_empty() { "the report".to_owned() } else { path.join(".") };
	Err(match mismatch {
		Mismatch::Unknown(field) => format!("{REPORT_RESULT}: unknown field {field} in {at}"),
		Mismatch::Message(text) => format!("{REPORT_RESULT}: {at} {text}"),
	})
}

/// JavaScript's `text.replace(/\s+/g, " ").trim()`.
fn flat(text: &str) -> String {
	text.split(|c: char| c.is_whitespace() || c == '\u{feff}')
		.filter(|part| !part.is_empty())
		.collect::<Vec<_>>()
		.join(" ")
}

/// The report as one line (task file, review prompt, `thread_status`): `outcome done · "…" · tests 1/1 · decisions: q = c`.
pub fn report_line(report: &ThreadReport) -> String {
	let passed = report.tests.iter().filter(|test| test.passed).count();
	let flats =
		|items: &[String], separator: &str| items.iter().map(|item| flat(item)).collect::<Vec<_>>().join(separator);
	let mut parts = vec![
		format!("outcome {}", report.outcome.as_str()),
		serde_json::to_string(&flat(&report.summary)).unwrap_or_default(),
		format!("tests {passed}/{}", report.tests.len()),
	];
	if !report.files.is_empty() {
		parts.push(format!("files: {}", flats(&report.files, ", ")));
	}
	if !report.decisions.is_empty() {
		let decisions: Vec<String> =
			report.decisions.iter().map(|d| format!("{} = {}", flat(&d.question), flat(&d.choice))).collect();
		parts.push(format!("decisions: {}", decisions.join("; ")));
	}
	if !report.questions.is_empty() {
		parts.push(format!("questions: {}", flats(&report.questions, "; ")));
	}
	if !report.follow_ups.is_empty() {
		parts.push(format!("follow-ups: {}", flats(&report.follow_ups, "; ")));
	}
	parts.join(" · ")
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ok() -> Value {
		json!({
			"outcome": "done",
			"summary": "s",
			"files": ["a"],
			"tests": [{"command": "c", "passed": true}],
			"decisions": [{"question": "q", "choice": "c", "why": "w"}],
			"questions": ["q"],
			"followUps": ["f"],
		})
	}

	fn with(key: &str, value: Value) -> Value {
		let mut report = ok();
		report[key] = value;
		report
	}

	fn problem(value: &Value) -> String {
		parse_report(value).unwrap_err()
	}

	#[test]
	fn a_fitting_report_parses_to_the_typed_report() {
		let report = parse_report(&ok()).unwrap();
		assert_eq!(report.outcome, Outcome::Done);
		assert_eq!(report.follow_ups, ["f"]);
		assert_eq!(serde_json::to_value(&report).unwrap(), ok());
		assert!(parse_report(&with("summary", json!("😀".repeat(150)))).is_ok());
		assert!(parse_report(&with("summary", json!("x".repeat(200)))).is_ok());
	}

	#[test]
	fn a_report_that_doesnt_fit_names_the_first_problem_as_typebox_words_it() {
		let cases: Vec<(Value, &str)> = vec![
			(Value::Null, "report_result: the report must be object"),
			(json!([]), "report_result: the report must be object"),
			(
				json!({}),
				"report_result: the report must have required properties outcome, summary, files, tests, decisions, questions, followUps",
			),
			(
				json!({"outcome": "done"}),
				"report_result: the report must have required properties summary, files, tests, decisions, questions, followUps",
			),
			(with("outcome", json!("x")), "report_result: outcome must be equal to constant"),
			(with("outcome", json!(1)), "report_result: outcome must be string"),
			(with("summary", json!("")), "report_result: summary must not have fewer than 1 characters"),
			(with("summary", json!("x".repeat(201))), "report_result: summary must not have more than 200 characters"),
			(with("summary", json!(3)), "report_result: summary must be string"),
			(with("files", json!("a")), "report_result: files must be array"),
			(with("files", Value::Null), "report_result: files must be array"),
			(with("files", json!([""])), "report_result: files.0 must not have fewer than 1 characters"),
			(with("files", json!([1])), "report_result: files.0 must be string"),
			(with("tests", json!([{"command": "c"}])), "report_result: tests.0 must have required properties passed"),
			(with("tests", json!([{"command": "c", "passed": "y"}])), "report_result: tests.0.passed must be boolean"),
			(with("tests", json!(["a"])), "report_result: tests.0 must be object"),
			(
				with("tests", json!([{"command": "c", "passed": true}, {"command": 1, "passed": true}])),
				"report_result: tests.1.command must be string",
			),
			(
				with("decisions", json!([{"question": "q", "choice": "c"}])),
				"report_result: decisions.0 must have required properties why",
			),
			(with("followUps", json!([null])), "report_result: followUps.0 must be string"),
		];
		for (value, expected) in cases {
			assert_eq!(problem(&value), expected, "{value}");
		}
	}

	#[test]
	fn an_unknown_field_is_named_and_comes_before_the_properties_checked_after_it() {
		assert_eq!(problem(&with("extra", json!(1))), "report_result: unknown field extra in the report");
		let both = with("summary", json!(""));
		let mut both = both;
		both["extra"] = json!(1);
		assert_eq!(problem(&both), "report_result: unknown field extra in the report");
		let tests = with("tests", json!([{"command": "c", "passed": true, "x": 1}]));
		assert_eq!(problem(&tests), "report_result: unknown field x in tests.0");
		let decisions = with("decisions", json!([{"question": "q", "choice": "c", "why": "w", "z": 1}]));
		assert_eq!(problem(&decisions), "report_result: unknown field z in decisions.0");
		// Missing properties come before unknown ones in the same object.
		let tests = with("tests", json!([{"x": 1}]));
		assert_eq!(problem(&tests), "report_result: tests.0 must have required properties command, passed");
	}

	#[test]
	fn the_schema_is_typeboxs_json_for_the_tools_input() {
		let schema = report_schema();
		assert_eq!(schema["type"], "object");
		assert_eq!(schema["additionalProperties"], false);
		assert_eq!(
			schema["required"],
			json!(["outcome", "summary", "files", "tests", "decisions", "questions", "followUps"])
		);
		assert_eq!(
			schema["properties"]["outcome"],
			json!({"anyOf": [{"type": "string", "const": "done"}, {"type": "string", "const": "blocked"}, {"type": "string", "const": "failed"}], "description": "done, blocked or failed"})
		);
		assert_eq!(
			schema["properties"]["summary"],
			json!({"type": "string", "minLength": 1, "maxLength": 200, "description": "What the task did, at most 200 characters"})
		);
		assert_eq!(
			schema["properties"]["tests"],
			json!({"type": "array", "items": {"type": "object", "required": ["command", "passed"], "properties": {"command": {"type": "string", "minLength": 1, "description": "The test command as you ran it"}, "passed": {"type": "boolean", "description": "Whether it passed"}}, "additionalProperties": false}, "description": "The tests you ran and whether each passed"})
		);
		assert_eq!(
			schema["properties"]["decisions"]["items"]["properties"]["why"],
			json!({"type": "string", "minLength": 1, "description": "Why"})
		);
		assert_eq!(
			schema["properties"]["followUps"],
			json!({"type": "array", "items": {"type": "string", "minLength": 1, "description": "A follow-up"}, "description": "Work left for later"})
		);
	}

	#[test]
	fn the_report_is_one_line() {
		let mut report = parse_report(&ok()).unwrap();
		assert_eq!(
			report_line(&report),
			"outcome done · \"s\" · tests 1/1 · files: a · decisions: q = c · questions: q · follow-ups: f"
		);
		report.summary = "two\n  lines \"quoted\"".to_owned();
		report.tests.push(ReportedTest { command: "d".to_owned(), passed: false });
		report.files = vec!["a\tb".to_owned(), "c".to_owned()];
		report.decisions.clear();
		report.questions = vec!["x  y".to_owned(), "z".to_owned()];
		report.follow_ups.clear();
		assert_eq!(
			report_line(&report),
			"outcome done · \"two lines \\\"quoted\\\"\" · tests 1/2 · files: a b, c · questions: x y; z"
		);
		let bare = ThreadReport {
			outcome: Outcome::Blocked,
			summary: "s".to_owned(),
			files: vec![],
			tests: vec![],
			decisions: vec![],
			questions: vec![],
			follow_ups: vec![],
		};
		assert_eq!(report_line(&bare), "outcome blocked · \"s\" · tests 0/0");
	}
}
