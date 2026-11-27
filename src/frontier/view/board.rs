//! The board window (0718): `jocasta board`, the open threads live in a window of its own (Linux app id
//! `jocasta-board`; ultron puts it on workspace 4), one at a time (`<data>/board.lock`). It asks the running service for
//! `board json` every few seconds and patches its rows by thread: only the first open has an entrance, a new row fades
//! in, a row whose phase changed lights its phase and lets the light fade; a poll that changed nothing moves nothing,
//! and an idle window asks for no frames.

use std::collections::HashMap;
use std::fs::{File, TryLockError};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use iced::widget::{Column, column, container, row, rule, scrollable, text};
use iced::{Alignment, Background, Color, Element, Fill, Padding, Size, Subscription, Task, border, window};

use crate::frontier::client;
use crate::frontier::protocol::{Board, BoardRow, PrCi, wait_label};
use crate::frontier::server::wording::money;
use crate::frontier::today::view::tokens;
use crate::frontier::tui::load_palette;
use crate::frontier::view::app::platform;
use crate::frontier::view::motion::{self, BASE_MS, Motion, REVEAL_MS, RISE_PX, ease, node_at, settled};
use crate::frontier::view::style::{self, BODY, Colours, HEADING, LABEL, META, RADIUS, TITLE, colours, fade};
use crate::shared::clock::now_ms;
use crate::shared::paths::{config_dir, data_dir};
use crate::shared::service_file::NOT_RUNNING;

#[cfg(test)]
#[path = "board_tests.rs"]
mod tests;

/// The board's Linux app id.
pub const APP_ID: &str = "jocasta-board";
/// How often it asks the service.
const POLL: Duration = Duration::from_secs(3);
/// What it says while the service doesn't answer (it keeps asking).
const DOWN: &str = "Jocasta isn't running.";
/// A second `jocasta board` while one is open.
const ALREADY_OPEN: &str = "The board is already open.";
/// No open thread.
const NOTHING: &str = "No thread is open.";

/// `jocasta board`: starts the board's window on its own (`jocasta board --window`, detached like a brief's) and returns
/// at once, so his terminal is his again; a board already open is exit 0 with a line saying so. The window takes the
/// lock itself and keeps it while it is open.
pub fn open() -> ExitCode {
	match hold(&data_dir()) {
		Ok(Some(lock)) => drop(lock),
		Ok(None) => {
			println!("{ALREADY_OPEN}");
			return ExitCode::SUCCESS;
		}
		Err(error) => {
			eprintln!("jocasta: the board's lock didn't open: {error}");
			return ExitCode::from(1);
		}
	}
	match super::spawn(WINDOW) {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => {
			eprintln!("jocasta: the board didn't open: {error}");
			ExitCode::from(1)
		}
	}
}

/// What starts the board's window process.
pub const WINDOW: &[&str] = &["board", "--window"];

/// `jocasta board --window`: the window until he closes it; a board already open is exit 0 with a line saying so.
pub fn run() -> ExitCode {
	let _lock = match hold(&data_dir()) {
		Ok(Some(lock)) => lock,
		Ok(None) => {
			println!("{ALREADY_OPEN}");
			return ExitCode::SUCCESS;
		}
		Err(error) => {
			eprintln!("jocasta: the board's lock didn't open: {error}");
			return ExitCode::from(1);
		}
	};
	load_palette(&config_dir().join("palette.json"));
	let settings = window::Settings {
		size: Size::new(760.0, 640.0),
		min_size: Some(Size::new(420.0, 320.0)),
		platform_specific: platform(APP_ID),
		..window::Settings::default()
	};
	let shown = iced::application(Watch::open, Watch::update, Watch::view)
		.title(|_: &Watch| "Jocasta board".to_owned())
		.theme(style::theme())
		.subscription(Watch::subscription)
		.default_font(style::ui_font())
		.window(settings)
		.run();
	match shown {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => {
			eprintln!("jocasta: the window didn't open: {error}");
			ExitCode::from(1)
		}
	}
}

/// Takes `<dir>/board.lock` for as long as the file is held: `None` when another board holds it. The OS lets go when
/// the process ends, crashed or not.
fn hold(dir: &Path) -> std::io::Result<Option<File>> {
	std::fs::create_dir_all(dir)?;
	let file = File::options().create(true).truncate(false).write(true).open(dir.join("board.lock"))?;
	match file.try_lock() {
		Ok(()) => Ok(Some(file)),
		Err(TryLockError::WouldBlock) => Ok(None),
		Err(TryLockError::Error(error)) => Err(error),
	}
}

/// The window's messages.
#[derive(Debug, Clone)]
enum Message {
	Tick(Instant),
	Poll,
	Answer(Result<Board, String>),
}

fn ask() -> Task<Message> {
	Task::perform(client::board(), Message::Answer)
}

