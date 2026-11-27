//! The brief window itself (0721): its state, its messages and how each section is drawn with iced's widgets. Only
//! the first open moves; an action's answer patches its card (or, for the thread's buttons, the brief) and nothing
//! replays. An idle window asks for no frames.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use iced::animation::Easing;
use iced::widget::{
	Column, button, canvas, column, container, image, mouse_area, pick_list, row, rule, scrollable, space, text,
	text_editor, text_input,
};
use iced::{Alignment, Animation, Color, Element, Fill, Length, Size, Subscription, Task, window};
use serde_json::Value;

use crate::frontier::brief::model::{
	Brief, Card, CheckRow, DiffFile, Fact, FlowEdge, FlowNode, ListRow, MapGroup, MapLink, Section, Severity,
	SystemEdge, SystemNode, SystemRow, Verdict, WorksRow, WorksState,
};
use crate::frontier::client::{brief_act, brief_live};
use crate::frontier::view::flow::{FlowCanvas, Presence, direction, shape};
use crate::frontier::view::model::{
	Act, CLOSED, EMPTY, NO_SYSTEM_CHANGE, answer_of, app_id, card_text, diff_summary, duration, exit, generated,
	is_empty, paragraphs, patch_card, place, proof_text, publishable, screenshot, system_nodes, verdict, works_state,
};
use crate::frontier::view::motion::{self, DISCLOSURE, Motion, SWITCH, ease, presence, settled};
use crate::frontier::view::style::{self, BODY, HEADING, LABEL, META, TITLE, Tone, colours, fade, mono_font};
use crate::shared::link::{open_file, open_in_browser, web_link};

/// What opens and closes in a window: a diff section (by its index), a card's proof (by its id) or a system row's
/// proof (by its section, list and index).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Fold {
	Diff(usize),
	Proof(String),
	Row(usize, &'static str, usize),
}

/// Whose buttons an action came from: the thread's, or a card's.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Target {
	Thread,
	Card(String),
}

impl Target {
	fn of(act: &Act) -> Self {
		act.card().map_or(Self::Thread, |card| Self::Card(card.to_owned()))
	}
}

/// The window's messages.
#[derive(Debug, Clone)]
pub enum Message {
	Tick(Instant),
	Resized(Size),
	Live(Result<bool, String>),
	Toggle(Fold),
	/// A system section's switch (by its index): After when true, Before otherwise.
	Side(usize, bool),
	Edit(String, text_editor::Action),
	Instruction(String, String),
	SendText(text_editor::Action),
	Act(Act),
	Answered(Act, Result<Value, String>),
	Dismiss,
	Link(String),
	File(PathBuf),
}

/// A verdict as the switch lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Choice(Verdict);

impl std::fmt::Display for Choice {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(verdict(self.0))
	}
}

const CHOICES: [Choice; 3] = [Choice(Verdict::Post), Choice(Verdict::Question), Choice(Verdict::Skip)];

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

