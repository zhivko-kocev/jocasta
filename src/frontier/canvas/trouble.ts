// Why the canvas page couldn't start (0432, 0552), in plain words: what `GET /whiteboard/connection` or loading
// whiteboard's canvas said, turned into a reason, what he can do, and whether trying again can help (0581).

/** The canvas's trouble, plainly. */
export interface CanvasTrouble {
	reason: string;
	remedy: string;
	/** Trying again on the page can help. */
	transient: boolean;
}

/** Why the canvas couldn't start, in plain words, what he can do, and whether trying again can help (0432, 0581). */
export function canvasTrouble(error: string): CanvasTrouble {
	// `loadCanvas`: a stylesheet or the canvas script didn't load (its message names the file, so this goes first).
	if (/^couldn't load /.test(error)) {
		return {
			reason: "Jocasta's whiteboard files didn't load.",
			remedy: "Try again; if it keeps failing, restart Jocasta.",
			transient: true,
		};
	}
	// The canvas build has no `mountReviewCanvas`: the page and whiteboard's build are from different versions.
	if (/no mount function/i.test(error)) {
		return {
			reason: "Jocasta's whiteboard files don't match this version.",
			remedy: "Reinstall Jocasta; in a development checkout, run bun run build.",
			transient: false,
		};
	}
	// A 401/403: the page's cookie holds an earlier run's token (a new one each run), so trying again can't help.
	if (/\banswered 40[13]\b/.test(error)) {
		return {
			reason: "This page is from an earlier run of Jocasta.",
			remedy: "Ask the boss to show the review again.",
			transient: false,
		};
	}
	// The browser's fetch rejected (Edge and Chrome, Firefox, Safari): nothing answers on Jocasta's port.
	if (/^(Failed to fetch|NetworkError when attempting to fetch resource|Load failed)/.test(error)) {
		return {
			reason: "Jocasta isn't running.",
			remedy: "Run jocasta, then ask the boss to show it.",
			transient: true,
		};
	}
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
