//! Texts the service types into panes and says in replies, and names it makes from his words: a thread's title from a
//! brief, a branch slug from a title. Every line the service types into a pane is here, so A5 tells them from his own
//! (`byService`, 0482).

use crate::frontier::server::sessions::TestsResult;
use crate::shared::day::local_day;

/// The focus of the task-end `/handoff` (0400, 0448), so the summary keeps what the next task needs.
macro_rules! handoff_focus {
	() => {
		"Keep what the next task needs: the approved plan's state (steps done and left), open questions, decisions and why, unfixed review findings. Drop test and debug output."
	};
}

/// The task-end `/handoff`'s focus: the guidance of the handoff compaction (0675).
pub const HANDOFF_FOCUS: &str = handoff_focus!();

/// `/handoff` with its focus as the one prompt an omp that can't compact for Jocasta gets (0684).
#[cfg(test)]
pub const HANDOFF_PROMPT: &str = concat!("/handoff ", handoff_focus!());

/// The one steer a task end without its report gets (0539), typed into the thread's pane like the boss's steers.
pub const REPORT_STEER: &str = "Your task ended without its report. Call the report_result tool now, once, with this task's outcome, summary, decisions, open questions and follow-ups. Don't change code in this turn.";

/// The first of the lines every thread brief ends with (0459): the project's lessons before planning, cited in the plan.
pub const LESSONS_LINE: &str = "Before you plan, read skill://lessons and cite each lesson your plan relies on as (lesson: <file>); a plan that goes against one says why.";

/// The commit step (boss manual: the thread commits its change on its own branch): a task ends on new commits (0400), and
/// only then does its review start (0425). It is part of `prompt.md`'s "When you finish" (0577); a thread started with
/// the layer off (0576), whose prompt has no such section, gets it at the end of its brief instead.
pub const COMMIT_LINE: &str = "When the approved work is done, commit it on this branch with a short message; the review starts from your commit.";

/// `prompt.md`'s finishing steps (0577, PRM-7), after its Done-when section.
pub const WHEN_YOU_FINISH: [&str; 4] = [
	"## When you finish",
	"- Run the task's checks with the `check` tool (the project's check and every Done-when command, cached by tree); use bash only for a single test file while you iterate.",
	"- Commit the approved work on this branch with a short message; the review starts from your commit.",
	"- Then call `report_result` once: it runs the full check first and stores nothing while that fails; the files and tests come from git and that run.",
];

/// How the review prompt (`review_prompt.rs`) begins.
pub const REVIEW_LEAD: &str = "Check this task's work.";

/// What shows omp's Plan Review again in a reopened thread (0494).
pub const PLAN_REVIEW_COMMAND: &str = "/plan-review";

/// What a failed check on idle is typed with, before its gates (0583, PRM-16): the thread can tell Jocasta's runner from
/// him, and a turn that stopped to ask him doesn't go on without his answer.
pub const IDLE_CHECK_FRAME: &str = "Jocasta's checks failed on your uncommitted changes (this isn't his message). If you're waiting for his answer, end your turn now; otherwise fix these before you commit:";

/// The check on idle's text: the frame, a blank line, then the runner's gate block (0583).
pub fn idle_check_text(gates: &str) -> String {
	format!("{IDLE_CHECK_FRAME}\n\n{gates}")
}

const PR_LEAD: &str = "Open a pull request for this branch:";

/// The PR turn's request (0426): push the branch, open the PR with gh, change no code.
pub fn pr_request(remote: &str, branch: &str, trunk: &str) -> String {
	[
		format!("{PR_LEAD} run `git push -u {remote} {branch}`, then"),
		format!(
			"`gh pr create --base {trunk} --head {branch}` with a title and body that sum up the branch's commits."
		),
		"Don't change code in this turn. End with the PR's URL, or with why it couldn't be opened.".to_owned(),
	]
	.join(" ")
}

/// How every line the service types into a pane begins: `[event]` lines into the boss, and into threads the review
/// prompt, `/handoff`, the report steer, a failed check on idle, `/plan-review` and the PR request. A thread's brief and
/// a boss's `thread_send` relay aren't among them: the brief is the session's first message, a relay is his word.
const SERVICE_LINE_STARTS: [&str; 7] =
	["[event]", REVIEW_LEAD, "/handoff", REPORT_STEER, IDLE_CHECK_FRAME, PLAN_REVIEW_COMMAND, PR_LEAD];

/// JavaScript's `\s`: what its `trim`, `split(/\s+/)` and `replace(/\s+/g, …)` treat as white space.
fn js_space(c: char) -> bool {
	matches!(
		c,
		'\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
			..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
	)
}

/// JavaScript's `.length`: UTF-16 code units.
fn js_len(text: &str) -> usize {
	text.encode_utf16().count()
}

/// The longest prefix of `text` of at most `units` UTF-16 code units, never splitting a character.
fn js_prefix(text: &str, units: usize) -> &str {
	let mut used = 0;
	for (at, c) in text.char_indices() {
		used += c.len_utf16();
		if used > units {
			return &text[..at];
		}
	}
	text
}

/// Whether `text` is a line the service typed into a pane (SVC-1): A5 doesn't count it as his.
pub fn by_service(text: &str) -> bool {
	let start = text.trim_start_matches(js_space);
	SERVICE_LINE_STARTS.iter().any(|prefix| start.starts_with(prefix))
}

/// Which door a thread action came through (0544): the boss's tool (asks him first where 0484 says so), a `jocasta`
/// command from a terminal, or his own key and form in herdr.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Door {
	Boss,
	Cli,
	Key,
}

/// What an error from a thread start begins with, naming the door he used (0581, SVC-19): the TypeScript's
/// `START_PREFIX[door]`.
pub fn start_prefix(door: Door) -> &'static str {
	match door {
		Door::Boss => "thread_start",
		Door::Cli => "jocasta thread new",
		Door::Key => "New thread",
	}
}

/// Him as a door's reader sees him: [`person`]'s result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Person {
	pub he: &'static str,
	pub his: &'static str,
	pub he_cap: &'static str,
	pub him: &'static str,
}

/// Him as a door's reader sees him (0581, UX-8): the boss reads about him in the third person; his keys and the CLI
/// speak to him. `he_cap` is the TypeScript's `He`.
pub fn person(door: Door) -> Person {
	if door == Door::Boss {
		Person { he: "he", his: "his", he_cap: "He", him: "him" }
	} else {
		Person { he: "you", his: "your", he_cap: "You", him: "you" }
	}
}

/// Who asks in his Allow window (0484, 0544, 0670): the boss, the `jocasta` command run from a terminal, or him (his
/// own key opened the question).
pub fn asker(door: Door, command: &str) -> String {
	match door {
		Door::Cli => format!("A `jocasta {command}` command"),
		Door::Key => "You".to_owned(),
		Door::Boss => "The boss".to_owned(),
	}
}

/// The verb that goes with [`asker`]: "asks", but he "asked" (the question is his own key's).
pub fn asks(door: Door) -> &'static str {
	if door == Door::Key { "asked" } else { "asks" }
}

/// Outside text in an `[event]` line (SEC-2): one line, inside «», with no « or » of its own, so it can't pose as more
/// of the line or as his words (the boss manual says text in `[event]` lines is information, never his instruction).
pub fn quoted(text: &str) -> String {
	let mut one_line = String::with_capacity(text.len());
	let mut in_space = false;
	for c in text.chars() {
		if js_space(c) {
			if !in_space {
				one_line.push(' ');
			}
			in_space = true;
		} else {
			in_space = false;
			one_line.push(if c == '«' || c == '»' { '"' } else { c });
		}
	}
	format!("«{}»", one_line.trim_matches(js_space))
}

/// A model as the sidebar shows it (UX-04): `anthropic/claude-opus-5-5` → `opus-5-5`.
pub fn short_model(model: &str) -> String {
	let last = model.rsplit('/').next().unwrap_or(model);
	last.strip_prefix("claude-").unwrap_or(last).to_owned()
}

