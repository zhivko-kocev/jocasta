import { UserError } from "./errors.ts";
import { addProject, listProjects, removeProject, syncCatalog } from "./projects.ts";
import { readProjectsFile } from "./projects-file.ts";
import { openCatalog } from "./schema.ts";

const PROJECT_USAGE = "usage: jocasta project list | add <path> | remove <name|id>";

/** `jocasta project …` (0111). Resolves the lines to print; usage (`--help` too) touches nothing on disk. */
export async function projectCommand(args: string[]): Promise<string[]> {
	const [verb, ...rest] = args;
	const known =
		(verb === "list" && rest.length === 0) || ((verb === "add" || verb === "remove") && rest.length === 1 && !!rest[0]);
	if (!known) throw new UserError(PROJECT_USAGE);
	const catalog = openCatalog();
	try {
		if (verb === "list" && rest.length === 0) {
			const file = readProjectsFile();
			await syncCatalog(catalog, file);
			const lines = listProjects(catalog, file).map((p) => `${p.id}  ${p.name}  ${p.mainPath}`);
			return lines.length > 0
				? lines
				: ["No projects yet: add one with jocasta project add <path>, or a watched root in projects.yaml."];
		}
		if (verb === "add" && rest.length === 1 && rest[0]) {
			const file = readProjectsFile();
			await syncCatalog(catalog, file);
			const known = listProjects(catalog, file).map((p) => p.id);
			const project = await addProject(catalog, rest[0]);
			if (known.includes(project.id)) {
				return [`${project.name} (${project.id}) is already a project, at ${project.mainPath}`];
			}
			const main = project.mainCheckoutOf ? ` (the main checkout of ${project.mainCheckoutOf})` : "";
			return [`added ${project.name} (${project.id}) at ${project.mainPath}${main}`];
		}
		if (verb === "remove" && rest.length === 1 && rest[0]) {
			const project = await removeProject(catalog, rest[0]);
			return [`removed ${project.name} (${project.id}); its paths are excluded in projects.yaml`];
		}
		throw new UserError(PROJECT_USAGE);
	} finally {
		catalog.close();
	}
}
