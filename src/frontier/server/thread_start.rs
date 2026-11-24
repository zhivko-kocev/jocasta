//! Starting a thread (0388, 0399, 0400, 0412, 0414, 0445, 0454, 0455, 0457, 0459, 0540, 0544): from the boss's
//! `thread_start`, his New thread form and `jocasta thread new`, one project's starts one after another.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use serde_json::Value;

use crate::frontier::protocol::ThreadState;
use crate::frontier::server::allow::Confirmation;
use crate::frontier::server::boss::{PANE_KEY_ENV, new_pane_key};
use crate::frontier::server::bridge::Deliver;
use crate::frontier::server::context::{Link, Shared};
use crate::frontier::server::day::ScheduleStart;
use crate::frontier::server::herdr::{AgentState, Env, PaneInfo, Prepare, WorktreeOptions};
use crate::frontier::server::overlay::ensure_overlay;
use crate::frontier::server::pane_launch::{THREAD_FLAGS, after_overlay};
use crate::frontier::server::task_file::{NewTask, TaskRecord, new_task, task_dir};
use crate::frontier::server::thread::{Thread, ThreadPlan, ThreadRef, subject};
use crate::frontier::server::thread_lifecycle::StateSource;
use crate::frontier::server::wording::{COMMIT_LINE, Door, LESSONS_LINE, branch_slug, brief_title, start_prefix};
use crate::frontier::server::worktrees::{
	GIT_MS, ThreadPlace, copy_worktree_include, git_out, remove_checkout, thread_place,
};
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::remove_tree_async;
use crate::shared::git::{GitOptions, git_async};
use crate::shared::guard::{GuardRule, Judged, Verdict, parse_guard, verdict};
use crate::shared::install::shipped_config_dir;
use crate::shared::pathkey::same_dir;
use crate::shared::poll::wait_for;
use crate::shared::projects::Project;
use crate::shared::schedules::Schedule;

/// How long `thread_start` waits for omp to come up in the new pane before sending the brief.
const AGENT_WAIT_MS: i64 = 20_000;
/// How often a start looks at the new pane meanwhile.
const POLL_MS: i64 = 250;
/// How long taking back a failed start waits for its checkout's processes to let go (SVC-7).
const UNDO_REMOVE_MS: i64 = 5_000;

/// `String.prototype.trim`: white space and the byte order mark.
fn js_trim(text: &str) -> &str {
	text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// `done_when` as given (0540): absent, or a list of command strings; anything else is a refusal named by the door.
pub fn done_when_of(value: Option<&Value>, door: Door) -> Result<Vec<String>> {
	let Some(value) = value else { return Ok(Vec::new()) };
	value
		.as_array()
		.and_then(|items| items.iter().map(|item| item.as_str().map(str::to_owned)).collect::<Option<Vec<_>>>())
		.ok_or_else(|| Error::user(format!("{}: done_when is a list of commands.", start_prefix(door))))
}

/// `heavy` as given (0590): absent, true or false; anything else is a refusal named by the door.
pub fn heavy_of(value: Option<&Value>, door: Door) -> Result<bool> {
	let Some(value) = value else { return Ok(false) };
	value.as_bool().ok_or_else(|| Error::user(format!("{}: heavy is true or false.", start_prefix(door))))
}

/// The shipped guard's rules (0450, 0580), read once: every `done_when` is checked against them before it's stored.
fn shipped_guard() -> Result<&'static [GuardRule]> {
	static RULES: OnceLock<Vec<GuardRule>> = OnceLock::new();
	if let Some(rules) = RULES.get() {
		return Ok(rules);
	}
	let file = shipped_config_dir(None).join("guard.yml");
	let rules = parse_guard(&std::fs::read_to_string(&file)?, &file.display().to_string())?;
	Ok(RULES.get_or_init(|| rules))
}

/// A thread start's outcome: its text, and whether it went all the way.
#[derive(Debug, Clone, Default)]
pub struct Started {
	pub text: String,
	pub done: bool,
}