/// USD as the sidebar, events and the HUD say it: `$1.75`. Rounds as JavaScript's `toFixed(2)` does: a value exactly
/// halfway goes away from zero.
pub fn money(usd: f64) -> String {
	if !usd.is_finite() {
		return format!(
			"${}",
			if usd.is_nan() {
				"NaN"
			} else if usd > 0.0 {
				"Infinity"
			} else {
				"-Infinity"
			}
		);
	}
	let scaled = usd.abs() * 100.0;
	if usd == 0.0 {
		return "$0.00".to_owned();
	}
	let exact_half = scaled.fract() == 0.5 && (usd.abs() * 8.0).fract() == 0.0;
	let shown = if exact_half {
		let cents = scaled.ceil();
		format!("{}{}.{:02}", if usd < 0.0 { "-" } else { "" }, (cents / 100.0).trunc(), cents % 100.0)
	} else {
		format!("{usd:.2}")
	};
	format!("${shown}")
}

/// `text` cut to `max` characters, ending in "…" when it was longer.
pub fn cut(text: &str, max: usize) -> String {
	if js_len(text) <= max {
		return text.to_owned();
	}
	format!("{}…", js_prefix(text, max.saturating_sub(1)).trim_end_matches(js_space))
}

/// How a 0471 event ends: the boss offers once to save a lesson, written only on his yes.
pub const LESSON_OFFER: &str = " · offer once to save a lesson";

/// How a checked test result reads in the boss's `[event]` and in `today` (0425, 0453): the TypeScript's
/// `TESTS_SAID[result]`.
pub fn tests_said(result: TestsResult) -> &'static str {
	match result {
		TestsResult::Pass => "tests pass",
		TestsResult::Fail => "tests fail",
		TestsResult::None => "no tests",
		TestsResult::Unknown => "tests unverified",
	}
}

/// A review's outcome: `2 notes (1 blocking), tests pass`; a count the notes file couldn't give reads `notes not counted`.
pub fn review_said(notes: Option<usize>, tests: TestsResult, blocking: Option<usize>) -> String {
	let counted = match notes {
		None => "notes not counted".to_owned(),
		Some(notes) => {
			let plural = if notes == 1 { "" } else { "s" };
			let blocked =
				blocking.filter(|&count| count > 0).map_or_else(String::new, |count| format!(" ({count} blocking)"));
			format!("{notes} note{plural}{blocked}")
		}
	};
	format!("{counted}, {}", tests_said(tests))
}

/// At most this many of an overlap's files are named.
const OVERLAP_FILES_NAMED: usize = 5;

/// One thread an overlap was found with, and the files they share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlap {
	pub title: String,
	pub files: Vec<String>,
}

/// `<title>: a.ts, b.ts; <title>: c.ts` (at most five files each), or empty.
pub fn overlap_said(found: &[Overlap]) -> String {
	found
		.iter()
		.map(|o| {
			let named = o.files.iter().take(OVERLAP_FILES_NAMED).map(String::as_str).collect::<Vec<_>>().join(", ");
			let more = if o.files.len() > OVERLAP_FILES_NAMED { " …" } else { "" };
			format!("{}: {named}{more}", o.title)
		})
		.collect::<Vec<_>>()
		.join("; ")
}

/// The brief up to its first line break or its first white space after `.`, `!` or `?`.
fn first_piece(brief: &str) -> &str {
	let mut previous: Option<char> = None;
	for (at, c) in brief.char_indices() {
		if c == '\n' || (js_space(c) && matches!(previous, Some('.' | '!' | '?'))) {
			return &brief[..at];
		}
		previous = Some(c);
	}
	brief
}

/// A thread's title from the brief he typed in New thread (0554): its first sentence, at most six words and 40
/// characters, cut at a word.
pub fn brief_title(brief: &str) -> String {
	let first = first_piece(brief.trim_matches(js_space)).trim_end_matches(['.', '!', '?']);
	let mut title = String::new();
	let mut pieces: Vec<&str> = Vec::new();
	let mut start = 0;
	let mut run = false;
	for (at, c) in first.char_indices() {
		if js_space(c) {
			if !run {
				pieces.push(&first[start..at]);
				run = true;
			}
			start = at + c.len_utf8();
		} else {
			run = false;
		}
	}
	pieces.push(&first[start..]);
	for word in pieces.into_iter().take(6) {
		let next = if title.is_empty() { word.to_owned() } else { format!("{title} {word}") };
		if js_len(&next) > 40 {
			break;
		}
		title = next;
	}
	if title.is_empty() { js_prefix(first, 40).to_owned() } else { title }
}

/// Cyrillic letters as Latin (0581, UX-11): Macedonian's romanization, with the other Cyrillic alphabets' own letters
/// beside it; lowercase only (the slug is lowercased first).
fn cyrillic(c: char) -> Option<&'static str> {
	Some(match c {
		'а' => "a",
		'б' => "b",
		'в' => "v",
		'г' => "g",
		'д' => "d",
		'ѓ' => "gj",
		'е' => "e",
		'ж' => "zh",
		'з' => "z",
		'ѕ' => "dz",
		'и' => "i",
		'ј' => "j",
		'к' => "k",
		'л' => "l",
		'љ' => "lj",
		'м' => "m",
		'н' => "n",
		'њ' => "nj",
		'о' => "o",
		'п' => "p",
		'р' => "r",
		'с' => "s",
		'т' => "t",
		'ќ' => "kj",
		'у' => "u",
		'ф' => "f",
		'х' => "h",
		'ц' => "c",
		'ч' => "ch",
		'џ' => "dzh",
		'ш' => "sh",
		'ђ' => "dj",
		'ћ' => "c",
		'ё' => "e",
		'й' => "j",
		'щ' => "shch",
		'ъ' | 'ь' => "",
		'ы' => "y",
		'э' => "e",
		'ю' => "ju",
		'я' => "ja",
		'і' => "i",
		'ї' => "ji",
		'є' => "je",
		'ґ' => "g",
		_ => return None,
	})
}

/// Latin letters NFKD doesn't take apart.
fn latin(c: char) -> Option<&'static str> {
	Some(match c {
		'ß' => "ss",
		'æ' => "ae",
		'ø' => "o",
		'œ' => "oe",
		'đ' => "d",
		'ł' => "l",
		'þ' => "th",
		'ð' => "d",
		'ı' => "i",
		_ => return None,
	})
}

/// What a non-ASCII character spells after NFKD with its marks dropped, each other character as `-`: generated from
/// `String.prototype.normalize("NFKD")` and `\p{M}` of Bun 1.4.3 (Unicode as that ICU has it), for every code point
/// that spells at least one of `[a-z0-9]` or is a mark. `\0` in a run marks a run of marks, which spell nothing. A
/// character in neither table is `-`.
fn fold_char(c: char, out: &mut String) {
	if c.is_ascii() {
		out.push(if c.is_ascii_lowercase() || c.is_ascii_digit() { c } else { '-' });
		return;
	}
	let cp = u32::from(c);
	if let Ok(at) = FOLD_MULTI.binary_search_by_key(&cp, |&(from, _)| from) {
		out.push_str(FOLD_MULTI[at].1);
		return;
	}
	let after = FOLD_RUNS.partition_point(|&(_, to, _)| to < cp);
	match FOLD_RUNS.get(after) {
		Some(&(from, _, first)) if from <= cp => {
			if first != '\0' {
				out.push(char::from_u32(u32::from(first) + (cp - from)).unwrap_or('-'));
			}
		}
		_ => out.push('-'),
	}
}

