//! The canary test of the privacy boundary (0681): the service runs with fake mail, Jira, Azure and calendar sources
//! whose lines, keys, resource ids and secrets are canaries, every tool is called as the boss, a thread and a project
//! pane, a turn-end check, a reword, a push to a phone and the failing rounds run, and nothing a model reads, a push
//! carries, a log or the journal holds may contain a canary. The one place the canaries may be is what his screens
//! show: the dock's `today` rows and the morning brief.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use axum::extract::{Request, State};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::frontier::brief::model::{Brief, Card, Kind, Proof, Severity, Verdict};
use crate::frontier::brief::routes::BriefActions;
use crate::frontier::brief::{BriefLinks, store};
use crate::frontier::protocol::ServerMessage;
use crate::frontier::server::bridge::BridgeService;
use crate::frontier::server::day::{DayOptions, PollerName, SourceSeams};
use crate::frontier::server::day_briefs;
use crate::frontier::server::helpers::{
	Page, TestServer, call, call_as, demo_project, start_thread, until_within, with_server, write_settings,
};
use crate::frontier::server::pr_review::{Platform, PrTarget, rebuilt};
use crate::frontier::server::review_notes::{ReviewNotes, review_file};
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::task_file::{NewTask, new_task, save_task};
use crate::frontier::server::tools::TOOLS;
use crate::shared::credentials::{CALENDAR_ENV, GMAIL_ENV, JIRA_ENV};
use crate::shared::events::queue::{EventPriority, EventSource, NewEvent};
use crate::shared::log::logs_dir;
use crate::shared::paths::{config_dir, data_dir};
use crate::shared::pollers::calendar::Fetched;
use crate::shared::pollers::cli::tests::fake_run;
use crate::shared::pollers::source_fakes::{FakeMail, serve_http, serve_imap};
use crate::shared::testing::temp_jocasta;

/// What no model, push, log or journal line may hold: what the sources say.
const CONTENT: [&str; 8] = [
	"Canary Sender",
	"canary-subject-4711",
	"canary-summary-0815",
	"Canary Operation",
	"canary-resource",
	"rg-canary",
	"CANARY-9",
	"sub-canary-1",
];

/// The secrets of the sources, and a password that is wrong.
const GMAIL_PASSWORD: &str = "canary-app-password";
const WRONG_PASSWORD: &str = "canary-wrong-password";
const JIRA_TOKEN: &str = "canary-jira-token-0123";
const CALENDAR_URL: &str = "https://calendar.example.test/private-canary-ical/basic.ics";
const NTFY_TOKEN: &str = "canary-ntfy-token";

/// Every canary in `text`.
fn leaks_in(text: &str, canaries: &[String]) -> Vec<String> {
	canaries.iter().filter(|canary| text.contains(canary.as_str())).cloned().collect()
}

fn canaries(extra: &[&str]) -> Vec<String> {
	CONTENT.iter().chain(extra).map(|canary| (*canary).to_owned()).collect()
}

/// The page server's stand-in: links by id, no browser.
struct NoBrowser;

impl BriefLinks for NoBrowser {
	fn url(&self, id: &str) -> String {
		format!("http://127.0.0.1:1/brief/{id}")
	}

	fn open(&self, _id: &str) {}
}

/// A local ntfy: what each push carried, without its `Authorization` header and its path (the topic is the path).
#[derive(Clone, Default)]
struct Pushes(Arc<Mutex<Vec<String>>>);

async fn record(State(pushes): State<Pushes>, request: Request) -> &'static str {
	let (parts, body) = request.into_parts();
	if parts.method == axum::http::Method::POST || parts.method == axum::http::Method::PUT {
		let body = axum::body::to_bytes(body, 1 << 20).await.unwrap_or_default();
		let headers: Vec<String> = parts
			.headers
			.iter()
			.filter(|(name, _)| name.as_str() != "authorization")
			.map(|(name, value)| format!("{name}: {}", String::from_utf8_lossy(value.as_bytes())))
			.collect();
		pushes.0.lock().unwrap_or_else(PoisonError::into_inner).push(format!(
			"{}\n{}",
			headers.join("\n"),
			String::from_utf8_lossy(&body)
		));
	}
	"{}"
}

