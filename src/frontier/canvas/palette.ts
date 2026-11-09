/// <reference lib="dom" />
// Tokyo Night for what CSS can't colour (0401, 0411, 0474): the canvas's Monaco theme (`monaco.ts`) and the
// Windows Terminal scheme `jocasta setup` writes. The hex values are read from palette.css, the one palette file; this
// module holds none of its own.
import paletteCss from "./palette.css" with { type: "text" };

/** The palette's colours, by name, as `#rrggbb`. */
export interface Palette {
	bg0: string;
	bg1: string;
	bg2: string;
	bg3: string;
	fg: string;
	fg2: string;
	fg3: string;
	mark3: string;
	comment: string;
	brightBlack: string;
	blue: string;
	cyan: string;
	yellow: string;
	red: string;
	green: string;
	purple: string;
	orange: string;
}

/** Each palette name's custom property in palette.css. */
const PROPERTIES: Record<keyof Palette, string> = {
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
};

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