/// What a start is given beyond its project, title and brief: the door it came through (the boss's by default), the
/// commands that must pass (0540) and whether the thread is heavy (0590).
#[derive(Debug, Clone)]
pub struct StartOptions {
	pub door: Door,
	pub done_when: Vec<String>,
	pub heavy: bool,
}

impl StartOptions {
	/// A start through `door` with no `done_when`, not heavy.
	pub fn new(door: Door) -> Self {
		Self { door, done_when: Vec::new(), heavy: false }
	}
}

/// One start as vetted: the title and brief as given, the checks trimmed.
struct Request<'a> {
	title: &'a str,
	brief: &'a str,
	checks: Vec<String>,
	door: Door,
	heavy: bool,
}

/// What a start's worktree step needs.
struct Launch<'a> {
	project: &'a Project,
	title: &'a str,
	task: &'a TaskRecord,
	command: &'a [String],
	fresh: &'a [String],
	main_env: &'a Env,
	key: &'a str,
	trunk: Option<&'a str>,
}

/// What herdr made for a start.
struct Created {
	workspace_id: String,
	pane_id: String,
	path: PathBuf,
	head: Option<String>,
}

#[derive(Default)]
struct State {
	/// Each project's `thread_start` under way, so parallel starts make one project workspace (CO-5).
	starts: HashMap<String, Arc<tokio::sync::Mutex<()>>>,
	/// The project of the last pane he focused in herdr that belongs to a project (0484, 0492): a focused pane in no
	/// project (the boss's, herdr's home shell) leaves it as it was; `None` before any project pane in this run.
	focused: Option<Project>,
	/// The project the last thread start went into (0581, UX-15): the New thread form's third choice.
	last_started_in: Option<Project>,
}

/// `thread-start.ts`'s `ThreadStarts`.
pub struct ThreadStarts {
	/// The service's context, reached only through here (SVC-15).
	pub link: Link,
	st: Mutex<State>,
}

/// `{...env, [name]: value}`: a name set again keeps its place and takes the new value.
fn set_env(env: &mut Env, name: &str, value: &str) {
	match env.iter_mut().find(|(have, _)| have == name) {
		Some(slot) => slot.1 = value.to_owned(),
		None => env.push((name.to_owned(), value.to_owned())),
	}
}

impl ThreadStarts {
	pub fn new(link: Link) -> Self {
		Self { link, st: Mutex::new(State::default()) }
	}

	fn st(&self) -> MutexGuard<'_, State> {
		self.st.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// The project of the last pane he focused in herdr that belongs to a project (0484, 0492); `None` before one.
	pub fn focused_project(&self) -> Option<Project> {
		self.st().focused.clone()
	}

	/// The project the last thread start went into (0581, UX-15): the New thread form's third choice.
	pub fn last_started_in(&self) -> Option<Project> {
		self.st().last_started_in.clone()
	}

	/// Remembers the project of the pane herdr has focused when it belongs to one (0484, 0492); the boss's pane and any
	/// other pane in no project leave it as it was. `thread_start` compares it.
	pub fn note_focus(&self, panes: &[PaneInfo]) -> Result<()> {
		let c = self.link.ctx();
		let Some(pane) = panes.iter().find(|p| p.focused) else { return Ok(()) };
		if c.boss().is_some_and(|boss| boss.with(|boss| boss.pane_id == pane.pane_id)) {
			return Ok(());
		}
		let mut project = c.threads.get(&pane.pane_id).map(|thread| thread.with(|t| t.project.clone()));
		if project.is_none()
			&& let Some(cwd) = &pane.cwd
		{
			project = c.places.project_at(cwd)?;
		}
		if let Some(project) = project {
			self.st().focused = Some(project);
		}
		Ok(())
	}

	/// The project of a pane Jocasta started, from its recorded worktree or folder (0484); `None` for any other pane.
	pub fn pane_project(&self, pane_id: &str) -> Result<Option<Project>> {
		let c = self.link.ctx();
		if let Some(thread) = c.threads.get(pane_id) {
			return Ok(Some(thread.with(|t| t.project.clone())));
		}
		if c.boss().is_some_and(|boss| boss.with(|boss| boss.pane_id == pane_id)) {
			return Ok(None);
		}
		match c.launched.cwd_of(pane_id) {
			Some(cwd) => c.places.project_at(Path::new(&cwd)),
			None => Ok(None),
		}
	}