async fn fake_ntfy() -> (String, Pushes) {
	let pushes = Pushes::default();
	let app = axum::Router::new().fallback(record).with_state(pushes.clone());
	let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
	let url = format!("http://{}", listener.local_addr().unwrap());
	tokio::spawn(async move {
		let _ = axum::serve(listener, app).await;
	});
	(url, pushes)
}

fn script(dir: &Path, name: &str, windows: &str, unix: &str) -> Vec<String> {
	let (file, text) = if cfg!(windows) { (format!("{name}.cmd"), windows) } else { (name.to_owned(), unix) };
	let path = dir.join(file);
	fs::write(&path, text).unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
	}
	vec![path.display().to_string()]
}

/// An omp that records its arguments in `omp-args.txt` and answers with a new wording.
fn fake_omp(dir: &Path) -> Vec<String> {
	if cfg!(windows) {
		let path = dir.join("omp.ps1");
		fs::write(
			&path,
			"$here = Split-Path $MyInvocation.MyCommand.Path\r\nSet-Content \"$here\\omp-args.txt\" (($args -join ' ') -replace \"`r?`n\", '\\n')\r\n'Please use a guard clause here.'\r\n",
		)
		.unwrap();
		return ["powershell.exe", "-NoProfile", "-NonInteractive", "-File"]
			.map(str::to_owned)
			.into_iter()
			.chain([path.display().to_string()])
			.collect();
	}
	script(
		dir,
		"omp",
		"",
		"#!/bin/sh\necho \"$@\" > \"$(dirname \"$0\")/omp-args.txt\"\necho 'Please use a guard clause here.'\n",
	)
}

/// The arguments each tool gets: harmless, and shaped so most answer rather than refuse.
fn args_of(tool: &str, thread: &str) -> Value {
	match tool {
		"history_search" => json!({ "query": "canary" }),
		"thread_start" => json!({ "project": "no-such-project", "title": "Canary", "brief": "Do it." }),
		"thread_send" => json!({ "thread": thread, "text": "hello" }),
		"thread_finish" | "thread_stop" => json!({ "thread": "no-such-thread" }),
		"report_result" => json!({ "outcome": "done", "summary": "Did it." }),
		"review_note" => {
			json!({ "kind": "question", "file": "README.md", "line": 1, "quote": "demo", "text": "A note." })
		}
		"show" => json!({ "title": "Canary", "sections": [{ "type": "text", "body": "Hello." }] }),
		"note" => json!({ "text": "a note" }),
		"remind" => json!({ "text": "a reminder", "at": "23:59" }),
		"lesson" => json!({ "text": "a lesson", "project": "demo" }),
		"big_picture_review" => json!({ "project": "no-such-project" }),
		"worktree_remove" => json!({ "path": "no-such-worktree" }),
		_ => json!({}),
	}
}

/// Every text file under `dir` (not the catalog, which holds his private rows for his screens, and not the briefs).
fn files_under(dir: &Path, found: &mut Vec<(PathBuf, String)>) {
	let Ok(entries) = fs::read_dir(dir) else { return };
	for entry in entries.flatten() {
		let path = entry.path();
		let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_owned();
		if path.is_dir() {
			if name != "briefs" {
				files_under(&path, found);
			}
		} else if !name.contains(".db")
			&& name != "ntfy-topics.json"
			&& let Ok(text) = fs::read_to_string(&path)
		{
			found.push((path, text));
		}
	}
}

fn key_of(server: &TestServer, pane: &str) -> String {
	server
		.herdr
		.env_of(pane)
		.and_then(|env| env.into_iter().find(|(name, _)| name == "JOCASTA_PANE_KEY").map(|(_, key)| key))
		.expect("the pane has a key")
}

