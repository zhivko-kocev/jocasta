import { expect, test } from "bun:test";
import { POPUP_PATH, type PopupView, TOKEN_COOKIE } from "../../../src/frontier/protocol.ts";
import type { Popup } from "../../../src/frontier/server/popups.ts";
import {
	ALLOW,
	allowQuestion,
	CANCEL,
	closeQuestion,
	DENY,
	finishQuestion,
	MERGE,
	newThreadQuestion,
	Popups,
	PR,
	START,
	STOP,
	stopQuestion,
	WRONG_CODES,
} from "../../../src/frontier/server/popups.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { Page, startTestServer, type TestServer, withServer } from "./helpers.ts";

useTempJocasta();

const ALLOW_BOX: PopupView = {
	title: "Jocasta: Allow or Deny",
	message: "Remove the leftover worktree acme-api/retry?",
	detail: "Its branch stays.",
	choices: ["Allow", "Deny"],
	fallback: "Deny",
};

/** Fetches a question route as what shows it would: with the service's token, as a bearer or the cookie. */
function popupFetch(
	server: TestServer,
	path: string,
	init: { method?: string; body?: string; token?: string; bearer?: boolean } = {},
): Promise<Response> {
	const { token = server.token, bearer = false, ...rest } = init;
	return fetch(`http://127.0.0.1:${server.port}${POPUP_PATH}${path}`, {
		...rest,
		headers: {
			...(bearer ? { authorization: `Bearer ${token}` } : { cookie: `${TOKEN_COOKIE}=${token}` }),
			"content-type": "application/json",
		},
	});
}

const choose = (choice: unknown) => ({ method: "POST", body: JSON.stringify({ choice }) });

test("a question's texts and answer need the service's token and its one-time code; the answer is taken once (0519)", async () => {
	const shown: Popup[] = [];
	await withServer(
		async (server) => {
			const page = await Page.open(server);
			const popup = server.question(ALLOW_BOX);
			expect(popup.code).toMatch(/^[0-9a-f]{64}$/);
			// What shows it gets the code; nothing else does.
			expect(shown).toEqual([popup]);
			expect(server.question(ALLOW_BOX).code).not.toBe(popup.code);

			// Without the token, with a wrong code or none, nothing is served.
			expect((await popupFetch(server, popup.code, { token: "wrong" })).status).toBe(403);
			expect((await popupFetch(server, "0".repeat(64))).status).toBe(403);
			expect((await popupFetch(server, "")).status).toBe(403);
			// One character off: the first one changed, whatever it was.
			const nearMiss = `${popup.code.startsWith("0") ? "1" : "0"}${popup.code.slice(1)}`;
			expect((await popupFetch(server, nearMiss, choose("Allow"))).status).toBe(403);
			expect((await popupFetch(server, popup.code, { ...choose("Allow"), token: "wrong" })).status).toBe(403);
			expect(popup.open).toBe(true);

			// With both: its texts, by cookie or bearer.
			const texts = await popupFetch(server, popup.code, { bearer: true });
			expect(texts.status).toBe(200);
			expect(await texts.json()).toEqual(ALLOW_BOX);

			// Only its own choices answer it; a choice it doesn't offer leaves it waiting.
			expect((await popupFetch(server, popup.code, choose(STOP))).status).toBe(400);
			expect((await popupFetch(server, popup.code, choose(true))).status).toBe(400);
			expect(popup.open).toBe(true);
			expect((await popupFetch(server, popup.code, choose("Allow"))).status).toBe(204);
			expect(await popup.answer).toBe("Allow");

			// The code dies with its answer: no second answer, no texts.
			expect((await popupFetch(server, popup.code, choose("Deny"))).status).toBe(403);
			expect((await popupFetch(server, popup.code)).status).toBe(403);
			expect(await popup.answer).toBe("Allow");

			// The state feed never hears of it.
			expect(page.received.some((m) => JSON.stringify(m).includes(popup.code))).toBe(false);
			page.close();
		},
		{ onQuestion: (popup) => void shown.push(popup) },
	);
});

test("a question closed unanswered is its fallback, and so is every open one when the server stops (0519)", async () => {
	const server = await startTestServer();
	let stopped = false;
	try {
		const cancel = server.question(stopQuestion({ working: 1, awaiting: 0 }));
		cancel.close();
		expect(await cancel.answer).toBe(CANCEL);
		expect(cancel.open).toBe(false);
		expect((await popupFetch(server, cancel.code, choose(STOP))).status).toBe(403);

		const deny = server.question(ALLOW_BOX);
		const aborted = new AbortController();
		const gone = server.question(ALLOW_BOX, aborted.signal);
		aborted.abort();
		expect(await gone.answer).toBe("Deny");
		await server.stop();
		stopped = true;
		expect(await deny.answer).toBe("Deny");
	} finally {
		if (!stopped) await server.stop();
	}
});

test("the close question counts working and awaiting threads, Stop everything or Cancel (0438, 0549)", () => {
	expect(closeQuestion({ working: 1, awaiting: 0 })).toBe("Stop 1 working thread? Their sessions are saved.");
	expect(closeQuestion({ working: 0, awaiting: 2 })).toBe("Stop 2 threads awaiting you? Their sessions are saved.");
	expect(closeQuestion({ working: 2, awaiting: 1 })).toBe(
		"Stop 2 working threads and 1 awaiting you? Their sessions are saved.",
	);
	expect(stopQuestion({ working: 1, awaiting: 1 }, "ABC")).toMatchObject({
		choices: [STOP, CANCEL],
		fallback: CANCEL,
		code: "ABC",
	});
});