/// One window's state.
struct Window {
	brief: Brief,
	/// Whether the buttons act on something: `None` until the service answers (or when it isn't running).
	live: Option<bool>,
	opened: Instant,
	now: Instant,
	/// The first-open entrance is still playing.
	entering: bool,
	motion: Motion,
	width: f32,
	folds: HashMap<Fold, Animation<bool>>,
	/// Each system section's switch, true for After; a section not in it shows After, settled.
	sides: HashMap<usize, Animation<bool>>,
	editors: HashMap<String, text_editor::Content>,
	instructions: HashMap<String, String>,
	send: text_editor::Content,
	busy: Option<(Target, &'static str)>,
	banners: HashMap<Target, String>,
	suggestion: Option<(String, String)>,
}

/// Opens the window for `brief` and runs it until it is closed.
pub fn run(brief: Brief) -> iced::Result {
	let title = brief.title.clone();
	let settings = window::Settings {
		size: Size::new(1040.0, 800.0),
		min_size: Some(Size::new(420.0, 320.0)),
		platform_specific: platform(app_id(&brief.id)),
		..window::Settings::default()
	};
	iced::application(move || Window::open(brief.clone()), Window::update, Window::view)
		.title(move |_: &Window| title.clone())
		.theme(style::theme())
		.subscription(Window::subscription)
		.default_font(style::ui_font())
		.window(settings)
		.run()
}

#[cfg(target_os = "linux")]
fn platform(app_id: &str) -> window::settings::PlatformSpecific {
	window::settings::PlatformSpecific { application_id: app_id.to_owned(), ..Default::default() }
}

#[cfg(not(target_os = "linux"))]
fn platform(_app_id: &str) -> window::settings::PlatformSpecific {
	window::settings::PlatformSpecific::default()
}

fn ask_live(id: &str) -> Task<Message> {
	let id = id.to_owned();
	Task::perform(async move { brief_live(&id).await }, Message::Live)
}

fn editors_of(brief: &Brief) -> HashMap<String, text_editor::Content> {
	brief
		.sections
		.iter()
		.flat_map(|section| match section {
			Section::Cards { cards, .. } => cards.as_slice(),
			_ => &[],
		})
		.map(|card| (card.id.clone(), text_editor::Content::with_text(card_text(card))))
		.collect()
}

impl Window {
	fn open(brief: Brief) -> (Self, Task<Message>) {
		let now = Instant::now();
		let has_flow = brief.sections.iter().any(|section| match section {
			Section::Flow { .. } => true,
			Section::System { nodes, .. } => !nodes.is_empty(),
			_ => false,
		});
		let asks = matches!(brief.kind.as_str(), "thread" | "pr");
		let task = if asks { ask_live(&brief.id) } else { Task::none() };
		let window = Self {
			editors: editors_of(&brief),
			brief,
			live: None,
			opened: now,
			now,
			entering: has_flow,
			motion: motion::motion(),
			width: 1040.0,
			folds: HashMap::new(),
			sides: HashMap::new(),
			instructions: HashMap::new(),
			send: text_editor::Content::new(),
			busy: None,
			banners: HashMap::new(),
			suggestion: None,
		};
		(window, task)
	}

	fn elapsed(&self) -> f32 {
		self.now.duration_since(self.opened).as_secs_f32() * 1_000.0
	}

	fn animating(&self) -> bool {
		self.entering
			|| self.folds.values().any(|fold| fold.is_animating(self.now))
			|| self.sides.values().any(|side| side.is_animating(self.now))
	}

	fn subscription(&self) -> Subscription<Message> {
		let resized = window::resize_events().map(|(_, size)| Message::Resized(size));
		if self.animating() { Subscription::batch([resized, window::frames().map(Message::Tick)]) } else { resized }
	}

	fn update(&mut self, message: Message) -> Task<Message> {
		match message {
			Message::Tick(now) => {
				self.now = now;
				self.entering = self.entering && !settled(self.elapsed());
			}
			Message::Resized(size) => self.width = size.width,
			Message::Live(answer) => self.live = answer.ok(),
			Message::Toggle(fold) => {
				self.now = Instant::now();
				let open = self.folds.get(&fold).is_some_and(Animation::value);
				let animation = self
					.folds
					.entry(fold)
					.or_insert_with(|| Animation::new(false).duration(DISCLOSURE).easing(Easing::Custom(ease)));
				animation.go_mut(!open, self.now);
			}
			Message::Side(index, after) => {
				self.now = Instant::now();
				let animation = self
					.sides
					.entry(index)
					.or_insert_with(|| Animation::new(true).duration(SWITCH).easing(Easing::Custom(ease)));
				animation.go_mut(after, self.now);
			}
			Message::Edit(card, action) => {
				if let Some(editor) = self.editors.get_mut(&card) {
					editor.perform(action);
				}
			}
			Message::Instruction(card, text) => {
				self.instructions.insert(card, text);
			}
			Message::SendText(action) => self.send.perform(action),
			Message::Act(act) => return self.act(act),
			Message::Answered(act, answer) => return self.answered(&act, answer),
			Message::Dismiss => self.suggestion = None,
			Message::Link(url) => {
				if let Some(link) = web_link(Some(&url)) {
					let _ = open_in_browser(&link);
				}
			}
			Message::File(path) => {
				let _ = open_file(&path);
			}
		}
		Task::none()
	}

	fn act(&mut self, act: Act) -> Task<Message> {
		if self.busy.is_some() {
			return Task::none();
		}
		let target = Target::of(&act);
		self.banners.remove(&target);
		self.busy = Some((target, act.waiting()));
		let (id, wire) = (self.brief.id.clone(), act.to_json());
		Task::perform(async move { brief_act(&id, wire).await }, move |answer| Message::Answered(act.clone(), answer))
	}

