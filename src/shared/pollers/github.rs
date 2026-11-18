//! The GitHub poller (0301).

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::shared::errors::Result;
use crate::shared::events::queue::{EventPriority, EventSource, NewEvent, PollRound};
use crate::shared::link::web_link;
use crate::shared::pollers::cli::{CliCommand, RunCli, run_json};

#[derive(Deserialize)]
struct Notification {
	id: String,
	reason: Option<String>,
	subject: Option<Subject>,
	repository: Option<NotificationRepository>,
}

#[derive(Deserialize)]
struct Subject {
	title: Option<String>,
	url: Option<String>,
	#[serde(rename = "type")]
	kind: Option<String>,
}

#[derive(Deserialize)]
struct NotificationRepository {
	full_name: Option<String>,
}

/// A status rollup context: a check run, or a commit status (the other fields absent).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CheckContext {
	name: Option<String>,
	conclusion: Option<String>,
	status: Option<String>,
	details_url: Option<String>,
	started_at: Option<String>,
	check_suite: Option<CheckSuite>,
	context: Option<String>,
	state: Option<String>,
	target_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckSuite {
	workflow_run: Option<WorkflowRun>,
}

#[derive(Deserialize)]
struct WorkflowRun {
	event: Option<String>,
	workflow: Option<Workflow>,
}

#[derive(Deserialize)]
struct Workflow {
	name: Option<String>,
}

