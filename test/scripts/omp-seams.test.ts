// 0693: `mise run omp-seams` reads a settings file's leaf keys and finds the ones omp doesn't know, a map setting's
// entries checked at the map's level.
import { expect, test } from "bun:test";
import { leafKeys, unknownKeys } from "../../scripts/omp-seams.ts";

test("leaf keys stop at lists and scalars", () => {
	const parsed = Bun.YAML.parse("a:\n  b: 1\n  c: [x, y]\nd: true\nmodelRoles:\n  web: web/duckduckgo\n");
	expect(leafKeys(parsed)).toEqual(["a.b", "a.c", "d", "modelRoles.web"]);
	expect(leafKeys(null)).toEqual([]);
});

test("a key is known when registered itself or under a registered map setting", () => {
	const registered = { "a.b": { type: "number" }, modelRoles: { type: "record" }, theme: { type: "string" } };
	expect(
		unknownKeys(["a.b", "a.c", "modelRoles.web", "retry.fallbackChains.default", "theme.dark"], registered),
	).toEqual(["a.c", "retry.fallbackChains.default", "theme.dark"]);
});
