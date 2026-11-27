//! The in-process [`Surface`] (0683): Jocasta's own shell is the surface, so every call is a call into the shell's
//! state (`frontier/shell`), with no server and no socket between. Workspaces are Jocasta's projects (and a thread's
//! worktree, its own workspace); a tab is one pane running a program, started through the vendored portable-pty on
//! `alacritty_terminal`'s grid. A pane's agent state is not read from its screen: it is what its bridge says (0675),
//! through the lookup the service sets.
//!
//! A pane's process ending closes its tab, and its workspace with the last tab, as the pane list the service reads has
//! always shown (the service finds a project or thread whose omp ended by its pane being gone). A pane the service
//! replaces (`respawn`) or closes has its whole process tree ended first (0486).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::future::BoxFuture;

use super::surface::{
	ASLEEP, AgentState, Env, Note, NotificationResult, OnChange, PaneIds, PaneInfo, PaneMetadata, RespawnOptions,
	Sound, StopWatch, Surface, SurfaceError, SurfaceResult, SurfaceSnapshot, TabPane, TabSpec, Tokens, WatchEvent,
	WorkspaceInfo, WorktreeCreated, WorktreeOptions, merge_env,
};
use crate::frontier::client::ask_wake;
use crate::frontier::server::boss::PANE_KEY_ENV;
use crate::frontier::shell::{Hub, Start, Wake, bytes_for_names, screen_text};
use crate::shared::clock::now_ms;
use crate::shared::git::{GitOptions, git_async};
use crate::shared::log::{LogLevel, log};
use crate::shared::process::{RunOptions, on_path, run_program};
use crate::shared::proctree::{ProcessId, end_processes, process_id_of};

/// How often a watch re-reads the panes' states when nothing woke it (the bridge's reports don't wake the shell).
const WATCH_EVERY: Duration = Duration::from_secs(1);
/// `git worktree add` may take this long (a large checkout).
const WORKTREE_MS: i64 = 120_000;

fn failed(what: &str, error: impl std::fmt::Display) -> SurfaceError {
	SurfaceError::new("error", format!("{what}: {error}"))
}

/// The surface over the shell's `Hub`.
pub struct OwnSurface {
	hub: Arc<Hub>,
	/// Makes "find the workspace, else create it" one step.
	creating: tokio::sync::Mutex<()>,
}

impl OwnSurface {
	pub fn new(hub: Arc<Hub>) -> Arc<Self> {
		Arc::new(Self { hub, creating: tokio::sync::Mutex::new(()) })
	}

	/// Starts a pane's process off the runtime's threads (opening a ConPTY takes tens of milliseconds); a sleeping
	/// pane's argv ([`ASLEEP`], 0689) is drawn by the shell instead, its key asking for the wake with the pane's key.
	async fn start(
		&self,
		command: &[String],
		cwd: &Path,
		env: Option<&Env>,
	) -> SurfaceResult<Arc<crate::frontier::shell::Pane>> {
		if command.first().map(String::as_str) == Some(ASLEEP) {
			let text = command.get(1).cloned().unwrap_or_default();
			let key = env
				.and_then(|env| env.iter().find(|(name, _)| name == PANE_KEY_ENV))
				.map(|(_, key)| key.clone())
				.unwrap_or_default();
			let runtime = tokio::runtime::Handle::current();
			let wake: Wake = Arc::new(move |done| {
				let key = key.clone();
				runtime.spawn(async move { done(ask_wake(&key).await) });
			});
			return Ok(self.hub.start_still(&text, wake));
		}
		let hub = Arc::clone(&self.hub);
		let start = Start { argv: command.to_vec(), cwd: cwd.to_path_buf(), env: env.cloned().unwrap_or_default() };
		tokio::task::spawn_blocking(move || hub.start(&start))
			.await
			.map_err(|error| failed("the pane's process didn't start", error))?
			.map_err(|error| {
				failed(&format!("`{}` didn't start", command.first().map_or("the shell", String::as_str)), error)
			})
	}

	/// Ends the processes of `panes`, each with all it started, then closes their ptys (0486).
	async fn end(panes: Vec<Arc<crate::frontier::shell::Pane>>) {
		let roots: Vec<ProcessId> = panes.iter().filter_map(|pane| pane.pid).filter_map(process_id_of).collect();
		end_processes(&roots, &[]).await;
		for pane in panes {
			pane.close();
		}
	}