#[derive(Deserialize)]
struct OpenPr {
	url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecentPr {
	number: Option<i64>,
	title: Option<String>,
	url: Option<String>,
	repository: Option<PrRepository>,
	commits: Option<Commits>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrRepository {
	name_with_owner: Option<String>,
}

#[derive(Deserialize)]
struct Commits {
	nodes: Option<Vec<Option<CommitNode>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitNode {
	commit: Option<Commit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
	status_check_rollup: Option<Rollup>,
}

#[derive(Deserialize)]
struct Rollup {
	contexts: Option<Contexts>,
}

#[derive(Deserialize)]
struct Contexts {
	nodes: Option<Vec<Option<CheckContext>>>,
}

#[derive(Deserialize)]
struct Search<T> {
	nodes: Option<Vec<Option<T>>>,
}

#[derive(Deserialize, Default)]
struct Answer {
	data: Option<AnswerData>,
}

#[derive(Deserialize)]
struct AnswerData {
	open: Option<Search<OpenPr>>,
	recent: Option<Search<RecentPr>>,
}

/// His open PRs read each round to tell which are still open; resolving a pipeline event needs its PR gone (SHR-8).
const MAX_OPEN: u32 = 100;
/// The PRs whose checks are read each round, his most recently updated first.
const MAX_PRS: u32 = 10;

/// One GraphQL request (PERF-8) for his open PRs, most recently updated first: the first 100 for the open set, and the
/// newest 10 with their last commit's checks (as `gh pr checks` reads them). No `"` or `%`: `run_cli` refuses those.
fn prs_query() -> String {
	[
		"query($q: String!) {".to_owned(),
		format!("open: search(type: ISSUE, query: $q, first: {MAX_OPEN}) {{ nodes {{ ... on PullRequest {{ url }} }} }}"),
		format!("recent: search(type: ISSUE, query: $q, first: {MAX_PRS}) {{ nodes {{ ... on PullRequest {{"),
		"number title url repository { nameWithOwner }".to_owned(),
		"commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) { nodes {".to_owned(),
		"... on CheckRun { name conclusion status detailsUrl startedAt checkSuite { workflowRun { event workflow { name } } } }"
			.to_owned(),
		"... on StatusContext { context state targetUrl }".to_owned(),
		"} } } } } }".to_owned(),
		"} } }".to_owned(),
		"}".to_owned(),
	]
	.join(" ")
}

/// `gh search prs --author @me --state open --sort updated` as a search query.
const PRS_SEARCH: &str = "is:pr is:open author:@me sort:updated-desc";

static API_PULL: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^https://api\.github\.com/repos/([^/]+/[^/]+)/pulls/(\d+)$").expect("pattern"));
static WEB_PULL: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^https://github\.com/([^/]+/[^/]+)/pull/(\d+)$").expect("pattern"));

/// `https://api.github.com/repos/o/r/pulls/42` → `https://github.com/o/r/pull/42`.
fn web_url(api: Option<&str>) -> Option<String> {
	let captures = API_PULL.captures(api?)?;
	Some(format!("https://github.com/{}/pull/{}", &captures[1], &captures[2]))
}

/// `https://github.com/o/r/pull/42` → `o/r#42`, the key of a pipeline event's source id.
fn pr_key(url: Option<&str>) -> Option<String> {
	let captures = WEB_PULL.captures(url?)?;
	Some(format!("{}#{}", &captures[1], &captures[2]))
}

/// `Date.parse` of a check's start, 0 when absent or not a date.
fn started(context: &CheckContext) -> i64 {
	context
		.started_at
		.as_deref()
		.and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
		.map_or(0, |at| at.timestamp_millis())
}

/// A failing check: its name, and the link to it when it has one.
struct Failing {
	name: String,
	link: Option<String>,
}

/// The checks `gh pr checks` puts in its `fail` bucket (cli/cli pkg/cmd/pr/checks/aggregate.go): only the newest run of
/// each check name, workflow and event (a re-run replaces a failed run), and of each status context; failing when its
/// state (a status's state, a completed run's conclusion) is ERROR, FAILURE, TIMED_OUT or ACTION_REQUIRED.
fn failing_checks(contexts: &[&CheckContext]) -> Vec<Failing> {
	let mut newest_first: Vec<&CheckContext> = contexts.to_vec();
	newest_first.sort_by_key(|c| std::cmp::Reverse(started(c)));
	let mut keys = HashSet::new();
	let mut failed = Vec::new();
	for c in newest_first {
		let run = c.check_suite.as_ref().and_then(|suite| suite.workflow_run.as_ref());
		let key = match c.context.as_deref().filter(|context| !context.is_empty()) {
			Some(context) => format!("status:{context}"),
			None => format!(
				"run:{}/{}/{}",
				c.name.as_deref().unwrap_or(""),
				run.and_then(|r| r.workflow.as_ref()).and_then(|w| w.name.as_deref()).unwrap_or(""),
				run.and_then(|r| r.event.as_deref()).unwrap_or(""),
			),
		};
		if !keys.insert(key) {
			continue;
		}
		let state = match c.state.as_deref().filter(|state| !state.is_empty()) {
			Some(state) => state,
			None if c.status.as_deref() == Some("COMPLETED") => c.conclusion.as_deref().unwrap_or(""),
			None => c.status.as_deref().unwrap_or(""),
		};
		if !["ERROR", "FAILURE", "TIMED_OUT", "ACTION_REQUIRED"].contains(&state) {
			continue;
		}
		let link = [&c.details_url, &c.target_url].into_iter().flatten().find(|link| !link.is_empty()).cloned();
		let name = [&c.name, &c.context].into_iter().flatten().find(|name| !name.is_empty());
		failed.push(Failing { name: name.cloned().unwrap_or_else(|| "?".to_owned()), link });
	}
	failed
}

/// The kinds the GitHub poller owns; open events of these kinds it saw go away are resolved.
pub const GITHUB_KINDS: [&str; 2] = ["review", "pipeline"];

/// One GitHub round (0301): reviews requested from him (every page of `gh api notifications`, normal, 0298) and failing
/// checks on his open PRs (one `gh api graphql` search, PERF-8; urgent, 0303: a red pipeline on his branch). A pipeline
/// event is observed (SHR-8) only when its PR's checks were read, or the PR is no longer among his open ones.
pub async fn poll_github(run: &dyn RunCli) -> Result<PollRound> {
	let mut events = Vec::new();
	let pages: Vec<Vec<Notification>> =
		run_json(run, CliCommand::Gh, &["api", "notifications", "--paginate", "--slurp"], Vec::new(), &[]).await?;
	for n in pages.into_iter().flatten() {
		let Some(subject) = &n.subject else { continue };
		if n.reason.as_deref() != Some("review_requested") || subject.kind.as_deref() != Some("PullRequest") {
			continue;
		}
		let url = web_url(subject.url.as_deref());
		let repo = n.repository.as_ref().and_then(|r| r.full_name.clone()).unwrap_or_else(|| "GitHub".to_owned());
		let number = url.as_deref().and_then(|u| u.rsplit('/').next()).map(str::to_owned);
		events.push(NewEvent {
			source_id: format!("github:review:{repo}#{}", number.as_deref().unwrap_or(&n.id)),
			source: EventSource::Github,
			kind: "review".to_owned(),
			priority: EventPriority::Normal,
			text: format!(
				"Review requested — {} ({repo}{})",
				subject.title.as_deref().unwrap_or("a pull request"),
				number.as_ref().map_or_else(String::new, |n| format!(" #{n}")),
			),
			url,
			project_id: None,
			thread_id: None,
		});
	}
	let query = format!("query={}", prs_query());
	let search = format!("q={PRS_SEARCH}");
	let answer: Answer =
		run_json(run, CliCommand::Gh, &["api", "graphql", "-f", &query, "-f", &search], Answer::default(), &[]).await?;
	let data = answer.data;
	let mut open = HashSet::new();
	for pr in data.as_ref().and_then(|d| d.open.as_ref()).and_then(|s| s.nodes.as_ref()).into_iter().flatten().flatten()
	{
		if let Some(key) = pr_key(pr.url.as_deref()) {
			open.insert(key);
		}
	}
	let mut checked = HashSet::new();
	for pr in
		data.as_ref().and_then(|d| d.recent.as_ref()).and_then(|s| s.nodes.as_ref()).into_iter().flatten().flatten()
	{
		let Some(repo) = pr.repository.as_ref().and_then(|r| r.name_with_owner.as_deref()) else { continue };
		let Some(number) = pr.number else { continue };
		checked.insert(format!("{repo}#{number}"));
		let contexts: Vec<&CheckContext> = pr
			.commits
			.as_ref()
			.and_then(|c| c.nodes.as_ref())
			.and_then(|nodes| nodes.first())
			.and_then(Option::as_ref)
			.and_then(|node| node.commit.as_ref())
			.and_then(|commit| commit.status_check_rollup.as_ref())
			.and_then(|rollup| rollup.contexts.as_ref())
			.and_then(|contexts| contexts.nodes.as_ref())
			.into_iter()
			.flatten()
			.flatten()
			.collect();
		let failed = failing_checks(&contexts);
		let Some(first) = failed.first() else { continue };
		// A check's link comes from the CI service: kept only as a well-formed http(s) link (SE-6).
		let url = web_link(first.link.as_deref()).or_else(|| web_link(pr.url.as_deref()));
		events.push(NewEvent {
			source_id: format!("github:pipeline:{repo}#{number}"),
			source: EventSource::Github,
			kind: "pipeline".to_owned(),
			priority: EventPriority::Urgent,
			text: format!(
				"Checks failing — {repo} #{number} {}: {}",
				pr.title.as_deref().unwrap_or(""),
				failed.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
			),
			url,
			project_id: None,
			thread_id: None,
		});
	}
	let observed = move |source_id: &str| match source_id.strip_prefix("github:pipeline:") {
		Some(pipeline) if !pipeline.is_empty() => checked.contains(pipeline) || !open.contains(pipeline),
		_ => true,
	};
	Ok(PollRound { events, observed: Box::new(observed), failures: Vec::new() })
}

#[cfg(test)]
mod tests {
	use serde_json::{Value, json};

	use super::*;
	use crate::shared::events::queue::EventQueue;
	use crate::shared::pollers::cli::tests::fake_run;
	use crate::shared::schema::open_catalog;
	use crate::shared::testing::temp_jocasta;

	/// A PR in the GraphQL answer's `recent` list, with its last commit's checks.
	fn recent_pr(number: i64, title: &str, contexts: Vec<Value>) -> Value {
		json!({
			"number": number,
			"title": title,
			"url": format!("https://github.com/acme/web/pull/{number}"),
			"repository": { "nameWithOwner": "acme/web" },
			"commits": { "nodes": [{ "commit": { "statusCheckRollup": { "contexts": { "nodes": contexts } } } }] },
		})
	}

	/// The GraphQL answer: his open PRs by number, and the newest of them with their checks.
	fn prs_answer(open: &[i64], recent: Vec<Value>) -> Value {
		json!({
			"data": {
				"open": { "nodes": open.iter().map(|n| json!({ "url": format!("https://github.com/acme/web/pull/{n}") })).collect::<Vec<_>>() },
				"recent": { "nodes": recent },
			}
		})
	}

	fn failed_test() -> Value {
		json!({
			"name": "test",
			"status": "COMPLETED",
			"conclusion": "FAILURE",
			"detailsUrl": "https://github.com/acme/web/actions/runs/1",
			"startedAt": "2026-10-08T09:00:00Z",
			"checkSuite": { "workflowRun": { "event": "pull_request", "workflow": { "name": "ci" } } },
		})
	}

	/// `base` with `fields` merged over it.
	fn with(base: Value, fields: Value) -> Value {
		let (Value::Object(mut base), Value::Object(fields)) = (base, fields) else { unreachable!() };
		base.extend(fields);
		Value::Object(base)
	}

	fn event(source_id: &str, kind: &str, priority: EventPriority, text: &str, url: Option<&str>) -> NewEvent {
		NewEvent {
			source_id: source_id.to_owned(),
			source: EventSource::Github,
			kind: kind.to_owned(),
			priority,
			text: text.to_owned(),
			url: url.map(str::to_owned),
			project_id: None,
			thread_id: None,
		}
	}

	#[tokio::test]
	async fn github_reviews_requested_on_every_notifications_page_are_normal_failing_checks_on_his_open_prs_are_urgent()
	{
		let _j = temp_jocasta();
		let (run, calls) = fake_run(vec![
			// `--paginate --slurp`: one array per page.
			(
				"gh api notifications",
				json!([
					[{
						"id": "1",
						"reason": "review_requested",
						"subject": { "title": "Add export", "url": "https://api.github.com/repos/acme/api/pulls/42", "type": "PullRequest" },
						"repository": { "full_name": "acme/api" },
					}],
					[
						{ "id": "2", "reason": "mention", "subject": { "title": "x", "type": "Issue" }, "repository": { "full_name": "acme/api" } },
						{
							"id": "3",
							"reason": "review_requested",
							"subject": { "title": "Old one", "url": "https://api.github.com/repos/acme/api/pulls/9", "type": "PullRequest" },
							"repository": { "full_name": "acme/api" },
						},
					],
				]),
			),
			(
				"gh api graphql",
				prs_answer(
					&[7, 8, 10],
					vec![
						recent_pr(
							7,
							"Fix login",
							vec![
								failed_test(),
								json!({ "name": "lint", "status": "COMPLETED", "conclusion": "SUCCESS" }),
							],
						),
						// A commit status in error counts as failing, as in `gh pr checks`.
						recent_pr(
							8,
							"Docs",
							vec![
								json!({ "context": "ci/build", "state": "ERROR", "targetUrl": "https://ci.example/b/2" }),
							],
						),
						recent_pr(
							10,
							"Green",
							vec![
								// A re-run replaced the failed run; cancelled, pending and startup failures aren't `fail` either.
								with(failed_test(), json!({ "startedAt": "2026-10-08T08:00:00Z" })),
								with(failed_test(), json!({ "conclusion": "SUCCESS" })),
								json!({ "name": "deploy", "status": "COMPLETED", "conclusion": "CANCELLED" }),
								json!({ "name": "e2e", "status": "IN_PROGRESS", "conclusion": null }),
								json!({ "name": "boot", "status": "COMPLETED", "conclusion": "STARTUP_FAILURE" }),
							],
						),
					],
				),
			),
		]);
		let round = poll_github(&run).await.unwrap();
		assert_eq!(
			round.events,
			[
				event(
					"github:review:acme/api#42",
					"review",
					EventPriority::Normal,
					"Review requested — Add export (acme/api #42)",
					Some("https://github.com/acme/api/pull/42"),
				),
				event(
					"github:review:acme/api#9",
					"review",
					EventPriority::Normal,
					"Review requested — Old one (acme/api #9)",
					Some("https://github.com/acme/api/pull/9"),
				),
				event(
					"github:pipeline:acme/web#7",
					"pipeline",
					EventPriority::Urgent,
					"Checks failing — acme/web #7 Fix login: test",
					Some("https://github.com/acme/web/actions/runs/1"),
				),
				event(
					"github:pipeline:acme/web#8",
					"pipeline",
					EventPriority::Urgent,
					"Checks failing — acme/web #8 Docs: ci/build",
					Some("https://ci.example/b/2"),
				),
			]
		);
		assert!(round.failures.is_empty());
		// Two processes a round: every notifications page, then one GraphQL search for his open PRs and their checks.
		let calls = calls.lock().unwrap();
		let heads: Vec<String> =
			calls.iter().map(|c| c.iter().take(3).cloned().collect::<Vec<_>>().join(" ")).collect();
		assert_eq!(heads, ["gh api notifications", "gh api graphql"]);
		assert_eq!(calls[0], ["gh", "api", "notifications", "--paginate", "--slurp"]);
		assert!(calls[1].iter().any(|a| a.starts_with("q=") && a.contains("author:@me") && a.contains("is:open")));
		assert!(
			calls[1].iter().any(|a| a.starts_with("query=") && a.contains("first: 100)") && a.contains("first: 10)"))
		);
		// Reviews are all seen; a pipeline only when its PR's checks were read or it is no longer open (SHR-8).
		assert!((round.observed)("github:review:acme/api#1"));
		assert!((round.observed)("github:pipeline:acme/web#10"));
		assert!((round.observed)("github:pipeline:acme/web#99"));
	}

	#[tokio::test]
	async fn github_a_failing_pr_beyond_the_newest_10_stays_open_one_merged_or_gone_green_is_resolved() {
		let _j = temp_jocasta();
		let db = open_catalog().unwrap();
		let queue = EventQueue::new(&db);
		let pipeline = |n: i64| {
			event(
				&format!("github:pipeline:acme/web#{n}"),
				"pipeline",
				EventPriority::Urgent,
				&format!("Checks failing — acme/web #{n}"),
				None,
			)
		};
		for n in [3, 4, 5] {
			queue.add(&pipeline(n)).unwrap();
		}
		// #3 is still open but not among the newest 10 (not checked); #4 was checked and is green; #5 is no longer open.
		let (run, _) = fake_run(vec![
			("gh api notifications", json!([])),
			(
				"gh api graphql",
				prs_answer(&[1, 3, 4], vec![recent_pr(1, "New", vec![]), recent_pr(4, "Green now", vec![])]),
			),
		]);
		let round = poll_github(&run).await.unwrap();
		queue.sync(EventSource::Github, &GITHUB_KINDS, &round.events, Some(round.observed.as_ref())).unwrap();
		let open: Vec<String> = queue.open().unwrap().into_iter().map(|r| r.source_id).collect();
		assert_eq!(open, ["github:pipeline:acme/web#3"]);
	}

	#[tokio::test]
	async fn links_from_a_ci_check_are_kept_only_as_http_links() {
		let _j = temp_jocasta();
		let (run, _) = fake_run(vec![
			("gh api notifications", json!([])),
			(
				"gh api graphql",
				prs_answer(
					&[7],
					vec![recent_pr(
						7,
						"Fix",
						vec![with(failed_test(), json!({ "detailsUrl": "javascript:alert(1)" }))],
					)],
				),
			),
		]);
		let urls: Vec<Option<String>> = poll_github(&run).await.unwrap().events.into_iter().map(|e| e.url).collect();
		assert_eq!(urls, [Some("https://github.com/acme/web/pull/7".to_owned())]);
	}
}
