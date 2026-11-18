//! The run report (0165, 0166): a static page with the pass rate and one row per trial.

use std::collections::HashMap;

use crate::evals::runner::{EvalCase, ResultRow};

/// What the numbers leave out (0375); the report and `compare.md` both say so.
pub const MEASUREMENT_NOTES: [&str; 5] = [
	"Total tokens and requests are omp's JSON stream: the main session's assistant turns plus the subagents each foreground `task` call ran, from its result details. Background task jobs, subagents' own subagents, and model calls that are not session turns (compaction summaries, titles, memory) are not counted.",
	"A variant's settings reach omp through the trial's `--config` overlay, layered over `memory.backend: off`; trials use his `~/.omp/agent` logins and settings otherwise, or the named `--profile`'s.",
	"Claude Code trials read its stream-json `result` events: cost and tokens as Claude Code reports them, subagents included, turns from `num_turns`.",
	"opencode trials read `opencode export` of each session: tokens and cost as opencode reports them, reasoning counted as output.",
	"The task benchmark also prices every arm's saved records with one pinned price table (`bench/prices/`, 0538): its `ledger` field and `ledger.jsonl`.",
];

fn escape_html(text: &str) -> String {
	text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// JavaScript's `toFixed`: the exact decimal value rounded to `digits` places, a tie away from zero.
pub fn to_fixed(x: f64, digits: usize) -> String {
	if !x.is_finite() {
		return if x.is_nan() {
			"NaN".to_owned()
		} else if x > 0.0 {
			"Infinity".to_owned()
		} else {
			"-Infinity".to_owned()
		};
	}
	if x < 0.0 {
		let text = to_fixed(-x, digits);
		return if text.bytes().all(|b| b == b'0' || b == b'.') { text } else { format!("-{text}") };
	}
	let exact = format!("{x:.80}");
	let (whole, fraction) = exact.split_once('.').unwrap_or((&exact, ""));
	let mut kept: Vec<u8> = whole.bytes().chain(fraction.bytes().take(digits)).collect();
	if fraction.as_bytes().get(digits).is_some_and(|d| *d >= b'5') {
		let mut at = kept.len();
		loop {
			if at == 0 {
				kept.insert(0, b'1');
				break;
			}
			at -= 1;
			if kept[at] == b'9' {
				kept[at] = b'0';
			} else {
				kept[at] += 1;
				break;
			}
		}
	}
	let split = kept.len() - digits;
	let text = String::from_utf8_lossy(&kept).into_owned();
	if digits == 0 { text } else { format!("{}.{}", &text[..split], &text[split..]) }
}

/// A number as JavaScript's `String(n)` writes it for the values here: a whole one without a fraction.
pub fn js_text(value: f64) -> String {
	if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e21 {
		format!("{}", value as i128)
	} else {
		format!("{value}")
	}
}

fn report_row(row: &ResultRow, tags: &HashMap<&str, String>) -> String {
	let failed: Vec<&str> =
		row.grade.iter().filter(|(_, score)| score.as_f64() != Some(1.0)).map(|(label, _)| label.as_str()).collect();
	let transcript = format!("transcripts/{}-{}.jsonl", row.case, row.rep);
	let cells = [
		escape_html(&row.case),
		escape_html(tags.get(row.case.as_str()).map_or("", String::as_str)),
		row.rep.to_string(),
		escape_html(row.model.as_deref().unwrap_or_default()),
		js_text(row.usage.input),
		js_text(row.usage.output),
		js_text(row.usage.cache_read),
		js_text(row.total_usage.total_tokens),
		row.requests.to_string(),
		row.turns.to_string(),
		row.tool_calls.to_string(),
		to_fixed(row.wall_ms / 1000.0, 1),
		format!("+{} \u{2212}{}", row.lines_added, row.lines_removed),
		if row.passed { "pass" } else { "fail" }.to_owned(),
		escape_html(&failed.join("; ")),
		format!("<a href=\"{}\">{}</a>", escape_html(&transcript), escape_html(&transcript)),
	];
	let class = if row.passed { "pass" } else { "fail" };
	let cells: String = cells.iter().map(|cell| format!("<td>{cell}</td>")).collect();
	format!("<tr class=\"{class}\">{cells}</tr>")
}

/// A static page: the pass rate with its noise floor (about 1/sqrt(trials), 0079) and one row per trial.
pub fn render_report(rows: &[ResultRow], cases: &[EvalCase], reps: u64, label: &str, harness_version: &str) -> String {
	let passed = rows.iter().filter(|row| row.passed).count();
	let (rate, noise) = if rows.is_empty() {
		(0.0, 0.0)
	} else {
		(100.0 * passed as f64 / rows.len() as f64, 100.0 / (rows.len() as f64).sqrt())
	};
	let tags: HashMap<&str, String> = cases.iter().map(|case| (case.id.as_str(), case.tags.join(", "))).collect();
	let body = rows.iter().map(|row| report_row(row, &tags)).collect::<Vec<_>>().join("\n");
	let notes =
		MEASUREMENT_NOTES.iter().map(|note| format!("<p>{}</p>", escape_html(note))).collect::<Vec<_>>().join("\n");
	format!(
		"<!doctype html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<title>Jocasta evals</title>
<style>
body {{ font-family: system-ui, sans-serif; margin: 2rem; }}
table {{ border-collapse: collapse; }}
th, td {{ border: 1px solid #ccc; padding: 0.25rem 0.5rem; text-align: left; }}
tr.fail td {{ background: #fde8e8; }}
</style>
</head>
<body>
<h1>Jocasta evals</h1>
<p>{}, variant {}: {} cases × {reps} reps. Passed {passed} of {} trials: {}% ± {} points.</p>
<table>
<thead><tr><th>Case</th><th>Tags</th><th>Rep</th><th>Model</th><th>Input</th><th>Output</th><th>Cache read</th><th>Total tokens</th><th>Requests</th><th>Turns</th><th>Tool calls</th><th>Wall (s)</th><th>Lines</th><th>Result</th><th>Failed checks</th><th>Transcript</th></tr></thead>
<tbody>
{body}
</tbody>
</table>
{notes}
</body>
</html>
",
		escape_html(harness_version),
		escape_html(label),
		cases.len(),
		rows.len(),
		to_fixed(rate, 1),
		to_fixed(noise, 1),
	)
}