	/// Takes the service's answer: a card's patches that card only; the thread's buttons take the brief and ask again
	/// whether the thread is still open.
	fn answered(&mut self, act: &Act, answer: Result<Value, String>) -> Task<Message> {
		let target = Target::of(act);
		self.busy = None;
		let parsed = answer.and_then(|value| answer_of(&value));
		let (fresh, suggestion) = match parsed {
			Ok(parsed) => parsed,
			Err(error) => {
				self.banners.insert(target, error);
				return Task::none();
			}
		};
		match act {
			Act::Edit { card, .. } | Act::Reword { card, .. } | Act::Verdict { card, .. } | Act::Publish { card } => {
				patch_card(&mut self.brief, &fresh, card);
				if let (Act::Edit { .. }, Some(shown)) = (act, crate::frontier::view::model::card(&self.brief, card)) {
					self.editors.insert(card.clone(), text_editor::Content::with_text(card_text(shown)));
					self.suggestion = None;
				}
				if matches!(act, Act::Reword { .. }) {
					self.suggestion = suggestion;
				}
				Task::none()
			}
			Act::Finish { .. } | Act::Send { .. } => {
				if matches!(act, Act::Send { .. }) {
					self.send = text_editor::Content::new();
				}
				for (id, editor) in editors_of(&fresh) {
					self.editors.entry(id).or_insert(editor);
				}
				self.brief = fresh;
				ask_live(&self.brief.id)
			}
		}
	}

	fn view(&self) -> Element<'_, Message> {
		let c = colours();
		let mut page = Column::new().spacing(32).max_width(920);
		page = page.push(self.header());
		if self.brief.kind == "thread" {
			page = page.push(self.thread_buttons());
		}
		for (index, section) in self.brief.sections.iter().enumerate() {
			page = page.push(self.section(index, section));
		}
		let body = container(page).padding([32, 40]).center_x(Fill);
		container(scrollable(body).height(Fill))
			.style(move |_| container::Style { background: Some(c.bg.into()), ..container::Style::default() })
			.into()
	}

