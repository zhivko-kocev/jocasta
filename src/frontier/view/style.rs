//! The brief windows' look (0721): the palette file's colours (Tokyo Night without it) and font, 6 px corners,
//! hairlines, the type scale 12/13/15/18/26 and outlined controls.

use iced::widget::{button, container, pick_list, text_editor, text_input};
use iced::{Background, Border, Color, Font, Theme, border, theme};
use ratatui::style::Color as Tui;

use crate::frontier::tui::{font_family, palette};

/// Corners, as SwayFX draws the windows (`corner_radius 6`).
pub const RADIUS: f32 = 6.0;

/// The type scale.
pub const META: f32 = 12.0;
/// Labels, badges, code.
pub const LABEL: f32 = 13.0;
/// Prose.
pub const BODY: f32 = 15.0;
/// Section headings.
pub const HEADING: f32 = 18.0;
/// The brief's title.
pub const TITLE: f32 = 26.0;

/// The colours the windows draw with, from the palette.
#[derive(Debug, Clone, Copy)]
pub struct Colours {
	pub bg: Color,
	pub raised: Color,
	pub panel: Color,
	pub line: Color,
	pub control: Color,
	pub fg: Color,
	pub fg2: Color,
	pub muted: Color,
	pub blue: Color,
	pub green: Color,
	pub yellow: Color,
	pub red: Color,
	pub purple: Color,
}

fn rgb(colour: Tui) -> Color {
	match colour {
		Tui::Rgb(r, g, b) => Color::from_rgb8(r, g, b),
		_ => Color::WHITE,
	}
}

/// The palette this run draws with, as iced colours.
pub fn colours() -> Colours {
	let p = palette();
	Colours {
		bg: rgb(p.bg0),
		raised: rgb(p.bg1),
		panel: rgb(p.bg2),
		line: rgb(p.bg3),
		control: rgb(p.mark3),
		fg: rgb(p.fg),
		fg2: rgb(p.fg2),
		muted: rgb(p.fg3),
		blue: rgb(p.blue),
		green: rgb(p.green),
		yellow: rgb(p.yellow),
		red: rgb(p.red),
		purple: rgb(p.purple),
	}
}

/// `colour` at `alpha` of its opacity.
pub fn fade(colour: Color, alpha: f32) -> Color {
	Color { a: colour.a * alpha, ..colour }
}

/// The window's theme: the palette as iced's.
pub fn theme() -> Theme {
	let c = colours();
	Theme::custom(
		"Jocasta",
		theme::Palette {
			background: c.bg,
			text: c.fg,
			primary: c.blue,
			success: c.green,
			warning: c.yellow,
			danger: c.red,
		},
	)
}

/// Prose: the palette file's font, else the system's sans.
pub fn ui_font() -> Font {
	font_family().map_or(Font::DEFAULT, Font::with_name)
}

/// Code: the palette file's font (ultron's is monospace), else the system's monospace.
pub fn mono_font() -> Font {
	font_family().map_or(Font::MONOSPACE, Font::with_name)
}

fn hairline(colour: Color) -> Border {
	Border { color: colour, width: 1.0, radius: RADIUS.into() }
}

/// How a button stands out: plain, or the primary one of its row (blue).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
	Plain,
	Primary,
}

/// A button: outlined, its tone only on its border and text.
pub fn control(tone: Tone) -> impl Fn(&Theme, button::Status) -> button::Style {
	move |_theme, status| {
		let c = colours();
		let (accent, edge) = match tone {
			Tone::Plain => (c.fg, c.control),
			Tone::Primary => (c.blue, c.blue),
		};
		let base =
			button::Style { background: None, text_color: accent, border: hairline(edge), ..button::Style::default() };
		match status {
			button::Status::Active => base,
			button::Status::Hovered => button::Style { background: Some(Background::Color(c.raised)), ..base },
			button::Status::Pressed => button::Style { background: Some(Background::Color(c.line)), ..base },
			button::Status::Disabled => button::Style { text_color: c.muted, border: hairline(c.line), ..base },
		}
	}
}

/// A link-like button: text only, underlined by colour.
pub fn link(_theme: &Theme, status: button::Status) -> button::Style {
	let c = colours();
	let colour = if status == button::Status::Hovered { c.fg } else { c.blue };
	button::Style { background: None, text_color: colour, border: Border::default(), ..button::Style::default() }
}

/// A panel: one step above the ground, no border.
pub fn panel(_theme: &Theme) -> container::Style {
	container::Style {
		background: Some(Background::Color(colours().raised)),
		border: border::rounded(RADIUS),
		..container::Style::default()
	}
}

/// A block of code or output: the ground under a hairline.
pub fn code(_theme: &Theme) -> container::Style {
	let c = colours();
	container::Style {
		background: Some(Background::Color(c.bg)),
		border: hairline(c.line),
		..container::Style::default()
	}
}

/// A small label in `colour` on a faint ground of it.
pub fn badge(colour: Color) -> impl Fn(&Theme) -> container::Style {
	move |_theme| container::Style {
		text_color: Some(colour),
		background: Some(Background::Color(fade(colour, 0.14))),
		border: border::rounded(RADIUS),
		..container::Style::default()
	}
}

/// A one-line input.
pub fn input(_theme: &Theme, status: text_input::Status) -> text_input::Style {
	let c = colours();
	text_input::Style {
		background: Background::Color(c.bg),
		border: hairline(if matches!(status, text_input::Status::Focused { .. }) { c.blue } else { c.control }),
		icon: c.muted,
		placeholder: c.muted,
		value: c.fg,
		selection: fade(c.blue, 0.35),
	}
}

/// A text box.
pub fn editor(_theme: &Theme, status: text_editor::Status) -> text_editor::Style {
	let c = colours();
	text_editor::Style {
		background: Background::Color(c.bg),
		border: hairline(if matches!(status, text_editor::Status::Focused { .. }) { c.blue } else { c.control }),
		placeholder: c.muted,
		value: c.fg,
		selection: fade(c.blue, 0.35),
	}
}

/// The verdict switch.
pub fn switch(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
	let c = colours();
	pick_list::Style {
		text_color: c.fg,
		placeholder_color: c.muted,
		handle_color: c.muted,
		background: Background::Color(c.bg),
		border: hairline(if matches!(status, pick_list::Status::Opened { .. }) { c.blue } else { c.control }),
	}
}
