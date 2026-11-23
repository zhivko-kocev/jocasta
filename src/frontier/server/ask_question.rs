//! The question an omp `ask` call puts to him, as one line (0589). Since omp 18.18.3 the call's arguments are one HTML
//! form, `{html}` (`tools/ask.ts:71-73`); before, `{questions: [{question, …}]}`. For a form the question is the first
//! named control's, worded as omp words it for its own blocked status (`modes/controllers/extension-ui-controller.ts:970-971`,
//! pi-tui `html-form/data.ts:134-158` `fieldQuestions`): the control's own label (a text field, number, select or
//! textarea wrapped in `<label>`, a `<label for>`, its `aria-label` or `title`; a radio or checkbox group has none),
//! else the text block just before it (`<p>`, a heading, …), else its fieldset's `<legend>`, else a lone checkbox's
//! label, else its name. Read with a tag scanner, not a DOM: the form is the model's plain semantic HTML
//! (`prompts/tools/ask.md`).

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde_json::Value;

use crate::shared::fsx::as_object;

fn pattern(source: &str) -> Regex {
	Regex::new(source).expect("a fixed pattern")
}

/// The opening of an element whose content is never form text (pi-tui `html-form/parse.ts` `SKIPPED`, and pictures),
/// or of a comment; [`drop_elements`] finds the end.
static DROPPED_OPEN: LazyLock<Regex> =
	LazyLock::new(|| pattern(r"(?i)<(script|style|template|button|head|noscript|iframe|title|svg)(?-u:\b)|<!--"));
static CONTROL_TAG: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)<(input|select|textarea)(?-u:\b)"));
static OPTION_TAG: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)<option(?-u:\b)"));
static TAG: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)<(/?)\s*([a-z][a-z0-9]*)(?-u:\b)[^>]*>"));
static ANY_TAG: LazyLock<Regex> = LazyLock::new(|| pattern(r"<[^>]*>"));
static SELECT_OR_TEXTAREA: LazyLock<Regex> =
	LazyLock::new(|| pattern(r"(?i)<select(?-u:\b)[\s\S]*?</select\s*>|<textarea(?-u:\b)[\s\S]*?</textarea\s*>"));
static ENTITY: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)&(#x[0-9a-f]+|#[0-9]+|[a-z]+);"));
static NAME_SEPARATORS: LazyLock<Regex> = LazyLock::new(|| pattern(r"[_-]+"));
/// One pattern per attribute the scanner reads.
static ATTRIBUTES: LazyLock<HashMap<&'static str, Regex>> = LazyLock::new(|| {
	["for", "name", "type", "id", "aria-label", "title", "value"]
		.into_iter()
		.map(|name| {
			let source = format!(r#"(?i)\s{name}\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#);
			(name, pattern(&source))
		})
		.collect()
});

/// `html` without the dropped elements and comments, each replaced by one space.
fn drop_elements(html: &str) -> String {
	let mut out = String::with_capacity(html.len());
	let mut copied = 0;
	let mut search = 0;
	while let Some(found) = DROPPED_OPEN.captures_at(html, search) {
		let Some(open) = found.get(0) else { break };
		let end = match found.get(1) {
			Some(name) => {
				let closer = pattern(&format!(r"(?i)</{}\s*>", name.as_str().to_ascii_lowercase()));
				closer.find_at(html, open.end()).map(|m| m.end())
			}
			None => html[open.end()..].find("-->").map(|at| open.end() + at + 3),
		};
		if let Some(end) = end {
			out.push_str(&html[copied..open.start()]);
			out.push(' ');
			copied = end;
			search = end;
		} else {
			// No closer: this opening is plain text; the next candidate starts after its `<`.
			search = open.start() + 1;
		}
	}
	out.push_str(&html[copied..]);
	out
}

fn entity(name: &str) -> Option<&'static str> {
	Some(match name.to_ascii_lowercase().as_str() {
		"amp" => "&",
		"lt" => "<",
		"gt" => ">",
		"quot" => "\"",
		"apos" => "'",
		"nbsp" => " ",
		_ => return None,
	})
}

