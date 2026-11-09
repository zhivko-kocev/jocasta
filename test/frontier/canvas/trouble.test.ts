// The canvas's trouble line for what the page itself sees (0432, 0581): an earlier run's page, Jocasta not running,
// and whiteboard's files not loading or not matching.
import { expect, test } from "bun:test";
import { canvasTrouble } from "../../../src/frontier/canvas/trouble.ts";

const line = (error: string) => {
	const t = canvasTrouble(error);
	return { text: `${t.reason} ${t.remedy}`, tryAgain: t.transient };
};

test("a refused token (401/403) is an earlier run's page: ask the boss again, no Try again", () => {
	for (const error of ["Jocasta answered 403", "Jocasta answered 401", "whiteboard answered 403"]) {
		expect(line(error)).toEqual({
			text: "This page is from an earlier run of Jocasta. Ask the boss to show the review again.",
			tryAgain: false,
		});
	}
});

test("a fetch that never reached Jocasta says it isn't running, with Try again", () => {
	for (const error of ["Failed to fetch", "NetworkError when attempting to fetch resource.", "Load failed"]) {
		expect(line(error)).toEqual({
			text: "Jocasta isn't running. Run jocasta, then ask the boss to show it.",
			tryAgain: true,
		});
	}
});

test("whiteboard's files that didn't load can be tried again; a canvas build without its mount function can't", () => {
	const notLoaded = {
		text: "Jocasta's whiteboard files didn't load. Try again; if it keeps failing, restart Jocasta.",
		tryAgain: true,
	};
	expect(line("couldn't load http://127.0.0.1:47401/assets/canvas.css")).toEqual(notLoaded);
	expect(
		line("couldn't load http://127.0.0.1:47401/assets/canvas.js: Failed to fetch dynamically imported module"),
	).toEqual(notLoaded);
	expect(line("whiteboard's canvas bundle has no mount function")).toEqual({
		text: "Jocasta's whiteboard files don't match this version. Reinstall Jocasta; in a development checkout, run bun run build.",
		tryAgain: false,
	});
});
