//! The Azure portal poller (0680): the operations that failed in the last 24 hours in his enabled subscriptions, read
//! through `az`. Private (0681): `text` is the fixed words `failed operation`; the operation, resource and group go in
//! `private_text`, the portal link in `private_url`.

use std::collections::HashSet;

use serde::Deserialize;

use crate::shared::errors::Result;
use crate::shared::events::queue::{EventSource, NewEvent, PollRound};
use crate::shared::pollers::cli::{CliCommand, RunCli, run_json};
use crate::shared::pollers::text::one_line;

/// The kinds this poller owns, for the queue's sync.
pub const AZURE_PORTAL_KINDS: [&str; 1] = ["azure-op"];

/// What `text` says of every failed operation (0681): the operation itself is private.
pub const AZURE_PORTAL_WORDS: &str = "failed operation";

/// At most this many events of one subscription are asked for.
const MAX_EVENTS: &str = "50";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Activity {
	operation_name: Option<Named>,
	resource_id: Option<String>,
	resource_group_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Named {
	value: Option<String>,
	localized_value: Option<String>,
}

/// The last path segment of an Azure resource id: the resource's name.
fn resource_name(id: &str) -> &str {
	let trimmed = id.trim_end_matches('/');
	trimmed.rsplit_once('/').map_or(trimmed, |(_, name)| name)
}

/// The resource group in a resource id (`/subscriptions/<s>/resourceGroups/<g>/…`), whatever its case.
fn group_of(id: &str) -> Option<&str> {
	let mut parts = id.split('/');
	parts.find(|part| part.eq_ignore_ascii_case("resourceGroups"))?;
	parts.next().filter(|group| !group.is_empty())
}

/// One event per distinct operation and resource of one subscription's failures, in the order `az` listed them.
fn events_of(subscription: &str, activities: Vec<Activity>) -> Vec<NewEvent> {
	let mut seen = HashSet::new();
	let mut events = Vec::new();
	for activity in activities {
		let (Some(id), Some(name)) = (activity.resource_id.filter(|id| !id.is_empty()), activity.operation_name) else {
			continue;
		};
		let Some(operation) = name.value.clone().filter(|value| !value.is_empty()) else { continue };
		if !seen.insert((operation.clone(), id.clone())) {
			continue;
		}
		let shown = name.localized_value.filter(|text| !text.is_empty()).unwrap_or(operation.clone());
		let group = activity.resource_group_name.as_deref().or_else(|| group_of(&id)).unwrap_or("?");
		let line = one_line(&format!("{shown} failed · {} ({group})", resource_name(&id)), 200);
		events.push(NewEvent::private(
			format!("azure-portal:{subscription}:{operation}|{id}"),
			EventSource::AzurePortal,
			"azure-op",
			AZURE_PORTAL_WORDS,
			(line, format!("https://portal.azure.com/#resource{id}")),
		));
	}
	events
}

/// One round: each enabled subscription's failed operations of the last 24 hours. A subscription whose listing fails
/// leaves its events out of the observed scope, so they stay as they were, and is returned as a failure; the rest goes
/// on (SHR-8).
pub async fn poll_azure_portal(run: &dyn RunCli) -> Result<PollRound> {
	let subscriptions: Vec<String> = run_json(
		run,
		CliCommand::Az,
		&["account", "list", "--query", "[?state=='Enabled'].id", "-o", "json"],
		Vec::new(),
		&[],
	)
	.await?;
	let (mut events, mut unseen, mut failures) = (Vec::new(), Vec::new(), Vec::new());
	for subscription in subscriptions.into_iter().filter(|id| !id.is_empty() && !id.contains(['"', '%'])) {
		let listed = run_json::<Vec<Activity>>(
			run,
			CliCommand::Az,
			&[
				"monitor",
				"activity-log",
				"list",
				"--subscription",
				&subscription,
				"--status",
				"Failed",
				"--offset",
				"24h",
				"--max-events",
				MAX_EVENTS,
				"-o",
				"json",
			],
			Vec::new(),
			&[],
		)
		.await;
		match listed {
			Ok(activities) => events.extend(events_of(&subscription, activities)),
			Err(error) => {
				unseen.push(format!("azure-portal:{subscription}:"));
				let message = error.to_string();
				if !failures.contains(&message) {
					failures.push(message);
				}
			}
		}
	}
	let observed = move |source_id: &str| !unseen.iter().any(|prefix| source_id.starts_with(prefix.as_str()));
	Ok(PollRound { events, observed: Box::new(observed), failures })
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::*;
	use crate::shared::pollers::cli::tests::fake_run;
	use crate::shared::private::Private;

	fn failure(operation: &str, resource: &str) -> serde_json::Value {
		json!({
			"operationName": { "value": operation, "localizedValue": "Create or Update Web App" },
			"resourceId": resource,
			"status": { "value": "Failed" },
		})
	}

	#[tokio::test]
	async fn a_failed_operation_is_one_private_event_per_operation_and_resource() {
		let site = "/subscriptions/sub-1/resourceGroups/rg-acme/providers/Microsoft.Web/sites/acme-api";
		let (run, calls) = fake_run(vec![
			("az account list", json!(["sub-1"])),
			(
				"az monitor activity-log list",
				json!([
					failure("Microsoft.Web/sites/write", site),
					failure("Microsoft.Web/sites/write", site),
					{ "operationName": { "value": "Microsoft.Web/sites/delete" }, "resourceId": site },
				]),
			),
		]);
		let round = poll_azure_portal(&run).await.unwrap();
		assert_eq!(round.events.len(), 2);
		for event in &round.events {
			assert_eq!(
				(event.source, event.kind.as_str(), event.text.as_str(), event.url.as_deref()),
				(EventSource::AzurePortal, "azure-op", "failed operation", None)
			);
		}
		assert_eq!(
			round.events[0].private_text,
			Some(Private::from("Create or Update Web App failed · acme-api (rg-acme)".to_owned()))
		);
		assert_eq!(
			round.events[0].private_url,
			Some(Private::from(format!("https://portal.azure.com/#resource{site}")))
		);
		let calls = calls.lock().unwrap();
		assert_eq!(calls[0], ["az", "account", "list", "--query", "[?state=='Enabled'].id", "-o", "json"]);
		assert_eq!(
			calls[1],
			[
				"az",
				"monitor",
				"activity-log",
				"list",
				"--subscription",
				"sub-1",
				"--status",
				"Failed",
				"--offset",
				"24h",
				"--max-events",
				"50",
				"-o",
				"json"
			]
		);
	}

	#[tokio::test]
	async fn a_subscription_that_fails_keeps_its_events_open_and_is_reported() {
		let (run, _) = fake_run(vec![("az account list", json!(["sub-1", "sub-2"]))]);
		let round = poll_azure_portal(&run).await.unwrap();
		assert!(round.events.is_empty());
		assert_eq!(round.failures.len(), 1);
		assert!(!(round.observed)("azure-portal:sub-1:op|/x"));
		assert!(!(round.observed)("azure-portal:sub-2:op|/x"));
		assert!((round.observed)("azure-portal:sub-3:op|/x"));
	}
}