	async fn create_worktree_now(&self, workspace_id: &str, opts: WorktreeOptions) -> SurfaceResult<WorktreeCreated> {
		let path = opts.path.clone();
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).map_err(|error| failed("the worktree's folder wasn't made", error))?;
		}
		// Checks out `branch` at `path`, creating it from `base` (else HEAD) when missing.
		let exists =
			git_async(&opts.repo, &["rev-parse", "--verify", "--quiet", &opts.branch], 30_000, GitOptions::default())
				.await
				.map(|out| out.code == Some(0))
				.unwrap_or(false);
		let path_arg = path.display().to_string();
		let mut args: Vec<&str> = vec!["worktree", "add"];
		if exists {
			args.extend([path_arg.as_str(), opts.branch.as_str()]);
		} else {
			args.extend(["-b", opts.branch.as_str(), path_arg.as_str()]);
			args.extend(opts.base.as_deref());
		}
		let out = git_async(&opts.repo, &args, WORKTREE_MS, GitOptions::default())
			.await
			.map_err(|error| failed("git worktree add didn't run", error))?;
		if out.code != Some(0) {
			let why = out.stderr.lines().last().unwrap_or("git exited non-zero").to_owned();
			return Err(failed("git worktree add failed", why));
		}
		let extra = match opts.prepare {
			Some(prepare) => prepare(path.clone()).await?,
			None => Env::new(),
		};
		let env = merge_env(opts.env.as_ref(), &extra);
		let pane = self.start(&opts.command, &path, Some(&env)).await?;
		let (workspace, tab, pane_id) = self.hub.add_workspace(&opts.title, &path, pane);
		let _ = workspace_id;
		Ok(WorktreeCreated { workspace_id: workspace, tab_id: tab, pane_id, path })
	}
}

/// Raises a notification through the system's own command: no new process stays, no crate is needed (0683).
async fn show(note: &Note) -> NotificationResult {
	let title = note.title.clone();
	let body = note.body.clone().unwrap_or_default();
	let mut env: HashMap<String, String> = std::env::vars().collect();
	env.insert("JOCASTA_NOTE_TITLE".to_owned(), title);
	env.insert("JOCASTA_NOTE_BODY".to_owned(), body);
	let silent = note.sound == Sound::None;
	let argv: Vec<String> = if cfg!(windows) {
		let audio = if silent {
			"$a=$t.CreateElement('audio');$a.SetAttribute('silent','true');$t.DocumentElement.AppendChild($a)|Out-Null;"
		} else {
			""
		};
		let script = format!(
			"[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]|Out-Null;\
			 $t=[Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02);\
			 $n=$t.GetElementsByTagName('text');\
			 $n.Item(0).AppendChild($t.CreateTextNode($env:JOCASTA_NOTE_TITLE))|Out-Null;\
			 $n.Item(1).AppendChild($t.CreateTextNode($env:JOCASTA_NOTE_BODY))|Out-Null;{audio}\
			 [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\\WindowsPowerShell\\v1.0\\powershell.exe').Show([Windows.UI.Notifications.ToastNotification]::new($t))"
		);
		["powershell.exe", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script]
			.map(str::to_owned)
			.to_vec()
	} else if cfg!(target_os = "macos") {
		vec![
			"osascript".to_owned(),
			"-e".to_owned(),
			"display notification (system attribute \"JOCASTA_NOTE_BODY\") with title (system attribute \"JOCASTA_NOTE_TITLE\")"
				.to_owned(),
		]
	} else {
		if on_path("notify-send", None).is_none() {
			return NotificationResult::Disabled;
		}
		let argv = notify_send_argv(note);
		if let Some(link) = note.link.clone() {
			// It waits for his click (0699, #13), so it runs on its own, outside the 10 s bound, as long as the daemon
			// keeps the notification.
			let options = RunOptions { env: Some(env), timeout_ms: Some(LINK_WAIT_MS), ..RunOptions::default() };
			tokio::spawn(async move {
				if let Ok(run) = run_program(&argv, options).await
					&& run.stdout.trim() == "default"
				{
					(link.open)();
				}
			});
			return NotificationResult::Shown;
		}
		argv
	};
	match run_program(&argv, RunOptions { env: Some(env), timeout_ms: Some(10_000), ..RunOptions::default() }).await {
		Ok(run) if run.code == Some(0) => {
			// A question's notification goes once it is answered (0699): `-p` printed its id.
			if cfg!(target_os = "linux")
				&& let Some(until) = note.until.clone()
				&& let Ok(id) = run.stdout.trim().parse::<u32>()
			{
				tokio::spawn(async move {
					(until.0)().await;
					let options = RunOptions { timeout_ms: Some(10_000), ..RunOptions::default() };
					if let Err(error) = run_program(&close_argv(id), options).await {
						log(
							LogLevel::Warn,
							"notify",
							&format!("the notification {id} couldn't be closed: {error}"),
							None,
						);
					}
				});
			}
			NotificationResult::Shown
		}
		Ok(run) => {
			log(
				LogLevel::Warn,
				"notify",
				&format!("the notification command exited {:?}: {}", run.code, run.stderr.lines().last().unwrap_or("")),
				None,
			);
			NotificationResult::Disabled
		}
		Err(error) => {
			log(LogLevel::Warn, "notify", &format!("the notification command didn't run: {error}"), None);
			NotificationResult::Disabled
		}
	}
}

