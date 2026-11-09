// A code peek's model (0393, 0411): the authored ranges merged per side, each read on its own side (FE-2).
import { expect, test } from "bun:test";
import { slices } from "../../../src/frontier/canvas/peek.ts";

const file = (tag: string, n: number) => Array.from({ length: n }, (_, i) => `${tag}${i + 1}`).join("\n");

test("ranges merge when they touch or overlap, in file order, with one ⋯ line between blocks", () => {
	const sliced = slices(
		{ head: file("h", 20) },
		[
			{ startLine: 10, endLine: 11 },
			{ startLine: 2, endLine: 3 },
			{ startLine: 4, endLine: 5 },
			{ startLine: 11, endLine: 12 },
		],
		"head",
	);
	expect(sliced.text.split("\n")).toEqual(["h2", "h3", "h4", "h5", "", "h10", "h11", "h12"]);
	expect(sliced.lines.map((l) => l?.line ?? "⋯")).toEqual([2, 3, 4, 5, "⋯", 10, 11, 12]);
});

test("ranges past the file are clipped to it, and one wholly past it shows nothing; no ranges shows the whole file", () => {
	const sliced = slices(
		{ base: file("b", 5) },
		[
			{ startLine: 0, endLine: 1 },
			{ startLine: 4, endLine: 9 },
			{ startLine: 7, endLine: 8 },
		],
		"base",
	);
	expect(sliced.text.split("\n")).toEqual(["b1", "", "b4", "b5"]);
	expect(slices({ base: file("b", 3) }, [], "base").text).toBe("b1\nb2\nb3");
});

test("a peek citing both sides reads each range on its own side: the default side's blocks first, numbered per side", () => {
	const sliced = slices(
		{ base: file("b", 5), head: file("h", 6) },
		[
			{ startLine: 2, endLine: 2, side: "base" },
			{ startLine: 2, endLine: 3 },
			{ startLine: 5, endLine: 5, side: "head" },
		],
		"head",
	);
	expect(sliced.text.split("\n")).toEqual(["h2", "h3", "", "h5", "", "b2"]);
	expect(sliced.lines).toEqual([
		{ side: "head", line: 2 },
		{ side: "head", line: 3 },
		null,
		{ side: "head", line: 5 },
		null,
		{ side: "base", line: 2 },
	]);
	// With base as the default side, base's blocks come first.
	expect(
		slices(
			{ base: "b1\nb2", head: "h1\nh2" },
			[
				{ startLine: 1, endLine: 1, side: "head" },
				{ startLine: 2, endLine: 2 },
			],
			"base",
		).text,
	).toBe("b2\n\nh1");
});