	/// The project's workspace (label = its name, 0392), or `None` when herdr has none.
	async fn project_workspace(&self, project: &Project) -> Result<Option<String>> {
		let c = self.link.ctx();
		let workspaces = c.herdr.workspaces().await?;
		c.status.set_workspaces(workspaces.clone());
		Ok(workspaces.into_iter().find(|w| w.label == project.name).map(|w| w.id))
	}

	/// [`ThreadStarts::start_with`] with no `done_when`, not heavy: the keys' and the form's start.
	pub async fn start(&self, project: &str, title: &str, brief: &str, door: Door) -> Result<Started> {
		self.start_with(project, title, brief, StartOptions::new(door)).await
	}

	/// A scheduled thread's start (0673): through the key's door, so it opens in plan mode and waits for his approval.
	/// Skipped, with the reason as his notification, when a thread with its title is already open in the project, or
	/// when its `done_when` is one the guard asks about or denies: an unattended Key door would run it unasked. Any
	/// other failure is the start's own error.
	pub async fn start_scheduled(&self, project_name: &str, schedule: &Schedule) -> Result<ScheduleStart> {
		let c = self.link.ctx();
		let title = brief_title(&schedule.brief);
		c.places.sync().await;
		let project = c.places.find(project_name)?;
		if c.threads.of_project(&project.id).iter().any(|thread| thread.with(|t| t.title == title)) {
			return Ok(ScheduleStart::Skipped(format!(
				"a thread named \"{title}\" is already open in {}",
				project.name
			)));
		}
		let checks: Vec<String> = schedule.done_when.iter().map(|command| js_trim(command).to_owned()).collect();
		match Self::vet_done_when(start_prefix(Door::Key), &checks, Door::Key) {
			Ok(false) => {}
			Ok(true) => {
				return Ok(ScheduleStart::Skipped(format!(
					"its done_when needs your Allow under Jocasta's guard, which an unattended start can't ask: {}",
					checks.join("; ")
				)));
			}
			Err(denied) => return Ok(ScheduleStart::Skipped(denied.to_string())),
		}
		let options = StartOptions { door: Door::Key, done_when: checks, heavy: schedule.heavy };
		self.start_with(&project.name, &title, &schedule.brief, options).await?;
		Ok(ScheduleStart::Started)
	}

