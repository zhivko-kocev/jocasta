import { describe, expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { readQuietHours } from "../../src/shared/events/quiet.ts";
import { readPoller } from "../../src/shared/pollers/settings.ts";
import { readWeatherPlace } from "../../src/shared/pollers/weather.ts";
import {
	checkSettings,
	goneSettings,
	readSettings,
	SettingsError,
	serviceSetting,
	settingsFile,
	settingsProblems,
} from "../../src/shared/settings.ts";
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

describe("settingsProblems (0584)", () => {
	test("an unknown key is said once, with its line, and ignored; the rest applies", () => {
		writeSettings(
			["pollers:", "  gihub:", "    everyMinutes: 3", "  github:", "    everyMinutes: 4", "poller: {}"].join("\n"),
		);
		expect(settingsProblems()).toEqual([
			"jocasta.yaml:2: unknown key gihub in pollers; ignored",
			"jocasta.yaml:6: unknown key poller; ignored",
		]);
		expect(readPoller(readSettings(), "github")).toEqual({ everyMinutes: 4 });
	});

	test("a bad value turns off only its own feature, naming the line", () => {
		writeSettings(
			[
				"service:",
				"  port: 70000",
				"  quietHours: late",
				"dashboard:",
				"  weather: Example City",
				"pollers:",
				"  github:",
				"    everyMinutes: 0",
			].join("\n"),
		);
		const settings = readSettings();
		const thrown = (work: () => unknown) => {
			try {
				work();
			} catch (error) {
				return error as SettingsError;
			}
		};
		// Before the service said them at start, a reader's settings error isn't marked said.
		expect(thrown(() => checkSettings(settings, "pollers.github"))?.said).toBe(false);
		expect(settingsProblems()).toEqual([
			"jocasta.yaml:2: service.port must be a port number from 1 to 65535; using 47400",
			'jocasta.yaml:3: service.quietHours must be like "22:00-08:00"; quiet hours are off',
			"jocasta.yaml:8: pollers.github.everyMinutes must be a number of minutes, 1 or more; the GitHub poller is off",
		]);
		const github = thrown(() => readPoller(settings, "github"));
		expect(github).toBeInstanceOf(SettingsError);
		expect(github?.message).toBe("jocasta.yaml:8: pollers.github.everyMinutes must be a number of minutes, 1 or more");
		expect(github?.said).toBe(true);
		// The other pollers and the weather read their own keys and run.
		expect(readPoller(settings, "calendar")).toEqual({ everyMinutes: 10 });
		expect(readPoller(settings, "news").everyMinutes).toBe(15);
		expect(() => readQuietHours(settings)).toThrow('jocasta.yaml:3: service.quietHours must be like "22:00-08:00"');
		expect(readWeatherPlace(settings)).toBe("Example City");
	});

	test("known and gone keys pass, the old frontier group still read; a malformed file is one problem, never a throw", () => {
		writeSettings(
			[
				"frontier:",
				"  port: 7420",
				"  quietHours: 22:00-07:00",
				"  autostart: false",
				"  dashboard: { display: primary }",
				"graph: { main: anthropic/example-main }",
				"pollers:",
				"  news: { feeds: [{ source: hn }, { topic: gaming, limit: 5 }] }",
			].join("\n"),
		);
		expect(settingsProblems()).toEqual([]);
		expect(readQuietHours(readSettings())).toEqual({ from: 22 * 60, to: 7 * 60 });
		expect(serviceSetting(readSettings(), "port")).toBe(7420);
		writeSettings("frontier:\n  port: 7420\npollers: [unclosed\n");
		expect(settingsProblems()).toEqual(["jocasta.yaml:3: Unexpected token; its settings are off"]);
	});

	test("service wins over the old frontier group key by key; a problem names the key as the file spells it", () => {
		writeSettings(["frontier:", "  port: 7420", "  quietHours: 22:00-07:00", "service:", "  port: 7421"].join("\n"));
		expect(settingsProblems()).toEqual([]);
		expect(serviceSetting(readSettings(), "port")).toBe(7421);
		// `service` doesn't set quiet hours: `frontier`'s still apply.
		expect(readQuietHours(readSettings())).toEqual({ from: 22 * 60, to: 7 * 60 });
		writeSettings(["service:", "  quietHours: 23:00-06:00", "frontier:", "  quietHours: late"].join("\n"));
		expect(readQuietHours(readSettings())).toEqual({ from: 23 * 60, to: 6 * 60 });
		writeSettings(["frontier:", "  port: 0"].join("\n"));
		expect(() => serviceSetting(readSettings(), "port")).toThrow(
			"jocasta.yaml:2: frontier.port must be a port number from 1 to 65535",
		);
		expect(serviceSetting({}, "port")).toBeUndefined();
	});
});