	fn header(&self) -> Element<'_, Message> {
		let c = colours();
		let mut head =
			column![text(&self.brief.title).size(TITLE).color(c.fg).wrapping(text::Wrapping::WordOrGlyph)].spacing(8);
		if !self.brief.subtitle.is_empty() {
			head = head.push(text(&self.brief.subtitle).size(BODY).color(c.fg2));
		}
		let made = generated(self.brief.generated_at_ms);
		if !made.is_empty() {
			head = head.push(text(format!("Made {made}")).size(META).color(c.muted));
		}
		head.into()
	}

	/// A button that sends `act`, quiet while another action waits.
	fn act_button(&self, label: &str, tone: Tone, act: Option<Act>) -> Element<'_, Message> {
		let press = if self.busy.is_some() { None } else { act.map(Message::Act) };
		button(text(label.to_owned()).size(LABEL))
			.padding([6, 14])
			.style(style::control(tone))
			.on_press_maybe(press)
			.into()
	}

	/// What waits and what was refused for `target`.
	fn status(&self, target: &Target) -> Option<Element<'_, Message>> {
		let c = colours();
		if let Some((busy, waiting)) = &self.busy
			&& busy == target
		{
			return Some(text(*waiting).size(LABEL).color(c.muted).into());
		}
		let refused = self.banners.get(target)?;
		Some(container(text(refused).size(LABEL)).padding([6, 10]).style(style::badge(colours().red)).into())
	}

	fn thread_buttons(&self) -> Element<'_, Message> {
		let c = colours();
		if self.live == Some(false) {
			return text(CLOSED).size(BODY).color(c.muted).into();
		}
		let words = self.send.text();
		let words = words.trim();
		let send = (!words.is_empty()).then(|| Act::Send { text: words.to_owned() });
		let mut bar = column![
			row![
				self.act_button("Merge", Tone::Primary, Some(Act::Finish { mode: "merge" })),
				self.act_button("Open PR", Tone::Plain, Some(Act::Finish { mode: "pr" })),
			]
			.spacing(8),
			text_editor(&self.send)
				.placeholder("Send the agent back with instructions\u{2026}")
				.on_action(Message::SendText)
				.height(Length::Fixed(88.0))
				.padding(10)
				.size(BODY)
				.style(style::editor),
			self.act_button("Send back", Tone::Plain, send),
		]
		.spacing(12);
		if let Some(status) = self.status(&Target::Thread) {
			bar = bar.push(status);
		}
		bar.into()
	}

	fn section<'a>(&'a self, index: usize, section: &'a Section) -> Element<'a, Message> {
		let c = colours();
		let heading = match section {
			Section::Text { heading, .. }
			| Section::Facts { heading, .. }
			| Section::Checks { heading, .. }
			| Section::Flow { heading, .. }
			| Section::Map { heading, .. }
			| Section::Diff { heading, .. }
			| Section::Cards { heading, .. }
			| Section::Works { heading, .. }
			| Section::System { heading, .. }
			| Section::List { heading, .. } => heading.as_deref(),
		};
		let body: Element<'a, Message> = if is_empty(section) {
			text(EMPTY).size(BODY).color(c.muted).into()
		} else {
			match section {
				Section::Text { body, .. } => prose(body),
				Section::Facts { rows, .. } => facts(rows),
				Section::Checks { rows, .. } => checks(rows),
				Section::Works { rows, .. } => self.works(rows),
				Section::System { nodes, edges, brings, takes, no_change, .. } => {
					if *no_change {
						text(NO_SYSTEM_CHANGE).size(BODY).color(c.fg2).into()
					} else {
						self.system(index, nodes, edges, [brings, takes])
					}
				}
				Section::Flow { nodes, edges, .. } => self.flow(nodes, edges),
				Section::Map { groups, links, .. } => map(groups, links),
				Section::Diff { files, .. } => self.diff(index, files),
				Section::Cards { cards, .. } => self.cards(cards),
				Section::List { rows, .. } => list(rows),
			}
		};
		let mut part = Column::new().spacing(16);
		if let Some(heading) = heading {
			part = part.push(text(heading).size(HEADING).color(c.fg).wrapping(text::Wrapping::WordOrGlyph));
		}
		part.push(body).into()
	}

	/// How open a fold is (its opacity), or `None` when it is shut.
	fn fold(&self, fold: &Fold) -> Option<f32> {
		let animation = self.folds.get(fold)?;
		let shown = animation.interpolate(0.0_f32, 1.0, self.now);
		(animation.value() || shown > 0.001).then_some(shown)
	}

	fn works<'a>(&'a self, rows: &'a [WorksRow]) -> Element<'a, Message> {
		let c = colours();
		let mut list = Column::new().spacing(16);
		for row in rows {
			let colour = match row.state {
				WorksState::Passed => c.green,
				WorksState::Failed => c.red,
				WorksState::NotRun => c.muted,
			};
			let mut item = column![
				row![
					badge(works_state(row.state), colour),
					text(&row.label).size(BODY).color(c.fg).wrapping(text::Wrapping::WordOrGlyph).width(Fill),
					text(duration(row.ms)).size(META).color(c.muted),
				]
				.spacing(12)
				.align_y(Alignment::Center)
			]
			.spacing(8);
			if !row.tail.trim().is_empty() {
				item = item.push(code_block(row.tail.trim_end(), 1.0));
			}
			for name in &row.screenshots {
				item = item.push(shot(&self.brief.id, name));
			}
			list = list.push(item);
		}
		list.into()
	}

	fn flow<'a>(&'a self, nodes: &'a [FlowNode], edges: &'a [FlowEdge]) -> Element<'a, Message> {
		self.drawing(Cow::Borrowed(nodes), edges, Presence::default())
	}

	/// A flow's canvas, scrolled sideways when it is wider than the window.
	fn drawing<'a>(&self, nodes: Cow<'a, [FlowNode]>, edges: &[FlowEdge], presence: Presence) -> Element<'a, Message> {
		let shape = shape(&nodes, edges, direction(&nodes, self.width));
		let size = shape.size;
		let elapsed = if self.entering { self.elapsed() } else { f32::MAX };
		let drawing = canvas(FlowCanvas { nodes, shape, elapsed, motion: self.motion, presence })
			.width(Length::Fixed(size.width))
			.height(Length::Fixed(size.height));
		scrollable(drawing)
			.direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new()))
			.width(Fill)
			.into()
	}

	/// A system (0710): the Before / After switch, the union drawn with each part as present as the switch makes it,
	/// then `lists`, Brings and Takes away.
	fn system<'a>(
		&'a self,
		index: usize,
		nodes: &'a [SystemNode],
		edges: &'a [SystemEdge],
		lists: [&'a [SystemRow]; 2],
	) -> Element<'a, Message> {
		let c = colours();
		let side = self.sides.get(&index);
		let after = side.map_or(1.0, |side| side.interpolate(0.0_f32, 1.0, self.now));
		let showing_after = side.is_none_or(Animation::value);
		let choice = |label: &'static str, wanted: bool| {
			let tone = if showing_after == wanted { Tone::Primary } else { Tone::Plain };
			button(text(label).size(LABEL))
				.padding([6, 14])
				.style(style::control(tone))
				.on_press(Message::Side(index, wanted))
		};
		let mut part = column![row![choice("Before", false), choice("After", true)].spacing(8)].spacing(16);
		if !nodes.is_empty() {
			let presence = Presence {
				nodes: nodes.iter().map(|node| presence(node.mark, after, self.motion)).collect(),
				edges: edges.iter().map(|edge| presence(edge.mark, after, self.motion).0).collect(),
			};
			let arrows: Vec<FlowEdge> = edges
				.iter()
				.map(|edge| FlowEdge { from: edge.from.clone(), to: edge.to.clone(), label: edge.label.clone() })
				.collect();
			part = part.push(self.drawing(Cow::Owned(system_nodes(nodes)), &arrows, presence));
		}
		let [brings, takes] = lists;
		for (list, label, rows) in [("brings", "Brings", brings), ("takes", "Takes away", takes)] {
			if rows.is_empty() {
				continue;
			}
			let mut block = column![text(label).size(LABEL).color(c.muted)].spacing(8);
			for (at, row) in rows.iter().enumerate() {
				block = block.push(self.system_row(Fold::Row(index, list, at), row));
			}
			part = part.push(block);
		}
		part.into()
	}

	/// One Brings or Takes away row: its text, then its `✓ proven` disclosure or its place in the code.
	fn system_row<'a>(&'a self, fold: Fold, row: &'a SystemRow) -> Element<'a, Message> {
		let c = colours();
		let mut item = column![text(&row.text).size(BODY).color(c.fg).wrapping(text::Wrapping::WordOrGlyph)].spacing(6);
		if let Some(anchor) = &row.anchor {
			item = item
				.push(text(place(anchor)).size(LABEL).font(mono_font()).color(c.fg2).wrapping(text::Wrapping::Glyph));
		}
		if let Some(ran) = &row.ran {
			item = item.push(
				button(text("\u{2713} proven").size(LABEL).color(c.green))
					.padding(0)
					.style(style::link)
					.on_press(Message::Toggle(fold.clone())),
			);
			if let Some(alpha) = self.fold(&fold) {
				item = item.push(code_block(&proof_text(ran), alpha));
			}
		}
		item.into()
	}

	fn diff<'a>(&'a self, index: usize, files: &'a [DiffFile]) -> Element<'a, Message> {
		let fold = Fold::Diff(index);
		let shown = self.fold(&fold);
		let label = if shown.is_some() { "Hide code".to_owned() } else { diff_summary(files) };
		let toggle = button(text(label).size(LABEL))
			.padding([6, 14])
			.style(style::control(Tone::Plain))
			.on_press(Message::Toggle(fold));
		let Some(alpha) = shown else { return toggle.into() };
		let mut part = column![toggle].spacing(16);
		for file in files {
			part = part.push(diff_file(file, alpha));
		}
		part.into()
	}

	fn cards<'a>(&'a self, cards: &'a [Card]) -> Element<'a, Message> {
		let mut list = Column::new().spacing(16);
		for card in cards {
			list = list.push(self.card(card));
		}
		list.into()
	}

	fn card<'a>(&'a self, card: &'a Card) -> Element<'a, Message> {
		let c = colours();
		let severity = match card.severity {
			Severity::P0 => c.red,
			Severity::P1 => c.yellow,
			Severity::P2 => c.blue,
		};
		let checked = if card.proof.ok {
			badge_owned(format!("checked at {}", card.proof.sha.chars().take(7).collect::<String>()), c.green)
		} else {
			badge("not checked", c.red)
		};
		let head = row![
			badge(card.severity.as_str(), severity),
			badge(card.kind.as_str(), c.purple),
			space::horizontal(),
			badge(verdict(card.verdict), c.muted),
			checked,
		]
		.spacing(8)
		.align_y(Alignment::Center);
		let mut body = column![head].spacing(12);
		if !card.file.is_empty() {
			let place = format!("{}:{}", card.file, card.line);
			body = body.push(text(place).size(LABEL).font(mono_font()).color(c.fg2).wrapping(text::Wrapping::Glyph));
		}
		if !card.quote.is_empty() {
			body = body.push(code_block(&card.quote, 1.0));
		}
		if let Some(ran) = &card.ran {
			let fold = Fold::Proof(card.id.clone());
			let label = if card.file.is_empty() { "Proof" } else { "\u{2713} proven" };
			body = body.push(
				button(text(label).size(LABEL).color(c.green))
					.padding(0)
					.style(style::link)
					.on_press(Message::Toggle(fold.clone())),
			);
			if let Some(alpha) = self.fold(&fold) {
				body = body.push(code_block(&proof_text(ran), alpha));
			}
		}
		let editable = card.published.is_none() && self.brief.kind == "pr" && self.live != Some(false);
		if !editable {
			body = body.push(text(card_text(card)).size(BODY).color(c.fg).wrapping(text::Wrapping::WordOrGlyph));
		}
		if let Some(published) = &card.published {
			body = body.push(
				button(text("Published").size(LABEL))
					.padding(0)
					.style(style::link)
					.on_press(Message::Link(published.url.clone())),
			);
		} else if editable {
			body = body.push(self.card_controls(card));
		}
		container(body).padding(16).width(Fill).style(style::panel).into()
	}

	fn card_controls<'a>(&'a self, card: &'a Card) -> Element<'a, Message> {
		let id = card.id.clone();
		let mut controls = Column::new().spacing(12);
		if let Some(editor) = self.editors.get(&card.id) {
			let words = editor.text();
			let save = Act::Edit { card: id.clone(), text: words.trim().to_owned() };
			let changed = !words.trim().is_empty() && words.trim() != card_text(card);
			let edit_id = id.clone();
			controls = controls.push(
				text_editor(editor)
					.on_action(move |action| Message::Edit(edit_id.clone(), action))
					.height(Length::Fixed(88.0))
					.padding(10)
					.size(BODY)
					.style(style::editor),
			);
			controls = controls.push(self.act_button("Save", Tone::Plain, changed.then_some(save)));
		}
		let instruction = self.instructions.get(&card.id).map_or("", String::as_str);
		let reword = (!instruction.trim().is_empty())
			.then(|| Act::Reword { card: id.clone(), instruction: instruction.trim().to_owned() });
		let input_id = id.clone();
		controls = controls.push(
			row![
				text_input("Reword: shorter, kinder, \u{2026}", instruction)
					.on_input(move |text| Message::Instruction(input_id.clone(), text))
					.padding(8)
					.size(BODY)
					.style(style::input),
				self.act_button("Reword", Tone::Plain, reword),
			]
			.spacing(8)
			.align_y(Alignment::Center),
		);
		if let Some((_, suggested)) = self.suggestion.as_ref().filter(|(card_id, _)| *card_id == card.id) {
			controls = controls.push(self.suggestion_box(&id, suggested));
		}
		let switch_id = id.clone();
		let publish = publishable(card).then_some(Act::Publish { card: id });
		controls = controls.push(
			row![
				pick_list(&CHOICES[..], Some(Choice(card.verdict)), move |choice: Choice| {
					Message::Act(Act::Verdict { card: switch_id.clone(), verdict: choice.0 })
				})
				.text_size(LABEL)
				.padding([6, 10])
				.style(style::switch),
				self.act_button("Publish", Tone::Primary, publish),
			]
			.spacing(8)
			.align_y(Alignment::Center),
		);
		if let Some(status) = self.status(&Target::Card(card.id.clone())) {
			controls = controls.push(status);
		}
		controls.into()
	}

	fn suggestion_box<'a>(&'a self, id: &str, suggested: &'a str) -> Element<'a, Message> {
		let c = colours();
		let accept = Act::Edit { card: id.to_owned(), text: suggested.to_owned() };
		container(
			column![
				text("Suggested wording").size(META).color(c.muted),
				text(suggested).size(BODY).color(c.fg).wrapping(text::Wrapping::WordOrGlyph),
				row![
					self.act_button("Accept", Tone::Primary, Some(accept)),
					button(text("Dismiss").size(LABEL))
						.padding([6, 14])
						.style(style::control(Tone::Plain))
						.on_press(Message::Dismiss),
				]
				.spacing(8),
			]
			.spacing(8),
		)
		.padding(12)
		.width(Fill)
		.style(style::code)
		.into()
	}
}