/// How long a notification with a link waits for his click before its `notify-send` is ended.
const LINK_WAIT_MS: i64 = 60 * 60_000;

/// `notify-send`'s argv (it takes its words as arguments). One that needs him (0699, #5) is critical, so the daemon
/// keeps it on screen, and asks for the freedesktop sound of a new message; one with a link (#13) offers the click as
/// its default action and waits for it; a question's prints its id, so it can be closed once answered.
fn notify_send_argv(note: &Note) -> Vec<String> {
	let mut argv = vec!["notify-send".to_owned(), "--app-name=Jocasta".to_owned()];
	if note.sound == Sound::Request {
		argv.extend(["-u", "critical", "--hint=string:sound-name:message-new-instant"].map(str::to_owned));
	}
	if note.link.is_some() {
		argv.extend(["-A", "default=Open", "--wait"].map(str::to_owned));
	} else if note.until.is_some() {
		argv.push("-p".to_owned());
	}
	argv.extend(["--".to_owned(), note.title.clone(), note.body.clone().unwrap_or_default()]);
	argv
}

/// The freedesktop call that takes notification `id` off the screen. `gdbus` comes with GLib, which libnotify's
/// `notify-send` needs, so it is there whenever notifications are.
fn close_argv(id: u32) -> Vec<String> {
	[
		"gdbus",
		"call",
		"--session",
		"--dest",
		"org.freedesktop.Notifications",
		"--object-path",
		"/org/freedesktop/Notifications",
		"--method",
		"org.freedesktop.Notifications.CloseNotification",
	]
	.map(str::to_owned)
	.into_iter()
	.chain([id.to_string()])
	.collect()
}

impl Surface for OwnSurface {
	fn started_server(&self) -> bool {
		true
	}