/// A review thread of pull request 7 with one note, and its stored brief; the brief's id.
fn review_brief() -> String {
	let project = format!("p-acme-{}", uuid::Uuid::new_v4().simple());
	let task = new_task(
		NewTask {
			project_id: project.clone(),
			project: "acme".to_owned(),
			title: "Review: Add a thing".to_owned(),
			pr: Some(PrTarget {
				platform: Platform::Github,
				url: "https://github.com/acme/app/pull/7".to_owned(),
				number: 7,
				repo: "acme/app".to_owned(),
				org_url: None,
				project: None,
				title: "Add a thing".to_owned(),
				author: "client-a".to_owned(),
				base_branch: "main".to_owned(),
				head_sha: "1111111111111111111111111111111111111111".to_owned(),
				base_sha: "2222222222222222222222222222222222222222".to_owned(),
				ci: "passing".to_owned(),
				threads: 0,
			}),
			..NewTask::default()
		},
		None,
	);
	save_task(&task).unwrap();
	let notes = ReviewNotes {
		head: "1111111111111111111111111111111111111111".to_owned(),
		notes: vec![Card {
			id: "n1".to_owned(),
			kind: Kind::Finding,
			severity: Severity::P1,
			file: "src/a.rs".to_owned(),
			line: 12,
			quote: "let x = 1;".to_owned(),
			text: "This can overflow.".to_owned(),
			verdict: Verdict::Post,
			proof: Proof { sha: "1111111111111111111111111111111111111111".to_owned(), ok: true },
			edited: None,
			published: None,
			ran: None,
		}],
		..ReviewNotes::default()
	};
	notes.write(&review_file(&task.project_id, &task.id)).unwrap();
	let id = format!("pr-{project}-7");
	let empty = Brief {
		id: id.clone(),
		kind: "pr".to_owned(),
		title: String::new(),
		subtitle: String::new(),
		generated_at_ms: 1,
		sections: Vec::new(),
	};
	store::save(&rebuilt(&empty, &task, "acme", &notes).unwrap()).unwrap();
	id
}

