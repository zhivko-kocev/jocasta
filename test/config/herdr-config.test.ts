// The shipped herdr config (`config/herdr.toml`, 0440), checked by herdr itself (0608): `herdr config check` with
// HERDR_CONFIG_PATH names each key herdr ignores, which a mistyped or renamed key would otherwise do in silence. The
// template is filled in as setup fills it (`herdrConfig`). Skipped where herdr isn't installed.
import { afterAll, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { findHerdr } from "../../src/frontier/server/herdr.ts";
import { herdrConfig } from "../../src/setup/setup.ts";
import { removeTree } from "../../src/shared/fsx.ts";
import { shippedConfigDir } from "../../src/shared/install.ts";
import { runProgram } from "../../src/shared/process.ts";

const herdr = findHerdr();
const root = mkdtempSync(join(tmpdir(), "jocasta-test-herdr-config-"));
afterAll(() => removeTree(root));

/** What `herdr config check` reports for `text` as a herdr.toml. */
async function check(text: string): Promise<string> {
	const path = join(root, "herdr.toml");
	writeFileSync(path, text);
	const result = await runProgram([herdr as string, "config", "check"], {
		env: { ...process.env, HERDR_CONFIG_PATH: path },
		timeoutMs: 30_000,
	});
	return `${result.stdout}${result.stderr}`;
}

test.skipIf(!herdr)("herdr reads the config at HERDR_CONFIG_PATH: a made-up key is named", async () => {
	expect(await check('onboarding = false\nmade_up_key = 1\n[theme]\nname = "tokyo-night"\n')).toContain(
		"unknown config key made_up_key",
	);
});

test.skipIf(!herdr)("the shipped herdr.toml, filled in as setup fills it, has no key herdr ignores", async () => {
	const template = readFileSync(join(shippedConfigDir(), "herdr.toml"), "utf8");
	const report = await check(herdrConfig(template, join(root, "data")));
	expect(report).not.toContain("unknown config key");
});