	fn ping(&self) -> BoxFuture<'_, SurfaceResult<Option<String>>> {
		Box::pin(async { Ok(Some(env!("CARGO_PKG_VERSION").to_owned())) })
	}

	fn ensure_workspace<'a>(
		&'a self,
		label: &'a str,
		cwd: &'a Path,
		command: Option<&'a [String]>,
		env: Option<&'a Env>,
	) -> BoxFuture<'a, SurfaceResult<String>> {
		Box::pin(async move {
			let _one = self.creating.lock().await;
			if let Some(id) = self.hub.find_workspace(label) {
				return Ok(id);
			}
			let pane = self.start(command.unwrap_or_default(), cwd, env).await?;
			Ok(self.hub.add_workspace(label, cwd, pane).0)
		})
	}

	fn workspaces(&self) -> BoxFuture<'_, SurfaceResult<Vec<WorkspaceInfo>>> {
		Box::pin(async move { Ok(self.hub.workspaces()) })
	}

	fn snapshot(&self) -> BoxFuture<'_, SurfaceResult<SurfaceSnapshot>> {
		Box::pin(async move { Ok(self.hub.snapshot()) })
	}

	fn close_workspace<'a>(&'a self, workspace_id: &'a str) -> BoxFuture<'a, SurfaceResult<()>> {
		Box::pin(async move {
			// The processes are listed while their roots live: the tree can't be found once a root is gone (0486).
			let panes = self.hub.remove_workspace(workspace_id);
			Self::end(panes).await;
			Ok(())
		})
	}

	fn create_tab<'a>(&'a self, workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, SurfaceResult<TabPane>> {
		Box::pin(async move {
			let pane = self.start(&tab.command, &tab.cwd, tab.env.as_ref()).await?;
			match self.hub.add_tab(workspace_id, &tab.title, &tab.cwd, Arc::clone(&pane)) {
				Some((tab_id, pane_id)) => Ok(TabPane { tab_id, pane_id }),
				None => {
					Self::end(vec![pane]).await;
					Err(SurfaceError::new("workspace_not_found", format!("no workspace {workspace_id}")))
				}
			}
		})
	}

	fn create_worktree<'a>(
		&'a self,
		workspace_id: &'a str,
		opts: WorktreeOptions,
	) -> BoxFuture<'a, SurfaceResult<WorktreeCreated>> {
		Box::pin(self.create_worktree_now(workspace_id, opts))
	}

	fn move_workspace<'a>(&'a self, workspace_id: &'a str, index: usize) -> BoxFuture<'a, SurfaceResult<()>> {
		Box::pin(async move {
			self.hub.move_workspace(workspace_id, index);
			Ok(())
		})
	}

	fn respawn<'a>(&'a self, pane_id: &'a str, opts: &'a RespawnOptions) -> BoxFuture<'a, SurfaceResult<TabPane>> {
		Box::pin(async move {
			if self.hub.pane(pane_id).is_none() {
				return Err(SurfaceError::new("pane_not_found", format!("no pane {pane_id}")));
			}
			let pane = self.start(&opts.command, &opts.cwd, opts.env.as_ref()).await?;
			let Some((tab_id, new_id, old)) = self.hub.replace(pane_id, &opts.cwd, Arc::clone(&pane)) else {
				Self::end(vec![pane]).await;
				return Err(SurfaceError::new("pane_not_found", format!("no pane {pane_id}")));
			};
			Self::end(vec![old]).await;
			Ok(TabPane { tab_id, pane_id: new_id })
		})
	}

	fn send_keys<'a>(&'a self, pane_id: &'a str, keys: &'a [String]) -> BoxFuture<'a, SurfaceResult<()>> {
		Box::pin(async move {
			let pane = self
				.hub
				.pane(pane_id)
				.ok_or_else(|| SurfaceError::new("pane_not_found", format!("no pane {pane_id}")))?;
			let mode = *pane.term.lock().mode();
			let bytes = bytes_for_names(keys, mode).map_err(|error| SurfaceError::new("invalid_keys", error))?;
			pane.send(&bytes);
			Ok(())
		})
	}

	fn notify<'a>(&'a self, note: &'a Note) -> BoxFuture<'a, SurfaceResult<NotificationResult>> {
		Box::pin(async move { Ok(show(note).await) })
	}

	fn focus<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, SurfaceResult<()>> {
		Box::pin(async move {
			if self.hub.focus(pane_id) {
				Ok(())
			} else {
				Err(SurfaceError::new("pane_not_found", format!("no pane {pane_id}")))
			}
		})
	}

	fn screen<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, SurfaceResult<String>> {
		Box::pin(async move {
			let pane = self
				.hub
				.pane(pane_id)
				.ok_or_else(|| SurfaceError::new("pane_not_found", format!("no pane {pane_id}")))?;
			let term = pane.term.lock();
			Ok(screen_text(&term))
		})
	}

	fn report_pane_metadata<'a>(
		&'a self,
		pane_id: &'a str,
		meta: &'a PaneMetadata,
	) -> BoxFuture<'a, SurfaceResult<()>> {
		Box::pin(async move {
			if self.hub.pane_metadata(pane_id, meta, now_ms()) {
				Ok(())
			} else {
				Err(SurfaceError::new("pane_not_found", format!("no pane {pane_id}")))
			}
		})
	}

	fn report_workspace_metadata<'a>(
		&'a self,
		workspace_id: &'a str,
		tokens: &'a Tokens,
		ttl_ms: Option<i64>,
	) -> BoxFuture<'a, SurfaceResult<()>> {
		Box::pin(async move {
			if self.hub.workspace_metadata(workspace_id, tokens, ttl_ms, now_ms()) {
				Ok(())
			} else {
				Err(SurfaceError::new("workspace_not_found", format!("no workspace {workspace_id}")))
			}
		})
	}

	fn panes(&self) -> BoxFuture<'_, SurfaceResult<Vec<PaneInfo>>> {
		Box::pin(async move { Ok(self.hub.snapshot().panes) })
	}

	fn pane_root<'a>(
		&'a self,
		pane_id: &'a str,
		_timeout_ms: Option<i64>,
	) -> BoxFuture<'a, SurfaceResult<Option<u32>>> {
		Box::pin(async move { Ok(self.hub.pane(pane_id).and_then(|pane| pane.pid)) })
	}

	fn watch(&self, pane_ids: PaneIds, on_change: OnChange) -> StopWatch {
		let hub = Arc::clone(&self.hub);
		let changed = hub.changed();
		let stopped = Arc::new(AtomicBool::new(false));
		let flag = Arc::clone(&stopped);
		let task = tokio::spawn(async move {
			let mut last: HashMap<String, AgentState> = HashMap::new();
			while !flag.load(Ordering::SeqCst) {
				let wanted = pane_ids();
				last.retain(|id, _| wanted.contains(id));
				for id in &wanted {
					let Some((workspace_id, state)) = hub.state_of(id) else { continue };
					if last.get(id) != Some(&state) {
						last.insert(id.clone(), state);
						on_change(WatchEvent { pane_id: id.clone(), workspace_id, state });
					}
				}
				tokio::select! {
					() = changed.notified() => {}
					() = tokio::time::sleep(WATCH_EVERY) => {}
				}
			}
		});
		let abort = task.abort_handle();
		Box::new(move || {
			stopped.store(true, Ordering::SeqCst);
			abort.abort();
		})
	}

	fn stop_server(&self) -> BoxFuture<'_, SurfaceResult<()>> {
		Box::pin(async move {
			Self::end(self.hub.all_panes()).await;
			Ok(())
		})
	}

	fn close(&self) {}
}

#[cfg(test)]
#[path = "own_surface_tests.rs"]
mod tests;
