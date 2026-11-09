/// <reference lib="dom" />
// One review's source on whiteboard's loopback server (0393), as whiteboard's Desktop reads it
// (`services/reviewApiSourceService.ts`): file texts and changed files, with the query `reviewSourceQuery` builds, and
// a text cache the code peeks, find, the source panel and the diffs share (0411).
import type {
	ReviewDiffFileWire,
	ReviewDiffSide,
	ReviewDisposable,
	ReviewSourcePins,
	ReviewSourceView,
	WhiteboardConnection,
} from "./contract.ts";

export const disposable = (dispose: () => void): ReviewDisposable => ({ dispose });

/** A listener set whose `add` returns its own removal. */
export class Emitter<T> {
	private readonly listeners = new Set<(value: T) => void>();
	add(listener: (value: T) => void): ReviewDisposable {
		this.listeners.add(listener);
		return disposable(() => this.listeners.delete(listener));
	}
	fire(value: T): void {
		for (const listener of this.listeners) listener(value);
	}
}

/** What the server says went wrong, else the status. */
export async function failure(response: Response): Promise<Error> {
	const body: unknown = await response.json().catch(() => null);
	const error = body && typeof body === "object" && "error" in body ? body.error : undefined;
	return new Error(typeof error === "string" && error ? error : `whiteboard answered ${response.status}`);
}

/** Where a peek or diff reads its source: the review's current view, or a reference's own pins. */
export interface SourceAt {
	view: ReviewSourceView | undefined;
	pins?: ReviewSourcePins | undefined;
	commit?: string | undefined;
}

/**
 * `reviewSourceQuery` (contracts.ts): the view's version, then a reference's own pins, else the view's pins, else the
 * commit (a reference's, else the view's); the server takes no commit with pins.
 */
export function sourceQuery(at: SourceAt, extra: Record<string, string> = {}): URLSearchParams {
	const query = new URLSearchParams();
	if (at.view) query.set("version", String(at.view.version));
	const pins = at.pins ?? at.view?.pins;
	if (pins) {
		query.set("repositoryId", pins.repositoryId);
		query.set("head", pins.head);
		if (pins.base) query.set("base", pins.base);
	} else {
		const commit = at.commit ?? at.view?.commit;
		if (commit) query.set("commit", commit);
	}
	for (const [key, value] of Object.entries(extra)) query.set(key, value);
	return query;
}

/** One review's source reads from whiteboard's server (`GET /reviews-api/<id>/file|diff`). */
export class ReviewSource {
	/** The review's current view; `setSourceView` replaces it, which changes every text's cache key by itself. */
	view: ReviewSourceView | undefined;
	/**
	 * File texts by their full query (path, side, the view's version and generation, pins or commit), so a peek, find,
	 * the panel and a diff reading the same text fetch it once (FE-6). A failed read leaves no entry, so it can be tried
	 * again.
	 */
	private readonly texts = new Map<string, Promise<string>>();

	constructor(
		private readonly connection: WhiteboardConnection,
		readonly reviewId: string,
	) {}

	/** Monaco's diff worker from the service. */
	get worker(): string {
		return this.connection.worker;
	}

	private async get(route: string, query: URLSearchParams): Promise<unknown> {
		const response = await fetch(
			`${this.connection.url}/reviews-api/${encodeURIComponent(this.reviewId)}${route}?${query}`,
			{ headers: { "x-review-token": this.connection.token }, signal: AbortSignal.timeout(30_000) },
		);
		if (!response.ok) throw await failure(response);
		return response.json();
	}

	/** A file's text on one side; a binary reads as a notice, as the Desktop shows it. */
	text(path: string, side: ReviewDiffSide, at: Omit<SourceAt, "view"> = {}): Promise<string> {
		const query = sourceQuery({ view: this.view, ...at }, { side, file: path, binary: "describe" });
		// The generation isn't part of the server's query, but a new one means new text under the same version.
		const key = `${this.view?.generation ?? ""} ${query}`;
		const cached = this.texts.get(key);
		if (cached) return cached;
		const read = this.get("/file", query).then((body) => {
			if (body && typeof body === "object") {
				if ("binary" in body && body.binary === true) return "Binary file cannot be displayed as text.";
				if ("text" in body && typeof body.text === "string") return body.text;
			}
			throw new Error(`whiteboard sent no text for ${path}`);
		});
		read.catch(() => {
			if (this.texts.get(key) === read) this.texts.delete(key);
		});
		this.texts.set(key, read);
		return read;
	}

	/** The changed files of the review's comparison, or of one commit. */
	async files(commit?: string): Promise<ReviewDiffFileWire[]> {
		const body = await this.get("/diff", sourceQuery({ view: this.view, commit }));
		if (!Array.isArray(body)) throw new Error("whiteboard sent no file list");
		return body.filter(
			(file): file is ReviewDiffFileWire =>
				file !== null && typeof file === "object" && "path" in file && typeof file.path === "string",
		);
	}
}
