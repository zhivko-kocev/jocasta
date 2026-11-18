//! The Azure DevOps poller (0301).

use std::collections::HashMap;

use serde::Deserialize;

use crate::shared::errors::Result;
use crate::shared::events::queue::{EventPriority, EventSource, NewEvent, PollRound};
use crate::shared::pollers::cli::{CliCommand, RunCli, run_json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzPr {
	pull_request_id: i64,
	title: String,
	repository: Option<AzRepository>,
	reviewers: Option<Vec<AzReviewer>>,
}

#[derive(Deserialize)]
struct AzRepository {
	name: Option<String>,
	project: Option<AzName>,
}

#[derive(Deserialize)]
struct AzName {
	name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzReviewer {
	unique_name: Option<String>,
	vote: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzRun {
	id: i64,
	status: Option<String>,
	result: Option<String>,
	source_branch: Option<String>,
	definition: Option<AzDefinition>,
	requested_for: Option<AzRequester>,
}

#[derive(Deserialize)]
struct AzDefinition {
	id: Option<i64>,
	name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzRequester {
	unique_name: Option<String>,
}

#[derive(Deserialize, Default)]
struct AzProjects {
	value: Option<Vec<AzProject>>,
}

#[derive(Deserialize)]
struct AzProject {
	name: String,
}

/// The kinds the Azure DevOps poller owns.
pub const AZURE_KINDS: [&str; 2] = ["review", "pipeline"];

/// Runs read per project each round, newest finished first.
const RUNS_PER_PROJECT: &str = "30";
const TRUNKS: [&str; 2] = ["refs/heads/main", "refs/heads/master"];

/// `encodeURIComponent`: everything but letters, digits and ``-_.!~*'()`` is percent-encoded as UTF-8.
fn encode_uri_component(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for byte in text.bytes() {
		if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
			out.push(char::from(byte));
		} else {
			out.push_str(&format!("%{byte:02X}"));
		}
	}
	out
}

/// His sign-in name, the reviewer and requester `az` reports.
async fn me(run: &dyn RunCli) -> Result<String> {
	let args = ["account", "show", "--query", "user.name", "--output", "tsv"].map(str::to_owned).to_vec();
	Ok(run.run(CliCommand::Az, args, Vec::new(), None).await?.trim().to_lowercase())
}

/// One review event for each active PR where he is a reviewer and hasn't voted (normal, 0298).
fn unvoted_reviews(prs: &[AzPr], base: &str, user: &str) -> Vec<NewEvent> {
	let mut events = Vec::new();
	for pr in prs {
		let mine = pr
			.reviewers
			.iter()
			.flatten()
			.find(|r| r.unique_name.as_deref().is_some_and(|name| name.to_lowercase() == user));
		if mine.is_some_and(|mine| mine.vote.unwrap_or(0) != 0) {
			continue;
		}
		let project =
			pr.repository.as_ref().and_then(|r| r.project.as_ref()).and_then(|p| p.name.as_deref()).unwrap_or("");
		let repo = pr.repository.as_ref().and_then(|r| r.name.as_deref()).unwrap_or("");
		let id = pr.pull_request_id;
		events.push(NewEvent {
			source_id: format!("azure:review:{base}/{project}/{repo}#{id}"),
			source: EventSource::Azure,
			kind: "review".to_owned(),
			priority: EventPriority::Normal,
			text: format!("Review requested — {} ({project}/{repo} !{id})", pr.title),
			url: Some(format!(
				"{base}/{}/_git/{}/pullrequest/{id}",
				encode_uri_component(project),
				encode_uri_component(repo)
			)),
			project_id: None,
			thread_id: None,
		});
	}
	events
}

/// One urgent event for each pipeline and branch of a project whose newest finished run failed on main or master,
/// or on a branch he pushed (0303).
fn red_pipelines(runs: &[AzRun], base: &str, name: &str, user: &str) -> Vec<NewEvent> {
	let mut events = Vec::new();
	// The newest finished run of each pipeline and branch decides whether it is red; their order is first seen.
	let mut latest: Vec<(String, &AzRun)> = Vec::new();
	let mut index: HashMap<String, usize> = HashMap::new();
	for r in runs {
		let Some(branch) = r.source_branch.as_deref().filter(|branch| !branch.is_empty()) else { continue };
		if r.status.as_deref() != Some("completed") {
			continue;
		}
		let id = r.definition.as_ref().and_then(|d| d.id).map_or_else(|| "?".to_owned(), |id| id.to_string());
		let key = format!("{id}:{branch}");
		if !index.contains_key(&key) {
			index.insert(key.clone(), latest.len());
			latest.push((key, r));
		}
	}
	for (key, r) in latest {
		let branch = r.source_branch.as_deref().unwrap_or("");
		let his =
			r.requested_for.as_ref().and_then(|u| u.unique_name.as_deref()).is_some_and(|n| n.to_lowercase() == user);
		if r.result.as_deref() != Some("failed") || !(TRUNKS.contains(&branch) || his) {
			continue;
		}
		let short = branch.strip_prefix("refs/heads/").unwrap_or(branch);
		events.push(NewEvent {
			source_id: format!("azure:pipeline:{base}/{name}/{key}"),
			source: EventSource::Azure,
			kind: "pipeline".to_owned(),
			priority: EventPriority::Urgent,
			text: format!(
				"Pipeline red — {name}/{} on {short}",
				r.definition.as_ref().and_then(|d| d.name.as_deref()).unwrap_or("pipeline")
			),
			url: Some(format!("{base}/{}/_build/results?buildId={}", encode_uri_component(name), r.id)),
			project_id: None,
			thread_id: None,
		});
	}
	events
}

/// What a round collects: events, the source id prefixes whose listing failed, and the distinct failure messages.
#[derive(Default)]
struct Round {
	events: Vec<NewEvent>,
	unseen: Vec<String>,
	failures: Vec<String>,
}

impl Round {
	fn failed(&mut self, prefix: String, error: &crate::shared::errors::Error) {
		self.unseen.push(prefix);
		let message = error.to_string();
		if !self.failures.contains(&message) {
			self.failures.push(message);
		}
	}
}

/// The runs of one project: `Err` is a failed listing.
async fn project_runs(run: &dyn RunCli, base: &str, name: &str) -> Result<Vec<AzRun>> {
	run_json(
		run,
		CliCommand::Az,
		&[
			"pipelines",
			"runs",
			"list",
			"--organization",
			base,
			"--project",
			name,
			"--top",
			RUNS_PER_PROJECT,
			"--query-order",
			"FinishTimeDesc",
			"--output",
			"json",
		],
		Vec::new(),
		&[],
	)
	.await
}

/// One org's round: its reviews, then each project's red pipelines.
async fn poll_org(run: &dyn RunCli, org: &str, user: &str, round: &mut Round) {
	let base = org.trim_end_matches('/');
	let prs = run_json::<Vec<AzPr>>(
		run,
		CliCommand::Az,
		&["repos", "pr", "list", "--organization", base, "--reviewer", user, "--status", "active", "--output", "json"],
		Vec::new(),
		&[],
	)
	.await;
	match prs {
		Ok(prs) => round.events.extend(unvoted_reviews(&prs, base, user)),
		Err(error) => round.failed(format!("azure:review:{base}/"), &error),
	}
	let projects = run_json::<AzProjects>(
		run,
		CliCommand::Az,
		&["devops", "project", "list", "--organization", base, "--output", "json"],
		AzProjects::default(),
		&[],
	)
	.await;
	let projects = match projects {
		Ok(projects) => projects,
		Err(error) => {
			round.failed(format!("azure:pipeline:{base}/"), &error);
			return;
		}
	};
	for AzProject { name } in projects.value.unwrap_or_default() {
		if name.contains(['"', '%']) {
			continue;
		}
		match project_runs(run, base, &name).await {
			Ok(runs) => round.events.extend(red_pipelines(&runs, base, &name, user)),
			Err(error) => round.failed(format!("azure:pipeline:{base}/{name}/"), &error),
		}
	}
}

/// One Azure DevOps round for each org in config (0301): active PRs where he is a reviewer and hasn't voted (normal,
/// 0298), and the latest run of each pipeline and branch that failed on main or master, or on a branch he pushed
/// (urgent, 0303). A listing that fails (an org, or one project the account can't read) leaves that org's or project's
/// events out of the observed scope, so they stay as they were, and is returned as a failure; the rest of the round
/// goes on (SHR-8).
pub async fn poll_azure(run: &dyn RunCli, orgs: &[String]) -> Result<PollRound> {
	if orgs.is_empty() {
		return Ok(PollRound { events: Vec::new(), observed: Box::new(|_| true), failures: Vec::new() });
	}
	let user = me(run).await?;
	let mut round = Round::default();
	for org in orgs {
		poll_org(run, org, &user, &mut round).await;
	}
	let Round { events, unseen, failures } = round;
	let observed = move |source_id: &str| !unseen.iter().any(|prefix| source_id.starts_with(prefix.as_str()));
	Ok(PollRound { events, observed: Box::new(observed), failures })
}

#[cfg(test)]
mod tests {
	use serde_json::{Value, json};

	use super::*;
	use crate::shared::errors::Error;
	use crate::shared::events::queue::EventQueue;
	use crate::shared::pollers::cli::tests::fake_run;
	use crate::shared::pollers::cli::{RunCli, RunFuture};
	use crate::shared::process::AbortSignal;
	use crate::shared::schema::open_catalog;
	use crate::shared::testing::temp_jocasta;

	fn event(source_id: &str, kind: &str, priority: EventPriority, text: &str, url: Option<&str>) -> NewEvent {
		NewEvent {
			source_id: source_id.to_owned(),
			source: EventSource::Azure,
			kind: kind.to_owned(),
			priority,
			text: text.to_owned(),
			url: url.map(str::to_owned),
			project_id: None,
			thread_id: None,
		}
	}

	#[tokio::test]
	async fn azure_devops_unvoted_reviews_per_org_the_newest_run_of_a_pipeline_red_on_main_or_his_branch() {
		let _j = temp_jocasta();
		let org = "https://dev.azure.com/example-org";
		let run_of =
			|id: i64, status: &str, result: Option<&str>, branch: &str, definition: Value, requested: Option<&str>| {
				let mut run = json!({ "id": id, "status": status, "sourceBranch": branch, "definition": definition });
				if let Some(result) = result {
					run["result"] = json!(result);
				}
				if let Some(requested) = requested {
					run["requestedFor"] = json!({ "uniqueName": requested });
				}
				run
			};
		let (run, calls) = fake_run(vec![
			("az account show", json!("him@example.test\n")),
			(
				"az repos pr list",
				json!([
					{
						"pullRequestId": 5,
						"title": "Export v2",
						"repository": { "name": "api", "project": { "name": "Client A" } },
						"reviewers": [{ "uniqueName": "Him@example.test", "vote": 0 }],
					},
					{
						"pullRequestId": 6,
						"title": "Approved already",
						"repository": { "name": "api", "project": { "name": "Client A" } },
						"reviewers": [{ "uniqueName": "him@example.test", "vote": 10 }],
					},
				]),
			),
			("az devops project list", json!({ "value": [{ "name": "Client A" }] })),
			(
				"az pipelines runs list",
				json!([
					// Newest first: main is red now; an older green main run doesn't hide it.
					run_of(30, "completed", Some("failed"), "refs/heads/main", json!({ "id": 1, "name": "web" }), None),
					run_of(
						29,
						"completed",
						Some("succeeded"),
						"refs/heads/main",
						json!({ "id": 1, "name": "web" }),
						None
					),
					// His branch went green again: not red.
					run_of(
						28,
						"completed",
						Some("succeeded"),
						"refs/heads/him/x",
						json!({ "id": 1 }),
						Some("him@example.test")
					),
					run_of(
						27,
						"completed",
						Some("failed"),
						"refs/heads/him/x",
						json!({ "id": 1 }),
						Some("him@example.test")
					),
					// Someone else's branch: not his business.
					run_of(
						26,
						"completed",
						Some("failed"),
						"refs/heads/other/y",
						json!({ "id": 1 }),
						Some("else@example.test")
					),
					run_of(25, "inProgress", None, "refs/heads/main", json!({ "id": 2 }), None),
				]),
			),
		]);
		assert!(poll_azure(&run, &[]).await.unwrap().events.is_empty());
		assert!(calls.lock().unwrap().is_empty());
		let round = poll_azure(&run, &[format!("{org}/")]).await.unwrap();
		assert!(round.failures.is_empty());
		assert_eq!(
			round.events,
			[
				event(
					&format!("azure:review:{org}/Client A/api#5"),
					"review",
					EventPriority::Normal,
					"Review requested — Export v2 (Client A/api !5)",
					Some(&format!("{org}/Client%20A/_git/api/pullrequest/5")),
				),
				event(
					&format!("azure:pipeline:{org}/Client A/1:refs/heads/main"),
					"pipeline",
					EventPriority::Urgent,
					"Pipeline red — Client A/web on main",
					Some(&format!("{org}/Client%20A/_build/results?buildId=30")),
				),
			]
		);
		let calls = calls.lock().unwrap();
		let repos = calls.iter().find(|c| c[1] == "repos").unwrap();
		assert_eq!(
			*repos,
			[
				"az",
				"repos",
				"pr",
				"list",
				"--organization",
				org,
				"--reviewer",
				"him@example.test",
				"--status",
				"active",
				"--output",
				"json"
			]
		);
	}

	#[tokio::test]
	async fn azure_devops_an_org_or_project_whose_listing_fails_keeps_its_events_open_the_rest_is_read_the_failure_reported()
	 {
		let _j = temp_jocasta();
		let good = "https://dev.azure.com/example-org";
		let bad = "https://dev.azure.com/acme";
		let run = |_: CliCommand, args: Vec<String>, _: Vec<i32>, _: Option<AbortSignal>| -> RunFuture {
			let value_of = |flag: &str| args.iter().position(|a| a == flag).map(|at| args[at + 1].clone());
			let (org, project) = (value_of("--organization"), value_of("--project"));
			let answer = if args[0] == "account" {
				Ok("him@example.test\n".to_owned())
			} else if org.as_deref() == Some(bad) && args[0] == "repos" {
				Err(Error::other("az repos pr failed: TF400813: not authorized"))
			} else if args[0] == "repos" {
				Ok("[]".to_owned())
			} else if args[0] == "devops" {
				if org.as_deref() == Some(bad) {
					Err(Error::other("az devops project failed: TF400813: not authorized"))
				} else {
					Ok(json!({ "value": [{ "name": "Client A" }, { "name": "Client B" }] }).to_string())
				}
			} else if project.as_deref() == Some("Client A") {
				Err(Error::other("az pipelines runs failed: TF401444: no access"))
			} else {
				Ok(json!([{ "id": 3, "status": "completed", "result": "failed", "sourceBranch": "refs/heads/main", "definition": { "id": 1 } }])
					.to_string())
			};
			Box::pin(async move { answer })
		};
		let _ = &run as &dyn RunCli;
		let db = open_catalog().unwrap();
		let queue = EventQueue::new(&db);
		let event = |source_id: String, kind: &str| event(&source_id, kind, EventPriority::Normal, &source_id, None);
		let kept = [
			event(format!("azure:review:{bad}/Web/site#1"), "review"),
			event(format!("azure:pipeline:{bad}/Web/1:refs/heads/main"), "pipeline"),
			event(format!("azure:pipeline:{good}/Client A/1:refs/heads/main"), "pipeline"),
		];
		let gone = [
			event(format!("azure:review:{good}/Client A/api#2"), "review"),
			event(format!("azure:pipeline:{good}/Client B/2:refs/heads/main"), "pipeline"),
		];
		for e in kept.iter().chain(&gone) {
			queue.add(e).unwrap();
		}
		let round = poll_azure(&run, &[good.to_owned(), bad.to_owned()]).await.unwrap();
		let ids: Vec<&str> = round.events.iter().map(|e| e.source_id.as_str()).collect();
		assert_eq!(ids, [format!("azure:pipeline:{good}/Client B/1:refs/heads/main")]);
		assert_eq!(
			round.failures,
			[
				"az pipelines runs failed: TF401444: no access",
				"az repos pr failed: TF400813: not authorized",
				"az devops project failed: TF400813: not authorized",
			]
		);
		queue.sync(EventSource::Azure, &AZURE_KINDS, &round.events, Some(round.observed.as_ref())).unwrap();
		let open: Vec<String> = queue.open().unwrap().into_iter().map(|r| r.source_id).collect();
		let mut expected: Vec<String> = kept.iter().map(|e| e.source_id.clone()).collect();
		expected.push(format!("azure:pipeline:{good}/Client B/1:refs/heads/main"));
		assert_eq!(open, expected);
	}
}
