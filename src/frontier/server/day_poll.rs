//! The day's pollers, the machine sample, the subscription windows, the leftover worktrees and the weekly review.

use chrono::{Datelike, Duration as Days, Timelike};
use tokio::sync::watch;

use super::{
	Day, EventPriority, Fault, Finish, PollerName, WEEKLY_REVIEW_DAY, WEEKLY_REVIEW_HOUR, event, finished, local, lock,
};
use crate::frontier::protocol::{ServerMessage, ToastLevel, ToastPriority, ToastReason, quota_high};
use crate::frontier::server::quota::{omp_usage, quota_warning, read_quota};
use crate::shared::errors::{Error, Result};
use crate::shared::events::queue::EventSource;
use crate::shared::paths::data_dir;
use crate::shared::pollers::azure::{AZURE_KINDS, poll_azure};
use crate::shared::pollers::calendar::{poll_calendar, poll_calendar_with};
use crate::shared::pollers::github::{GITHUB_KINDS, poll_github};
use crate::shared::pollers::machine::{cpu_times, sample_machine};
use crate::shared::pollers::news::{NEWS_ENDPOINT, poll_news_at};
use crate::shared::pollers::settings::{AzureDevOpsSettings, NewsSettings, read_poller};
use crate::shared::pollers::weather::{
	FORECAST_ENDPOINT, GEOCODING_ENDPOINT, find_place_at, poll_weather_at, read_weather_place,
};
use crate::shared::process::aborted;
use crate::shared::projects::Project;
use crate::shared::settings::read_settings;

impl Day {
	/// One round of a poller; an overlapping round is skipped, and none starts once the day stopped. News and weather
	/// only feed the dashboard (0429): a failure is reported once and shown in its section, and the last good list stays.
	pub async fn poll(&self, name: PollerName) {
		let finish = {
			let mut state = self.st();
			if self.stopped() || state.rounds.contains_key(&name) {
				return;
			}
			let (finish, done) = watch::channel(false);
			state.rounds.insert(name, done);
			finish
		};
		let _round = Round { day: self, name, _finish: Finish(finish) };
		self.round(name).await;
	}

	async fn round(&self, name: PollerName) {
		let outcome = self.run_round(name).await;
		// A round the stop ended says nothing.
		if let Err(fault) = outcome
			&& !self.stopped()
		{
			let message = fault.message();
			{
				let mut state = self.st();
				if name == PollerName::News {
					state.news_error = Some(message.clone());
				}
				if name == PollerName::Weather {
					state.weather_error = Some(message.clone());
				}
			}
			let shown = if matches!(name, PollerName::News | PollerName::Weather) {
				message
			} else {
				format!("Poller {} failed: {message}", name.name())
			};
			self.report_once(name.name(), &shown, fault.said());
		}
		if !self.stopped() {
			self.push_dashboard();
		}
	}

	async fn run_round(&self, name: PollerName) -> std::result::Result<(), Fault> {
		match name {
			PollerName::Github | PollerName::AzureDevOps => {
				if self.poll_cli(name).await? {
					return Ok(());
				}
			}
			PollerName::Calendar => {
				let url = self
					.secret()
					.ok_or_else(|| Error::other("calendar: no secret iCal URL; `jocasta setup` saves it"))?;
				let now = self.now_ms();
				let meetings = match &self.inner.calendar_fetch {
					Some(fetch) => self.abortable(poll_calendar_with(&url, now, |address| fetch(address))).await?,
					None => self.abortable(poll_calendar(&url, now)).await?,
				};
				self.st().meetings = meetings;
				self.meetings_soon()?;
			}
			PollerName::News => {
				let settings = read_settings()?;
				let feeds = read_poller::<NewsSettings>(&settings).map_err(Fault::Settings)?.feeds;
				let endpoint = self.inner.news_endpoint.as_deref().unwrap_or(NEWS_ENDPOINT);
				let news = self.abortable(poll_news_at(&feeds, endpoint)).await?;
				let mut state = self.st();
				state.news = news;
				state.news_error = None;
			}
			PollerName::Weather => self.poll_weather().await?,
		}
		self.st().last_error.remove(name.name());
		if !self.stopped() {
			self.push_today()?;
		}
		Ok(())
	}

