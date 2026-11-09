import { existsSync } from "node:fs";
import { basename, dirname, isAbsolute, join } from "node:path";
import page from "../canvas/index.html";

/** One built file of the page: its body and content type. */
interface Asset {
	body: Blob;
	type: string;
}

/**
 * The directory a bundle's relative `page.files` paths start from: the bundle's root, the nearest directory at or
 * above this module's that holds the page's HTML at its listed path. Never the working directory (0367).
 */
function bundleRoot(html: string): string {
	for (let dir = import.meta.dir; ; dir = dirname(dir)) {
		if (existsSync(join(dir, html))) return dir;
		if (dirname(dir) === dir) return import.meta.dir;
	}
}

/**
 * The canvas page's files by URL path (`/` is the HTML). A compiled `jocasta` carries the bundle Bun built ahead of
 * time (`page.files`, absolute paths in its embedded file system), as does a server bundle (0367), whose paths are
 * relative to the bundle's root. A source run bundles `src/frontier/canvas/index.html` once, in memory, on the first
 * request.
 */
async function build(): Promise<Map<string, Asset>> {
	const assets = new Map<string, Asset>();
	if (page.files) {
		const html = page.files.find((f) => f.loader === "html")?.path ?? "";
		const root = isAbsolute(html) ? "" : bundleRoot(html);
		for (const file of page.files) {
			const path = isAbsolute(file.path) ? file.path : join(root, file.path);
			assets.set(file.loader === "html" ? "/" : `/${basename(path)}`, {
				body: Bun.file(path),
				type: file.headers["content-type"],
			});
		}
		return assets;
	}
	const result = await Bun.build({
		entrypoints: [page.index],
		minify: true,
		target: "browser",
		// Lazy chunks stay lazy (0352): whiteboard's canvas and Monaco (bridge.ts `loadMonaco`) load on first use.
		splitting: true,
	});
	if (!result.success) throw new AggregateError(result.logs, "the canvas page didn't build");
	for (const output of result.outputs) {
		const name = basename(output.path);
		assets.set(name.endsWith(".html") ? "/" : `/${name}`, { body: output, type: output.type });
	}
	return assets;
}

let built: Promise<Map<string, Asset>> | undefined;

/** A file name Bun content-hashed (`index-37eqm9x7.js`): its bytes never change under that name. */
const HASHED = /-[A-Za-z0-9_-]{8}\.(js|css|woff2|ttf|wasm)$/;

/**
 * How long the browser may keep a page file (PM-3): a hashed file for a year, so it isn't fetched and compiled again
 * for every review; the HTML and anything unhashed never, so a new build shows at once.
 */
export function assetCacheControl(pathname: string): string {
	return HASHED.test(pathname) ? "public, max-age=31536000, immutable" : "no-store";
}

/** The page file at `pathname`, or undefined when there is none. */
export async function uiAsset(pathname: string): Promise<Response | undefined> {
	built ??= build();
	const asset = (await built).get(pathname === "/index.html" ? "/" : pathname);
	return (
		asset &&
		new Response(asset.body, {
			headers: { "content-type": asset.type, "cache-control": assetCacheControl(pathname) },
		})
	);
}
