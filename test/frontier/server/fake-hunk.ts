// A stand-in for hunk's binary in the service's tests: `session comment list --repo <path> --json` prints two live
// notes in hunk 0.23.0's shape (`{comments: [...]}`); `daemon serve` answers /health on `HUNK_MCP_PORT` until ended;
// anything else exits 0 silently.
const args = process.argv.slice(2);
if (args.join(" ") === "daemon serve") {
	Bun.serve({
		port: Number(process.env.HUNK_MCP_PORT),
		hostname: "127.0.0.1",
		fetch: () => Response.json({ ok: true }),
	});
}
if (args.slice(0, 3).join(" ") === "session comment list" && args.includes("--json")) {
	const note = (line: number) => ({
		commentId: `mcp:test:${line}`,
		filePath: "README.md",
		hunkIndex: 0,
		side: "new",
		line,
		summary: `Note on line ${line}.`,
		author: "reviewer",
	});
	process.stdout.write(`${JSON.stringify({ comments: [note(1), note(2)] })}\n`);
}
