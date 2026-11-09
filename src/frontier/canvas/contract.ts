// whiteboard v0.2.4's host contract (0393), the part Jocasta's bridge implements: the canvas module's mount, its
// contents, the `ReviewCanvasBridge` and the editor factories. Mirrored, not imported, from
// `forks/whiteboard/packages/review-protocol/src/contracts.ts` (lines 98–343, 593–797, 1071–1080, 1230–1359), so the
// page builds without whiteboard's sources; a whiteboard bump re-checks it (plan §5.5).

export type ReviewTheme = "light" | "dark";
export type ReviewDiffSide = "base" | "head";
export type ReviewDiffLayout = "split" | "unified";
export type ReviewView = "review" | "commits" | "diff" | "map" | "trace";

export interface ReviewDisposable {
	dispose(): void;
}

export interface ReviewRuntimeConfig {
	serverUrl: string;
	reviewId: string;
	token: string;
	wasmUrl: string;
	appVersion: string;
	theme: ReviewTheme;
	host: "desktop";
}

export interface ReviewInlineEditorRange {
	startLine: number;
	endLine: number;
	side?: ReviewDiffSide;
}

export interface ReviewSourcePins {
	readonly repositoryId: string;
	readonly head: string;
	readonly base?: string;
}

export interface ReviewSourceView {
	readonly reviewId: string;
	readonly version: number;
	readonly generation?: string;
	readonly commit?: string;
	readonly pins?: ReviewSourcePins;
}

export interface ReviewInlineEditorSpec {
	container: HTMLElement;
	path: string;
	title: string;
	description?: string;
	side: ReviewDiffSide;
	pins?: ReviewSourcePins;
	ranges: readonly ReviewInlineEditorRange[];
	countRanges?: readonly ReviewInlineEditorRange[];
	heightMode: "capped" | "content";
	active: boolean;
	onDidFocus?: () => void;
	onDidOpen?: () => void;
	onDidNavigate?: () => void;
	onDidShowHover?: () => void;
}

export interface ReviewFindQuery {
	text: string;
	matchCase: boolean;
	wholeWord: boolean;
	isRegex: boolean;
}

export interface ReviewInlineFindSpec {
	path: string;
	side: ReviewDiffSide;
	pins?: ReviewSourcePins;
	ranges: readonly ReviewInlineEditorRange[];
}

export interface ReviewInlineEditorHandle extends ReviewDisposable {
	readonly height: number;
	setActive(active: boolean): void;
	setCollapsed(collapsed: boolean): void;
	onDidChangeHeight(listener: (height: number) => void): ReviewDisposable;
	onDidError(listener: (message: string) => void): ReviewDisposable;
	setFindQuery(query: ReviewFindQuery): Promise<{ matchCount: number }>;
	revealFindMatch(index: number): void;
	clearActiveFindMatch(): void;
	clearFind(): void;
}

export interface ReviewDiffLensRange {
	side: ReviewDiffSide;
	file: string;
	fromLine: number;
	toLine: number;
}

export interface ReviewDiffLens {
	wholeFiles?: boolean;
	id: string;
	title: string;
	reviewId: string;
	version: number;
	ranges: readonly ReviewDiffLensRange[];
}

export interface ReviewCommitScope {
	commit: string;
}

export interface ReviewDiffViewSpec {
	document?: {
		heightMode: "capped" | "content";
		onDidChangeHeight(height: number): void;
		onDidFocus?: () => void;
		onDidOpen?: () => void;
	};
	container: HTMLElement;
	fileTreeContainer?: HTMLElement;
	lens?: ReviewDiffLens;
	scope?: ReviewCommitScope;
}

export interface ReviewDiffViewHandle extends ReviewDisposable {
	focus(): void;
	revealSource?(source: ReviewDiffLensRange, sectionId?: string): void;
	revealFile?(path: string, options?: { restore?: boolean }): void;
	onDidError(listener: (message: string) => void): ReviewDisposable;
}

export interface ReviewDiffFileWire {
	path: string;
	previousPath?: string;
	status: "added" | "modified" | "deleted" | "renamed" | "unchanged";
	additions: number;
	deletions: number;
	binary?: true;
	patch?: string;
}

