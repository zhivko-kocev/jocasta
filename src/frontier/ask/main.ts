// `jocasta plugin ask`: the `ask` popup (0553, 0558, 0563, 0568), an OpenTUI program herdr opens in front with the
// question's one-time code in `JOCASTA_POPUP`. It reads the question, holds the service's held request while it shows
// it (closing the popup any other way ends that request, which takes the fallback: Deny, Cancel), posts his answer and
// exits; it also exits when the service ends the question (answered, withdrawn, Jocasta closing).
import { createCliRenderer, TextRenderable } from "@opentui/core";
import { TOKYO_NIGHT } from "../canvas/palette.ts";
import { POPUP_ENV, type PopupAnswer } from "../protocol.ts";
import { answerQuestion, holdQuestion, questionView } from "../today/api.ts";
import { styled } from "../today/main.ts";
import { askLines, askState, paste, press } from "./view.ts";

/** Runs the popup until the question is answered or over; resolves its exit code. */
export async function runAsk(): Promise<number> {
	const code = process.env[POPUP_ENV] ?? "";
	const view = code ? await questionView(code) : "No question was handed to this popup.";
	const renderer = await createCliRenderer({ exitOnCtrlC: false, useMouse: false, backgroundColor: TOKYO_NIGHT.bg0 });
	const text = new TextRenderable(renderer, { content: "" });
	renderer.root.add(text);
	const { promise: over, resolve: end } = Promise.withResolvers<number>();
	const width = () => Math.max(20, renderer.terminalWidth - 2);
	if (typeof view === "string") {
		text.content = styled([[{ text: view, fg: TOKYO_NIGHT.yellow }]]);
		renderer.requestRender();
		setTimeout(() => end(1), 2_000);
	} else {
		const state = askState(view);
		const draw = () => {
			text.content = styled(askLines(state, width()));
			renderer.requestRender();
		};
		const release = holdQuestion(code, () => end(0));
		let sending = false;
		const send = async (answer: PopupAnswer | undefined) => {
			if (!answer || sending) return;
			sending = true;
			const result = await answerQuestion(code, answer);
			sending = false;
			if (result === "taken") {
				release();
				end(0);
				return;
			}
			state.typed = "";
			state.note = result === "wrong" ? "Wrong code; nothing was done." : result;
			draw();
		};
		renderer.keyInput.on("keypress", (key) => {
			const answer = press(state, key);
			draw();
			void send(answer);
		});
		renderer.keyInput.on("paste", (event) => {
			paste(state, new TextDecoder().decode(event.bytes));
			draw();
		});
		draw();
	}
	for (const signal of ["SIGTERM", "SIGHUP", "SIGINT"] as const) process.on(signal, () => end(0));
	const exitCode = await over;
	renderer.destroy();
	return exitCode;
}