	/// The project's workspace with its project pane, the project's omp overlay, a herdr worktree workspace on branch
	/// `feature/<title slug>` from the project's trunk in `<project>.wt/<title slug>` (`-2`, `-3`… when the branch or
	/// folder is taken; `thread-<YYYYMMDD>-<n>` for a title with no slug, 0581), its `.worktreeinclude` files copied in,
	/// running a fresh omp session with that overlay (so it opens in plan mode), the thread's env and pane key (0578) and
	/// its task file's prompt appended to its system prompt, then the brief, told to read the project's lessons, once omp
	/// is up. Each `done_when` goes through the shipped guard first (0580): deny refuses, prompt needs his Allow at the
	/// boss's door and is refused at the CLI's, which asks nothing (0544). A `heavy` thread (0590) is recorded so on its
	/// task and its omp loads `config/heavy.yml` after the overlay, at this launch and every later one. A start that fails
	/// once herdr made its worktree takes back what it made (SVC-7). Its texts name the door (0581); `done` is false for a
	/// start that is only half done (the brief not sent).
	pub async fn start_with(
		&self,
		project_name: &str,
		title: &str,
		brief: &str,
		options: StartOptions,
	) -> Result<Started> {
		let c = self.link.ctx();
		let prefix = start_prefix(options.door);
		c.places.sync().await;
		let project = c.places.find(project_name)?;
		if !project.git {
			return Err(Error::user(format!(
				"{prefix}: {} isn't a git repository; threads need a worktree.",
				project.name
			)));
		}
		let checks: Vec<String> = options.done_when.iter().map(|command| js_trim(command).to_owned()).collect();
		if checks.iter().any(|command| command.is_empty() || command.contains(['\r', '\n'])) {
			return Err(Error::user(format!("{prefix}: each done_when is one command on one line.")));
		}
		let prompted = Self::vet_done_when(prefix, &checks, options.door)?;
		let request = Request { title, brief, checks, door: options.door, heavy: options.heavy };
		// A boss's start in a project other than the one of the last project pane he focused, or with a done_when the
		// guard asks about, waits for his Allow (0484, 0492, 0580); one he started with his own key and form, or with
		// `jocasta thread new`, needs none: the thread waits at its plan for him (0544).
		self.confirm(&project, &request, prompted).await?;
		let overlay = ensure_overlay(&project.id).await?;
		let mut command = c.panes.pane_omp().to_vec();
		command.extend(["--config".to_owned(), overlay.display().to_string()]);
		let trunk = c.places.trunk(&project).await?;
		// Its task file (0540), whose brief and done-when its omp appends to the system prompt at every launch.
		let base = match &trunk {
			Some(trunk) => git_out(&project.main_path, &["rev-parse", "--short", trunk]).await?,
			None => None,
		};
		let mut task = new_task(
			NewTask {
				project_id: project.id.clone(),
				project: project.name.clone(),
				title: js_trim(title).to_owned(),
				brief: js_trim(brief).to_owned(),
				done_when: request.checks.clone(),
				heavy: request.heavy.then_some(true),
				base,
				..NewTask::default()
			},
			None,
		);
		// A new thread never resumes a session saved for a reused folder (0455); a reopened one still does (0399). It
		// warns at $50 (0541); a heavy one loads `heavy.yml` after the overlay (0590), as `PaneLauncher::respawn` does.
		let mut fresh: Vec<String> = command.iter().filter(|arg| *arg != "--continue").cloned().collect();
		fresh.extend(after_overlay(Some(&task)));
		fresh.extend(THREAD_FLAGS.map(str::to_owned));
		// The project pane and the thread pane load the same overlays; the worktree is trunk's checkout of the project,
		// so its own `.omp/config.yml` is the main checkout's (0561). Asked before anything is made.
		let main_env = c.panes.with_mise(&project.id, &project.main_path, c.panes.pane_env()).await;
		let key = new_pane_key();
		let title_trimmed = js_trim(title);
		let launch = Launch {
			project: &project,
			title: title_trimmed,
			task: &task,
			command: &command,
			fresh: &fresh,
			main_env: &main_env,
			key: &key,
			trunk: trunk.as_deref(),
		};
		let mut place = None;
		let created = match self.one_after_another(&launch, &mut place).await {
			Ok(created) => created,
			Err(error) => {
				// No thread: its launch prompt goes with it, and what herdr made for it so far.
				if let Err(removed) = remove_tree_async(&task_dir(&task.project_id, &task.id)).await {
					c.warn(removed);
				}
				if let Some(place) = place {
					self.undo(&project, &place).await;
				}
				return Err(error);
			}
		};
		let branch = place.map(|place| place.branch).unwrap_or_default();
		task.path = created.path.display().to_string();
		task.branch.clone_from(&branch);
		let thread = Self::register(&project, &task, &created, &branch);
		self.finished_start(&thread, &request, &created, &key).await
	}