fn badge<'a>(label: &'a str, colour: Color) -> Element<'a, Message> {
	container(text(label).size(META)).padding([2, 8]).style(style::badge(colour)).into()
}

fn badge_owned<'a>(label: String, colour: Color) -> Element<'a, Message> {
	container(text(label).size(META)).padding([2, 8]).style(style::badge(colour)).into()
}

/// Monospace text on the ground under a hairline, at `alpha` (a fold opening).
fn code_block<'a>(body: &str, alpha: f32) -> Element<'a, Message> {
	let c = colours();
	container(
		text(body.to_owned())
			.size(LABEL)
			.font(mono_font())
			.color(fade(c.fg2, alpha))
			.wrapping(text::Wrapping::WordOrGlyph),
	)
	.padding([10, 12])
	.width(Fill)
	.style(style::code)
	.into()
}

fn prose(body: &str) -> Element<'_, Message> {
	let c = colours();
	let mut column = Column::new().spacing(12).max_width(640);
	for para in paragraphs(body) {
		column = column.push(text(para).size(BODY).color(c.fg).wrapping(text::Wrapping::WordOrGlyph));
	}
	column.into()
}

/// A link that opens in his browser when it is a web link; plain text otherwise.
fn linked<'a>(label: &'a str, url: Option<&'a str>) -> Element<'a, Message> {
	let c = colours();
	match url.and_then(|url| web_link(Some(url))) {
		Some(link) => button(text(label).size(BODY).wrapping(text::Wrapping::WordOrGlyph))
			.padding(0)
			.style(style::link)
			.on_press(Message::Link(link))
			.into(),
		None => text(label).size(BODY).color(c.fg).wrapping(text::Wrapping::WordOrGlyph).into(),
	}
}

