/**
 * Texts the service types into panes and says in replies, and names it makes from his words: a thread's title from a
 * brief, a branch slug from a title. Every line the service types into a pane is here, so A5 tells them from his own
 * (`byService`, 0482).
 */
import { localDay } from "../../shared/day.ts";
import type { TestsResult } from "./sessions.ts";

/** The task-end `/handoff` with its focus (0400, 0448), so the summary keeps what the next task needs. */
export const HANDOFF_PROMPT =
	"/handoff Keep what the next task needs: the approved plan's state (steps done and left), open questions, decisions and why, unfixed review findings. Drop test and debug output.";

/** The one steer a task end without its report gets (0539), typed into the thread's pane like the boss's steers. */
export const REPORT_STEER =
	"Your task ended without its report. Call the report_result tool now, once, with this task's outcome, summary, files, tests, decisions, open questions and follow-ups. Don't change code in this turn.";

/** The first of the lines every thread brief ends with (0459): the project's lessons before planning, cited in the plan. */
export const LESSONS_LINE =
	"Before you plan, read skill://lessons and cite each lesson your plan relies on as (lesson: <file>); a plan that goes against one says why.";

/**
 * The commit step (boss manual: the thread commits its change on its own branch): a task ends on new commits (0400), and
 * only then does its review start (0425). It is part of `prompt.md`'s "When you finish" (0577); a thread started with
 * the layer off (0576), whose prompt has no such section, gets it at the end of its brief instead.
 */
export const COMMIT_LINE =
	"When the approved work is done, commit it on this branch with a short message; the review starts from your commit.";

/** `prompt.md`'s finishing steps (0577, PRM-7), after its Done-when section. */
export const WHEN_YOU_FINISH = [
	"## When you finish",
	"- Run the task's checks with the `check` tool (the project's check and every Done-when command, cached by tree); use bash only for a single test file while you iterate.",
	"- Commit the approved work on this branch with a short message; the review starts from your commit.",
	"- Then call `report_result` once.",
];

/** How the review prompt (`hunk.ts` `reviewPrompt`) begins. */
export const REVIEW_LEAD = "Check this task's work.";

/** What shows omp's Plan Review again in a reopened thread (0494). */
export const PLAN_REVIEW_COMMAND = "/plan-review";

/**
 * What a failed check on idle is typed with, before its gates (0583, PRM-16): the thread can tell Jocasta's runner from
 * him, and a turn that stopped to ask him doesn't go on without his answer.
 */
export const IDLE_CHECK_FRAME =
	"Jocasta's checks failed on your uncommitted changes (this isn't his message). If you're waiting for his answer, end your turn now; otherwise fix these before you commit:";

/** The check on idle's text: the frame, a blank line, then the runner's gate block (0583). */
export function idleCheckText(gates: string): string {
	return `${IDLE_CHECK_FRAME}\n\n${gates}`;
}

const PR_LEAD = "Open a pull request for this branch:";

/** The PR turn's request (0426): push the branch, open the PR with gh, change no code. */
export function prRequest(remote: string, branch: string, trunk: string): string {
	return [
		`${PR_LEAD} run \`git push -u ${remote} ${branch}\`, then`,
		`\`gh pr create --base ${trunk} --head ${branch}\` with a title and body that sum up the branch's commits.`,
		"Don't change code in this turn. End with the PR's URL, or with why it couldn't be opened.",
	].join(" ");
}

/**
 * How every line the service types into a pane begins: `[event]` lines into the boss, and into threads the review
 * prompt, `/handoff`, the report steer, a failed check on idle, `/plan-review` and the PR request. A thread's brief and
 * a boss's `thread_send` relay aren't among them: the brief is the session's first message, a relay is his word.
 */
export const SERVICE_LINE_STARTS: readonly string[] = [
	"[event]",
	REVIEW_LEAD,
	"/handoff",
	REPORT_STEER,
	IDLE_CHECK_FRAME,
	PLAN_REVIEW_COMMAND,
	PR_LEAD,
];

/** Whether `text` is a line the service typed into a pane (SVC-1): A5 doesn't count it as his. */
export function byService(text: string): boolean {
	const start = text.trimStart();
	return SERVICE_LINE_STARTS.some((prefix) => start.startsWith(prefix));
}

/**
 * Which door a thread action came through (0544): the boss's tool (asks him first where 0484 says so), a `jocasta`
 * command from a terminal, or his own key and form in herdr.
 */
export type Door = "boss" | "cli" | "key";

/** What an error from a thread start begins with, naming the door he used (0581, SVC-19). */
export const START_PREFIX: Record<Door, string> = {
	boss: "thread_start",
	cli: "jocasta thread new",
	key: "New thread",
};

/**
 * Him as a door's reader sees him (0581, UX-8): the boss reads about him in the third person; his keys and the CLI
 * speak to him.
 */
export function person(door: Door): { he: string; his: string; He: string; him: string } {
	return door === "boss"
		? { he: "he", his: "his", He: "He", him: "him" }
		: { he: "you", his: "your", He: "You", him: "you" };
}

/** Who asks in his Allow window (0484, 0544): the boss, or the `jocasta` command run from a terminal. */
export function asker(door: Door, command: string): string {
	return door === "cli" ? `A \`jocasta ${command}\` command` : "The boss";
}