	/// His own Allow-less doors check each `done_when` against the shipped guard (0580): Jocasta runs it itself, outside
	/// omp's guard. omp matches a command with its quotes removed, Jocasta's copy the raw text, so both forms are matched
	/// and the stricter verdict counts: `git push --for"ce"` is still `--force`. Returns whether any command needs his
	/// Allow; one at the CLI's door, which asks nothing (0544), is refused.
	pub(super) fn vet_done_when(prefix: &str, checks: &[String], door: Door) -> Result<bool> {
		if checks.is_empty() {
			return Ok(false);
		}
		let rules = shipped_guard()?;
		let mut prompted: Vec<&String> = Vec::new();
		for command in checks {
			let forms: [Judged; 2] = [verdict(command, rules), verdict(&command.replace(['"', '\''], ""), rules)];
			let said = forms
				.iter()
				.find(|form| form.verdict == Verdict::Deny)
				.or_else(|| forms.iter().find(|form| form.verdict == Verdict::Prompt))
				.unwrap_or(&forms[0]);
			if said.verdict == Verdict::Deny {
				return Err(Error::user(format!(
					"{prefix}: done_when `{command}` is denied by Jocasta's guard (`{}`); no thread was started.",
					said.rule.as_deref().unwrap_or_default()
				)));
			}
			if said.verdict == Verdict::Prompt {
				prompted.push(command);
			}
		}
		if let (Some(first), Door::Cli) = (prompted.first(), door) {
			return Err(Error::user(format!(
				"{prefix}: done_when `{first}` needs your Allow under Jocasta's guard, and jocasta thread new asks nothing; have the boss start it. No thread was started."
			)));
		}
		Ok(!prompted.is_empty())
	}

	/// The boss's start waits for his Allow when it is in a project other than the last project pane he focused, or
	/// has a `done_when` the guard asks about (0484, 0492, 0580).
	async fn confirm(&self, project: &Project, request: &Request<'_>, prompted: bool) -> Result<()> {
		let c = self.link.ctx();
		self.note_focus(&c.herdr.panes().await?)?;
		let focused = self.focused_project();
		let elsewhere = focused.as_ref().map(|focused| &focused.id) != Some(&project.id);
		if request.door != Door::Boss || !(elsewhere || prompted) {
			return Ok(());
		}
		let mut detail = Vec::new();
		if elsewhere {
			detail.push(match &focused {
				Some(focused) => {
					format!("The last project pane you focused is in {}, not {}.", focused.name, project.name)
				}
				None => "You haven't focused a project's pane yet in this run.".to_owned(),
			});
		}
		detail.extend(request.checks.iter().map(|command| format!("It runs when the work is reviewed:  `{command}`")));
		detail.push(String::new());
		detail.push("The boss's brief:".to_owned());
		detail.push(js_trim(request.brief).to_owned());
		let ask = Confirmation {
			message: format!("Start the thread \"{}\" in {}?", js_trim(request.title), project.name),
			detail: detail.join("\n"),
		};
		c.allow.ask(&ask, "no thread was started", request.door).await?;
		Ok(())
	}

	/// The project's gate: one project's starts go one after another (CO-5), as omp runs a turn's tool calls at once and
	/// two that both find no project workspace would make two.
	fn gate(&self, project_id: &str) -> Arc<tokio::sync::Mutex<()>> {
		Arc::clone(self.st().starts.entry(project_id.to_owned()).or_default())
	}

	/// Drops the project's gate once no other start holds or waits for it.
	fn release(&self, project_id: &str, gate: Arc<tokio::sync::Mutex<()>>) {
		let mut st = self.st();
		// The map's own handle and this one: no other start has it.
		if Arc::strong_count(&gate) == 2 {
			st.starts.remove(project_id);
		}
	}

	async fn one_after_another(&self, launch: &Launch<'_>, place: &mut Option<ThreadPlace>) -> Result<Created> {
		let gate = self.gate(&launch.project.id);
		let made = {
			let _turn = gate.lock().await;
			self.make(launch, place).await
		};
		self.release(&launch.project.id, gate);
		made
	}

