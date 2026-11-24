//! The morning brief (0680): `morning-<yyyy-mm-dd>`, built by code from what `today`, the HUD and the dashboard hold. It
//! is one of the two places that may draw the private sources' rows for his eyes (0681: `HumanView::for_his_eyes()` is
//! called only under `src/frontier/today/` and here), so the brief is the one stored thing that holds their text; no
//! tool reads a morning brief back (`e2e_privacy_tests.rs` holds that). Nothing here asks a model.

use chrono::DateTime;

use crate::frontier::brief::model::{Brief, Fact, FlowEdge, FlowNode, ListRow, NodeState, Section};
use crate::frontier::protocol::{
	Dashboard, Headline, NeedsYou, NeedsYouKind, PrivateSection, QuotaAccount, QuotaCapacity, QuotaSpan, QuotaWindow,
	Today, quota_high, quota_percent, wait_label,
};
use crate::shared::day::{DayRef, day_label, hhmm, weekday};
use crate::shared::private::HumanView;

/// The most headlines the brief carries.
const NEWS_ITEMS: usize = 20;

/// What the morning brief is built from.
pub struct MorningInput<'a> {
	/// The local day, `YYYY-MM-DD`.
	pub date: &'a str,
	/// Now, epoch ms.
	pub now: i64,
	pub today: &'a Today,
	pub dashboard: &'a Dashboard,
	pub accounts: &'a [QuotaAccount],
	pub capacity: &'a [QuotaCapacity],
	/// The work graph's boxes and arrows: every open thread and every waiting start, thread → PR → CI (empty: none).
	pub work: (Vec<FlowNode>, Vec<FlowEdge>),
}

/// The brief's id for a local day.
pub fn brief_id(date: &str) -> String {
	format!("morning-{date}")
}

/// The brief, its flows not yet laid out (`layout::lay_out` does that).
pub fn build(input: &MorningInput<'_>) -> Brief {
	let today = input.today;
	let (others, reviews, pipelines) = needs_you_split(&today.needs_you);
	let mut sections = vec![
		calendar(today, input.now),
		list("Needs you", rows_of(&others, input.now), "Nothing waits on you."),
		list("Pull requests awaiting your review", rows_of(&reviews, input.now), "No review is requested."),
		work(&input.work),
		list("Red pipelines", rows_of(&pipelines, input.now), "No pipeline is red."),
	];
	let view = HumanView::for_his_eyes();
	for (heading, section, nothing) in [
		("Mail", today.mail.as_ref(), "No unread mail."),
		("Jira", today.jira.as_ref(), "No open issues."),
		("Azure", today.azure.as_ref(), "Nothing failed in the last 24 hours."),
	] {
		if let Some(section) = section {
			sections.push(list(heading, private_rows(section, &view), nothing));
		}
	}
	sections.push(quota(input));
	sections.push(list("News", news_rows(input.dashboard), "No headlines yet."));
	Brief {
		id: brief_id(input.date),
		kind: "morning".to_owned(),
		title: "Morning brief".to_owned(),
		subtitle: day_label(DayRef::Day(input.date), true),
		generated_at_ms: input.now,
		sections,
	}
}

/// NEEDS YOU's rows apart from the review requests and red pipelines, which have sections of their own.
fn needs_you_split(rows: &[NeedsYou]) -> (Vec<&NeedsYou>, Vec<&NeedsYou>, Vec<&NeedsYou>) {
	let of = |kind: NeedsYouKind| rows.iter().filter(move |row| row.kind == kind).collect::<Vec<_>>();
	let others = rows.iter().filter(|row| !matches!(row.kind, NeedsYouKind::Review | NeedsYouKind::Pipeline)).collect();
	(others, of(NeedsYouKind::Review), of(NeedsYouKind::Pipeline))
}

fn rows_of(rows: &[&NeedsYou], now: i64) -> Vec<ListRow> {
	rows.iter()
		.map(|row| {
			let project = row.project.as_deref().filter(|p| !p.is_empty()).map(|p| format!("{p}/")).unwrap_or_default();
			let waited = row
				.since
				.as_deref()
				.and_then(|since| DateTime::parse_from_rfc3339(since).ok())
				.map(|since| format!("waiting {}", wait_label(now - since.timestamp_millis())));
			ListRow {
				text: format!("{project}{}", row.text),
				url: row.url.clone().filter(|url| !url.is_empty()),
				note: waited.or_else(|| row.time.clone()),
			}
		})
		.collect()
}

/// A `list` section; `nothing` is its one row when there are none.
fn list(heading: &str, mut rows: Vec<ListRow>, nothing: &str) -> Section {
	if rows.is_empty() {
		rows.push(ListRow { text: nothing.to_owned(), url: None, note: None });
	}
	Section::List { heading: Some(heading.to_owned()), rows }
}

