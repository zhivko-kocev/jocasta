import { createHash } from "node:crypto";
import { basename } from "node:path";
import { git } from "./git.ts";

const AZURE = "dev.azure.com";

/** Maps an Azure DevOps host and path to `[org, project, repo]`, or undefined for any other host (0103). */
function azureParts(host: string, segments: string[]): string[] | undefined {
	if (host === AZURE) {
		// dev.azure.com/{org}/{project}/_git/{repo}
		const [org, project, marker, repo] = segments;
		return marker === "_git" && org && project && repo ? [org, project, repo] : undefined;
	}
	if (host === "ssh.dev.azure.com" || host === "vs-ssh.visualstudio.com") {
		// v3/{org}/{project}/{repo}
		const [v3, org, project, repo] = segments;
		return v3 === "v3" && org && project && repo ? [org, project, repo] : undefined;
	}
	if (host.endsWith(".visualstudio.com")) {
		// {org}.visualstudio.com/[DefaultCollection/]{project}/_git/{repo}
		const rest = segments[0] === "defaultcollection" ? segments.slice(1) : segments;
		const [project, marker, repo] = rest;
		const org = host.slice(0, -".visualstudio.com".length);
		return marker === "_git" && project && repo ? [org, project, repo] : undefined;
	}
	return undefined;
}

/**
 * The normalized origin key (0103): lowercased host plus path, `.git` stripped, user and port dropped;
 * every Azure DevOps form maps to `dev.azure.com/<org>/<project>/<repo>`.
 */
export function normalizeOrigin(url: string): string {
	const raw = url.trim();
	let host: string;
	let path: string;
	const scheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(raw);
	const scp = /^(?:[^@/\\]+@)?([^:/\\]{2,}):(.*)$/.exec(raw);
	if (scheme && !raw.toLowerCase().startsWith("file:")) {
		const parsed = new URL(raw);
		host = parsed.hostname;
		path = parsed.pathname;
	} else if (!scheme && scp?.[1] && scp[2] !== undefined) {
		host = scp[1];
		path = scp[2];
	} else {
		// A local path or file URL origin: the path itself is the key.
		return raw
			.replaceAll("\\", "/")
			.replace(/\/+$/, "")
			.replace(/\.git$/i, "")
			.toLowerCase();
	}
	const segments = decodeURIComponent(path)
		.replace(/\.git\/?$/i, "")
		.split("/")
		.filter(Boolean)
		.map((s) => s.toLowerCase());
	const lowerHost = host.toLowerCase();
	const azure = azureParts(lowerHost, segments);
	return azure ? [AZURE, ...azure].join("/") : [lowerHost, ...segments].join("/");
}

/** A path as identity: resolved, forward slashes, lowercased on Windows. */
export function normalizePathKey(path: string): string {
	const forward = path.replaceAll("\\", "/").replace(/\/+$/, "");
	return process.platform === "win32" ? forward.toLowerCase() : forward;
}

/** A project's identity key (0103): the origin key, else `root:<sha>`, else `path:<path>`. */
export function identityKey(dir: string): string {
	const origin = git(dir, ["config", "--get", "remote.origin.url"]);
	if (origin) return normalizeOrigin(origin);
	const roots = git(dir, ["rev-list", "--max-parents=0", "HEAD"]);
	// rev-list lists newest first; the first root commit is the last line.
	const first = roots?.split(/\r?\n/).filter(Boolean).at(-1);
	if (first) return `root:${first}`;
	return `path:${normalizePathKey(dir)}`;
}

/** `p-` plus the first 16 hex of SHA-256 of the identity key (0107). */
export function projectIdFor(identity: string): string {
	return `p-${createHash("sha256").update(identity).digest("hex").slice(0, 16)}`;
}

/** The last segment of the origin key, else the main path's folder name (0110). */
export function defaultProjectName(identity: string, mainPath: string): string {
	if (identity.startsWith("root:") || identity.startsWith("path:")) return basename(mainPath);
	return identity.split("/").at(-1) || basename(mainPath);
}