test("Popups: each code answers once, with one of its own choices; closeAll ends the rest unanswered", async () => {
	const popups = new Popups();
	const a = popups.open(ALLOW_BOX);
	const b = popups.open(stopQuestion({ working: 1, awaiting: 0 }));
	expect(popups.list()).toEqual([a, b]);
	expect(popups.view(a.code)).toEqual(ALLOW_BOX);
	expect(popups.answer(a.code, "Maybe")).toBe("invalid");
	expect(popups.answer(b.code, STOP)).toBe("wrong");
	expect(popups.answer(a.code, "Deny")).toBe("taken");
	expect(popups.answer(a.code, "Allow")).toBe("unknown");
	expect(popups.view(a.code)).toBeUndefined();
	expect(await a.answer).toBe("Deny");
	popups.closeAll();
	expect(await b.answer).toBe(CANCEL);
	expect(popups.list()).toEqual([]);
	expect(popups.answer(b.code, STOP)).toBe("unknown");
});

test("Allow takes its three letters only: Enter's bare Allow and wrong letters don't allow, the third wrong code denies (0558)", async () => {
	const popups = new Popups();
	const view = allowQuestion({ message: "Remove it?", detail: "Its branch stays." }, "KQX");
	expect(view).toMatchObject({ choices: [ALLOW, DENY], fallback: DENY, code: "KQX" });
	expect(allowQuestion({ message: "m", detail: "d" }).code).toMatch(/^[A-HJ-NP-Z]{3}$/);
	const right = popups.open(view);
	expect(popups.answer(right.code, { choice: ALLOW })).toBe("wrong");
	expect(popups.answer(right.code, { choice: ALLOW, typed: "KQZ" })).toBe("wrong");
	expect(right.open).toBe(true);
	expect(popups.answer(right.code, { choice: ALLOW, typed: "kqx" })).toBe("taken");
	expect(await right.answer).toBe(ALLOW);

	const guessed = popups.open(view);
	for (const typed of ["AAA", "BBB"]) expect(popups.answer(guessed.code, { choice: ALLOW, typed })).toBe("wrong");
	expect(popups.answer(guessed.code, { choice: ALLOW, typed: "CCC" })).toBe("taken");
	expect(await guessed.answer).toBe(DENY);
	expect(WRONG_CODES).toBe(3);
});

test("Finish's Merge and PR and the close question's Stop everything take the code; Cancel doesn't; three wrong cancel (0568)", async () => {
	const popups = new Popups();
	const finish = popups.open(finishQuestion("acme-api/retry", "2 commits on feature/retry, onto main.", "KQX"));
	expect(finish.view).toMatchObject({ choices: [MERGE, PR, CANCEL], fallback: CANCEL, code: "KQX" });
	expect(finishQuestion("a", "d").code).toMatch(/^[A-HJ-NP-Z]{3}$/);
	expect(popups.answer(finish.code, { choice: MERGE })).toBe("wrong");
	expect(popups.answer(finish.code, { choice: PR, typed: "KQZ" })).toBe("wrong");
	expect(popups.answer(finish.code, { choice: PR, typed: "kqx" })).toBe("taken");
	expect(await finish.answer).toBe(PR);

	const guessed = popups.open(finishQuestion("acme-api/retry", "d", "KQX"));
	for (const typed of ["AAA", "BBB"]) expect(popups.answer(guessed.code, { choice: MERGE, typed })).toBe("wrong");
	expect(popups.answer(guessed.code, { choice: MERGE, typed: "CCC" })).toBe("taken");
	expect(await guessed.answer).toBe(CANCEL);

	const cancel = popups.open(finishQuestion("acme-api/retry", "d", "KQX"));
	expect(popups.answer(cancel.code, { choice: CANCEL })).toBe("taken");
	expect(await cancel.answer).toBe(CANCEL);

	const stop = popups.open(stopQuestion({ working: 1, awaiting: 0 }, "XYZ"));
	expect(popups.answer(stop.code, STOP)).toBe("wrong");
	expect(popups.answer(stop.code, { choice: STOP, typed: "xyz" })).toBe("taken");
	expect(await stop.answer).toBe(STOP);
	const kept = popups.open(stopQuestion({ working: 1, awaiting: 0 }, "XYZ"));
	for (const typed of ["AAA", "BBB", "CCC"]) popups.answer(kept.code, { choice: STOP, typed });
	expect(await kept.answer).toBe(CANCEL);
});

test("New thread takes a listed project and a brief, without a code (0554)", async () => {
	const popups = new Popups();
	const thread = popups.open(newThreadQuestion(["acme-api", "client-a"], "client-a"));
	expect(thread.view.form).toEqual({ projects: ["acme-api", "client-a"], project: "client-a" });
	expect(popups.answer(thread.code, { choice: START, project: "other", brief: "Fix it." })).toBe("invalid");
	expect(popups.answer(thread.code, { choice: START, project: "acme-api", brief: "  " })).toBe("invalid");
	expect(popups.answer(thread.code, { choice: START, project: "acme-api", brief: " Fix the retry. " })).toBe("taken");
	expect(await thread.answer).toBe(START);
	expect(thread.fields).toEqual({ project: "acme-api", brief: "Fix the retry." });
});