/// A branch slug from a title (0414, 0445, 0581): lowercase, Cyrillic transliterated, Latin letters without their
/// marks, runs of anything else become `-`, at most 40 characters. Empty when the title has no letter or digit it can
/// spell (another script): the start then takes `fallbackSlug`'s name.
pub fn branch_slug(title: &str) -> String {
	let mut folded = String::with_capacity(title.len());
	for c in title.to_lowercase().chars() {
		match cyrillic(c).or_else(|| latin(c)) {
			Some(spelled) => spelled.chars().for_each(|s| fold_char(s, &mut folded)),
			None => fold_char(c, &mut folded),
		}
	}
	let mut slug = String::with_capacity(folded.len());
	for c in folded.chars() {
		if c != '-' || !slug.ends_with('-') {
			slug.push(c);
		}
	}
	let slug = slug.trim_start_matches('-');
	let slug = js_prefix(slug, 40);
	slug.trim_end_matches('-').to_owned()
}

/// A thread's name when its title gives no slug (0581): `thread-<YYYYMMDD>-<n>`, by the local day. `at` is epoch ms.
pub fn fallback_slug(at: i64, n: usize) -> String {
	format!("thread-{}-{n}", local_day(at).replace('-', ""))
}

const FOLD_RUNS: [(u32, u32, char); 661] = [
	(0xAA, 0xAA, 'a'),
	(0xB2, 0xB3, '2'),
	(0xB9, 0xB9, '1'),
	(0xBA, 0xBA, 'o'),
	(0xE0, 0xE0, 'a'),
	(0xE1, 0xE1, 'a'),
	(0xE2, 0xE2, 'a'),
	(0xE3, 0xE3, 'a'),
	(0xE4, 0xE4, 'a'),
	(0xE5, 0xE5, 'a'),
	(0xE7, 0xE7, 'c'),
	(0xE8, 0xE8, 'e'),
	(0xE9, 0xE9, 'e'),
	(0xEA, 0xEA, 'e'),
	(0xEB, 0xEB, 'e'),
	(0xEC, 0xEC, 'i'),
	(0xED, 0xED, 'i'),
	(0xEE, 0xEE, 'i'),
	(0xEF, 0xEF, 'i'),
	(0xF1, 0xF2, 'n'),
	(0xF3, 0xF3, 'o'),
	(0xF4, 0xF4, 'o'),
	(0xF5, 0xF5, 'o'),
	(0xF6, 0xF6, 'o'),
	(0xF9, 0xF9, 'u'),
	(0xFA, 0xFA, 'u'),
	(0xFB, 0xFB, 'u'),
	(0xFC, 0xFC, 'u'),
	(0xFD, 0xFD, 'y'),
	(0xFF, 0xFF, 'y'),
	(0x101, 0x101, 'a'),
	(0x103, 0x103, 'a'),
	(0x105, 0x105, 'a'),
	(0x107, 0x107, 'c'),
	(0x109, 0x109, 'c'),
	(0x10B, 0x10B, 'c'),
	(0x10D, 0x10D, 'c'),
	(0x10F, 0x10F, 'd'),
	(0x113, 0x113, 'e'),
	(0x115, 0x115, 'e'),
	(0x117, 0x117, 'e'),
	(0x119, 0x119, 'e'),
	(0x11B, 0x11B, 'e'),
	(0x11D, 0x11D, 'g'),
	(0x11F, 0x11F, 'g'),
	(0x121, 0x121, 'g'),
	(0x123, 0x123, 'g'),
	(0x125, 0x125, 'h'),
	(0x129, 0x129, 'i'),
	(0x12B, 0x12B, 'i'),
	(0x12D, 0x12D, 'i'),
	(0x12F, 0x12F, 'i'),
	(0x135, 0x135, 'j'),
	(0x137, 0x137, 'k'),
	(0x13A, 0x13A, 'l'),
	(0x13C, 0x13C, 'l'),
	(0x13E, 0x13E, 'l'),
	(0x144, 0x144, 'n'),
	(0x146, 0x146, 'n'),
	(0x148, 0x148, 'n'),
	(0x14D, 0x14D, 'o'),
	(0x14F, 0x14F, 'o'),
	(0x151, 0x151, 'o'),
	(0x155, 0x155, 'r'),
	(0x157, 0x157, 'r'),
	(0x159, 0x159, 'r'),
	(0x15B, 0x15B, 's'),
	(0x15D, 0x15D, 's'),
	(0x15F, 0x15F, 's'),
	(0x161, 0x161, 's'),
	(0x163, 0x163, 't'),
	(0x165, 0x165, 't'),
	(0x169, 0x169, 'u'),
	(0x16B, 0x16B, 'u'),
	(0x16D, 0x16D, 'u'),
	(0x16F, 0x16F, 'u'),
	(0x171, 0x171, 'u'),
	(0x173, 0x173, 'u'),
	(0x175, 0x175, 'w'),
	(0x177, 0x177, 'y'),
	(0x17A, 0x17A, 'z'),
	(0x17C, 0x17C, 'z'),
	(0x17E, 0x17E, 'z'),
	(0x17F, 0x17F, 's'),
	(0x1A1, 0x1A1, 'o'),
	(0x1B0, 0x1B0, 'u'),
	(0x1CE, 0x1CE, 'a'),
	(0x1D0, 0x1D0, 'i'),
	(0x1D2, 0x1D2, 'o'),
	(0x1D4, 0x1D4, 'u'),
	(0x1D6, 0x1D6, 'u'),
	(0x1D8, 0x1D8, 'u'),
	(0x1DA, 0x1DA, 'u'),
	(0x1DC, 0x1DC, 'u'),
	(0x1DF, 0x1DF, 'a'),
	(0x1E1, 0x1E1, 'a'),
	(0x1E7, 0x1E7, 'g'),
	(0x1E9, 0x1E9, 'k'),
	(0x1EB, 0x1EB, 'o'),
	(0x1ED, 0x1ED, 'o'),
	(0x1F0, 0x1F0, 'j'),
	(0x1F5, 0x1F5, 'g'),
	(0x1F9, 0x1F9, 'n'),
	(0x1FB, 0x1FB, 'a'),
	(0x201, 0x201, 'a'),
	(0x203, 0x203, 'a'),
	(0x205, 0x205, 'e'),
	(0x207, 0x207, 'e'),
	(0x209, 0x209, 'i'),
	(0x20B, 0x20B, 'i'),
	(0x20D, 0x20D, 'o'),
	(0x20F, 0x20F, 'o'),
	(0x211, 0x211, 'r'),
	(0x213, 0x213, 'r'),
	(0x215, 0x215, 'u'),
	(0x217, 0x217, 'u'),
	(0x219, 0x219, 's'),
	(0x21B, 0x21B, 't'),
	(0x21F, 0x21F, 'h'),
	(0x227, 0x227, 'a'),
	(0x229, 0x229, 'e'),
	(0x22B, 0x22B, 'o'),
	(0x22D, 0x22D, 'o'),
	(0x22F, 0x22F, 'o'),
	(0x231, 0x231, 'o'),
	(0x233, 0x233, 'y'),
	(0x2B0, 0x2B0, 'h'),
	(0x2B2, 0x2B2, 'j'),
	(0x2B3, 0x2B3, 'r'),
	(0x2B7, 0x2B7, 'w'),
	(0x2B8, 0x2B8, 'y'),
	(0x2E1, 0x2E1, 'l'),
	(0x2E2, 0x2E2, 's'),
	(0x2E3, 0x2E3, 'x'),
	(0x300, 0x36F, '\0'),
	(0x483, 0x489, '\0'),
	(0x591, 0x5BD, '\0'),
	(0x5BF, 0x5BF, '\0'),
	(0x5C1, 0x5C2, '\0'),
	(0x5C4, 0x5C5, '\0'),
	(0x5C7, 0x5C7, '\0'),
	(0x610, 0x61A, '\0'),
	(0x64B, 0x65F, '\0'),
	(0x670, 0x670, '\0'),
	(0x6D6, 0x6DC, '\0'),
	(0x6DF, 0x6E4, '\0'),
	(0x6E7, 0x6E8, '\0'),
	(0x6EA, 0x6ED, '\0'),
	(0x711, 0x711, '\0'),
	(0x730, 0x74A, '\0'),
	(0x7A6, 0x7B0, '\0'),
	(0x7EB, 0x7F3, '\0'),
	(0x7FD, 0x7FD, '\0'),
	(0x816, 0x819, '\0'),
	(0x81B, 0x823, '\0'),
	(0x825, 0x827, '\0'),
	(0x829, 0x82D, '\0'),
	(0x859, 0x85B, '\0'),
	(0x897, 0x89F, '\0'),
	(0x8CA, 0x8E1, '\0'),
	(0x8E3, 0x903, '\0'),
	(0x93A, 0x93C, '\0'),
	(0x93E, 0x94F, '\0'),
	(0x951, 0x957, '\0'),
	(0x962, 0x963, '\0'),
	(0x981, 0x983, '\0'),
	(0x9BC, 0x9BC, '\0'),
	(0x9BE, 0x9C4, '\0'),
	(0x9C7, 0x9C8, '\0'),
	(0x9CB, 0x9CD, '\0'),
	(0x9D7, 0x9D7, '\0'),
	(0x9E2, 0x9E3, '\0'),
	(0x9FE, 0x9FE, '\0'),
	(0xA01, 0xA03, '\0'),
	(0xA3C, 0xA3C, '\0'),
	(0xA3E, 0xA42, '\0'),
	(0xA47, 0xA48, '\0'),
	(0xA4B, 0xA4D, '\0'),
	(0xA51, 0xA51, '\0'),
	(0xA70, 0xA71, '\0'),
	(0xA75, 0xA75, '\0'),
	(0xA81, 0xA83, '\0'),
	(0xABC, 0xABC, '\0'),
	(0xABE, 0xAC5, '\0'),
	(0xAC7, 0xAC9, '\0'),
	(0xACB, 0xACD, '\0'),
	(0xAE2, 0xAE3, '\0'),
	(0xAFA, 0xAFF, '\0'),
	(0xB01, 0xB03, '\0'),
	(0xB3C, 0xB3C, '\0'),
	(0xB3E, 0xB44, '\0'),
	(0xB47, 0xB48, '\0'),
	(0xB4B, 0xB4D, '\0'),
	(0xB55, 0xB57, '\0'),
	(0xB62, 0xB63, '\0'),
	(0xB82, 0xB82, '\0'),
	(0xBBE, 0xBC2, '\0'),
	(0xBC6, 0xBC8, '\0'),
	(0xBCA, 0xBCD, '\0'),
	(0xBD7, 0xBD7, '\0'),
	(0xC00, 0xC04, '\0'),
	(0xC3C, 0xC3C, '\0'),
	(0xC3E, 0xC44, '\0'),
	(0xC46, 0xC48, '\0'),
	(0xC4A, 0xC4D, '\0'),
	(0xC55, 0xC56, '\0'),
	(0xC62, 0xC63, '\0'),
	(0xC81, 0xC83, '\0'),
	(0xCBC, 0xCBC, '\0'),
	(0xCBE, 0xCC4, '\0'),
	(0xCC6, 0xCC8, '\0'),
	(0xCCA, 0xCCD, '\0'),
	(0xCD5, 0xCD6, '\0'),
	(0xCE2, 0xCE3, '\0'),
	(0xCF3, 0xCF3, '\0'),
	(0xD00, 0xD03, '\0'),
	(0xD3B, 0xD3C, '\0'),
	(0xD3E, 0xD44, '\0'),
	(0xD46, 0xD48, '\0'),
	(0xD4A, 0xD4D, '\0'),
	(0xD57, 0xD57, '\0'),
	(0xD62, 0xD63, '\0'),
	(0xD81, 0xD83, '\0'),
	(0xDCA, 0xDCA, '\0'),
	(0xDCF, 0xDD4, '\0'),
	(0xDD6, 0xDD6, '\0'),
	(0xDD8, 0xDDF, '\0'),
	(0xDF2, 0xDF3, '\0'),
	(0xE31, 0xE31, '\0'),
	(0xE34, 0xE3A, '\0'),
	(0xE47, 0xE4E, '\0'),
	(0xEB1, 0xEB1, '\0'),
	(0xEB4, 0xEBC, '\0'),
	(0xEC8, 0xECE, '\0'),
	(0xF18, 0xF19, '\0'),
	(0xF35, 0xF35, '\0'),
	(0xF37, 0xF37, '\0'),
	(0xF39, 0xF39, '\0'),
	(0xF3E, 0xF3F, '\0'),
	(0xF71, 0xF84, '\0'),
	(0xF86, 0xF87, '\0'),
	(0xF8D, 0xF97, '\0'),
	(0xF99, 0xFBC, '\0'),
	(0xFC6, 0xFC6, '\0'),
	(0x102B, 0x103E, '\0'),
	(0x1056, 0x1059, '\0'),
	(0x105E, 0x1060, '\0'),
	(0x1062, 0x1064, '\0'),
	(0x1067, 0x106D, '\0'),
	(0x1071, 0x1074, '\0'),
	(0x1082, 0x108D, '\0'),
	(0x108F, 0x108F, '\0'),
	(0x109A, 0x109D, '\0'),
	(0x135D, 0x135F, '\0'),
	(0x1712, 0x1715, '\0'),
	(0x1732, 0x1734, '\0'),
	(0x1752, 0x1753, '\0'),
	(0x1772, 0x1773, '\0'),
	(0x17B4, 0x17D3, '\0'),
	(0x17DD, 0x17DD, '\0'),
	(0x180B, 0x180D, '\0'),
	(0x180F, 0x180F, '\0'),
	(0x1885, 0x1886, '\0'),
	(0x18A9, 0x18A9, '\0'),
	(0x1920, 0x192B, '\0'),
	(0x1930, 0x193B, '\0'),
	(0x1A17, 0x1A1B, '\0'),
	(0x1A55, 0x1A5E, '\0'),
	(0x1A60, 0x1A7C, '\0'),
	(0x1A7F, 0x1A7F, '\0'),
	(0x1AB0, 0x1ADD, '\0'),
	(0x1AE0, 0x1AEB, '\0'),
	(0x1B00, 0x1B04, '\0'),
	(0x1B34, 0x1B44, '\0'),
	(0x1B6B, 0x1B73, '\0'),
	(0x1B80, 0x1B82, '\0'),
	(0x1BA1, 0x1BAD, '\0'),
	(0x1BE6, 0x1BF3, '\0'),
	(0x1C24, 0x1C37, '\0'),
	(0x1CD0, 0x1CD2, '\0'),
	(0x1CD4, 0x1CE8, '\0'),
	(0x1CED, 0x1CED, '\0'),
	(0x1CF4, 0x1CF4, '\0'),
	(0x1CF7, 0x1CF9, '\0'),
	(0x1D43, 0x1D43, 'a'),
	(0x1D47, 0x1D47, 'b'),
	(0x1D48, 0x1D49, 'd'),
	(0x1D4D, 0x1D4D, 'g'),
	(0x1D4F, 0x1D4F, 'k'),
	(0x1D50, 0x1D50, 'm'),
	(0x1D52, 0x1D52, 'o'),
	(0x1D56, 0x1D56, 'p'),
	(0x1D57, 0x1D58, 't'),
	(0x1D5B, 0x1D5B, 'v'),
	(0x1D62, 0x1D62, 'i'),
	(0x1D63, 0x1D63, 'r'),
	(0x1D64, 0x1D65, 'u'),
	(0x1D9C, 0x1D9C, 'c'),
	(0x1DA0, 0x1DA0, 'f'),
	(0x1DBB, 0x1DBB, 'z'),
	(0x1DC0, 0x1DFF, '\0'),
	(0x1E01, 0x1E01, 'a'),
	(0x1E03, 0x1E03, 'b'),
	(0x1E05, 0x1E05, 'b'),
	(0x1E07, 0x1E07, 'b'),
	(0x1E09, 0x1E09, 'c'),
	(0x1E0B, 0x1E0B, 'd'),
	(0x1E0D, 0x1E0D, 'd'),
	(0x1E0F, 0x1E0F, 'd'),
	(0x1E11, 0x1E11, 'd'),
	(0x1E13, 0x1E13, 'd'),
	(0x1E15, 0x1E15, 'e'),
	(0x1E17, 0x1E17, 'e'),
	(0x1E19, 0x1E19, 'e'),
	(0x1E1B, 0x1E1B, 'e'),
	(0x1E1D, 0x1E1D, 'e'),
	(0x1E1F, 0x1E1F, 'f'),
	(0x1E21, 0x1E21, 'g'),
	(0x1E23, 0x1E23, 'h'),
	(0x1E25, 0x1E25, 'h'),
	(0x1E27, 0x1E27, 'h'),
	(0x1E29, 0x1E29, 'h'),
	(0x1E2B, 0x1E2B, 'h'),
	(0x1E2D, 0x1E2D, 'i'),
	(0x1E2F, 0x1E2F, 'i'),
	(0x1E31, 0x1E31, 'k'),
	(0x1E33, 0x1E33, 'k'),
	(0x1E35, 0x1E35, 'k'),
	(0x1E37, 0x1E37, 'l'),
	(0x1E39, 0x1E39, 'l'),
	(0x1E3B, 0x1E3B, 'l'),
	(0x1E3D, 0x1E3D, 'l'),
	(0x1E3F, 0x1E3F, 'm'),
	(0x1E41, 0x1E41, 'm'),
	(0x1E43, 0x1E43, 'm'),
	(0x1E45, 0x1E45, 'n'),
	(0x1E47, 0x1E47, 'n'),
	(0x1E49, 0x1E49, 'n'),
	(0x1E4B, 0x1E4B, 'n'),
	(0x1E4D, 0x1E4D, 'o'),
	(0x1E4F, 0x1E4F, 'o'),
	(0x1E51, 0x1E51, 'o'),
	(0x1E53, 0x1E53, 'o'),
	(0x1E55, 0x1E55, 'p'),
	(0x1E57, 0x1E57, 'p'),
	(0x1E59, 0x1E59, 'r'),
	(0x1E5B, 0x1E5B, 'r'),
	(0x1E5D, 0x1E5D, 'r'),
	(0x1E5F, 0x1E5F, 'r'),
	(0x1E61, 0x1E61, 's'),
	(0x1E63, 0x1E63, 's'),
	(0x1E65, 0x1E65, 's'),
	(0x1E67, 0x1E67, 's'),
	(0x1E69, 0x1E69, 's'),
	(0x1E6B, 0x1E6B, 't'),
	(0x1E6D, 0x1E6D, 't'),
	(0x1E6F, 0x1E6F, 't'),
	(0x1E71, 0x1E71, 't'),
	(0x1E73, 0x1E73, 'u'),
	(0x1E75, 0x1E75, 'u'),
	(0x1E77, 0x1E77, 'u'),
	(0x1E79, 0x1E79, 'u'),
	(0x1E7B, 0x1E7B, 'u'),
	(0x1E7D, 0x1E7D, 'v'),
	(0x1E7F, 0x1E7F, 'v'),
	(0x1E81, 0x1E81, 'w'),
	(0x1E83, 0x1E83, 'w'),
	(0x1E85, 0x1E85, 'w'),
	(0x1E87, 0x1E87, 'w'),
	(0x1E89, 0x1E89, 'w'),
	(0x1E8B, 0x1E8B, 'x'),
	(0x1E8D, 0x1E8D, 'x'),
	(0x1E8F, 0x1E8F, 'y'),
	(0x1E91, 0x1E91, 'z'),
	(0x1E93, 0x1E93, 'z'),
	(0x1E95, 0x1E95, 'z'),
	(0x1E96, 0x1E96, 'h'),
	(0x1E97, 0x1E97, 't'),
	(0x1E98, 0x1E98, 'w'),
	(0x1E99, 0x1E99, 'y'),
	(0x1E9B, 0x1E9B, 's'),
	(0x1EA1, 0x1EA1, 'a'),
	(0x1EA3, 0x1EA3, 'a'),
	(0x1EA5, 0x1EA5, 'a'),
	(0x1EA7, 0x1EA7, 'a'),
	(0x1EA9, 0x1EA9, 'a'),
	(0x1EAB, 0x1EAB, 'a'),
	(0x1EAD, 0x1EAD, 'a'),
	(0x1EAF, 0x1EAF, 'a'),
	(0x1EB1, 0x1EB1, 'a'),
	(0x1EB3, 0x1EB3, 'a'),
	(0x1EB5, 0x1EB5, 'a'),
	(0x1EB7, 0x1EB7, 'a'),
	(0x1EB9, 0x1EB9, 'e'),
	(0x1EBB, 0x1EBB, 'e'),
	(0x1EBD, 0x1EBD, 'e'),
	(0x1EBF, 0x1EBF, 'e'),
	(0x1EC1, 0x1EC1, 'e'),
	(0x1EC3, 0x1EC3, 'e'),
	(0x1EC5, 0x1EC5, 'e'),
	(0x1EC7, 0x1EC7, 'e'),
	(0x1EC9, 0x1EC9, 'i'),
	(0x1ECB, 0x1ECB, 'i'),
	(0x1ECD, 0x1ECD, 'o'),
	(0x1ECF, 0x1ECF, 'o'),
	(0x1ED1, 0x1ED1, 'o'),
	(0x1ED3, 0x1ED3, 'o'),
	(0x1ED5, 0x1ED5, 'o'),
	(0x1ED7, 0x1ED7, 'o'),
	(0x1ED9, 0x1ED9, 'o'),
	(0x1EDB, 0x1EDB, 'o'),
	(0x1EDD, 0x1EDD, 'o'),
	(0x1EDF, 0x1EDF, 'o'),
	(0x1EE1, 0x1EE1, 'o'),
	(0x1EE3, 0x1EE3, 'o'),
	(0x1EE5, 0x1EE5, 'u'),
	(0x1EE7, 0x1EE7, 'u'),
	(0x1EE9, 0x1EE9, 'u'),
	(0x1EEB, 0x1EEB, 'u'),
	(0x1EED, 0x1EED, 'u'),
	(0x1EEF, 0x1EEF, 'u'),
	(0x1EF1, 0x1EF1, 'u'),
	(0x1EF3, 0x1EF3, 'y'),
	(0x1EF5, 0x1EF5, 'y'),
	(0x1EF7, 0x1EF7, 'y'),
	(0x1EF9, 0x1EF9, 'y'),
	(0x2070, 0x2070, '0'),
	(0x2071, 0x2071, 'i'),
	(0x2074, 0x2079, '4'),
	(0x207F, 0x207F, 'n'),
	(0x2080, 0x2089, '0'),
	(0x2090, 0x2090, 'a'),
	(0x2091, 0x2091, 'e'),
	(0x2092, 0x2092, 'o'),
	(0x2093, 0x2093, 'x'),
	(0x2095, 0x2095, 'h'),
	(0x2096, 0x2099, 'k'),
	(0x209A, 0x209A, 'p'),
	(0x209B, 0x209C, 's'),
	(0x20D0, 0x20F0, '\0'),
	(0x210A, 0x210A, 'g'),
	(0x210E, 0x210E, 'h'),
	(0x2113, 0x2113, 'l'),
	(0x212F, 0x212F, 'e'),
	(0x2134, 0x2134, 'o'),
	(0x2139, 0x2139, 'i'),
	(0x2146, 0x2147, 'd'),
	(0x2148, 0x2149, 'i'),
	(0x2170, 0x2170, 'i'),
	(0x2174, 0x2174, 'v'),
	(0x2179, 0x2179, 'x'),
	(0x217C, 0x217C, 'l'),
	(0x217D, 0x217E, 'c'),
	(0x217F, 0x217F, 'm'),
	(0x2460, 0x2468, '1'),
	(0x24D0, 0x24E9, 'a'),
	(0x24EA, 0x24EA, '0'),
	(0x2C7C, 0x2C7C, 'j'),
	(0x2CEF, 0x2CF1, '\0'),
	(0x2D7F, 0x2D7F, '\0'),
	(0x2DE0, 0x2DFF, '\0'),
	(0x302A, 0x302F, '\0'),
	(0x3099, 0x309A, '\0'),
	(0xA66F, 0xA672, '\0'),
	(0xA674, 0xA67D, '\0'),
	(0xA69E, 0xA69F, '\0'),
	(0xA6F0, 0xA6F1, '\0'),
	(0xA802, 0xA802, '\0'),
	(0xA806, 0xA806, '\0'),
	(0xA80B, 0xA80B, '\0'),
	(0xA823, 0xA827, '\0'),
	(0xA82C, 0xA82C, '\0'),
	(0xA880, 0xA881, '\0'),
	(0xA8B4, 0xA8C5, '\0'),
	(0xA8E0, 0xA8F1, '\0'),
	(0xA8FF, 0xA8FF, '\0'),
	(0xA926, 0xA92D, '\0'),
	(0xA947, 0xA953, '\0'),
	(0xA980, 0xA983, '\0'),
	(0xA9B3, 0xA9C0, '\0'),
	(0xA9E5, 0xA9E5, '\0'),
	(0xAA29, 0xAA36, '\0'),
	(0xAA43, 0xAA43, '\0'),
	(0xAA4C, 0xAA4D, '\0'),
	(0xAA7B, 0xAA7D, '\0'),
	(0xAAB0, 0xAAB0, '\0'),
	(0xAAB2, 0xAAB4, '\0'),
	(0xAAB7, 0xAAB8, '\0'),
	(0xAABE, 0xAABF, '\0'),
	(0xAAC1, 0xAAC1, '\0'),
	(0xAAEB, 0xAAEF, '\0'),
	(0xAAF5, 0xAAF6, '\0'),
	(0xABE3, 0xABEA, '\0'),
	(0xABEC, 0xABED, '\0'),
	(0xFB1E, 0xFB1E, '\0'),
	(0xFE00, 0xFE0F, '\0'),
	(0xFE20, 0xFE2F, '\0'),
	(0xFF10, 0xFF19, '0'),
	(0xFF41, 0xFF5A, 'a'),
	(0xFF9E, 0xFF9F, '\0'),
	(0x101FD, 0x101FD, '\0'),
	(0x102E0, 0x102E0, '\0'),
	(0x10376, 0x1037A, '\0'),
	(0x107A5, 0x107A5, 'q'),
	(0x10A01, 0x10A03, '\0'),
	(0x10A05, 0x10A06, '\0'),
	(0x10A0C, 0x10A0F, '\0'),
	(0x10A38, 0x10A3A, '\0'),
	(0x10A3F, 0x10A3F, '\0'),
	(0x10AE5, 0x10AE6, '\0'),
	(0x10D24, 0x10D27, '\0'),
	(0x10D69, 0x10D6D, '\0'),
	(0x10EAB, 0x10EAC, '\0'),
	(0x10EFA, 0x10EFF, '\0'),
	(0x10F46, 0x10F50, '\0'),
	(0x10F82, 0x10F85, '\0'),
	(0x11000, 0x11002, '\0'),
	(0x11038, 0x11046, '\0'),
	(0x11070, 0x11070, '\0'),
	(0x11073, 0x11074, '\0'),
	(0x1107F, 0x11082, '\0'),
	(0x110B0, 0x110BA, '\0'),
	(0x110C2, 0x110C2, '\0'),
	(0x11100, 0x11102, '\0'),
	(0x11127, 0x11134, '\0'),
	(0x11145, 0x11146, '\0'),
	(0x11173, 0x11173, '\0'),
	(0x11180, 0x11182, '\0'),
	(0x111B3, 0x111C0, '\0'),
	(0x111C9, 0x111CC, '\0'),
	(0x111CE, 0x111CF, '\0'),
	(0x1122C, 0x11237, '\0'),
	(0x1123E, 0x1123E, '\0'),
	(0x11241, 0x11241, '\0'),
	(0x112DF, 0x112EA, '\0'),
	(0x11300, 0x11303, '\0'),
	(0x1133B, 0x1133C, '\0'),
	(0x1133E, 0x11344, '\0'),
	(0x11347, 0x11348, '\0'),
	(0x1134B, 0x1134D, '\0'),
	(0x11357, 0x11357, '\0'),
	(0x11362, 0x11363, '\0'),
	(0x11366, 0x1136C, '\0'),
	(0x11370, 0x11374, '\0'),
	(0x113B8, 0x113C0, '\0'),
	(0x113C2, 0x113C2, '\0'),
	(0x113C5, 0x113C5, '\0'),
	(0x113C7, 0x113CA, '\0'),
	(0x113CC, 0x113D0, '\0'),
	(0x113D2, 0x113D2, '\0'),
	(0x113E1, 0x113E2, '\0'),
	(0x11435, 0x11446, '\0'),
	(0x1145E, 0x1145E, '\0'),
	(0x114B0, 0x114C3, '\0'),
	(0x115AF, 0x115B5, '\0'),
	(0x115B8, 0x115C0, '\0'),
	(0x115DC, 0x115DD, '\0'),
	(0x11630, 0x11640, '\0'),
	(0x116AB, 0x116B7, '\0'),
	(0x1171D, 0x1172B, '\0'),
	(0x1182C, 0x1183A, '\0'),
	(0x11930, 0x11935, '\0'),
	(0x11937, 0x11938, '\0'),
	(0x1193B, 0x1193E, '\0'),
	(0x11940, 0x11940, '\0'),
	(0x11942, 0x11943, '\0'),
	(0x119D1, 0x119D7, '\0'),
	(0x119DA, 0x119E0, '\0'),
	(0x119E4, 0x119E4, '\0'),
	(0x11A01, 0x11A0A, '\0'),
	(0x11A33, 0x11A39, '\0'),
	(0x11A3B, 0x11A3E, '\0'),
	(0x11A47, 0x11A47, '\0'),
	(0x11A51, 0x11A5B, '\0'),
	(0x11A8A, 0x11A99, '\0'),
	(0x11B60, 0x11B67, '\0'),
	(0x11C2F, 0x11C36, '\0'),
	(0x11C38, 0x11C3F, '\0'),
	(0x11C92, 0x11CA7, '\0'),
	(0x11CA9, 0x11CB6, '\0'),
	(0x11D31, 0x11D36, '\0'),
	(0x11D3A, 0x11D3A, '\0'),
	(0x11D3C, 0x11D3D, '\0'),
	(0x11D3F, 0x11D45, '\0'),
	(0x11D47, 0x11D47, '\0'),
	(0x11D8A, 0x11D8E, '\0'),
	(0x11D90, 0x11D91, '\0'),
	(0x11D93, 0x11D97, '\0'),
	(0x11EF3, 0x11EF6, '\0'),
	(0x11F00, 0x11F01, '\0'),
	(0x11F03, 0x11F03, '\0'),
	(0x11F34, 0x11F3A, '\0'),
	(0x11F3E, 0x11F42, '\0'),
	(0x11F5A, 0x11F5A, '\0'),
	(0x13440, 0x13440, '\0'),
	(0x13447, 0x13455, '\0'),
	(0x1611E, 0x1612F, '\0'),
	(0x16AF0, 0x16AF4, '\0'),
	(0x16B30, 0x16B36, '\0'),
	(0x16F4F, 0x16F4F, '\0'),
	(0x16F51, 0x16F87, '\0'),
	(0x16F8F, 0x16F92, '\0'),
	(0x16FE4, 0x16FE4, '\0'),
	(0x16FF0, 0x16FF1, '\0'),
	(0x1BC9D, 0x1BC9E, '\0'),
	(0x1CCF0, 0x1CCF9, '0'),
	(0x1CF00, 0x1CF2D, '\0'),
	(0x1CF30, 0x1CF46, '\0'),
	(0x1D165, 0x1D169, '\0'),
	(0x1D16D, 0x1D172, '\0'),
	(0x1D17B, 0x1D182, '\0'),
	(0x1D185, 0x1D18B, '\0'),
	(0x1D1AA, 0x1D1AD, '\0'),
	(0x1D242, 0x1D244, '\0'),
	(0x1D41A, 0x1D433, 'a'),
	(0x1D44E, 0x1D454, 'a'),
	(0x1D456, 0x1D467, 'i'),
	(0x1D482, 0x1D49B, 'a'),
	(0x1D4B6, 0x1D4B9, 'a'),
	(0x1D4BB, 0x1D4BB, 'f'),
	(0x1D4BD, 0x1D4C3, 'h'),
	(0x1D4C5, 0x1D4CF, 'p'),
	(0x1D4EA, 0x1D503, 'a'),
	(0x1D51E, 0x1D537, 'a'),
	(0x1D552, 0x1D56B, 'a'),
	(0x1D586, 0x1D59F, 'a'),
	(0x1D5BA, 0x1D5D3, 'a'),
	(0x1D5EE, 0x1D607, 'a'),
	(0x1D622, 0x1D63B, 'a'),
	(0x1D656, 0x1D66F, 'a'),
	(0x1D68A, 0x1D6A3, 'a'),
	(0x1D7CE, 0x1D7D7, '0'),
	(0x1D7D8, 0x1D7E1, '0'),
	(0x1D7E2, 0x1D7EB, '0'),
	(0x1D7EC, 0x1D7F5, '0'),
	(0x1D7F6, 0x1D7FF, '0'),
	(0x1DA00, 0x1DA36, '\0'),
	(0x1DA3B, 0x1DA6C, '\0'),
	(0x1DA75, 0x1DA75, '\0'),
	(0x1DA84, 0x1DA84, '\0'),
	(0x1DA9B, 0x1DA9F, '\0'),
	(0x1DAA1, 0x1DAAF, '\0'),
	(0x1E000, 0x1E006, '\0'),
	(0x1E008, 0x1E018, '\0'),
	(0x1E01B, 0x1E021, '\0'),
	(0x1E023, 0x1E024, '\0'),
	(0x1E026, 0x1E02A, '\0'),
	(0x1E08F, 0x1E08F, '\0'),
	(0x1E130, 0x1E136, '\0'),
	(0x1E2AE, 0x1E2AE, '\0'),
	(0x1E2EC, 0x1E2EF, '\0'),
	(0x1E4EC, 0x1E4EF, '\0'),
	(0x1E5EE, 0x1E5EF, '\0'),
	(0x1E6E3, 0x1E6E3, '\0'),
	(0x1E6E6, 0x1E6E6, '\0'),
	(0x1E6EE, 0x1E6EF, '\0'),
	(0x1E6F5, 0x1E6F5, '\0'),
	(0x1E8D0, 0x1E8D6, '\0'),
	(0x1E944, 0x1E94A, '\0'),
	(0x1FBF0, 0x1FBF9, '0'),
	(0xE0100, 0xE01EF, '\0'),
];
const FOLD_MULTI: [(u32, &str); 332] = [
	(0xBC, "1-4"),
	(0xBD, "1-2"),
	(0xBE, "3-4"),
	(0x133, "ij"),
	(0x140, "l-"),
	(0x149, "-n"),
	(0x1C5, "-z"),
	(0x1C6, "dz"),
	(0x1C8, "-j"),
	(0x1C9, "lj"),
	(0x1CB, "-j"),
	(0x1CC, "nj"),
	(0x1F2, "-z"),
	(0x1F3, "dz"),
	(0x1E9A, "a-"),
	(0x20A8, "-s"),
	(0x2100, "a-c"),
	(0x2101, "a-s"),
	(0x2105, "c-o"),
	(0x2106, "c-u"),
	(0x2116, "-o"),
	(0x2150, "1-7"),
	(0x2151, "1-9"),
	(0x2152, "1-10"),
	(0x2153, "1-3"),
	(0x2154, "2-3"),
	(0x2155, "1-5"),
	(0x2156, "2-5"),
	(0x2157, "3-5"),
	(0x2158, "4-5"),
	(0x2159, "1-6"),
	(0x215A, "5-6"),
	(0x215B, "1-8"),
	(0x215C, "3-8"),
	(0x215D, "5-8"),
	(0x215E, "7-8"),
	(0x215F, "1-"),
	(0x2171, "ii"),
	(0x2172, "iii"),
	(0x2173, "iv"),
	(0x2175, "vi"),
	(0x2176, "vii"),
	(0x2177, "viii"),
	(0x2178, "ix"),
	(0x217A, "xi"),
	(0x217B, "xii"),
	(0x2189, "0-3"),
	(0x2469, "10"),
	(0x246A, "11"),
	(0x246B, "12"),
	(0x246C, "13"),
	(0x246D, "14"),
	(0x246E, "15"),
	(0x246F, "16"),
	(0x2470, "17"),
	(0x2471, "18"),
	(0x2472, "19"),
	(0x2473, "20"),
	(0x2474, "-1-"),
	(0x2475, "-2-"),
	(0x2476, "-3-"),
	(0x2477, "-4-"),
	(0x2478, "-5-"),
	(0x2479, "-6-"),
	(0x247A, "-7-"),
	(0x247B, "-8-"),
	(0x247C, "-9-"),
	(0x247D, "-10-"),
	(0x247E, "-11-"),
	(0x247F, "-12-"),
	(0x2480, "-13-"),
	(0x2481, "-14-"),
	(0x2482, "-15-"),
	(0x2483, "-16-"),
	(0x2484, "-17-"),
	(0x2485, "-18-"),
	(0x2486, "-19-"),
	(0x2487, "-20-"),
	(0x2488, "1-"),
	(0x2489, "2-"),
	(0x248A, "3-"),
	(0x248B, "4-"),
	(0x248C, "5-"),
	(0x248D, "6-"),
	(0x248E, "7-"),
	(0x248F, "8-"),
	(0x2490, "9-"),
	(0x2491, "10-"),
	(0x2492, "11-"),
	(0x2493, "12-"),
	(0x2494, "13-"),
	(0x2495, "14-"),
	(0x2496, "15-"),
	(0x2497, "16-"),
	(0x2498, "17-"),
	(0x2499, "18-"),
	(0x249A, "19-"),
	(0x249B, "20-"),
	(0x249C, "-a-"),
	(0x249D, "-b-"),
	(0x249E, "-c-"),
	(0x249F, "-d-"),
	(0x24A0, "-e-"),
	(0x24A1, "-f-"),
	(0x24A2, "-g-"),
	(0x24A3, "-h-"),
	(0x24A4, "-i-"),
	(0x24A5, "-j-"),
	(0x24A6, "-k-"),
	(0x24A7, "-l-"),
	(0x24A8, "-m-"),
	(0x24A9, "-n-"),
	(0x24AA, "-o-"),
	(0x24AB, "-p-"),
	(0x24AC, "-q-"),
	(0x24AD, "-r-"),
	(0x24AE, "-s-"),
	(0x24AF, "-t-"),
	(0x24B0, "-u-"),
	(0x24B1, "-v-"),
	(0x24B2, "-w-"),
	(0x24B3, "-x-"),
	(0x24B4, "-y-"),
	(0x24B5, "-z-"),
	(0x3251, "21"),
	(0x3252, "22"),
	(0x3253, "23"),
	(0x3254, "24"),
	(0x3255, "25"),
	(0x3256, "26"),
	(0x3257, "27"),
	(0x3258, "28"),
	(0x3259, "29"),
	(0x325A, "30"),
	(0x325B, "31"),
	(0x325C, "32"),
	(0x325D, "33"),
	(0x325E, "34"),
	(0x325F, "35"),
	(0x32B1, "36"),
	(0x32B2, "37"),
	(0x32B3, "38"),
	(0x32B4, "39"),
	(0x32B5, "40"),
	(0x32B6, "41"),
	(0x32B7, "42"),
	(0x32B8, "43"),
	(0x32B9, "44"),
	(0x32BA, "45"),
	(0x32BB, "46"),
	(0x32BC, "47"),
	(0x32BD, "48"),
	(0x32BE, "49"),
	(0x32BF, "50"),
	(0x32C0, "1-"),
	(0x32C1, "2-"),
	(0x32C2, "3-"),
	(0x32C3, "4-"),
	(0x32C4, "5-"),
	(0x32C5, "6-"),
	(0x32C6, "7-"),
	(0x32C7, "8-"),
	(0x32C8, "9-"),
	(0x32C9, "10-"),
	(0x32CA, "11-"),
	(0x32CB, "12-"),
	(0x32CC, "-g"),
	(0x32CD, "erg"),
	(0x32CE, "e-"),
	(0x3358, "0-"),
	(0x3359, "1-"),
	(0x335A, "2-"),
	(0x335B, "3-"),
	(0x335C, "4-"),
	(0x335D, "5-"),
	(0x335E, "6-"),
	(0x335F, "7-"),
	(0x3360, "8-"),
	(0x3361, "9-"),
	(0x3362, "10-"),
	(0x3363, "11-"),
	(0x3364, "12-"),
	(0x3365, "13-"),
	(0x3366, "14-"),
	(0x3367, "15-"),
	(0x3368, "16-"),
	(0x3369, "17-"),
	(0x336A, "18-"),
	(0x336B, "19-"),
	(0x336C, "20-"),
	(0x336D, "21-"),
	(0x336E, "22-"),
	(0x336F, "23-"),
	(0x3370, "24-"),
	(0x3371, "h-a"),
	(0x3372, "da"),
	(0x3374, "bar"),
	(0x3375, "o-"),
	(0x3376, "pc"),
	(0x3377, "dm"),
	(0x3378, "dm2"),
	(0x3379, "dm3"),
	(0x3380, "p-"),
	(0x3381, "n-"),
	(0x3383, "m-"),
	(0x3384, "k-"),
	(0x3388, "cal"),
	(0x3389, "kcal"),
	(0x338A, "p-"),
	(0x338B, "n-"),
	(0x338D, "-g"),
	(0x338E, "mg"),
	(0x338F, "kg"),
	(0x3390, "-z"),
	(0x3391, "k-z"),
	(0x3392, "--z"),
	(0x3393, "--z"),
	(0x3394, "--z"),
	(0x3395, "-l"),
	(0x3396, "ml"),
	(0x3397, "dl"),
	(0x3398, "kl"),
	(0x3399, "fm"),
	(0x339A, "nm"),
	(0x339B, "-m"),
	(0x339C, "mm"),
	(0x339D, "cm"),
	(0x339E, "km"),
	(0x339F, "mm2"),
	(0x33A0, "cm2"),
	(0x33A1, "m2"),
	(0x33A2, "km2"),
	(0x33A3, "mm3"),
	(0x33A4, "cm3"),
	(0x33A5, "m3"),
	(0x33A6, "km3"),
	(0x33A7, "m-s"),
	(0x33A8, "m-s2"),
	(0x33A9, "-a"),
	(0x33AA, "k-a"),
	(0x33AB, "--a"),
	(0x33AC, "--a"),
	(0x33AD, "rad"),
	(0x33AE, "rad-s"),
	(0x33AF, "rad-s2"),
	(0x33B0, "ps"),
	(0x33B1, "ns"),
	(0x33B2, "-s"),
	(0x33B3, "ms"),
	(0x33B4, "p-"),
	(0x33B5, "n-"),
	(0x33B7, "m-"),
	(0x33B8, "k-"),
	(0x33BA, "p-"),
	(0x33BB, "n-"),
	(0x33BD, "m-"),
	(0x33BE, "k-"),
	(0x33C0, "k-"),
	(0x33C2, "a-m-"),
	(0x33C3, "-q"),
	(0x33C4, "cc"),
	(0x33C5, "cd"),
	(0x33C6, "--kg"),
	(0x33C7, "-o-"),
	(0x33C8, "d-"),
	(0x33C9, "-y"),
	(0x33CA, "ha"),
	(0x33CC, "in"),
	(0x33CF, "kt"),
	(0x33D0, "lm"),
	(0x33D1, "ln"),
	(0x33D2, "log"),
	(0x33D3, "lx"),
	(0x33D4, "mb"),
	(0x33D5, "mil"),
	(0x33D6, "mol"),
	(0x33D8, "p-m-"),
	(0x33DB, "sr"),
	(0x33DC, "-v"),
	(0x33DD, "-b"),
	(0x33DE, "--m"),
	(0x33DF, "--m"),
	(0x33E0, "1-"),
	(0x33E1, "2-"),
	(0x33E2, "3-"),
	(0x33E3, "4-"),
	(0x33E4, "5-"),
	(0x33E5, "6-"),
	(0x33E6, "7-"),
	(0x33E7, "8-"),
	(0x33E8, "9-"),
	(0x33E9, "10-"),
	(0x33EA, "11-"),
	(0x33EB, "12-"),
	(0x33EC, "13-"),
	(0x33ED, "14-"),
	(0x33EE, "15-"),
	(0x33EF, "16-"),
	(0x33F0, "17-"),
	(0x33F1, "18-"),
	(0x33F2, "19-"),
	(0x33F3, "20-"),
	(0x33F4, "21-"),
	(0x33F5, "22-"),
	(0x33F6, "23-"),
	(0x33F7, "24-"),
	(0x33F8, "25-"),
	(0x33F9, "26-"),
	(0x33FA, "27-"),
	(0x33FB, "28-"),
	(0x33FC, "29-"),
	(0x33FD, "30-"),
	(0x33FE, "31-"),
	(0x33FF, "gal"),
	(0xFB00, "ff"),
	(0xFB01, "fi"),
	(0xFB02, "fl"),
	(0xFB03, "ffi"),
	(0xFB04, "ffl"),
	(0xFB05, "st"),
	(0xFB06, "st"),
	(0x1F100, "0-"),
	(0x1F101, "0-"),
	(0x1F102, "1-"),
	(0x1F103, "2-"),
	(0x1F104, "3-"),
	(0x1F105, "4-"),
	(0x1F106, "5-"),
	(0x1F107, "6-"),
	(0x1F108, "7-"),
	(0x1F109, "8-"),
	(0x1F10A, "9-"),
];

/// `text` from outside (a PR's words, a CI log) as a block of data for a task file (0681): terminal escapes stripped,
/// control characters other than newline and tab dropped, at most `max_lines` lines kept (`… <n> more lines` after
/// them), every line prefixed with `│ ` and the whole wrapped in `<external source="<source>">`…`</external>`. Every
/// inner line starts with `│ `, so none can begin with a service line's lead (`SERVICE_LINE_STARTS`) or close the block.
pub fn fence_external(source: &str, text: &str, max_lines: usize) -> String {
	let source: String = source.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).collect();
	let clean = crate::frontier::tui::strip_ansi(text);
	let lines: Vec<String> =
		clean.lines().map(|line| line.chars().filter(|c| !c.is_control() || *c == '\t').collect()).collect();
	let more = lines.len().saturating_sub(max_lines);
	let mut out = vec![format!("<external source=\"{source}\">")];
	out.extend(lines.iter().take(max_lines).map(|line| format!("│ {line}")));
	if more > 0 {
		out.push(format!("│ … {more} more lines"));
	}
	out.push("</external>".to_owned());
	out.join("\n")
}
#[cfg(test)]
#[path = "wording_tests.rs"]
mod tests;