/// The window's state.
struct Watch {
	board: Option<Board>,
	/// Why the last ask brought no board: [`NOT_RUNNING`], or what went wrong.
	why: Option<String>,
	asking: bool,
	/// When the first board arrived: the entrance plays from it, once.
	first: Option<Instant>,
	now: Instant,
	/// The wall clock at the last answer, for the rows' ages.
	at_ms: i64,
	motion: Motion,
	/// Rows that came after the first board, by pane, with when.
	appeared: HashMap<String, Instant>,
	/// Rows whose phase changed, by pane, with when.
	lit: HashMap<String, Instant>,
}

impl Watch {
	fn open() -> (Self, Task<Message>) {
		let watch = Self {
			board: None,
			why: None,
			asking: true,
			first: None,
			now: Instant::now(),
			at_ms: now_ms(),
			motion: motion::motion(),
			appeared: HashMap::new(),
			lit: HashMap::new(),
		};
		(watch, ask())
	}

	/// Milliseconds from `since` to now.
	fn ms(&self, since: Instant) -> f32 {
		self.now.saturating_duration_since(since).as_secs_f32() * 1_000.0
	}

	fn animating(&self) -> bool {
		self.first.is_some_and(|first| !settled(self.ms(first)))
			|| self.appeared.values().any(|at| self.ms(*at) < REVEAL_MS)
			|| self.lit.values().any(|at| self.ms(*at) < BASE_MS)
	}

	fn subscription(&self) -> Subscription<Message> {
		let mut parts = vec![iced::time::every(POLL).map(|_| Message::Poll)];
		if self.animating() {
			parts.push(window::frames().map(Message::Tick));
		}
		Subscription::batch(parts)
	}

	fn update(&mut self, message: Message) -> Task<Message> {
		match message {
			Message::Tick(now) => {
				self.now = now;
				let ms = |at: &Instant| now.saturating_duration_since(*at).as_secs_f32() * 1_000.0;
				self.appeared.retain(|_, at| ms(at) < REVEAL_MS);
				self.lit.retain(|_, at| ms(at) < BASE_MS);
			}
			Message::Poll if !self.asking => {
				self.asking = true;
				return ask();
			}
			Message::Poll => {}
			Message::Answer(answer) => {
				self.asking = false;
				self.now = Instant::now();
				self.at_ms = now_ms();
				match answer {
					Ok(board) => self.patch(board),
					Err(why) => {
						if why == NOT_RUNNING {
							self.board = None;
						}
						self.why = Some(why);
					}
				}
			}
		}
		Task::none()
	}

	/// Takes a new board: rows that weren't there fade in, rows whose phase changed light up; the first board plays the
	/// entrance instead.
	fn patch(&mut self, board: Board) {
		let now = self.now;
		if self.first.is_none() {
			self.first = Some(now);
		} else {
			for row in &board.rows {
				let before = self.board.as_ref().and_then(|old| old.rows.iter().find(|old| old.pane_id == row.pane_id));
				match before {
					None => {
						self.appeared.insert(row.pane_id.clone(), now);
					}
					Some(old) if old.phase != row.phase => {
						self.lit.insert(row.pane_id.clone(), now);
					}
					Some(_) => {}
				}
			}
		}
		let open = |pane: &String| board.rows.iter().any(|row| &row.pane_id == pane);
		self.appeared.retain(|pane, _| open(pane));
		self.lit.retain(|pane, _| open(pane));
		self.why = None;
		self.board = Some(board);
	}

	/// How the row of `pane` at `index` looks now: its opacity, how far below its place it still is, and how lit its
	/// phase is.
	fn presence(&self, pane: &str, index: usize) -> (f32, f32, f32) {
		let (mut alpha, rise) = self.first.map_or((1.0, 0.0), |first| node_at(self.ms(first), index, self.motion));
		if let Some(at) = self.appeared.get(pane) {
			alpha = alpha.min(ease(self.ms(*at) / REVEAL_MS));
		}
		let light = self.lit.get(pane).map_or(0.0, |at| 1.0 - ease(self.ms(*at) / BASE_MS));
		(alpha, rise, light)
	}

