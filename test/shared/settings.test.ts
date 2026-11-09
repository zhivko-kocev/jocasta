import { describe, expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { goneSettings, readSettings, settingsFile } from "../../src/shared/settings.ts";
import { YamlFileError } from "../../src/shared/yaml.ts";
import { useTempJocasta } from "./fixture.ts";

const env = useTempJocasta();

function writeSettings(text: string): void {
	mkdirSync(env.config, { recursive: true });
	writeFileSync(join(env.config, "jocasta.yaml"), text);
}

describe("readSettings (0387)", () => {
	test("reads jocasta.yaml in the config dir", () => {
		expect(settingsFile()).toBe(join(env.config, "jocasta.yaml"));
		writeSettings(
			[
				"frontier:",
				"  port: 7420",
				"  autostart: false",
				"  quietHours: { start: '22:00', end: '07:00' }",
				"pollers:",
				"  azureDevOps:",
				"    orgs: [example-org]",
				"graph:",
				"  small: anthropic/example-small",
			].join("\n"),
		);
		expect(readSettings()).toEqual({
			frontier: { port: 7420, autostart: false, quietHours: { start: "22:00", end: "07:00" } },
			pollers: { azureDevOps: { orgs: ["example-org"] } },
			graph: { small: "anthropic/example-small" },
		});
	});

	test("a missing, empty or comment-only file reads as {}", () => {
		expect(readSettings()).toEqual({});
		writeSettings("");
		expect(readSettings()).toEqual({});
		writeSettings("# nothing set yet\n");
		expect(readSettings()).toEqual({});
	});

	test("a file that isn't YAML, or isn't a mapping, throws naming the file and line", () => {
		const file = join(env.config, "jocasta.yaml");
		writeSettings("frontier:\n  port: 7420\npollers: [unclosed\n");
		expect(() => readSettings()).toThrow(YamlFileError);
		expect(() => readSettings()).toThrow(new RegExp(`^${file.replace(/\\/g, "\\\\")}:3: `));
		writeSettings("- a\n- b\n");
		expect(() => readSettings()).toThrow(`${file}:1: `);
	});

	test("Pi's settings.json is no longer read", () => {
		mkdirSync(env.config, { recursive: true });
		writeFileSync(join(env.config, "settings.json"), JSON.stringify({ frontier: { port: 1 } }));
		expect(readSettings()).toEqual({});
	});
});

test("frontier.autostart and frontier.dashboard.display are said to be gone (0557)", () => {
	expect(goneSettings({})).toEqual([]);
	expect(goneSettings({ frontier: { port: 7420, quietHours: "22:00-07:00" } })).toEqual([]);
	expect(goneSettings({ frontier: { autostart: false, dashboard: { display: 2 } } })).toEqual([
		"settings: frontier.autostart is gone; Jocasta no longer starts at logon: run `jocasta` (0557)",
		"settings: frontier.dashboard.display is gone; there is no companion window to place (0557)",
	]);
});