#[tokio::test(flavor = "multi_thread")]
#[allow(
	clippy::too_many_lines,
	reason = "one world, every surface scanned once: splitting it would start the sources twice"
)]
async fn no_canary_reaches_a_model_a_push_a_log_or_a_file_a_model_reads_and_only_his_screens_hold_them_0681() {
	let mut j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	for (name, value) in [(CALENDAR_ENV, CALENDAR_URL), (GMAIL_ENV, GMAIL_PASSWORD), (JIRA_ENV, JIRA_TOKEN)] {
		j.set_var(name, Some(value));
	}
	let (ntfy, pushes) = fake_ntfy().await;
	write_settings(
		&config_dir(),
		&json!({
			"pollers": {
				"gmail": { "address": "someone@example.org" },
				"jira": { "site": "https://example.atlassian.net", "email": "someone@example.org" },
				"azurePortal": {},
			},
			"ntfy": { "server": ntfy, "ownServer": true, "token": NTFY_TOKEN },
		}),
	);
	let omp_dir = j.root().join("fake-omp");
	fs::create_dir_all(&omp_dir).unwrap();
	let omp = fake_omp(&omp_dir);

	let mails: Vec<FakeMail> = (1..=3)
		.map(|i| FakeMail {
			uid: i,
			msgid: 0xabc0 + u64::from(i),
			from: "\"Canary Sender\" <sender@example.org>",
			subject: "canary-subject-4711",
		})
		.collect();
	let imap = serve_imap(mails, GMAIL_PASSWORD).await;
	let issues = json!({ "issues": [
		{ "key": "CANARY-9", "fields": { "summary": "canary-summary-0815", "status": { "name": "To Do" } } },
	] })
	.to_string();
	let (site, _) = serve_http(200, issues).await;
	let resource = "/subscriptions/sub-canary-1/resourceGroups/rg-canary/providers/Microsoft.Web/sites/canary-resource";
	let (run, _) = fake_run(vec![
		("az account list", json!(["sub-canary-1"])),
		(
			"az monitor activity-log list",
			json!([{ "operationName": { "value": "w/write", "localizedValue": "Canary Operation" }, "resourceId": resource }]),
		),
	]);
	let password = Arc::new(Mutex::new(GMAIL_PASSWORD.to_owned()));
	let sent_password = Arc::clone(&password);
	let day = DayOptions {
		pollers: Some(false),
		run: Some(Arc::new(run)),
		sources: Some(SourceSeams {
			gmail_secret: Some(Arc::new(move || {
				Some(sent_password.lock().unwrap_or_else(PoisonError::into_inner).clone())
			})),
			jira_secret: Some(Arc::new(|| Some(JIRA_TOKEN.to_owned()))),
			imap_endpoint: Some(imap),
			jira_site: Some(site),
		}),
		calendar_secret: Some(Arc::new(|| Some(CALENDAR_URL.to_owned()))),
		// A calendar that fails: the error path must not echo the URL.
		calendar_fetch: Some(Arc::new(|_| Box::pin(async { Some(Fetched { status: 500, body: String::new() }) }))),
		..DayOptions::default()
	};
	let options =
		ServerOptions { day: Some(day), omp: Some(omp), briefs: Some(Arc::new(NoBrowser)), ..ServerOptions::default() };
	let reword_dir = omp_dir.clone();
	with_server(options, |server| async move {
		let started = start_thread(&server, Default::default()).await;
		let project_pane = server
			.herdr
			.state()
			.pane_list
			.iter()
			.find(|pane| {
				pane.cwd.as_deref().is_some_and(|cwd| crate::shared::pathkey::same_dir(cwd, &project.main_path))
			})
			.map(|pane| pane.pane_id.clone())
			.expect("the project pane");
		let page = Page::open(&server, "").await;
		let day = server.service.day();

		// His screens: the rounds, then the morning brief he asked for.
		for poller in [PollerName::Gmail, PollerName::Jira, PollerName::AzurePortal, PollerName::Calendar] {
			day.poll(poller).await;
		}
		day_briefs::open_morning(server.service.context()).await.unwrap();
		let briefs: Vec<String> = fs::read_dir(data_dir().join("briefs"))
			.unwrap()
			.flatten()
			.filter(|entry| entry.file_name().to_string_lossy().starts_with("morning-"))
			.map(|entry| fs::read_to_string(entry.path()).unwrap())
			.collect();
		assert_eq!(briefs.len(), 1, "one morning brief");
		let his = leaks_in(&briefs[0], &canaries(&[]));
		for canary in ["Canary Sender", "canary-subject-4711", "CANARY-9", "canary-summary-0815", "Canary Operation"] {
			assert!(his.iter().any(|found| found == canary), "his morning brief should hold {canary}: {his:?}");
		}
		// The scanner sees them where they are: that is what a leak into `today_text` would look like.
		let leaky = format!("{}\nmail: canary-subject-4711", day.today_text().unwrap());
		assert!(!leaks_in(&leaky, &canaries(&[])).is_empty());

		// A sign-in the server refuses, and a push to his phone of something that needs him.
		*password.lock().unwrap_or_else(PoisonError::into_inner) = WRONG_PASSWORD.to_owned();
		day.poll(PollerName::Gmail).await;
		let needs = NewEvent {
			source_id: "github:review:acme/api#42".to_owned(),
			source: EventSource::Github,
			kind: "review".to_owned(),
			priority: EventPriority::Normal,
			text: "Review requested — Add export (acme/api #42)".to_owned(),
			url: Some("https://github.com/acme/api/pull/42".to_owned()),
			project_id: None,
			thread_id: None,
			private_text: None,
			private_url: None,
		};
		day.raise(&needs).unwrap();
		until_within(|| !pushes.0.lock().unwrap_or_else(PoisonError::into_inner).is_empty(), 20_000).await;

		// Every tool, as the boss, a thread and a project pane.
		let thread_key = started.pane_id.clone();
		let mut answers: Vec<(String, String)> = Vec::new();
		for tool in TOOLS {
			let args = args_of(tool.name, &thread_key);
			let boss = call(&server, tool.name, &args).await;
			answers.push((format!("boss {}", tool.name), boss.text));
			let thread = call_as(&server, tool.name, &args, Some(&started.pane_id), None).await;
			answers.push((format!("thread {}", tool.name), thread.text));
			let project = call_as(&server, tool.name, &args, Some(&project_pane), None).await;
			answers.push((format!("project {}", tool.name), project.text));
		}
		// The boss's `today` is where the sources show up, as counts.
		let today = call(&server, "today", &json!({})).await.text;
		for line in ["mail: 0 unread", "jira: 1 open", "azure: 1 failed"] {
			// The mail round was refused above, so mail says why instead of a count.
			if line.starts_with("mail") {
				continue;
			}
			assert!(today.lines().any(|found| found == line), "{line} in {today}");
		}
		answers.push(("today".to_owned(), today));

		// A turn-end check in each kind of pane.
		for pane in [&started.pane_id, &project_pane] {
			let (hold, signal) = tokio::sync::watch::channel(None::<String>);
			let context = BridgeService::stop(&*server.service, key_of(&server, pane), signal).await;
			drop(hold);
			answers.push((format!("stop {pane}"), context));
		}

		// A reword whose instruction holds the Jira token: omp's argv holds `[secret]`.
		let brief = review_brief();
		let instruction = format!("shorter, and mention {JIRA_TOKEN}");
		BriefActions::act(&*server.service, brief, json!({ "op": "reword", "card": "n1", "instruction": instruction }))
			.await
			.expect("the reword runs");
		let argv = fs::read_to_string(reword_dir.join("omp-args.txt")).expect("omp was run");
		assert!(argv.contains("[secret]") && !argv.contains(JIRA_TOKEN), "{argv}");
		answers.push(("reword argv".to_owned(), argv));
		let sealed = server.service.context().bridge.seal(&format!("token {JIRA_TOKEN} here"));
		assert_eq!(sealed.as_str(), "token [secret] here");

		// What a model could read, one by one.
		let topics = crate::frontier::server::phone::topics(&data_dir());
		let secrets = [GMAIL_PASSWORD, WRONG_PASSWORD, JIRA_TOKEN, CALENDAR_URL, "private-canary-ical"];
		let all = canaries(&[secrets.as_slice(), &[topics.main.as_str(), topics.reply.as_str()]].concat());
		let mut surfaces: Vec<(String, String)> = answers;
		let calls: Vec<String> = server.herdr.methods();
		for method in calls.iter().collect::<std::collections::BTreeSet<_>>() {
			surfaces.push((format!("herdr {method}"), format!("{:?}", server.herdr.of(method))));
		}
		surfaces.push(("the activity".to_owned(), format!("{:?}", server.service.day().dashboard().activity)));
		surfaces.push(("the day's notes".to_owned(), format!("{:?}", server.service.day().dashboard())));
		surfaces.push((
			"the messages he saw besides today".to_owned(),
			page.received()
				.iter()
				.filter(|message| !matches!(message, ServerMessage::Today { .. } | ServerMessage::Snapshot { .. }))
				.map(|message| format!("{message:?}"))
				.collect::<Vec<_>>()
				.join("\n"),
		));
		let mut files = Vec::new();
		files_under(&data_dir(), &mut files);
		if let Ok(logs) = fs::read_dir(logs_dir()) {
			for log in logs.flatten() {
				files.push((log.path(), fs::read_to_string(log.path()).unwrap_or_default()));
			}
		}
		for (path, text) in files {
			surfaces.push((format!("file {}", path.display()), text));
		}
		let pushed: Vec<String> = pushes.0.lock().unwrap_or_else(PoisonError::into_inner).clone();
		assert!(!pushed.is_empty(), "the phone was told something");
		for (n, push) in pushed.iter().enumerate() {
			surfaces.push((format!("ntfy push {n}"), push.clone()));
		}
		let mut leaked = Vec::new();
		for (name, text) in &surfaces {
			let found = leaks_in(text, &all);
			if !found.is_empty() {
				leaked.push(format!("{name}: {found:?}"));
			}
		}
		assert!(leaked.is_empty(), "canaries leaked:\n{}", leaked.join("\n"));
		// The tools' own words carry nothing from the sources even when a count is told.
		assert!(surfaces.iter().any(|(name, _)| name == "boss today"));
	})
	.await;
}

