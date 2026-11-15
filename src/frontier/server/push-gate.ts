/**
 * The service's answer to the pre-push hook of every pane (0613): a push that updates a protected branch is decided per
 * repo by the `push:` map of `projects.yaml` (0109), which his first Allow or Deny on that repo writes. `direct` passes
 * unasked, `pull-requests` refuses unasked, and a repo with no entry asks him through the Allow box (0484, 0558).
 */
import { basename } from "node:path";
import { UserError } from "../../shared/errors.ts";
import { pathKey, realPath } from "../../shared/pathkey.ts";
import { projectsFile } from "../../shared/paths.ts";
import { readProjectsFile, setPushWay } from "../../shared/projects-file.ts";
import { isProtected, NO_COMMIT, type PushAnswer, type PushedRef, type PushRequest } from "../../shared/push-hook.ts";
import { AllowDenied } from "./allow.ts";
import type { ServiceContext } from "./context.ts";
import { subject } from "./thread.ts";
import { cut, quoted } from "./wording.ts";
import { gitOut } from "./worktrees.ts";

/** How many commit subjects his box names before "and N more". */
const SUBJECTS_NAMED = 5;

/** `request` as the hook sends it (`POST /api/push`), or undefined when the body isn't one. */
export function parsePushRequest(body: Record<string, unknown>): PushRequest | undefined {
	const { dir, remote, refs, paneKey } = body;
	if (typeof dir !== "string" || typeof remote !== "string" || !Array.isArray(refs)) return undefined;
	const parsed: PushedRef[] = [];
	for (const ref of refs as Record<string, unknown>[]) {
		if (typeof ref?.ref !== "string" || typeof ref.local !== "string" || typeof ref.remote !== "string")
			return undefined;
		parsed.push({ ref: ref.ref, local: ref.local, remote: ref.remote });
	}
	return { dir, remote, refs: parsed, ...(typeof paneKey === "string" ? { paneKey } : {}) };
}

const refuse = (reason: string): PushAnswer => ({ allow: false, reason });

/** The address as his box shows it: without the user name and password an `https://user:token@host` carries. */
function withoutCredentials(address: string): string {
	return address.replace(/^([a-z][a-z0-9+.-]*:\/\/)[^/@]*@/i, "$1");
}

export class PushGate {
	/** The questions open per repo, so a second push to the same repo waits for the answer instead of asking again. */
	private readonly asking = new Map<string, Promise<unknown>>();

	constructor(private readonly c: ServiceContext) {}

	/**
	 * Whether the push `request` describes may go ahead. Only the updates of main, master, develop and the project's own
	 * trunk are decided; any other push passes. A refusal's reason is for the agent that pushed.
	 */
	async verdict(request: PushRequest): Promise<PushAnswer> {
		const checkout = this.c.places.checkout(request.dir);
		if (!checkout) return refuse(`Jocasta found no git repository at ${request.dir}, so this push is refused.`);
		// Also from a worktree: the answer belongs to the repo's main checkout.
		const repo = realPath(checkout.main);
		const project = this.c.places.projectAt(repo);
		const trunk = project ? await this.c.places.trunk(project) : undefined;
		const updated = request.refs.filter((r) => isProtected(r.ref, trunk));
		if (updated.length === 0) return { allow: true };
		const name = project?.name ?? basename(repo);
		for (let open = this.asking.get(pathKey(repo)); open; open = this.asking.get(pathKey(repo))) {
			await open.catch(() => {});
		}
		try {
			const file = projectsFile();
			const known = Object.entries(readProjectsFile(file).push).find(
				([key]) => pathKey(realPath(key)) === pathKey(repo),
			);
			if (known?.[1].way === "direct") return { allow: true };
			if (known) return refuse(this.takesPullRequests(name, updated, file, known[1].line));
			const asked = this.ask(request, repo, name, updated, file);
			this.asking.set(pathKey(repo), asked);
			try {
				return await asked;
			} finally {
				this.asking.delete(pathKey(repo));
			}
		} catch (error) {
			if (error instanceof UserError) return refuse(error.message);
			throw error;
		}
	}

	/** What a repo that takes pull requests tells the agent that pushed to its protected branch. */
	private takesPullRequests(name: string, updated: PushedRef[], file: string, line: number): string {
		return `${name} takes pull requests, so a push to ${branches(updated)} is refused: push a feature branch and open a pull request instead. The user chose this for the repo in ${file}, line ${line}, and can change it there.`;
	}

	/** Asks him once for `repo` (0613) and keeps his Allow or Deny; no answer, or any end that isn't his Deny, keeps nothing. */
	private async ask(
		request: PushRequest,
		repo: string,
		name: string,
		updated: PushedRef[],
		file: string,
	): Promise<PushAnswer> {
		const lines = [
			`${await this.who(request.paneKey)} asks to push to ${branches(updated)} of ${name} (${withoutCredentials(request.remote)}).`,
			...(await this.commitLines(request.dir, updated)),
			`Allow: ${name} takes pushes straight to its protected branches from now on, unasked. Deny: it takes pull requests instead. Either is kept in ${file}.`,
		];
		try {
			await this.c.allow.ask(
				{ message: `Push to ${branches(updated)} in ${name}?`, detail: lines.join("\n") },
				"nothing was pushed",
			);
		} catch (error) {
			if (!(error instanceof AllowDenied && error.chose)) throw error;
			setPushWay(repo, "pull-requests", file);
			return refuse(`${error.message} ${name} now takes pull requests, as written in ${file}.`);
		}
		setPushWay(repo, "direct", file);
		return { allow: true };
	}

	/** The pane that pushed, from its key: a thread, the boss, or a project pane. */
	private async who(paneKey: string | undefined): Promise<string> {
		const paneId = this.c.launched.paneOfKey(paneKey);
		if (!paneId) return "A process without a Jocasta pane key";
		const thread = this.c.threads.get(paneId);
		if (thread) return `The thread ${subject(thread)}`;
		return paneId === this.c.boss?.paneId ? "The boss" : "A project pane";
	}

	/** The commits the push brings, as lines for his box: their count and the first subjects; deletes say so. */
	private async commitLines(dir: string, updated: PushedRef[]): Promise<string[]> {
		const lines: string[] = [];
		for (const ref of updated) {
			if (NO_COMMIT.test(ref.local)) {
				lines.push(`It deletes ${branchName(ref)}.`);
				continue;
			}
			// A new branch brings what no remote-tracking branch has yet.
			const range = NO_COMMIT.test(ref.remote) ? [ref.local, "--not", "--remotes"] : [`${ref.remote}..${ref.local}`];
			const count = Number(await gitOut(dir, ["rev-list", "--count", ...range]));
			const log = (await gitOut(dir, ["log", `-${SUBJECTS_NAMED}`, "--format=%s", ...range])) ?? "";
			const named = log
				.split("\n")
				.filter(Boolean)
				.map((s) => quoted(cut(s, 72)));
			const more = count > named.length ? `, and ${count - named.length} more` : "";
			lines.push(
				count > 0
					? `${count} commit${count === 1 ? "" : "s"} to ${branchName(ref)}: ${named.join(", ")}${more}.`
					: `Nothing new reaches ${branchName(ref)}.`,
			);
		}
		return lines;
	}
}

/** The branch a ref names. */
function branchName(ref: PushedRef): string {
	return ref.ref.replace("refs/heads/", "");
}

/** The updated branches' names, `main` or `main, develop`. */
function branches(updated: PushedRef[]): string {
	return updated.map(branchName).join(", ");
}
