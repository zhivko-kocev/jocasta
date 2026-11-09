import { expect, test } from "bun:test";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { defaultProjectName, identityKey, normalizeOrigin, projectIdFor } from "../../src/shared/identity.ts";
import { makeRepo, sh, useTempJocasta } from "./fixture.ts";

const env = useTempJocasta();

test.each([
	["https://github.com/example-org/demo.git", "github.com/example-org/demo"],
	["git@github.com:example-org/demo.git", "github.com/example-org/demo"],
	["ssh://git@GitHub.com:22/Example-Org/Demo.git", "github.com/example-org/demo"],
	["https://user:token@github.com/example-org/demo/", "github.com/example-org/demo"],
	["https://dev.azure.com/acme/Web/_git/api", "dev.azure.com/acme/web/api"],
	["https://acme@dev.azure.com/acme/Web/_git/api", "dev.azure.com/acme/web/api"],
	["https://acme.visualstudio.com/Web/_git/api", "dev.azure.com/acme/web/api"],
	["https://acme.visualstudio.com/DefaultCollection/Web/_git/api", "dev.azure.com/acme/web/api"],
	["git@ssh.dev.azure.com:v3/acme/Web/api", "dev.azure.com/acme/web/api"],
	["acme@vs-ssh.visualstudio.com:v3/acme/Web/api", "dev.azure.com/acme/web/api"],
	["https://dev.azure.com/acme/My%20Project/_git/api", "dev.azure.com/acme/my project/api"],
])("normalizes origin %s (0103)", (url, key) => {
	expect(normalizeOrigin(url)).toBe(key);
});

test("identity precedence: origin, then the first root commit, then the path (0103)", () => {
	const withOrigin = makeRepo(join(env.root, "a"), { origin: "https://github.com/example-org/demo.git" });
	expect(identityKey(withOrigin)).toBe("github.com/example-org/demo");

	const noOrigin = makeRepo(join(env.root, "b"));
	const root = sh(noOrigin, "rev-list", "--max-parents=0", "HEAD");
	expect(identityKey(noOrigin)).toBe(`root:${root}`);

	const empty = join(env.root, "c");
	mkdirSync(empty);
	sh(empty, "init", "-q");
	expect(identityKey(empty)).toMatch(/^path:.*\/c$/);
});

test("project ids are p- plus 16 hex of SHA-256 of the key; names come from origin, else the folder (0107, 0110)", () => {
	const id = projectIdFor("github.com/example-org/demo");
	expect(id).toMatch(/^p-[0-9a-f]{16}$/);
	expect(projectIdFor("github.com/example-org/demo")).toBe(id);
	expect(projectIdFor("github.com/example-org/other")).not.toBe(id);
	expect(defaultProjectName("github.com/example-org/demo", "D:/code/demo-clone")).toBe("demo");
	expect(defaultProjectName("root:abc", join("x", "notes"))).toBe("notes");
});
