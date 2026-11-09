import { jsonLines } from "./timing.ts";

/**
 * omp session files (`~/.omp/agent/sessions/<cwd>/<time>_<id>.jsonl`, omp 18.12.6): one entry per line with an ISO
 * `timestamp`; `message` entries carry `{role, content, timestamp?}`. Times follow `sessions.ts` `entryTime`: the
 * message's own epoch-ms `timestamp` when non-zero, else the entry's ISO time.
 */

type Entry = Record<string, unknown> & { type?: string; timestamp?: unknown; mode?: unknown };
type Message = Record<string, unknown> & { role?: string; content?: unknown; timestamp?: unknown };

const asObject = (value: unknown): Record<string, unknown> | undefined =>
	typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : undefined;

function entryTime(entry: Entry, message?: Message): number {
	if (typeof message?.timestamp === "number" && message.timestamp > 0) return message.timestamp;
	return typeof entry.timestamp === "string" ? Date.parse(entry.timestamp) : Number.NaN;
}

function messages(text: string): { at: number; entry: Entry; message: Message }[] {
	return jsonLines<Entry>(text).flatMap((entry) => {
		if (entry.type !== "message") return [];
		const message = asObject(entry.message) as Message | undefined;
		return message ? [{ at: entryTime(entry, message), entry, message }] : [];
	});
}

/** The session's start: its header's time. */
export function sessionStart(text: string): number {
	const header = jsonLines<Entry>(text).find((entry) => entry.type === "session");
	return header && typeof header.timestamp === "string" ? Date.parse(header.timestamp) : Number.NaN;
}

/** When the agent asked him something: assistant messages with an `ask` tool call (`sessions.ts` reads the same). */
export function askTimes(text: string): number[] {
	return messages(text)
		.filter(
			({ message }) =>
				message.role === "assistant" &&
				Array.isArray(message.content) &&
				message.content.some((part) => asObject(part)?.type === "toolCall" && asObject(part)?.name === "ask"),
		)
		.map(({ at }) => at);
}

/** When a plan became ready for his review: a successful `write` result proposing the plan (`sessions.ts` `proposedPlan`). */
export function planReadyTimes(text: string): number[] {
	return messages(text)
		.filter(({ message }) => {
			if (message.role !== "toolResult" || message.toolName !== "write" || message.isError === true) return false;
			const xdev = asObject(asObject(message.details)?.xdev);
			return xdev?.tool === "propose" && xdev.mode === "execute";
		})
		.map(({ at }) => at);
}

/** When he left plan mode after a plan was ready: `mode_change` entries away from `plan` (his approval). */
export function planApprovalTimes(text: string): number[] {
	const ready = planReadyTimes(text);
	return jsonLines<Entry>(text)
		.filter((entry) => entry.type === "mode_change" && entry.mode !== "plan")
		.map((entry) => entryTime(entry))
		.filter((at) => ready.some((readyAt) => readyAt <= at));
}

export interface UserMessage {
	at: number;
	text: string;
}

/** The user-role messages that aren't synthetic, with their text. */
export function userMessages(text: string): UserMessage[] {
	return messages(text)
		.filter(({ message }) => message.role === "user" && message.synthetic !== true)
		.map(({ at, message }) => ({
			at,
			text:
				typeof message.content === "string"
					? message.content
					: Array.isArray(message.content)
						? message.content
								.map((part) => asObject(part))
								.filter((part) => part?.type === "text" && typeof part.text === "string")
								.map((part) => part?.text as string)
								.join("\n")
						: "",
		}));
}