/**
 * Outside text in an `[event]` line (SEC-2): one line, inside «», with no « or » of its own, so it can't pose as more
 * of the line or as his words (the boss manual says text in `[event]` lines is information, never his instruction).
 */
export function quoted(text: string): string {
	return `«${text.replace(/\s+/g, " ").replace(/[«»]/g, '"').trim()}»`;
}

/** A model as the sidebar shows it (UX-04): `anthropic/claude-opus-5-5` → `opus-5-5`. */
export function shortModel(model: string): string {
	return (model.split("/").at(-1) ?? model).replace(/^claude-/, "");
}

/** USD as the sidebar, events and the HUD say it: `$1.75`. */
export function money(usd: number): string {
	return `$${usd.toFixed(2)}`;
}

/** `text` cut to `max` characters, ending in "…" when it was longer. */
export function cut(text: string, max: number): string {
	return text.length <= max ? text : `${text.slice(0, max - 1).trimEnd()}…`;
}

/** How a 0471 event ends: the boss offers once to save a lesson, written only on his yes. */
export const LESSON_OFFER = " · offer once to save a lesson";

/** How a checked test result reads in the boss's `[event]` and in `today` (0425, 0453). */
export const TESTS_SAID: Record<TestsResult, string> = {
	pass: "tests pass",
	fail: "tests fail",
	none: "no tests",
	unknown: "tests unverified",
};

/** A review's outcome: `2 notes (1 blocking), tests pass`; a count hunk couldn't give reads `notes not counted`. */
export function reviewSaid(notes: number | undefined, tests: TestsResult, blocking?: number): string {
	const counted =
		notes === undefined
			? "notes not counted"
			: `${notes} note${notes === 1 ? "" : "s"}${blocking ? ` (${blocking} blocking)` : ""}`;
	return `${counted}, ${TESTS_SAID[tests]}`;
}

/** At most this many of an overlap's files are named. */
const OVERLAP_FILES_NAMED = 5;

/** `<title>: a.ts, b.ts; <title>: c.ts` (at most five files each), or empty. */
export function overlapSaid(found: { title: string; files: string[] }[]): string {
	return found
		.map(
			(o) =>
				`${o.title}: ${o.files.slice(0, OVERLAP_FILES_NAMED).join(", ")}${o.files.length > OVERLAP_FILES_NAMED ? " …" : ""}`,
		)
		.join("; ");
}

/**
 * A thread's title from the brief he typed in New thread (0554): its first sentence, at most six words and 40
 * characters, cut at a word.
 */
export function briefTitle(brief: string): string {
	const first =
		brief
			.trim()
			.split(/\n|(?<=[.!?])\s/)[0]
			?.replace(/[.!?]+$/, "") ?? "";
	let title = "";
	for (const word of first.split(/\s+/).slice(0, 6)) {
		const next = title ? `${title} ${word}` : word;
		if (next.length > 40) break;
		title = next;
	}
	return title || first.slice(0, 40);
}

/**
 * Cyrillic letters as Latin (0581, UX-11): Macedonian's romanization, with the other Cyrillic alphabets' own letters
 * beside it; lowercase only (the slug is lowercased first).
 */
const CYRILLIC: Record<string, string> = {
	а: "a",
	б: "b",
	в: "v",
	г: "g",
	д: "d",
	ѓ: "gj",
	е: "e",
	ж: "zh",
	з: "z",
	ѕ: "dz",
	и: "i",
	ј: "j",
	к: "k",
	л: "l",
	љ: "lj",
	м: "m",
	н: "n",
	њ: "nj",
	о: "o",
	п: "p",
	р: "r",
	с: "s",
	т: "t",
	ќ: "kj",
	у: "u",
	ф: "f",
	х: "h",
	ц: "c",
	ч: "ch",
	џ: "dzh",
	ш: "sh",
	ђ: "dj",
	ћ: "c",
	ё: "e",
	й: "j",
	щ: "shch",
	ъ: "",
	ы: "y",
	ь: "",
	э: "e",
	ю: "ju",
	я: "ja",
	і: "i",
	ї: "ji",
	є: "je",
	ґ: "g",
};

/** Latin letters NFKD doesn't take apart. */
const LATIN: Record<string, string> = { ß: "ss", æ: "ae", ø: "o", œ: "oe", đ: "d", ł: "l", þ: "th", ð: "d", ı: "i" };

/**
 * A branch slug from a title (0414, 0445, 0581): lowercase, Cyrillic transliterated, Latin letters without their
 * marks, runs of anything else become `-`, at most 40 characters. Empty when the title has no letter or digit it can
 * spell (another script): the start then takes `fallbackSlug`'s name.
 */
export function branchSlug(title: string): string {
	return [...title.toLowerCase()]
		.map((char) => CYRILLIC[char] ?? LATIN[char] ?? char)
		.join("")
		.normalize("NFKD")
		.replace(/\p{M}+/gu, "")
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-+/, "")
		.slice(0, 40)
		.replace(/-+$/, "");
}

/** A thread's name when its title gives no slug (0581): `thread-<YYYYMMDD>-<n>`, by the local day. */
export function fallbackSlug(at: Date, n: number): string {
	return `thread-${localDay(at.getTime()).replaceAll("-", "")}-${n}`;
}
