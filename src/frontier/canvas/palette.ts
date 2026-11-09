/// <reference lib="dom" />
// Tokyo Night for what CSS can't colour (0401, 0411, 0474): the canvas's Monaco theme (`monaco.ts`) and the
// Windows Terminal scheme `jocasta setup` writes. The hex values are read from palette.css, the one palette file; this
// module holds none of its own.
import paletteCss from "./palette.css" with { type: "text" };

/** Each palette colour's custom property in palette.css, by the name the code uses. */
const PROPERTIES = {
	bg0: "--bg-0",
	bg1: "--bg-1",
	bg2: "--bg-2",
	bg3: "--bg-3",
	fg: "--fg",
	fg2: "--fg-2",
	fg3: "--fg-3",
	mark3: "--mark-3",
	comment: "--comment",
	brightBlack: "--bright-black",
	blue: "--blue",
	cyan: "--cyan",
	yellow: "--yellow",
	red: "--red",
	green: "--green",
	purple: "--purple",
	orange: "--orange",
} as const;

/** The palette's colours, by name, as `#rrggbb`. */
export type Palette = Record<keyof typeof PROPERTIES, string>;

/** Reads the palette's hex custom properties from palette.css's text; a missing one is a broken palette file. */
export function readPalette(css: string): Palette {
	const found = new Map<string, string>();
	for (const match of css.matchAll(/(--[\w-]+)\s*:\s*(#[0-9a-fA-F]{6})\s*;/g)) {
		found.set(match[1] as string, (match[2] as string).toLowerCase());
	}
	const palette: Partial<Palette> = {};
	for (const [name, property] of Object.entries(PROPERTIES) as [keyof Palette, string][]) {
		const value = found.get(property);
		if (!value) throw new Error(`palette.css has no ${property} hex colour`);
		palette[name] = value;
	}
	return palette as Palette;
}

/**
 * palette.css's custom properties as text. Bun's page bundle also has palette.css among the page's stylesheets and
 * then hands this text import over as an empty object, so in the page they are read from the stylesheet the page
 * links instead (B-2): the same file's values.
 */
function paletteText(): string {
	if (typeof paletteCss === "string") return paletteCss;
	const root = getComputedStyle(document.documentElement);
	return Object.values(PROPERTIES)
		.map((property) => `${property}: ${root.getPropertyValue(property).trim()};`)
		.join("\n");
}

/** Tokyo Night as palette.css holds it. */
export const TOKYO_NIGHT: Readonly<Palette> = Object.freeze(readPalette(paletteText()));

/** A palette colour with an alpha byte, `#rrggbbaa`, as Monaco takes translucent colours. */
export function withAlpha(colour: string, alpha: number): string {
	return `${colour}${Math.round(alpha).toString(16).padStart(2, "0")}`;
}

/** A palette colour at an opacity in percent, as palette.css writes its translucent colours (`/ 22%`). */
export const withOpacity = (colour: string, percent: number): string => withAlpha(colour, (percent * 255) / 100);

/**
 * The opacities, in percent, of palette.css's hairlines `--line-1` and `--line-2` (the comment colour at 22% and
 * 40%), so Monaco's guides and borders match the page's lines (0474).
 */
export const LINE_OPACITY = { line1: 22, line2: 40 } as const;

/** page.css's `--mono`, for Monaco, which takes its font as an option rather than from the page's CSS. */
export const MONO_FONT = '"Geist Mono", "Cascadia Mono", Consolas, ui-monospace, monospace';

/** The colours of code drawn outside CSS: the canvas's Monaco theme. */
export const CODE = {
	ink: TOKYO_NIGHT.fg,
	quiet: TOKYO_NIGHT.fg2,
	comment: TOKYO_NIGHT.comment,
	keyword: TOKYO_NIGHT.purple,
	string: TOKYO_NIGHT.green,
	number: TOKYO_NIGHT.orange,
	type: TOKYO_NIGHT.cyan,
	error: TOKYO_NIGHT.red,
} as const;