fn decode(text: &str) -> String {
	ENTITY
		.replace_all(text, |found: &Captures| {
			let whole = found[0].to_owned();
			let name = &found[1];
			let Some(digits) = name.strip_prefix('#') else {
				return entity(name).map_or(whole, str::to_owned);
			};
			let code = match digits.strip_prefix(['x', 'X']) {
				Some(hex) => u32::from_str_radix(hex, 16).ok(),
				None => digits.parse::<u32>().ok(),
			};
			code.and_then(char::from_u32).map_or(whole, String::from)
		})
		.into_owned()
}

/// Markup as one line of text: tags out, entities decoded, whitespace runs one space.
fn one_line(markup: &str) -> String {
	let text = decode(&ANY_TAG.replace_all(markup, " "));
	text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A `<label>`'s text without its controls' own text (a select's options, a textarea's value; `parse.ts` `#spansOf`).
fn label_text(markup: &str) -> String {
	one_line(&SELECT_OR_TEXTAREA.replace_all(markup, " "))
}

/// A tag's attribute value, entities decoded; `None` when absent.
fn attribute(tag: &str, name: &str) -> Option<String> {
	let found = ATTRIBUTES.get(name)?.captures(tag)?;
	let value = found.get(1).or_else(|| found.get(2)).or_else(|| found.get(3)).map_or("", |m| m.as_str());
	Some(decode(value))
}

/// An attribute that is present and not empty (what the TypeScript tests for truthiness).
fn attribute_set(tag: &str, name: &str) -> Option<String> {
	attribute(tag, name).filter(|value| !value.is_empty())
}

/// The first of `candidates` that isn't empty, else an empty string.
fn first_set<'a>(candidates: &[&'a str]) -> &'a str {
	candidates.iter().find(|text| !text.is_empty()).copied().unwrap_or("")
}

/// The open `<label>`: where it ends and its text (its control's own label, or an option's).
struct OpenLabel {
	end: usize,
	text: String,
}

/// One pass of the tag scanner: the caption, legend, pending text and open label are state every tag case reads (0610).
struct Scan<'a> {
	doc: &'a str,
	/// `doc` lowercased in ASCII, so a byte offset in one is the same in the other.
	lower: String,
	label_for: HashMap<String, String>,
	names: HashMap<String, usize>,
	legend: String,
	caption: String,
	pending: String,
	label: Option<OpenLabel>,
	/// Where the text after the last tag begins, and where the next tag is looked for.
	at: usize,
	pos: usize,
}

impl<'a> Scan<'a> {
	fn new(doc: &'a str) -> Self {
		Scan {
			doc,
			lower: doc.to_ascii_lowercase(),
			label_for: HashMap::new(),
			names: HashMap::new(),
			legend: String::new(),
			caption: String::new(),
			pending: String::new(),
			label: None,
			at: 0,
			pos: 0,
		}
	}

	/// The offset of `closer` from `from`, else the document's end.
	fn until(&self, closer: &str, from: usize) -> usize {
		self.lower[from..].find(closer).map_or(self.doc.len(), |at| from + at)
	}

	/// The first pass: each `<label for>`'s text and how many controls carry each name.
	fn index_labels(&mut self) {
		for tag in TAG.captures_iter(self.doc) {
			let (Some(whole), Some(slash), Some(name)) = (tag.get(0), tag.get(1), tag.get(2)) else { continue };
			if !slash.as_str().is_empty() {
				continue;
			}
			let name = name.as_str().to_ascii_lowercase();
			if name == "label" {
				let start = whole.end();
				if let Some(id) = attribute_set(whole.as_str(), "for")
					&& !self.label_for.contains_key(&id)
				{
					let text = label_text(&self.doc[start..self.until("</label", start)]);
					self.label_for.insert(id, text);
				}
			} else if matches!(name.as_str(), "input" | "select" | "textarea")
				&& let Some(control) = attribute_set(whole.as_str(), "name")
			{
				*self.names.entry(control).or_insert(0) += 1;
			}
		}
	}

