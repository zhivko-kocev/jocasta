import { type Static, Type } from "typebox";
import type * as TypeboxValue from "typebox/value";

/**
 * `report_result` (0539, K4): what a thread hands in when its task ends, one schema that `jocasta mcp` lists as the
 * tool's input and the service validates before storing it with the thread. Its `tests` are the thread's claim, never
 * the review's proof (0453).
 */
export const REPORT_RESULT = "report_result";

const Text = (description: string) => Type.String({ minLength: 1, description });

export const ReportSchema = Type.Object(
	{
		outcome: Type.Union([Type.Literal("done"), Type.Literal("blocked"), Type.Literal("failed")], {
			description: "done, blocked or failed",
		}),
		summary: Type.String({ minLength: 1, maxLength: 200, description: "What the task did, at most 200 characters" }),
		files: Type.Array(Text("A file path"), { description: "The files the task changed" }),
		tests: Type.Array(
			Type.Object(
				{ command: Text("The test command as you ran it"), passed: Type.Boolean({ description: "Whether it passed" }) },
				{ additionalProperties: false },
			),
			{ description: "The tests you ran and whether each passed" },
		),
		decisions: Type.Array(
			Type.Object(
				{
					question: Text("What had to be decided"),
					choice: Text("What you chose"),
					why: Text("Why"),
				},
				{ additionalProperties: false },
			),
			{ description: "Choices you settled that later work shouldn't reopen" },
		),
		questions: Type.Array(Text("A question"), { description: "Open questions for him" }),
		followUps: Type.Array(Text("A follow-up"), { description: "Work left for later" }),
	},
	{ additionalProperties: false },
);

export type ThreadReport = Static<typeof ReportSchema>;

/** The tool's description as thread panes see it. */
export const REPORT_DESCRIPTION =
	"Hand in this task's report once its work is committed.\nGive the outcome, a summary of at most 200 characters, the files changed, the tests you ran, the decisions you settled, open questions for him and follow-ups. Jocasta stores it with the thread and its review reads it; a test you mark passed is your claim, the review runs its own.";

/**
 * typebox's value module, loaded on the first report checked (PERF-6): `jocasta mcp` and the plugin roles import this
 * file for the schema and texts, and shouldn't parse the validator at start.
 */
let values: typeof TypeboxValue.Value | undefined;

/** The report, or the first reason it doesn't fit the schema (`report_result: <path> <message>`). */
export function parseReport(value: unknown): { report: ThreadReport } | { problem: string } {
	values ??= (require("typebox/value") as typeof TypeboxValue).Value;
	if (values.Check(ReportSchema, value)) return { report: value };
	const errors = [...values.Errors(ReportSchema, value)];
	// An unknown key yields a bare "schema is false" for the key and a clearer error for its parent; report the latter.
	const first = errors.find((e) => e.keyword !== "boolean") ?? errors[0];
	if (!first) return { problem: `${REPORT_RESULT}: the report doesn't fit its schema` };
	const extra = first.keyword === "additionalProperties" ? (first.params as { additionalProperties?: string[] }) : {};
	const unknown = extra.additionalProperties?.[0];
	const at = first.instancePath.replace(/^\//, "").replace(/\//g, ".") || "the report";
	if (unknown !== undefined) return { problem: `${REPORT_RESULT}: unknown field ${unknown} in ${at}` };
	return { problem: `${REPORT_RESULT}: ${at} ${first.message}` };
}

/** The report as one line (task file, review prompt, `thread_status`): `outcome done · "…" · tests 1/1 · decisions: q = c`. */
export function reportLine(report: ThreadReport): string {
	const passed = report.tests.filter((t) => t.passed).length;
	const flat = (text: string) => text.replace(/\s+/g, " ").trim();
	return [
		`outcome ${report.outcome}`,
		JSON.stringify(flat(report.summary)),
		`tests ${passed}/${report.tests.length}`,
		...(report.files.length > 0 ? [`files: ${report.files.map(flat).join(", ")}`] : []),
		...(report.decisions.length > 0
			? [`decisions: ${report.decisions.map((d) => `${flat(d.question)} = ${flat(d.choice)}`).join("; ")}`]
			: []),
		...(report.questions.length > 0 ? [`questions: ${report.questions.map(flat).join("; ")}`] : []),
		...(report.followUps.length > 0 ? [`follow-ups: ${report.followUps.map(flat).join("; ")}`] : []),
	].join(" · ");
}