/** The verbs the canvas posts to its host (`ReviewVerbRequestSchema`). */
export type ReviewVerbRequest =
	| { name: "authoringCapabilities"; args: Record<string, never> }
	| { name: "joinDiscord"; args: Record<string, never> }
	| { name: "showReviewView"; args: { view: ReviewView } }
	| { name: "openSourceTree"; args: Record<string, never> }
	| { name: "openDiff"; args: { path: string; previousPath?: string } }
	| {
			name: "reveal";
			args: {
				path: string;
				startLine: number;
				endLine: number;
				side?: ReviewDiffSide;
				pins?: ReviewSourcePins;
				highlight?: boolean;
				preserveFocus?: boolean;
			};
	  }
	| { name: "focusWindow"; args: Record<string, never> }
	| { name: "captureScreenshot"; args: Record<string, never> }
	| { name: "openReviewRevision"; args: { revision?: string; sealedAt?: number } }
	| { name: "openReview"; args: { reviewUuid: string; active: boolean } }
	| { name: "openApiReview"; args: { reviewId: string; title: string } };

export type ReviewVerbResponse = { ok: true; result?: unknown } | { ok: false; error: string; code?: string };

export type ReviewSurfaceEvent =
	| { event: "themeChanged"; theme: ReviewTheme }
	| { event: "showReviewView"; view: ReviewView };

export interface ReviewCanvasBridge {
	readonly config: ReviewRuntimeConfig;
	readonly inlineEditors: {
		create(spec: ReviewInlineEditorSpec): ReviewInlineEditorHandle;
		find(spec: ReviewInlineFindSpec, query: ReviewFindQuery): Promise<{ matchCount: number }>;
	};
	readonly diffView: {
		create(spec: ReviewDiffViewSpec): ReviewDiffViewHandle;
		files(scope?: ReviewCommitScope): Promise<readonly ReviewDiffFileWire[]>;
	};
	request(url: string, init?: RequestInit): Promise<Response>;
	post(request: ReviewVerbRequest): Promise<ReviewVerbResponse>;
	subscribe(listener: (event: ReviewSurfaceEvent) => void): ReviewDisposable;
	currentTheme(): ReviewTheme;
	onDidChangeTheme(listener: (theme: ReviewTheme) => void): ReviewDisposable;
	currentDiffLayout(): ReviewDiffLayout;
	setDiffLayout(layout: ReviewDiffLayout): Promise<void>;
	onDidChangeDiffLayout(listener: (layout: ReviewDiffLayout) => void): ReviewDisposable;
	notify?(message: { kind: "success" | "error"; text: string }): void;
	ready(): void;
	reportDiagnostic?(diagnostic: { level: "error" | "warning"; source: string; message: string; stack?: string }): void;
}

/** A review in the server's catalog (`GET /reviews-api`), as Home lists it; passed through as the server sends it. */
export interface ReviewApiSummary {
	reviewId: string;
	title: string;
	kind?: "scratchpad";
}

export type ReviewCanvasContent =
	| { kind: "loading" }
	| { kind: "error"; message: string }
	| {
			kind: "api";
			reviewId: string;
			bridge: ReviewCanvasBridge;
			setTitle?(title: string): void;
			setSourceView?(selection: unknown, view: ReviewSourceView): void;
			openSource?(
				source: { view: ReviewSourceView; file: string; side: ReviewDiffSide },
				range: ReviewInlineEditorRange,
			): Promise<void>;
	  }
	| {
			kind: "home";
			reviews: readonly ReviewApiSummary[];
			openReview(uuid: string): void;
			deleteReview?(uuid: string): Promise<void>;
			openTutorial(): void;
	  };

export interface ReviewCanvasHandle extends ReviewDisposable {
	update(content: ReviewCanvasContent): void;
	focus(): void;
	showFind(seed?: string): boolean;
}

export interface ReviewMenuRequest {
	anchor: HTMLElement;
	items: readonly { id: string; label: string; checked?: boolean; enabled?: boolean }[];
	onSelect(id: string): void | Promise<void>;
	onHide(): void;
}

/** The canvas module (`packages/review/app/src/desktop-entry.tsx`). */
export interface ReviewCanvasModule {
	mountReviewCanvas(
		container: HTMLElement,
		content: ReviewCanvasContent,
		ui?: {
			confirmDelete?(title: string): Promise<boolean>;
			showMenu(request: ReviewMenuRequest): ReviewDisposable;
		},
	): ReviewCanvasHandle;
}

/** `GET /whiteboard/connection` from the Jocasta service (`src/frontier/server/server.ts`). */
export interface WhiteboardConnection {
	url: string;
	token: string;
	version: string;
	canvas: { script: string; styles: string[]; wasm: string };
	worker: string;
}