#[test]
fn the_scanner_flags_a_today_text_that_holds_a_private_row_and_passes_the_counts() {
	let counts = "mail: 3 unread\njira: 2 open\nazure: 1 failed";
	assert!(leaks_in(counts, &canaries(&[])).is_empty());
	for row in [
		"mail: Canary Sender: canary-subject-4711",
		"jira: CANARY-9 canary-summary-0815 · To Do",
		"azure: Canary Operation failed · canary-resource (rg-canary)",
		"azure: /subscriptions/sub-canary-1/resourceGroups/x",
	] {
		assert!(!leaks_in(&format!("{counts}\n{row}"), &canaries(&[])).is_empty(), "{row}");
	}
}

#[test]
fn a_tool_never_reads_a_stored_brief_so_the_morning_brief_is_for_his_eyes_only() {
	// The tools save what `show` draws and never load, list or open a brief: the morning brief holds his private rows.
	for (file, text) in [
		("tools.rs", include_str!("tools.rs")),
		("tool_specs.rs", include_str!("tool_specs.rs")),
		("history.rs", include_str!("history.rs")),
	] {
		for forbidden in ["store::load", "store::exists", "morning", "brief::morning"] {
			assert!(!text.contains(forbidden), "{file} mentions {forbidden}");
		}
	}
}
