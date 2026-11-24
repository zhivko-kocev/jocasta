use super::*;
use crate::frontier::brief::layout::lay_out;
use crate::frontier::protocol::{PrivateRow, PrivateSection};
use crate::frontier::today::view::tests::{dashboard, now, today};
use crate::shared::private::Private;

fn row(text: &str, url: &str) -> PrivateRow {
	PrivateRow { text: Private::from(text.to_owned()), url: Some(Private::from(url.to_owned())) }
}

fn input<'a>(today: &'a Today, dashboard: &'a Dashboard) -> MorningInput<'a> {
	MorningInput {
		date: "2026-11-09",
		now: now(),
		today,
		dashboard,
		accounts: &[],
		capacity: &[],
		work: (vec![], vec![]),
	}
}

fn heading_of(section: &Section) -> &str {
	match section {
		Section::Text { heading, .. }
		| Section::Facts { heading, .. }
		| Section::Flow { heading, .. }
		| Section::List { heading, .. } => heading.as_deref().unwrap_or(""),
		_ => "",
	}
}

#[test]
fn the_morning_brief_has_the_planned_sections_in_order_and_lays_out() {
	let (today, dashboard) = (today(), dashboard());
	let mut brief = build(&input(&today, &dashboard));
	lay_out(&mut brief).unwrap();
	assert_eq!((brief.id.as_str(), brief.kind.as_str()), ("morning-2026-11-09", "morning"));
	let headings: Vec<&str> = brief.sections.iter().map(heading_of).collect();
	assert_eq!(
		headings,
		[
			"Calendar",
			"Needs you",
			"Pull requests awaiting your review",
			"Work",
			"Red pipelines",
			"Quota and machine",
			"News"
		]
	);
	let types: Vec<&str> = brief.sections.iter().map(Section::type_name).collect();
	assert_eq!(types, ["flow", "list", "list", "text", "list", "facts", "list"]);
}

#[test]
fn meetings_are_a_flow_in_time_order_with_the_past_done_the_one_under_way_active_and_a_chain_of_edges() {
	let (today, dashboard) = (today(), dashboard());
	let Section::Flow { nodes, edges, .. } = build(&input(&today, &dashboard)).sections.remove(0) else {
		panic!("the calendar is a flow");
	};
	let states: Vec<(String, NodeState)> = nodes.iter().map(|node| (node.label.clone(), node.state)).collect();
	// 14:30: the 09:00 standup (no end) is over, the 14:00–15:00 sync is under way, 16:00 is ahead, all day waits.
	assert_eq!(
		states,
		[
			("all day Release week".to_owned(), NodeState::Pending),
			("09:00 Standup".to_owned(), NodeState::Done),
			("14:00 Client-a sync".to_owned(), NodeState::Active),
			("16:00 Review with client-a".to_owned(), NodeState::Pending),
		]
	);
	assert_eq!(edges.len(), 3);
	assert_eq!((edges[0].from.as_str(), edges[0].to.as_str()), ("m0", "m1"));
}

#[test]
fn review_requests_and_red_pipelines_have_their_own_sections_and_needs_you_keeps_the_rest() {
	let (mut today, dashboard) = (today(), dashboard());
	let mut red = today.needs_you[1].clone();
	red.kind = NeedsYouKind::Pipeline;
	red.text = "client-a/api — build failed".to_owned();
	red.url = Some("https://example.org/ci/9".to_owned());
	today.needs_you.push(red);
	let brief = build(&input(&today, &dashboard));
	let rows = |at: usize| match &brief.sections[at] {
		Section::List { rows, .. } => rows.clone(),
		other => panic!("{other:?}"),
	};
	assert_eq!(rows(1).len(), 1);
	assert!(rows(1)[0].text.contains("Retry — which backoff?"));
	assert_eq!(rows(2)[0].url.as_deref(), Some("https://example.org/web/pull/12"));
	assert_eq!(rows(4)[0].url.as_deref(), Some("https://example.org/ci/9"));
	// An empty section says so in one row.
	today.needs_you.clear();
	let brief = build(&input(&today, &dashboard));
	assert_eq!(rows_text(&brief, 1), ["Nothing waits on you."]);
}