fn facts(rows: &[Fact]) -> Element<'_, Message> {
	let c = colours();
	let mut list = Column::new().spacing(8);
	for fact in rows {
		list = list.push(
			row![
				text(&fact.label).size(LABEL).color(c.muted).width(Length::Fixed(176.0)),
				container(linked(&fact.value, fact.url.as_deref())).width(Fill),
			]
			.spacing(16),
		);
	}
	list.into()
}

fn checks(rows: &[CheckRow]) -> Element<'_, Message> {
	let c = colours();
	let mut list = Column::new().spacing(8);
	for check in rows {
		let colour = match check.exit {
			Some(0) => c.green,
			Some(_) => c.red,
			None => c.muted,
		};
		list = list.push(
			row![
				text(&check.command).size(LABEL).font(mono_font()).color(c.fg).width(Fill),
				text(exit(check.exit)).size(LABEL).color(colour),
				text(duration(Some(check.ms))).size(META).color(c.muted),
			]
			.spacing(16)
			.align_y(Alignment::Center),
		);
	}
	list.into()
}

fn list(rows: &[ListRow]) -> Element<'_, Message> {
	let c = colours();
	let mut list = Column::new().spacing(8);
	for item in rows {
		let mut line = row![linked(&item.text, item.url.as_deref())].spacing(12).align_y(Alignment::Center);
		if let Some(note) = &item.note {
			line = line.push(text(note).size(LABEL).color(c.muted));
		}
		list = list.push(line);
	}
	list.into()
}