	/// The project workspace (made when herdr has none), the branch and folder, then the thread's worktree workspace.
	async fn make(&self, launch: &Launch<'_>, place: &mut Option<ThreadPlace>) -> Result<Created> {
		let c = self.link.ctx();
		let project = launch.project;
		let project_ws = match self.project_workspace(project).await? {
			Some(found) => found,
			None => {
				let project_key = new_pane_key();
				let mut env = launch.main_env.clone();
				set_env(&mut env, PANE_KEY_ENV, &project_key);
				let env = c.panes.bridged(env);
				let pane_command = launch.command.to_vec();
				let made = c
					.herdr
					.ensure_workspace(&project.name, &project.main_path, Some(&pane_command), Some(&env))
					.await?;
				if let Some(pane) = c.herdr.panes().await?.iter().find(|p| p.workspace_id == made) {
					c.launched.add(&pane.pane_id, &project.main_path.to_string_lossy(), Some(&project_key))?;
				}
				made
			}
		};
		// The branch and folder are picked here, after the project's earlier starts made theirs.
		let picked = thread_place(&project.main_path, &branch_slug(launch.title))?;
		*place = Some(picked.clone());
		// Its HEAD is read before omp starts in the pane, so no state omp reports comes before the thread is known.
		let head = Arc::new(Mutex::new(None::<String>));
		let prepare: Prepare = {
			let (ctx, project, key, head) = (Arc::clone(&c), project.clone(), launch.key.to_owned(), Arc::clone(&head));
			Box::new(move |path: PathBuf| {
				Box::pin(async move {
					copy_worktree_include(&project.main_path, &path).await?;
					*head.lock().unwrap_or_else(PoisonError::into_inner) =
						git_out(&path, &["rev-parse", "HEAD"]).await?;
					let mut env = ctx.panes.thread_pane_env(&project, &path).await?;
					set_env(&mut env, PANE_KEY_ENV, &key);
					Ok(env)
				})
			})
		};
		let created = c
			.herdr
			.create_worktree(
				&project_ws,
				WorktreeOptions {
					repo: project.main_path.clone(),
					branch: picked.branch.clone(),
					base: launch.trunk.map(str::to_owned),
					path: picked.path.clone(),
					title: launch.title.to_owned(),
					// Written only now, once the relay check passed and nothing refused the start.
					command: c.panes.task_prompted(launch.fresh, Some(launch.task))?,
					env: None,
					prepare: Some(prepare),
				},
			)
			.await?;
		let head = head.lock().unwrap_or_else(PoisonError::into_inner).take();
		Ok(Created { workspace_id: created.workspace_id, pane_id: created.pane_id, path: created.path, head })
	}

	/// The thread of a start, registered with the service.
	fn register(project: &Project, task: &TaskRecord, created: &Created, branch: &str) -> ThreadRef {
		let mut thread = Thread::new(
			created.pane_id.clone(),
			created.workspace_id.clone(),
			project.clone(),
			js_trim(&task.title),
			branch,
		);
		thread.path.clone_from(&created.path);
		thread.state = AgentState::Unknown;
		thread.shown = ThreadState::Working;
		thread.head.clone_from(&created.head);
		thread.plan = Some(ThreadPlan { sent_back: Some(0), ..ThreadPlan::default() });
		thread.spend_said = Some(false);
		thread.task = Some(task.clone());
		Shared::new(thread)
	}

	/// The thread is made: it is registered and remembered, and its brief goes in once omp is up.
	async fn finished_start(
		&self,
		thread: &ThreadRef,
		request: &Request<'_>,
		created: &Created,
		key: &str,
	) -> Result<Started> {
		let c = self.link.ctx();
		let project = thread.with(|t| t.project.clone());
		c.threads.add(thread.clone());
		c.launched.add(&created.pane_id, &created.path.to_string_lossy(), Some(key))?;
		self.st().last_started_in = Some(project);
		c.status.write_task(thread);
		c.omps.remember(thread, None).await;
		let (label, branch, heavy) = thread
			.with(|t| (subject(t), t.branch.clone(), t.task.as_ref().is_some_and(|task| task.heavy == Some(true))));
		let started = if request.door == Door::Boss {
			format!("Started {label} on branch {branch}")
		} else {
			format!("Started {label} on {branch}")
		};
		// The commit step is in `prompt.md`'s When you finish (0577); with the layer off there is none, so it ends the brief.
		let commit = if c.layer { String::new() } else { format!("\n{COMMIT_LINE}") };
		let said = format!("{}\n\n{LESSONS_LINE}{commit}", request.brief.trim_end());
		if !self.agent_up(&created.pane_id).await? {
			let wait = ((self.agent_wait_ms() as f64) / 1000.0 + 0.5).floor() as i64;
			return Ok(Started {
				text: format!("{started}, but omp didn't come up in its pane within {wait} s; the brief wasn't sent."),
				done: false,
			});
		}
		// Through the bridge (0675): omp takes it as a user message, so there is no paste to lose and no Enter to send.
		if let Err(error) = c.bridge.prompt(&created.pane_id, c.bridge.seal(&said), Deliver::FollowUp).await {
			return Ok(Started { text: format!("{started}, but the brief couldn't be sent: {error}"), done: false });
		}
		// A heavy thread says so (0590): it runs on his slow role, his strongest model.
		let slow = if heavy {
			format!(" Heavy: it runs on {} slow role.", if request.door == Door::Boss { "his" } else { "your" })
		} else {
			String::new()
		};
		let text = if request.door == Door::Boss {
			format!("{started}. It opens in plan mode and waits for his approval of its plan in its pane.{slow}")
		} else {
			format!("{started}: it plans first; approve its plan in its pane.{slow}")
		};
		Ok(Started { text, done: true })
	}

