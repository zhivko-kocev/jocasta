// The canvas host's source reads (0393): `reviewSourceQuery`'s precedence and the shared text cache (FE-6).
import { afterEach, expect, test } from "bun:test";
import type { ReviewSourceView } from "../../../src/frontier/canvas/contract.ts";
import { ReviewSource, sourceQuery } from "../../../src/frontier/canvas/source.ts";

const view: ReviewSourceView = {
	reviewId: "r-1",
	version: 4,
	commit: "c0ffee",
	pins: { repositoryId: "acme", head: "h1", base: "b1" },
};

test("sourceQuery: the view's version, a reference's pins before the view's, and a commit only without pins", () => {
	const pins = { repositoryId: "acme", head: "h2" };
	expect(Object.fromEntries(sourceQuery({ view, pins }, { side: "head" }))).toEqual({
		version: "4",
		repositoryId: "acme",
		head: "h2",
		side: "head",
	});
	expect(Object.fromEntries(sourceQuery({ view }))).toEqual({
		version: "4",
		repositoryId: "acme",
		head: "h1",
		base: "b1",
	});
	const unpinned: ReviewSourceView = { reviewId: "r-1", version: 4, commit: "c0ffee" };
	expect(Object.fromEntries(sourceQuery({ view: unpinned }))).toEqual({ version: "4", commit: "c0ffee" });
	expect(Object.fromEntries(sourceQuery({ view: unpinned, commit: "beef" }))).toEqual({ version: "4", commit: "beef" });
	expect(Object.fromEntries(sourceQuery({ view: undefined }))).toEqual({});
});

let stop: (() => void) | undefined;
afterEach(() => {
	stop?.();
	stop = undefined;
});

/** A stand-in for whiteboard's `GET /reviews-api/<id>/file`, counting the reads it answers; `fail` 500s the next one. */
function fakeServer() {
	const state = { reads: [] as string[], fail: false };
	const server = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		fetch(request) {
			const url = new URL(request.url);
			state.reads.push(url.search);
			if (state.fail) {
				state.fail = false;
				return Response.json({ error: "busy" }, { status: 500 });
			}
			return Response.json({ text: `${url.searchParams.get("side")}:${url.searchParams.get("file")}` });
		},
	});
	stop = () => server.stop(true);
	const source = new ReviewSource(
		{
			url: `http://127.0.0.1:${server.port}`,
			token: "t",
			version: "0.2.4",
			canvas: { script: "", styles: [], wasm: "" },
			worker: "",
		},
		"r-1",
	);
	return { state, source };
}

test("a file's text is read once per full query, shared by every reader, and read again for a new view", async () => {
	const { state, source } = fakeServer();
	source.view = { reviewId: "r-1", version: 1, generation: "g1" };
	const [a, b] = await Promise.all([source.text("src/a.ts", "head"), source.text("src/a.ts", "head")]);
	expect([a, b]).toEqual(["head:src/a.ts", "head:src/a.ts"]);
	expect(await source.text("src/a.ts", "base")).toBe("base:src/a.ts");
	expect(await source.text("src/a.ts", "head", { pins: { repositoryId: "acme", head: "h" } })).toBe("head:src/a.ts");
	expect(state.reads).toHaveLength(3);

	// A new generation under the same version is new text.
	source.view = { reviewId: "r-1", version: 1, generation: "g2" };
	await source.text("src/a.ts", "head");
	expect(state.reads).toHaveLength(4);
});

test("a failed read isn't kept: the next reader asks again", async () => {
	const { state, source } = fakeServer();
	state.fail = true;
	await expect(source.text("src/a.ts", "head")).rejects.toThrow("busy");
	expect(await source.text("src/a.ts", "head")).toBe("head:src/a.ts");
	expect(state.reads).toHaveLength(2);
});