/// A screenshot of a works row: the image, opened in his image viewer on click; a line when it is missing.
fn shot<'a>(id: &str, name: &str) -> Element<'a, Message> {
	let c = colours();
	match screenshot(id, name).filter(|path| path.is_file()) {
		Some(path) => mouse_area(
			container(image(image::Handle::from_path(&path)).width(Length::Fixed(480.0))).padding(1).style(style::code),
		)
		.interaction(iced::mouse::Interaction::Pointer)
		.on_press(Message::File(path))
		.into(),
		None => text(format!("Screenshot {name} is missing.")).size(LABEL).color(c.muted).into(),
	}
}

/// The bars of one file of a map: lines added in green, removed in red, scaled to the largest file.
fn bars<'a>(added: usize, removed: usize, most: usize) -> Element<'a, Message> {
	let c = colours();
	let width = |lines: usize| if lines == 0 { 0.0 } else { (lines as f32 / most.max(1) as f32 * 120.0).max(2.0) };
	let bar = move |lines: usize, colour: Color| {
		container(space::horizontal()).width(Length::Fixed(width(lines))).height(Length::Fixed(6.0)).style(move |_| {
			container::Style {
				background: Some(colour.into()),
				border: iced::border::rounded(1),
				..container::Style::default()
			}
		})
	};
	row![bar(added, c.green), bar(removed, c.red)].width(Length::Fixed(124.0)).spacing(2).into()
}

