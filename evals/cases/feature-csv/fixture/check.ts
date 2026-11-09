import * as report from "./report.ts";

type ToCsv = (rows: Record<string, string | number>[]) => string;
const toCsv = (report as Record<string, unknown>).toCsv as ToCsv | undefined;
if (typeof toCsv !== "function") {
	console.error("report.ts exports no toCsv");
	process.exit(1);
}

const cases: [string, Record<string, string | number>[], string][] = [
	["plain fields", [{ client: "acme", hours: 3, note: "setup" }], "client,hours,note\nacme,3,setup"],
	[
		"two rows",
		[
			{ a: "x", b: 1 },
			{ a: "y", b: 2 },
		],
		"a,b\nx,1\ny,2",
	],
	["a comma", [{ client: "client-a, west", hours: 1 }], 'client,hours\n"client-a, west",1'],
	["a quote", [{ note: 'said "hi"' }], 'note\n"said ""hi"""'],
	["a newline", [{ note: "line one\nline two" }], 'note\n"line one\nline two"'],
	["an empty field", [{ a: "", b: "z" }], "a,b\n,z"],
];
let failed = 0;
for (const [name, rows, expected] of cases) {
	// Either line ending is fine, and so is a final line break.
	const actual = toCsv(rows).replace(/\r\n/g, "\n").replace(/\n$/, "");
	if (actual !== expected) {
		console.error(`${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
		failed++;
	}
}
if (failed > 0) process.exit(1);
console.log("toCsv: all cases pass");