	/// Takes back what a failed start made (SVC-7): the workspace herdr opened in its worktree, the worktree and the
	/// branch, which `thread_place` picked as one no branch held. Each failure is logged; nothing of his is touched.
	async fn undo(&self, project: &Project, place: &ThreadPlace) {
		if let Err(error) = self.undo_steps(project, place).await {
			self.link.ctx().warn(Error::other(format!(
				"a failed start in {} left {}: {error}",
				project.name,
				place.path.display()
			)));
		}
	}

	async fn undo_steps(&self, project: &Project, place: &ThreadPlace) -> Result<()> {
		let c = self.link.ctx();
		let mut workspaces: Vec<String> = Vec::new();
		for pane in c.herdr.panes().await? {
			if pane.cwd.as_deref().is_some_and(|cwd| same_dir(cwd, &place.path))
				&& !workspaces.contains(&pane.workspace_id)
			{
				workspaces.push(pane.workspace_id);
			}
		}
		for workspace_id in &workspaces {
			c.herdr.close_workspace(workspace_id).await?;
		}
		if place.path.exists()
			&& let Some(failed) = remove_checkout(&project.main_path, &place.path, Some(UNDO_REMOVE_MS)).await
		{
			return Err(Error::other(format!("{} couldn't be removed ({failed})", place.path.display())));
		}
		let verify = ["rev-parse", "--verify", "-q", &format!("refs/heads/{}", place.branch)];
		if git_out(&project.main_path, &verify).await?.is_some_and(|found| !found.is_empty()) {
			git_async(&project.main_path, &["branch", "-D", &place.branch], GIT_MS, GitOptions::default()).await?;
		}
		c.numbers.forget(&place.path)
	}

	fn agent_wait_ms(&self) -> i64 {
		self.link.ctx().options.agent_wait_ms.unwrap_or(AGENT_WAIT_MS)
	}

	/// Waits until herdr sees an agent in the pane (its state isn't `unknown`), at most `agent_wait_ms`. A thread takes
	/// its first state from herdr's pane list here: an omp that reported it as herdr made the pane did so before the
	/// thread was registered, so its event found no thread (0456).
	async fn agent_up(&self, pane_id: &str) -> Result<bool> {
		let c = self.link.ctx();
		let wait = self.agent_wait_ms();
		let (c, id) = (&c, pane_id);
		wait_for(
			|| async move {
				let pane = c.herdr.panes().await?.into_iter().find(|p| p.pane_id == id);
				let thread = c.threads.get(id);
				if let (Some(pane), Some(thread)) = (&pane, &thread)
					&& let Some(session) = &pane.session_path
				{
					thread.with(|t| t.session_path = Some(session.clone()));
				}
				let Some(pane) = pane else { return Ok(false) };
				if pane.agent_state == AgentState::Unknown {
					return Ok(false);
				}
				if thread.is_some_and(|t| t.with(|t| t.state == AgentState::Unknown)) && !c.lifecycle.busy(id) {
					c.lifecycle.state_changed(id, pane.agent_state, StateSource::Reconcile);
				}
				Ok(true)
			},
			wait,
			POLL_MS,
		)
		.await
	}
}

#[cfg(test)]
#[path = "thread_start_tests.rs"]
mod tests;