fn map<'a>(groups: &'a [MapGroup], links: &'a [MapLink]) -> Element<'a, Message> {
	let c = colours();
	let most = groups.iter().flat_map(|group| &group.items).map(|item| item.added + item.removed).max().unwrap_or(1);
	let label_of = |id: &str| {
		groups
			.iter()
			.flat_map(|group| &group.items)
			.find(|item| item.id == id)
			.map_or(id.to_owned(), |i| i.label.clone())
	};
	let mut columns = row![].spacing(32);
	for group in groups {
		let mut part = column![text(&group.label).size(LABEL).color(c.muted)].spacing(8).width(Length::Fixed(280.0));
		for item in &group.items {
			let linked: Vec<String> = links
				.iter()
				.filter_map(|link| match (link.a == item.id, link.b == item.id) {
					(true, _) => Some(label_of(&link.b)),
					(_, true) => Some(label_of(&link.a)),
					_ => None,
				})
				.collect();
			let mut entry = column![
				row![
					text(&item.label).size(LABEL).font(mono_font()).color(c.fg).width(Fill),
					bars(item.added, item.removed, most),
					text(format!("+{} \u{2212}{}", item.added, item.removed)).size(META).color(c.muted),
				]
				.spacing(8)
				.align_y(Alignment::Center)
			];
			if !linked.is_empty() {
				entry = entry.push(text(format!("\u{2194} {}", linked.join(", "))).size(META).color(c.muted));
			}
			part = part.push(entry);
		}
		columns = columns.push(part);
	}
	scrollable(columns).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new())).into()
}

/// One file of a diff at `alpha`: its path, then each hunk's header and lines with their numbers.
fn diff_file(file: &DiffFile, alpha: f32) -> Element<'_, Message> {
	let c = colours();
	let mut part = column![text(&file.path).size(LABEL).font(mono_font()).color(fade(c.fg, alpha))].spacing(4);
	for hunk in &file.hunks {
		part = part.push(text(&hunk.header).size(META).font(mono_font()).color(fade(c.muted, alpha)));
		let mut lines = Column::new();
		for line in &hunk.lines {
			let (sign, tint) = match line.kind.as_str() {
				"+" => ("+", Some(c.green)),
				"-" => ("\u{2212}", Some(c.red)),
				_ => (" ", None),
			};
			let number = |n: Option<usize>| {
				text(n.map(|n| n.to_string()).unwrap_or_default())
					.size(META)
					.font(mono_font())
					.color(fade(c.muted, alpha))
					.width(Length::Fixed(40.0))
					.align_x(iced::alignment::Horizontal::Right)
			};
			let colour = fade(tint.unwrap_or(c.fg2), alpha);
			let content = row![
				number(line.old),
				number(line.new),
				text(sign).size(LABEL).font(mono_font()).color(colour).width(Length::Fixed(16.0)),
				text(&line.text).size(LABEL).font(mono_font()).color(fade(c.fg, alpha)).wrapping(text::Wrapping::Glyph),
			]
			.spacing(8);
			lines = lines.push(container(content).width(Fill).padding([1, 8]).style(move |_| container::Style {
				background: tint.map(|tint| fade(tint, 0.10 * alpha).into()),
				..container::Style::default()
			}));
		}
		part = part.push(container(lines).width(Fill).style(style::code).padding([6, 0]));
	}
	column![
		rule::horizontal(1).style(move |_| rule::Style {
			color: c.line,
			radius: 0.0.into(),
			fill_mode: rule::FillMode::Full,
			snap: true,
		}),
		part
	]
	.spacing(12)
	.into()
}
