//! Tests of `day.rs` ported from `test/frontier/server/day-settings.test.ts`: a bad value in jocasta.yaml turns off only
//! its own feature (0584): one bad poller key stops that poller alone, the other pollers and the weather run, and the
//! problem is said once (the service's notification at start).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::tests::{Stand, at, project, serve, write_settings};
use super::*;
use crate::shared::settings::settings_problems;
use crate::shared::testing::temp_jocasta;

#[tokio::test]
async fn a_bad_pollers_github_value_turns_off_github_only_azure_devops_the_calendar_news_and_weather_run_said_once() {
	let _j = temp_jocasta();
	write_settings(
		&[
			"dashboard:",
			"  weather: Example City",
			"pollers:",
			"  github:",
			"    everyMinutes: 0",
			"  azureDevOps:",
			"    orgs: [https://dev.azure.com/example-org]",
			"  news:",
			"    feeds: [{source: hn}]",
			"",
		]
		.join("\n"),
	);
	// The service says each settings problem once at start, before the day starts (C7).
	let said: Vec<String> = settings_problems().into_iter().map(|problem| format!("settings: {problem}")).collect();

	let commands = Arc::new(Mutex::new(Vec::<String>::new()));
	let called = Arc::clone(&commands);
	let run =
		move |command: CliCommand, _args: Vec<String>, _ok: Vec<i32>, _signal: Option<AbortSignal>| -> RunFuture {
			called.lock().unwrap().push(command.as_str().to_owned());
			Box::pin(async move { Err(Error::other(format!("{} isn't here", command.as_str()))) })
		};
	// News, the weather's geocoding and the calendar's feed all answer 503.
	let (base, asked) = serve(|_| (503, "not here".to_owned())).await;
	let s = Stand::with(
		at(2026, 10, 8, 10, 0),
		vec![project("p1", "acme-api")],
		DayOptions {
			run: Some(Arc::new(run)),
			news_endpoint: Some(format!("{base}/brutalist")),
			geocoding_endpoint: Some(format!("{base}/geocoding")),
			forecast_endpoint: Some(format!("{base}/forecast")),
			calendar_secret: Some(Arc::new({
				let url = format!("{base}/calendar.example/his.ics");
				move || Some(url.clone())
			})),
			usage: Some(Arc::new(|| Box::pin(async { Err(Error::other("no omp here")) }))),
			..DayOptions::default()
		},
	);
	s.shared.notes.lock().unwrap().extend(said);
	s.day.start();
	let reached = |what: &str| asked.lock().unwrap().iter().any(|target| target.contains(what));
	for _ in 0..500 {
		if commands.lock().unwrap().iter().any(|c| c == "az")
			&& reached("brutalist")
			&& reached("calendar.example")
			&& reached("geocoding")
		{
			break;
		}
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	s.day.stop().await;
	let commands = commands.lock().unwrap().clone();
	assert!(!commands.contains(&"gh".to_owned()), "{commands:?}");
	assert!(commands.contains(&"az".to_owned()), "{commands:?}");
	assert!(reached("brutalist") && reached("calendar.example") && reached("geocoding"));
	let about_github: Vec<String> = s.notes().into_iter().filter(|note| note.contains("pollers.github")).collect();
	assert_eq!(
		about_github,
		[
			"settings: jocasta.yaml:5: pollers.github.everyMinutes must be a number of minutes, 1 or more; the GitHub poller is off"
		]
	);
}
