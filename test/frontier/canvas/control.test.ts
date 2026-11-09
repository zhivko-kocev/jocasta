// whiteboard's control stream (0393): the server-sent event framing the canvas host reads, and the menu's keys.
import { expect, test } from "bun:test";
import { sseFrames } from "../../../src/frontier/canvas/control.ts";
import { MENU_MOVES } from "../../../src/frontier/canvas/menu.ts";

/** A byte stream that delivers `chunks` one read at a time. */
function stream(chunks: string[]): ReadableStream<Uint8Array> {
	const encoder = new TextEncoder();
	return new ReadableStream({
		start(controller) {
			for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
			controller.close();
		},
	});
}

async function frames(chunks: string[]): Promise<string[]> {
	return Array.fromAsync(sseFrames(stream(chunks)));
}

test("frames end at a blank line, LF or CRLF, and keep-alives without data are skipped", async () => {
	expect(
		await frames(['data: {"id":"1"}\n\n', ": keep-alive\r\n\r\n", 'event: desktop-verb\r\ndata: {"id":"2"}\r\n\r\n']),
	).toEqual(['{"id":"1"}', '{"id":"2"}']);
});

test("a frame split across chunks, even inside its line break, is read whole", async () => {
	expect(await frames(["da", 'ta: {"id":', '"1"}\r', "\n\r", "\ndata: x\n", "\n"])).toEqual(['{"id":"1"}', "x"]);
});

test("a frame's data lines join with newlines, and an unfinished frame at the end is dropped", async () => {
	expect(await frames(["data: one\ndata:two\n\n", "data: cut off"])).toEqual(["one\ntwo"]);
});

test("menu keys move through the enabled items, wrapping, and Home and End jump", () => {
	const { ArrowDown, ArrowUp, Home, End } = MENU_MOVES;
	expect([ArrowDown?.(0, 3), ArrowDown?.(2, 3), ArrowUp?.(0, 3), ArrowUp?.(2, 3)]).toEqual([1, 0, 2, 1]);
	expect([Home?.(2, 3), End?.(0, 3)]).toEqual([0, 2]);
	expect(MENU_MOVES.Enter).toBeUndefined();
});
