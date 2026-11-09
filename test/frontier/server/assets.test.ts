// The app's server bundle (0367): a Bun.build of the server with the page inside serves the page from any working
// directory, since the bundle's file paths resolve from the bundle, not the cwd.
import { expect, test } from "bun:test";
import { mkdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { assetCacheControl } from "../../../src/frontier/server/assets.ts";
import { useTempJocasta } from "../../shared/fixture.ts";

const env = useTempJocasta();
const REPO = resolve(import.meta.dir, "../../..");

test("a bundled server serves its page files from another working directory", async () => {
	// In memory, rooted at the repo: with an `outdir`, Bun writes an imported HTML file that sits outside the entry's
	// root back over its source, so the outputs are written here, each checked to stay inside the bundle.
	const built = await Bun.build({
		entrypoints: [join(import.meta.dir, "fixtures", "serve-page.ts")],
		root: REPO,
		target: "bun",
		splitting: true,
	});
	expect(built.success).toBe(true);
	const out = join(env.root, "bundle");
	for (const output of built.outputs) {
		const target = resolve(out, output.path);
		expect(relative(out, target).startsWith("..")).toBe(false);
		mkdirSync(dirname(target), { recursive: true });
		await Bun.write(target, output);
	}

	const elsewhere = join(env.root, "elsewhere");
	mkdirSync(elsewhere);
	const entry = join(out, "test", "frontier", "server", "fixtures", "serve-page.js");
	const run = Bun.spawnSync([process.execPath, entry], { cwd: elsewhere, stderr: "pipe", windowsHide: true });
	expect(run.stderr.toString()).toBe("");
	expect(JSON.parse(run.stdout.toString())).toEqual({ page: 200, title: true, chunk: 200 });
}, 60_000);

test("hashed page files are kept a year; the HTML and unhashed files never (PM-3)", () => {
	expect(assetCacheControl("/index-37eqm9x7.js")).toBe("public, max-age=31536000, immutable");
	expect(assetCacheControl("/MonaSans-VF-d0xc4p05.woff2")).toBe("public, max-age=31536000, immutable");
	expect(assetCacheControl("/")).toBe("no-store");
	expect(assetCacheControl("/ptt-worker.js")).toBe("no-store");
});