	/// The second pass: the first named control's question; `None` when the form has none.
	fn question(&mut self) -> Option<String> {
		while let Some(tag) = TAG.captures_at(self.doc, self.pos) {
			let (whole, slash, name) = (tag.get(0)?, tag.get(1)?, tag.get(2)?);
			let (start, end) = (whole.start(), whole.end());
			self.pos = end;
			if self.label.is_none() {
				self.pending.push_str(&self.doc[self.at..start]);
			}
			self.at = end;
			if self.label.as_ref().is_some_and(|open| start >= open.end) {
				self.label = None;
			}
			let name = name.as_str().to_ascii_lowercase();
			if let Some(question) = self.tag(whole.as_str(), slash.as_str() == "/", &name) {
				return Some(question);
			}
		}
		None
	}

	/// One tag; the question when it is the first named control.
	fn tag(&mut self, whole: &str, closing: bool, name: &str) -> Option<String> {
		match name {
			"fieldset" | "legend" => {
				if name == "legend" && closing {
					self.legend = one_line(&self.pending);
				}
				if name == "fieldset" {
					self.legend.clear();
				}
				self.pending.clear();
				self.caption.clear();
			}
			"label" if !closing => {
				let end = self.until("</label", self.at);
				self.label = Some(OpenLabel { end, text: label_text(&self.doc[self.at..end]) });
			}
			"ul" | "ol" | "table" | "pre" if !closing => self.context(name),
			"br" => self.pending.push(' '),
			"img" if !closing => {
				self.pending.clear();
				self.caption.clear();
			}
			"p" | "div" | "li" | "dd" | "dt" | "dl" | "section" | "article" | "aside" | "header" | "footer"
			| "main" | "nav" | "blockquote" | "details" | "figure" | "figcaption" | "address" | "hr" | "tr" | "td"
			| "th" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "summary" => self.end_text_block(),
			"input" | "select" | "textarea" if !closing => return self.control(whole, name),
			_ => {}
		}
		None
	}

	/// A block's end (or start) ends the text before it: the last text block is the caption.
	fn end_text_block(&mut self) {
		let text = one_line(&self.pending);
		if !text.is_empty() {
			self.caption = text;
		}
		self.pending.clear();
	}

	/// A list, table or preformatted block opens (`parse.ts` `#list`, `#table`, `pre`).
	fn context(&mut self, name: &str) {
		let end = self.until(&format!("</{name}"), self.at);
		if CONTROL_TAG.is_match(&self.doc[self.at..end]) {
			// A list or table of controls is layout: its text before it stays the caption.
			let text = one_line(&self.pending);
			if !text.is_empty() {
				self.caption = text;
			}
		} else {
			// A list, table or preformatted block without controls is context, not a question.
			self.caption.clear();
			self.at = end;
			self.pos = end;
		}
		self.pending.clear();
	}

	/// A control: the question when it is named, else it only ends the caption.
	fn control(&mut self, whole: &str, name: &str) -> Option<String> {
		let kind = if name == "input" {
			attribute(whole, "type").unwrap_or_else(|| "text".to_owned()).to_lowercase()
		} else {
			name.to_owned()
		};
		// `<input>` types that aren't fields (`parse.ts` `IGNORED_INPUTS`, and `hidden`).
		if matches!(kind.as_str(), "hidden" | "submit" | "button" | "reset" | "image" | "file") {
			return None;
		}
		self.end_text_block();
		let Some(control) = attribute_set(whole, "name") else {
			// An unnamed control is a field omp shows but `fieldQuestions` callers skip; it ends the caption.
			self.caption.clear();
			return None;
		};
		let choice = kind == "radio" || kind == "checkbox";
		let id = attribute_set(whole, "id");
		let wrapping = if choice { "" } else { self.label.as_ref().map_or("", |open| open.text.as_str()) };
		let detached = id.as_ref().and_then(|id| self.label_for.get(id)).cloned().unwrap_or_else(|| {
			one_line(&attribute(whole, "aria-label").or_else(|| attribute(whole, "title")).unwrap_or_default())
		});
		// A text field without a label is labelled by its name (`parse.ts` `#nameLabel`: `sentry_dsn` → `sentry dsn`).
		let by_name =
			if name == "select" { String::new() } else { NAME_SEPARATORS.replace_all(&control, " ").into_owned() };
		let own = if choice { "" } else { first_set(&[wrapping, &detached, &by_name]) };
		let mut lone = String::new();
		if choice && self.names.get(&control) == Some(&1) {
			let value = attribute_set(whole, "value").unwrap_or_default();
			let label = self.label.as_ref().map_or("", |open| open.text.as_str());
			lone = first_set(&[label, &value, &control]).to_owned();
		}
		if name == "select" {
			let options = &self.doc[self.at..self.until("</select", self.at)];
			if OPTION_TAG.find_iter(options).count() == 1 {
				lone = one_line(options);
			}
		}
		Some(one_line(first_set(&[own, &self.caption, &self.legend, &lone, &control])))
	}
}