/// Today's meetings in time order as a flow: the past ones done, the one under way or else the next one active.
fn calendar(today: &Today, now: i64) -> Section {
	let heading = Some("Calendar".to_owned());
	if today.calendar.is_empty() {
		return Section::Text { heading, body: "Nothing on the calendar today.".to_owned() };
	}
	let clock = hhmm(now);
	let started = |time: &str| time != "all day" && time <= clock.as_str();
	let over = |entry: &crate::frontier::protocol::CalendarEntry| {
		entry.time != "all day" && entry.end.as_deref().unwrap_or(&entry.time) <= clock.as_str()
	};
	// The one under way, else the first that is still ahead.
	let active = today
		.calendar
		.iter()
		.position(|entry| started(&entry.time) && !over(entry))
		.or_else(|| today.calendar.iter().position(|entry| entry.time != "all day" && !started(&entry.time)));
	let nodes: Vec<FlowNode> = today
		.calendar
		.iter()
		.enumerate()
		.map(|(index, entry)| FlowNode {
			id: format!("m{index}"),
			label: format!("{} {}", entry.time, entry.title),
			group: None,
			state: if over(entry) {
				NodeState::Done
			} else if Some(index) == active {
				NodeState::Active
			} else {
				NodeState::Pending
			},
			detail: entry.url.clone().filter(|url| !url.is_empty()),
			layer: 0,
			order: 0,
		})
		.collect();
	let edges = (1..nodes.len())
		.map(|index| FlowEdge { from: format!("m{}", index - 1), to: format!("m{index}"), label: None })
		.collect();
	Section::Flow { heading, nodes, edges }
}

/// The work graph: every open thread and waiting start, and each thread's PR and its CI.
fn work(graph: &(Vec<FlowNode>, Vec<FlowEdge>)) -> Section {
	let heading = Some("Work".to_owned());
	if graph.0.is_empty() {
		return Section::Text { heading, body: "No thread is open and none waits to start.".to_owned() };
	}
	Section::Flow { heading, nodes: graph.0.clone(), edges: graph.1.clone() }
}

/// A private source's rows for his eyes: its failure line first, then each row with its link.
fn private_rows(section: &PrivateSection, view: &HumanView) -> Vec<ListRow> {
	let error = section.error.iter().map(|line| ListRow { text: line.clone(), url: None, note: None });
	let rows = section.rows.iter().map(|row| ListRow {
		text: row.text.reveal(view).clone(),
		url: row.url.as_ref().map(|url| url.reveal(view).clone()).filter(|url| !url.is_empty()),
		note: None,
	});
	error.chain(rows).collect()
}

/// `5h 42% · 7d 18% · resets 16:30` for one account.
fn account_value(account: &QuotaAccount, now: i64) -> String {
	let windows: Vec<(&str, &QuotaWindow)> = [("5h", account.five_h.as_ref()), ("7d", account.seven_d.as_ref())]
		.into_iter()
		.filter_map(|(label, window)| window.map(|window| (label, window)))
		.collect();
	let at = |window: &QuotaWindow| {
		window
			.resets_at
			.as_deref()
			.and_then(|text| DateTime::parse_from_rfc3339(text).ok())
			.map(|t| t.timestamp_millis())
	};
	let mut parts: Vec<String> = windows
		.iter()
		.map(|(label, window)| {
			let high = if quota_high(window) { " (high)" } else { "" };
			format!("{label} {}%{high}", quota_percent(window))
		})
		.collect();
	if let Some(reset) = windows.iter().filter_map(|(_, window)| at(window)).filter(|at| *at > now).min() {
		let when = if reset - now <= 86_400_000 { hhmm(reset) } else { format!("{} {}", weekday(reset), hhmm(reset)) };
		parts.push(format!("resets {when}"));
	}
	parts.join(" · ")
}

fn gigabytes(bytes: u64) -> String {
	format!("{:.0} GB", bytes as f64 / 1_073_741_824.0)
}

/// Quota per account and the machine.
fn quota(input: &MorningInput<'_>) -> Section {
	let mut rows: Vec<Fact> = input
		.accounts
		.iter()
		.map(|account| Fact { label: account.name.clone(), value: account_value(account, input.now), url: None })
		.collect();
	for capacity in input.capacity {
		let window = if capacity.window == QuotaSpan::FiveHours { "5h" } else { "7d" };
		let word = if capacity.accounts == 1 { "account" } else { "accounts" };
		rows.push(Fact {
			label: format!("Left in {window}"),
			value: format!("{} of {} {word}", capacity.remaining, capacity.accounts),
			url: None,
		});
	}
	if rows.is_empty() {
		rows.push(Fact { label: "Quota".to_owned(), value: "not read yet".to_owned(), url: None });
	}
	if let Some(machine) = input.dashboard.machine {
		let memory = if machine.ram.total > 0 {
			(machine.ram.used as f64 / machine.ram.total as f64 * 100.0).round()
		} else {
			0.0
		};
		rows.push(Fact {
			label: "Machine".to_owned(),
			value: format!(
				"cpu {}% · mem {memory}% · disk {} free",
				(machine.cpu * 100.0).round(),
				gigabytes(machine.disk.free)
			),
			url: None,
		});
	}
	Section::Facts { heading: Some("Quota and machine".to_owned()), rows }
}

/// The headlines of every feed, newest first, as `Feed: title` rows with their links.
fn news_rows(dashboard: &Dashboard) -> Vec<ListRow> {
	let mut items: Vec<(&str, &Headline)> = dashboard
		.news
		.iter()
		.flat_map(|feed| feed.items.iter().map(move |item| (feed.heading.as_str(), item)))
		.collect();
	let at = |item: &Headline| DateTime::parse_from_rfc3339(&item.at).ok().map(|t| t.timestamp_millis());
	items.sort_by_key(|(_, item)| std::cmp::Reverse(at(item).unwrap_or(i64::MIN)));
	items
		.into_iter()
		.take(NEWS_ITEMS)
		.map(|(feed, item)| ListRow {
			text: format!("{feed}: {}", item.title),
			url: Some(item.url.clone()).filter(|url| !url.is_empty()),
			note: None,
		})
		.collect()
}

#[cfg(test)]
#[path = "morning_tests.rs"]
mod tests;
