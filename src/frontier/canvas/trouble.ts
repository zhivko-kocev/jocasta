// Why the canvas page couldn't start (0432, 0552), in plain words: what `GET /whiteboard/connection` or loading
// whiteboard's canvas said, turned into a reason, what he can do, and whether trying again can help.

/** The canvas's trouble, plainly. */
export interface CanvasTrouble {
	reason: string;
	remedy: string;
	/** Trying again on the page can help. */
	transient: boolean;
}

/** Why the canvas couldn't start, in plain words, what he can do, and whether trying again can help (0432). */
export function canvasTrouble(error: string): CanvasTrouble {
	if (/isn't built|ENOENT|no such file|not found|manifest|cannot find/i.test(error)) {
		return {
			reason: "Jocasta's whiteboard files are missing.",
			remedy: "Reinstall Jocasta; in a development checkout, run bun run build.",
			transient: false,
		};
	}
	if (/is off/i.test(error)) {
		return {
			reason: "The canvas is off for this run of Jocasta.",
			remedy: "Restart Jocasta, then open the review again.",
			transient: false,
		};
	}
	if (/stopped answering/i.test(error)) {
		return {
			reason: "whiteboard's server stopped answering.",
			remedy: "Try again; if it keeps failing, restart Jocasta.",
			transient: true,
		};
	}
	return {
		reason: "whiteboard's server didn't start.",
		remedy: "Try again; if it keeps failing, restart Jocasta.",
		transient: true,
	};
}
