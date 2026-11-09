// One palette file (0474): palette.css holds Tokyo Night's hex values; everything else reads them from it.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
	CODE,
	LINE_OPACITY,
	MONO_FONT,
	readPalette,
	TOKYO_NIGHT,
	withAlpha,
	withOpacity,
} from "../../../src/frontier/canvas/palette.ts";

const canvas = join(import.meta.dir, "../../../src/frontier/canvas");

test("palette.ts reads Tokyo Night from palette.css (0401's values)", () => {
	expect(TOKYO_NIGHT).toMatchObject({
		bg0: "#1a1b26",
		bg2: "#24283b",
		bg3: "#2d3650",
		fg: "#c0caf5",
		fg2: "#a9b1d6",
		mark3: "#697196",
		comment: "#565f89",
		brightBlack: "#414868",
		blue: "#7aa2f7",
		cyan: "#7dcfff",
		yellow: "#e0af68",
		red: "#f7768e",
		green: "#9ece6a",
		purple: "#bb9af7",
		orange: "#ff9e64",
	});
	expect(CODE.keyword).toBe(TOKYO_NIGHT.purple);
	expect(withAlpha(TOKYO_NIGHT.blue, 0x33)).toBe("#7aa2f733");
	expect(withAlpha(TOKYO_NIGHT.green, 0x14)).toBe("#9ece6a14");
});

test("a palette file missing a colour is refused by name", () => {
	expect(() => readPalette(":root { --bg-0: #1a1b26; }")).toThrow("palette.css has no --bg-1 hex colour");
});

test("Monaco's hairlines and mono face are palette.css's --line-1, --line-2 and page.css's --mono", () => {
	const palette = readFileSync(join(canvas, "palette.css"), "utf8");
	const opacity = (name: string) => Number(palette.match(new RegExp(`${name}:[^;]*/ (\\d+)%`))?.[1]);
	expect({ line1: opacity("--line-1"), line2: opacity("--line-2") }).toEqual(LINE_OPACITY);
	// The bytes Monaco's theme gets: 22% and 40% of 255.
	expect(withOpacity(TOKYO_NIGHT.comment, LINE_OPACITY.line1)).toBe("#565f8938");
	expect(withOpacity(TOKYO_NIGHT.comment, LINE_OPACITY.line2)).toBe("#565f8966");
	const page = readFileSync(join(canvas, "page.css"), "utf8");
	expect(page.match(/--mono:\s*([^;]+);/)?.[1]).toBe(MONO_FONT);
});

test("no other page file holds a colour of its own: the stylesheets, the HTML and Monaco's theme read the palette", () => {
	for (const file of ["page.css", "whiteboard.css", "index.html", "palette.ts", "monaco.ts", "main.ts"]) {
		const text = readFileSync(join(canvas, file), "utf8");
		// Monaco's fully transparent colour is no colour of the palette.
		const own = [...text.matchAll(/#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?\b|rgba?\(\s*\d/g)].filter(
			(m) => m[0] !== "#00000000",
		);
		expect({ file, own: own.map((m) => m[0]) }).toEqual({ file, own: [] });
	}
});