	fn view(&self) -> Element<'_, Message> {
		let c = colours();
		let mut page = Column::new().spacing(24).width(Fill).max_width(920).push(self.header());
		match (&self.board, &self.why) {
			(Some(board), why) => {
				if let Some(why) = why {
					page =
						page.push(text(why.clone()).size(LABEL).color(c.yellow).wrapping(text::Wrapping::WordOrGlyph));
				}
				page = page.push(self.rows(board));
				if !board.queued.is_empty() {
					let mut waiting = column![text("Waiting to start").size(HEADING).color(c.fg)].spacing(8);
					for line in &board.queued {
						waiting = waiting
							.push(text(line.clone()).size(LABEL).color(c.fg2).wrapping(text::Wrapping::WordOrGlyph));
					}
					page = page.push(waiting);
				}
			}
			(None, Some(why)) => {
				let said = if why == NOT_RUNNING { DOWN.to_owned() } else { why.clone() };
				page = page.push(text(said).size(BODY).color(c.fg2).wrapping(text::Wrapping::WordOrGlyph));
			}
			(None, None) => {}
		}
		let body = container(page).padding([32, 40]).center_x(Fill);
		container(scrollable(body).height(Fill))
			.style(move |_| container::Style { background: Some(c.bg.into()), ..container::Style::default() })
			.into()
	}

	fn header(&self) -> Element<'_, Message> {
		let c = colours();
		let mut head = column![text("Board").size(TITLE).color(c.fg)].spacing(8);
		if let Some(board) = &self.board {
			head = head.push(text(summary(board)).size(META).color(c.muted).wrapping(text::Wrapping::WordOrGlyph));
		}
		head.into()
	}

	fn rows<'a>(&'a self, board: &'a Board) -> Element<'a, Message> {
		let c = colours();
		if board.rows.is_empty() {
			return text(NOTHING).size(BODY).color(c.fg2).into();
		}
		let mut rows = Column::new();
		for (index, row) in board.rows.iter().enumerate() {
			rows = rows.push(rule::horizontal(1).style(move |_| rule::Style {
				color: c.line,
				radius: 0.0.into(),
				fill_mode: rule::FillMode::Full,
				snap: true,
			}));
			rows = rows.push(self.row(row, index, &c));
		}
		rows.into()
	}

	fn row<'a>(&'a self, r: &'a BoardRow, index: usize, c: &Colours) -> Element<'a, Message> {
		let (alpha, rise, light) = self.presence(&r.pane_id, index);
		let colour = phase_colour(&r.phase, c);
		let badge =
			container(text(r.phase.clone()).size(META).color(fade(colour, alpha))).padding([2, 8]).style(move |_| {
				container::Style {
					background: Some(Background::Color(fade(colour, alpha * (0.14 + 0.36 * light)))),
					border: border::rounded(RADIUS),
					..container::Style::default()
				}
			});
		let title = text(format!("{}/{}", r.project, r.title))
			.size(BODY)
			.color(fade(c.fg, alpha))
			.wrapping(text::Wrapping::WordOrGlyph)
			.width(Fill);
		let mut part = column![row![title, badge].spacing(12).align_y(Alignment::Center)].spacing(4);
		if !r.line.is_empty() {
			part = part
				.push(text(r.line.clone()).size(LABEL).color(fade(c.fg2, alpha)).wrapping(text::Wrapping::WordOrGlyph));
		}
		part = part.push(
			text(facts(r, self.at_ms)).size(META).color(fade(c.muted, alpha)).wrapping(text::Wrapping::WordOrGlyph),
		);
		// The rise moves the row inside a constant height, so the rows under it never shift.
		container(part)
			.padding(Padding { top: 10.0 + rise, bottom: 10.0 + RISE_PX - rise, left: 0.0, right: 0.0 })
			.into()
	}
}

/// The header's line: how many threads are open, the check queue and today's spend.
fn summary(board: &Board) -> String {
	let mut parts = vec![match board.rows.len() {
		1 => "1 thread open".to_owned(),
		n => format!("{n} threads open"),
	}];
	parts.push(match (board.checks.running, board.checks.waiting) {
		(0, 0) => "no checks running".to_owned(),
		(running, waiting) => format!("checks: {running} running, {waiting} waiting"),
	});
	if let Some(spent) = board.spend_today {
		parts.push(format!("today {}", money(spent)));
	}
	parts.join(" · ")
}

/// A row's facts: its age, cost against budget, context, last check and its PR's CI, what is known of them.
fn facts(r: &BoardRow, at_ms: i64) -> String {
	let mut parts = vec![wait_label(at_ms - r.since)];
	match (r.cost, r.budget) {
		(Some(cost), Some(budget)) => parts.push(format!("{} of {}", money(cost), money(budget))),
		(Some(cost), None) => parts.push(money(cost)),
		(None, Some(budget)) => parts.push(format!("budget {}", money(budget))),
		(None, None) => {}
	}
	if let Some(context) = r.context {
		parts.push(format!("{} context", tokens(context)));
	}
	match r.check {
		Some(true) => parts.push("check passed".to_owned()),
		Some(false) => parts.push("check failed".to_owned()),
		None => {}
	}
	if let Some(pr) = &r.pr {
		let ci = match pr.ci {
			PrCi::Pending => "CI pending",
			PrCi::Passing => "CI passing",
			PrCi::Failing => "CI failing",
			PrCi::Merged => "merged",
			PrCi::Closed => "closed",
		};
		parts.push(format!("PR #{} {ci}", pr.number));
	}
	parts.join(" · ")
}

/// A phase's colour: a stop red, waiting on him yellow, at work blue, the review steps purple, done green.
fn phase_colour(phase: &str, c: &Colours) -> Color {
	match phase {
		"stuck" => c.red,
		"awaiting you" | "plan ready" => c.yellow,
		"working" => c.blue,
		"review" | "handoff" | "finishing" => c.purple,
		"done" => c.green,
		_ => c.muted,
	}
}