	/// A `gh` or `az` round ingested into the queue; whether it ended the round early because a part of it failed (one
	/// org or project): that is said once and the rest was taken (SHR-8).
	async fn poll_cli(&self, name: PollerName) -> std::result::Result<bool, Fault> {
		let run = &self.inner.run;
		let (source, kinds, round) = if name == PollerName::Github {
			(EventSource::Github, GITHUB_KINDS, poll_github(run).await?)
		} else {
			let settings = read_settings()?;
			let orgs = read_poller::<AzureDevOpsSettings>(&settings).map_err(Fault::Settings)?.orgs;
			(EventSource::Azure, AZURE_KINDS, poll_azure(run, &orgs).await?)
		};
		self.ingest(source, &kinds, &round)?;
		if round.failures.is_empty() {
			return Ok(false);
		}
		self.report_once(name.name(), &format!("Poller {} failed: {}", name.name(), round.failures.join("; ")), false);
		if !self.stopped() {
			self.push_today()?;
		}
		Ok(true)
	}

	/// Ends `work` when the day stops: its fetches abort with it.
	async fn abortable<T>(&self, work: impl Future<Output = Result<T>>) -> Result<T> {
		let mut signal = Some(self.inner.halt_signal.clone());
		tokio::select! {
			done = work => done,
			reason = aborted(&mut signal) => Err(Error::other(reason)),
		}
	}

	/// The weather at `dashboard.weather` (0429); none while it is unset.
	async fn poll_weather(&self) -> std::result::Result<(), Fault> {
		let settings = read_settings()?;
		let query = read_weather_place(&settings).map_err(Fault::Settings)?;
		let Some(query) = query else {
			let mut state = self.st();
			state.place = None;
			state.weather = None;
			state.weather_error = None;
			return Ok(());
		};
		let known = self.st().place.as_ref().filter(|(asked, _)| *asked == query).map(|(_, place)| place.clone());
		let place = match known {
			Some(place) => place,
			None => {
				let endpoint = self.inner.geocoding_endpoint.as_deref().unwrap_or(GEOCODING_ENDPOINT);
				let place = self.abortable(find_place_at(&query, endpoint)).await?;
				self.st().place = Some((query, place.clone()));
				place
			}
		};
		let endpoint = self.inner.forecast_endpoint.as_deref().unwrap_or(FORECAST_ENDPOINT);
		let weather = self.abortable(poll_weather_at(&place, endpoint)).await?;
		let mut state = self.st();
		state.weather = Some(weather);
		state.weather_error = None;
		Ok(())
	}

	/// CPU, RAM and disk, and whether the services answer (0429).
	pub async fn sample(&self) {
		{
			let mut state = self.st();
			if state.sampling {
				return;
			}
			state.sampling = true;
		}
		let now = cpu_times();
		let before = self.st().cpu_before.unwrap_or(now);
		self.guard("machine", || {
			self.st().machine = Some(sample_machine(before, now, &data_dir())?);
			Ok(())
		});
		self.st().cpu_before = Some(now);
		match self.inner.hooks.services().await {
			Ok(services) => {
				let mut state = self.st();
				state.services = services;
				state.last_error.remove("services");
			}
			Err(error) => self.report_once("services", &format!("services: {error}"), false),
		}
		self.push_dashboard();
		self.st().sampling = false;
	}

	/// The subscription windows from `omp usage --json` (0456), for the HUD and the rail; one toast when a window passes
	/// the warning, again only after it went back under. A failure (omp missing, no login) is logged once, never a
	/// toast: the windows then stay empty.
	pub async fn read_quota(&self) {
		{
			let mut state = self.st();
			if state.quota_reading {
				return;
			}
			state.quota_reading = true;
		}
		let usage = match &self.inner.usage {
			Some(usage) => usage().await,
			None => omp_usage().await,
		};
		let windows = usage.and_then(|text| read_quota(&text));
		let windows = {
			let mut state = self.st();
			state.quota_reading = false;
			match windows {
				Ok(windows) => {
					state.quota_windows.clone_from(&windows);
					state.last_error.remove("quota");
					windows
				}
				Err(error) => {
					let message = format!("omp usage: {error}");
					let known = state.last_error.get("quota").is_some_and(|last| *last == message);
					state.last_error.insert("quota".to_owned(), message.clone());
					drop(state);
					if !known {
						crate::shared::log::log(crate::shared::log::LogLevel::Warn, "day", &message, None);
					}
					return;
				}
			}
		};
		for q in windows {
			let warn = {
				let mut state = self.st();
				if !quota_high(&q) {
					state.quota_warned.retain(|window| *window != q.window);
					false
				} else if state.quota_warned.contains(&q.window) {
					false
				} else {
					state.quota_warned.push(q.window);
					true
				}
			};
			if warn {
				self.inner.hooks.send(ServerMessage::Toast {
					reason: ToastReason::Event,
					level: ToastLevel::Warning,
					priority: Some(ToastPriority::Urgent),
					text: quota_warning(&q),
					pane_id: None,
					url: None,
				});
			}
		}
	}

