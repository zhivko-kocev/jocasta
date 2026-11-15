import { expect, test } from "bun:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { UserError } from "../../src/shared/errors.ts";
import { readProjectsFile, setPushWay } from "../../src/shared/projects-file.ts";
import { useTempJocasta } from "./fixture.ts";

const env = useTempJocasta();

/** A projects.yaml in the temp root with `text`. */
function fileWith(text: string): string {
	const file = join(env.root, "projects.yaml");
	writeFileSync(file, text);
	return file;
}

test("the push map is read with each entry's line, and a file without one has none (0613)", () => {
	const file = fileWith(
		"version: 1\nadd:\n  - C:/code/acme\npush:\n  C:/code/acme: direct\n  C:/code/client-a: pull-requests\n",
	);
	expect(readProjectsFile(file).push).toEqual({
		"C:/code/acme": { way: "direct", line: 5 },
		"C:/code/client-a": { way: "pull-requests", line: 6 },
	});
	expect(readProjectsFile(fileWith("version: 1\n")).push).toEqual({});
	expect(readProjectsFile(fileWith("version: 1\npush:\n")).push).toEqual({});
});

test("a bad push value is reported by file, line and key like the file's other problems (0613)", () => {
	const file = fileWith("version: 1\npush:\n  C:/code/acme: direkt\n");
	let error: unknown;
	try {
		readProjectsFile(file);
	} catch (caught) {
		error = caught;
	}
	expect(error).toBeInstanceOf(UserError);
	expect((error as UserError).message).toStartWith(`${file}:3: push.C:/code/acme`);
});

test("both answers are written into a file that has no push map, keeping the file's comments, order and keys (0613)", () => {
	const text = "# my projects\nversion: 1\nroots:\n  - path: C:/code # all of it\nnames:\n  p-1: Acme\nscopes: old\n";
	const file = fileWith(text);
	expect(setPushWay("C:\\code\\acme", "direct", file)).toBe(true);
	expect(readFileSync(file, "utf8")).toBe(`${text}push:\n  C:/code/acme: direct\n`);
	expect(setPushWay("C:/code/client-a", "pull-requests", file)).toBe(true);
	expect(readFileSync(file, "utf8")).toBe(`${text}push:\n  C:/code/acme: direct\n  C:/code/client-a: pull-requests\n`);
	expect(readProjectsFile(file).push["C:/code/client-a"]?.way).toBe("pull-requests");
});

test("an entry for the same folder is changed in place with its comment; a repeat changes nothing; keys after the map stay (0613)", () => {
	const text =
		"version: 1\npush:\n  # per repo\n  C:/code/acme: pull-requests # PRs\n  C:/code/other: direct\nnames:\n  p-1: Acme\n";
	const file = fileWith(text);
	expect(setPushWay("c:/CODE/acme/", "direct", file)).toBe(true);
	expect(readFileSync(file, "utf8")).toBe(
		text.replace("pull-requests # PRs", "direct # PRs").replace("C:/code/acme", "c:/CODE/acme/"),
	);
	expect(setPushWay("C:/code/other", "direct", file)).toBe(false);
	expect(setPushWay("C:/code/new", "direct", file)).toBe(true);
	expect(readFileSync(file, "utf8")).toContain("  C:/code/other: direct\n  C:/code/new: direct\nnames:\n");
});

test("an empty or flow push map takes the entry as a block, and a missing file is made (0613)", () => {
	const flow = fileWith("version: 1\npush: {} # none yet\nnames: {}\n");
	setPushWay("C:/code/acme", "direct", flow);
	expect(readFileSync(flow, "utf8")).toBe("version: 1\npush: # none yet\n  C:/code/acme: direct\nnames: {}\n");
	const empty = fileWith("push:\nnames: {}\n");
	setPushWay("C:/code/acme", "pull-requests", empty);
	expect(readFileSync(empty, "utf8")).toBe("push:\n  C:/code/acme: pull-requests\nnames: {}\n");
	const made = join(env.root, "none.yaml");
	setPushWay("C:/code/acme", "direct", made);
	expect(readFileSync(made, "utf8")).toBe("version: 1\npush:\n  C:/code/acme: direct\n");
});