/// The form's first named control's question, as omp words it; `None` when the form has no named control.
pub fn form_question(html: &str) -> Option<String> {
	let doc = drop_elements(html);
	let mut scan = Scan::new(&doc);
	scan.index_labels();
	scan.question()
}

/// The question of an `ask` call's arguments: omp ≥ 18.18.3's form (`{html}`), else the first of omp ≤ 18.18.2's
/// `questions[]`; `None` when neither holds one.
pub fn ask_question(args: &Value) -> Option<String> {
	let fields = as_object(args);
	if let Some(html) = fields.and_then(|fields| fields.get("html")).and_then(Value::as_str) {
		return form_question(html);
	}
	let first = fields.and_then(|fields| fields.get("questions")).and_then(Value::as_array).and_then(|all| all.first());
	first.and_then(as_object).and_then(|first| first.get("question")).and_then(Value::as_str).map(str::to_owned)
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::*;

	// Each expectation is what omp 18.22.0's own `fieldQuestions` (pi-tui `html-form/data.ts:141-165`) gives for the
	// form's first named field, checked against omp's parser when these cases were written (0589).

	fn question(html: &str) -> Option<String> {
		form_question(html)
	}

	#[test]
	fn omps_own_example_form_the_radio_groups_fieldset_legend_not_the_context_before_it() {
		// omp 18.22.0 `prompts/tools/ask.md:29-37`, the example form.
		let html = r#"<form>
  <p>Sessions live in memory and are lost on restart.</p>
  <fieldset>
    <legend>Session store</legend>
    <label><input type="radio" name="store" value="sqlite" checked> SQLite: one file, no new service</label>
    <label><input type="radio" name="store" value="redis"> Redis: shared across instances, needs ops</label>
  </fieldset>
  <label>Session lifetime (days) <input type="number" name="ttl_days" value="30" min="1"></label>
</form>"#;
		assert_eq!(question(html).as_deref(), Some("Session store"));
	}

	#[test]
	fn a_fields_own_label_comes_first_wrapping_label_for_aria_label_title() {
		let wrapped = r#"<form><p>Context first.</p><label>What should the branch be called? <input type="text" name="branch" value="fix/x"></label></form>"#;
		assert_eq!(question(wrapped).as_deref(), Some("What should the branch be called?"));
		let label_for = r#"<form><label for="t">Ticket id</label><input id="t" name="ticket"></form>"#;
		assert_eq!(question(label_for).as_deref(), Some("Ticket id"));
		let aria = r#"<form><select name="env" aria-label="Target environment"><option>prod</option></select></form>"#;
		assert_eq!(question(aria).as_deref(), Some("Target environment"));
		let title = r#"<form><textarea name="msg" title="Commit message"></textarea></form>"#;
		assert_eq!(question(title).as_deref(), Some("Commit message"));
		// A select's options are not its label's text.
		let select = r#"<form><label>Target <select name="env"><option>prod</option><option>dev</option></select></label></form>"#;
		assert_eq!(question(select).as_deref(), Some("Target"));
	}

	const GROUP: &str = r#"<label><input type="radio" name="db" value="a" checked> A</label><label><input type="radio" name="db" value="b"> B</label>"#;

	#[test]
	fn a_group_without_its_own_label_takes_the_text_block_just_before_it() {
		let with_group = |before: &str| question(&format!("<form>{before}{GROUP}</form>"));
		assert_eq!(
			with_group("<p>Which database should the cache use?</p>").as_deref(),
			Some("Which database should the cache use?")
		);
		let select =
			r#"<form><h3>Deploy now?</h3><select name="go"><option>Yes</option><option>No</option></select></form>"#;
		assert_eq!(question(select).as_deref(), Some("Deploy now?"));
		assert_eq!(with_group("Choose one: ").as_deref(), Some("Choose one:"));
		// The last text block before the group, not every text before it; a line break inside a block is a space.
		assert_eq!(
			with_group("<p>Sessions are lost on restart.</p><p>Which store keeps them?</p>").as_deref(),
			Some("Which store keeps them?")
		);
		assert_eq!(with_group("<div>Line one<br>Line two</div>").as_deref(), Some("Line one Line two"));
		// A hidden input is no field; an unnamed control is, and ends the caption.
		assert_eq!(
			with_group(r#"<input type="hidden" name="k" value="1"><p>Ship it?</p>"#).as_deref(),
			Some("Ship it?")
		);
		assert_eq!(with_group(r#"<p>Note</p><input type="text">"#).as_deref(), Some("db"));
		// A list or table without controls is context between the text and the group: no caption.
		assert_eq!(with_group("<p>Findings</p><ul><li>one</li><li>two</li></ul>").as_deref(), Some("db"));
		// A list of controls is layout: the text before it stays the question.
		let layout = r#"<form><p>Pick a store</p><ul><li><label><input type="radio" name="s" value="a"> A</label></li><li><label><input type="radio" name="s" value="b"> B</label></li></ul></form>"#;
		assert_eq!(question(layout).as_deref(), Some("Pick a store"));
	}

	#[test]
	fn then_the_fieldsets_legend_entities_decoded() {
		let html = r#"<form><fieldset><legend>Keep &quot;main&quot; &amp; rebase?</legend><label><input type="radio" name="k" value="y"> Yes</label><label><input type="radio" name="k" value="n"> No</label></fieldset></form>"#;
		assert_eq!(question(html).as_deref(), Some(r#"Keep "main" & rebase?"#));
	}

	#[test]
	fn then_a_lone_checkbox_or_radios_label_then_the_fields_name() {
		let checkbox =
			r#"<form><label><input type="checkbox" name="force" value="1"> Force-push the branch</label></form>"#;
		assert_eq!(question(checkbox).as_deref(), Some("Force-push the branch"));
		let radio = r#"<form><label><input type="radio" name="only" value="x"> Only choice</label></form>"#;
		assert_eq!(question(radio).as_deref(), Some("Only choice"));
		// A text field without a label is labelled by its name, `_` and `-` as spaces; a group keeps its name as is.
		let text = r#"<form><p>Ship it?</p><input type="text" name="ship_note"></form>"#;
		assert_eq!(question(text).as_deref(), Some("ship note"));
		let group = r#"<form><label><input type="radio" name="db_kind" value="a"> A</label><label><input type="radio" name="db_kind" value="b"> B</label></form>"#;
		assert_eq!(question(group).as_deref(), Some("db_kind"));
	}

	#[test]
	fn a_form_without_a_named_control_has_no_question() {
		assert_eq!(question(r#"<form><p>Just text</p><input type="submit" name="go"></form>"#), None);
	}

	#[test]
	fn an_ask_calls_arguments_omp_18_20_2s_form_or_an_older_sessions_questions() {
		let html = r#"<form><p>CSV or JSON?</p><label><input type="radio" name="f" value="csv" checked> CSV</label><label><input type="radio" name="f" value="json"> JSON</label></form>"#;
		assert_eq!(ask_question(&json!({ "html": html })).as_deref(), Some("CSV or JSON?"));
		// omp 18.18.2's shape (`tools/ask.ts:150-151`, which 19.1.1 still reads: `legacyQuestionsHtml`).
		let older = json!({ "questions": [{ "question": "CSV or JSON?", "options": [{ "label": "CSV" }] }, { "question": "Later" }] });
		assert_eq!(ask_question(&older).as_deref(), Some("CSV or JSON?"));
		assert_eq!(ask_question(&json!({ "questions": [] })), None);
		assert_eq!(ask_question(&json!({})), None);
		assert_eq!(ask_question(&Value::Null), None);
	}
}