fn rows_text(brief: &Brief, at: usize) -> Vec<String> {
	match &brief.sections[at] {
		Section::List { rows, .. } => rows.iter().map(|row| row.text.clone()).collect(),
		other => panic!("{other:?}"),
	}
}

#[test]
fn the_private_sources_are_sections_only_when_configured_and_hold_the_real_lines_for_his_eyes() {
	let (mut today, dashboard) = (today(), dashboard());
	let plain = build(&input(&today, &dashboard));
	assert!(!plain.sections.iter().any(|section| ["Mail", "Jira", "Azure"].contains(&heading_of(section))));
	today.mail = Some(PrivateSection {
		rows: vec![row("Jane Doe: Café menu", "https://mail.google.com/mail/u/0/#inbox/1a2b3c01")],
		error: None,
	});
	today.jira = Some(PrivateSection { rows: vec![], error: None });
	today.azure = Some(PrivateSection { rows: vec![], error: Some("azure: not signed in".to_owned()) });
	let brief = build(&input(&today, &dashboard));
	let at = |name: &str| brief.sections.iter().position(|section| heading_of(section) == name).expect(name);
	assert!(at("Red pipelines") < at("Mail") && at("Mail") < at("Jira") && at("Jira") < at("Azure"));
	assert!(at("Azure") < at("Quota and machine"));
	assert_eq!(rows_text(&brief, at("Mail")), ["Jane Doe: Café menu"]);
	assert_eq!(rows_text(&brief, at("Jira")), ["No open issues."]);
	assert_eq!(rows_text(&brief, at("Azure")), ["azure: not signed in"]);
	let Section::List { rows, .. } = &brief.sections[at("Mail")] else { panic!("a list") };
	assert_eq!(rows[0].url.as_deref(), Some("https://mail.google.com/mail/u/0/#inbox/1a2b3c01"));
}

#[test]
fn quota_lists_each_account_the_capacity_and_the_machine_and_news_comes_newest_first_with_links() {
	let (today, dashboard) = (today(), dashboard());
	let accounts = [QuotaAccount {
		name: "zi*".to_owned(),
		five_h: Some(QuotaWindow { window: QuotaSpan::FiveHours, used: 0.42, resets_at: None }),
		seven_d: Some(QuotaWindow { window: QuotaSpan::SevenDays, used: 0.18, resets_at: None }),
	}];
	let capacity = [QuotaCapacity { window: QuotaSpan::FiveHours, remaining: 1, accounts: 2 }];
	let mut given = input(&today, &dashboard);
	given.accounts = &accounts;
	given.capacity = &capacity;
	let brief = build(&given);
	let Section::Facts { rows, .. } = &brief.sections[5] else { panic!("facts") };
	assert_eq!(rows[0].label, "zi*");
	assert!(rows[0].value.starts_with("5h 42%"), "{}", rows[0].value);
	assert_eq!((rows[1].label.as_str(), rows[1].value.as_str()), ("Left in 5h", "1 of 2 accounts"));
	assert_eq!(rows[2].label, "Machine");
	assert!(rows[2].value.starts_with("cpu 23% · mem 60%"), "{}", rows[2].value);
	let Section::List { rows, .. } = &brief.sections[6] else { panic!("a list") };
	assert_eq!(rows[0].text, "Hacker News: A headline");
	assert_eq!(rows[0].url.as_deref(), Some("https://example.org/1"));
}

#[test]
fn a_work_graph_is_a_flow_and_none_is_one_text_row() {
	let (today, dashboard) = (today(), dashboard());
	let node = |id: &str, state| FlowNode {
		id: id.to_owned(),
		label: id.to_owned(),
		group: None,
		state,
		detail: None,
		layer: 0,
		order: 0,
	};
	let mut given = input(&today, &dashboard);
	given.work = (
		vec![node("t-1", NodeState::Done), node("t-2", NodeState::Pending)],
		vec![FlowEdge { from: "t-1".to_owned(), to: "t-2".to_owned(), label: None }],
	);
	let mut brief = build(&given);
	lay_out(&mut brief).unwrap();
	let Section::Flow { nodes, .. } = &brief.sections[3] else { panic!("a flow") };
	assert_eq!((nodes[0].layer, nodes[1].layer), (0, 1));
	given.work = (vec![], vec![]);
	assert_eq!(build(&given).sections[3].type_name(), "text");
}