	/// The subscription windows (0456): 5h then 7d, those omp reported.
	pub fn quota(&self) -> Vec<crate::frontier::protocol::QuotaWindow> {
		self.st().quota_windows.clone()
	}

	/// Looks for thread worktrees left on disk (0456), for the dashboard; one look at a time.
	pub async fn refresh_leftovers(&self) {
		let claim = {
			let mut state = self.st();
			match state.leftovers_reading.clone() {
				Some(reading) => Err(reading),
				None => {
					let (finish, done) = watch::channel(false);
					state.leftovers_reading = Some(done);
					Ok(Finish(finish))
				}
			}
		};
		let finish = match claim {
			Ok(finish) => finish,
			Err(reading) => return finished(reading).await,
		};
		match self.inner.hooks.leftovers().await {
			Ok(leftovers) => {
				{
					let mut state = self.st();
					state.leftover_list = leftovers;
					state.last_error.remove("leftovers");
				}
				self.push_dashboard();
			}
			Err(error) => self.report_once("leftovers", &format!("Leftover worktrees: {error}"), false),
		}
		self.st().leftovers_reading = None;
		drop(finish);
	}

	/// The weekly big-picture review (0470): from Monday 10:00 on, outside quiet hours, once per project and week, each
	/// project's review runs; one with merges in the past 7 days ends in a `weekly-review` event. The event's row in the
	/// queue marks the week done across restarts; a result that came during quiet hours is delivered after them.
	pub(super) fn weekly_reviews(&self, quiet: bool) -> Result<()> {
		if quiet {
			return Ok(());
		}
		for row in self.queue(|queue| queue.open())? {
			if row.kind != "weekly-review" || row.raised_at.is_some() {
				continue;
			}
			self.deliver(&row, false)?;
			self.queue(|queue| queue.resolve(&row.source_id))?;
		}
		let now = local(self.now_ms());
		let back = (now.weekday().num_days_from_sunday() + 7 - WEEKLY_REVIEW_DAY) % 7;
		if back == 0 && now.hour() < WEEKLY_REVIEW_HOUR {
			return Ok(());
		}
		let week = (now.date_naive() - Days::days(i64::from(back))).format("%Y-%m-%d").to_string();
		for project in self.inner.hooks.projects() {
			let key = format!("{week}|{}", project.id);
			let source_id = format!("weekly-review:{}:{week}", project.id);
			if self.st().weekly_done.contains(&key) || self.queue(|queue| queue.by_source(&source_id))?.is_some() {
				continue;
			}
			self.st().weekly_done.insert(key);
			let day = self.clone();
			tokio::spawn(async move { day.weekly_review(&project, &source_id).await });
		}
		Ok(())
	}

	/// One project's weekly review; a failure is said once and the week isn't tried again until a restart.
	async fn weekly_review(&self, project: &Project, source_id: &str) {
		if let Err(error) = self.review_project(project, source_id).await {
			self.report_once(
				&format!("weekly review {}", project.id),
				&format!("Weekly review of {}: {error}", project.name),
				false,
			);
		}
	}

	async fn review_project(&self, project: &Project, source_id: &str) -> Result<()> {
		let line = self.inner.hooks.weekly_review(project).await?;
		let Some(line) = line.as_deref().map(str::trim).filter(|line| !line.is_empty()) else { return Ok(()) };
		if self.stopped() {
			return Ok(());
		}
		let mut review = event(
			source_id,
			EventSource::Thread,
			"weekly-review",
			EventPriority::Normal,
			&format!("Weekly review: {} — {line}", project.name),
		);
		review.project_id = Some(project.id.clone());
		self.raise(&review)?;
		// Delivered once, never reminded of; in quiet hours it stays open until they end.
		if self.queue(|queue| queue.by_source(source_id))?.is_some_and(|row| row.raised_at.is_some()) {
			self.queue(|queue| queue.resolve(source_id))?;
		}
		Ok(())
	}
}

/// A round under way: leaves `rounds` when it ends, however it ends.
struct Round<'a> {
	day: &'a Day,
	name: PollerName,
	_finish: Finish,
}

impl Drop for Round<'_> {
	fn drop(&mut self) {
		lock(&self.day.inner.state).rounds.remove(&self.name);
	}
}
