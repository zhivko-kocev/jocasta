// Headless Edge driven over the Chrome DevTools Protocol, for page checks against a real Frontier server. One Edge per
// test file (`Edge.launch`); each test gets a tab in a browser context of its own (`edge.tab()`), so no cookie, cache
// or storage carries over from another test.
import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { removeTree } from "../../../src/shared/fsx.ts";
import { endProcesses, processIdOf } from "../../../src/shared/proctree.ts";

export const EDGE = "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";

/** How long Edge may take to open DevTools on a loaded machine. */
const LAUNCH_TIMEOUT_MS = 30_000;

interface CdpReply {
	id?: number;
	method?: string;
	result?: Record<string, unknown>;
	error?: { message: string };
	params?: { exceptionDetails?: { text: string; exception?: { description?: string } } };
}

/** One DevTools WebSocket: requests matched to replies by id; anything else goes to `onEvent`. */
class Connection {
	private id = 0;
	private readonly pending = new Map<number, (reply: CdpReply) => void>();

	private constructor(
		private readonly ws: WebSocket,
		onEvent: (reply: CdpReply) => void,
	) {
		ws.addEventListener("message", (event) => {
			const reply = JSON.parse(String(event.data)) as CdpReply;
			if (reply.id === undefined) return onEvent(reply);
			this.pending.get(reply.id)?.(reply);
			this.pending.delete(reply.id);
		});
	}

	static async open(url: string, onEvent: (reply: CdpReply) => void = () => {}): Promise<Connection> {
		const ws = new WebSocket(url);
		const { promise, resolve } = Promise.withResolvers<void>();
		ws.addEventListener("open", () => resolve(), { once: true });
		await promise;
		return new Connection(ws, onEvent);
	}

	send(method: string, params: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
		const id = ++this.id;
		const { promise, resolve, reject } = Promise.withResolvers<Record<string, unknown>>();
		this.pending.set(id, (reply) =>
			reply.error ? reject(new Error(reply.error.message)) : resolve(reply.result ?? {}),
		);
		this.ws.send(JSON.stringify({ id, method, params }));
		return promise;
	}

	close(): void {
		this.ws.close();
	}
}

/** A headless Edge process on a throwaway profile, driven through its browser-level DevTools connection. */
export class Edge {
	private constructor(
		private readonly proc: ChildProcess,
		private readonly profile: string,
		private readonly port: string,
		private readonly browser: Connection,
	) {}

	/** Starts headless Edge on a throwaway profile; `flags` add Edge switches. */
	static async launch(flags: readonly string[] = []): Promise<Edge> {
		const profile = mkdtempSync(join(tmpdir(), "jocasta-edge-"));
		const proc = spawn(
			EDGE,
			[
				"--headless=new",
				"--remote-debugging-port=0",
				`--user-data-dir=${profile}`,
				"--window-size=1280,800",
				"--no-first-run",
				"--disable-extensions",
				"--disable-crash-reporter",
				"--disable-background-networking",
				...flags,
				"about:blank",
			],
			{ windowsHide: true },
		);
		// Edge writes the port file once DevTools listens; under load that can take a while, and the file may be
		// half-written when first seen, so poll until it holds a port or the deadline passes.
		const portFile = join(profile, "DevToolsActivePort");
		const end = Date.now() + LAUNCH_TIMEOUT_MS;
		let port = "";
		while (!/^\d+$/.test(port)) {
			if (Date.now() > end) {
				proc.kill();
				throw new Error("headless Edge didn't start");
			}
			await Bun.sleep(100);
			try {
				port = existsSync(portFile) ? (readFileSync(portFile, "utf8").split("\n")[0] ?? "").trim() : "";
			} catch {
				// Edge is still writing the file (EBUSY on Windows); the next poll reads it.
			}
		}
		const version = (await (await fetch(`http://127.0.0.1:${port}/json/version`)).json()) as {
			webSocketDebuggerUrl: string;
		};
		return new Edge(proc, profile, port, await Connection.open(version.webSocketDebuggerUrl));
	}

	/** A blank tab in a browser context of its own; `tab.close()` ends the context with it. */
	async tab(): Promise<Tab> {
		const { browserContextId } = await this.browser.send("Target.createBrowserContext");
		const { targetId } = await this.browser.send("Target.createTarget", { url: "about:blank", browserContextId });
		const tab = await Tab.attach(`ws://127.0.0.1:${this.port}/devtools/page/${targetId}`, async () => {
			await this.browser.send("Target.disposeBrowserContext", { browserContextId });
		});
		return tab;
	}

	/**
	 * Closes Edge through CDP, ends whatever of its tree still runs (its helper processes can hold the profile's files
	 * for over 30 s after the browser exits on a loaded machine), then removes the profile.
	 */
	async close(): Promise<void> {
		const root = this.proc.pid === undefined ? undefined : processIdOf(this.proc.pid);
		const exited = Promise.withResolvers<void>();
		if (this.proc.exitCode !== null) exited.resolve();
		else this.proc.once("exit", () => exited.resolve());
		await this.browser.send("Browser.close").catch(() => this.proc.kill());
		await Promise.race([exited.promise, Bun.sleep(5000)]);
		this.browser.close();
		if (root) await endProcesses([root]);
		// An ended process's open files close a moment after it's gone.
		const end = Date.now() + LAUNCH_TIMEOUT_MS;
		for (;;) {
			try {
				removeTree(this.profile);
				return;
			} catch (error) {
				const code = error instanceof Error && "code" in error ? error.code : undefined;
				if ((code !== "EBUSY" && code !== "EPERM") || Date.now() > end) throw error;
				await Bun.sleep(250);
			}
		}
	}
}

/** One page of the shared Edge. */
export class Tab {
	/** Uncaught page exceptions, for the test to assert none. */
	readonly errors: string[] = [];
	private connection: Connection | undefined;

	private constructor(private readonly dispose: () => Promise<void>) {}

	static async attach(url: string, dispose: () => Promise<void>): Promise<Tab> {
		const tab = new Tab(dispose);
		tab.connection = await Connection.open(url, (event) => {
			if (event.method !== "Runtime.exceptionThrown") return;
			const details = event.params?.exceptionDetails;
			tab.errors.push(details?.exception?.description ?? details?.text ?? "exception");
		});
		await tab.send("Page.enable");
		await tab.send("Runtime.enable");
		// The machine's own "show animations" setting (off on CI's Windows Server) never decides what a page test sees.
		await tab.send("Emulation.setEmulatedMedia", {
			features: [{ name: "prefers-reduced-motion", value: "no-preference" }],
		});
		// What the page's Content-Security-Policy blocks (0368), for tests to assert none.
		await tab.send("Page.addScriptToEvaluateOnNewDocument", {
			source: `window.__csp = []; document.addEventListener("securitypolicyviolation", (e) => window.__csp.push(e.effectiveDirective + " " + e.blockedURI + " " + (e.sourceFile || "") + ":" + e.lineNumber));`,
		});
		return tab;
	}

	/** Everything the page's Content-Security-Policy blocked since it loaded. */
	cspViolations(): Promise<string[]> {
		return this.eval<string[]>("window.__csp ?? []");
	}

	send(method: string, params: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
		if (!this.connection) throw new Error("page: the tab isn't attached");
		return this.connection.send(method, params);
	}

	async eval<T>(expression: string): Promise<T> {
		const reply = await this.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
		const result = reply.result;
		if (!result || typeof result !== "object" || !("value" in result)) return undefined as T;
		// returnByValue serializes whatever the expression returned; the caller names its type.
		const value = result.value as T;
		return value;
	}

	/** Polls a page expression until it is truthy; throws with `what` when time runs out. */
	async waitFor(expression: string, what: string, timeoutMs = 30_000): Promise<void> {
		const end = Date.now() + timeoutMs;
		while (!(await this.eval<boolean>(expression))) {
			if (Date.now() > end) throw new Error(`page: ${what} did not happen`);
			await Bun.sleep(100);
		}
	}

	/** Types into the focused element and presses Enter. */
	async typeAndEnter(text: string): Promise<void> {
		await this.send("Input.insertText", { text });
		for (const type of ["keyDown", "keyUp"]) {
			await this.send("Input.dispatchKeyEvent", { type, key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
		}
	}

	/**
	 * Clicks the middle of the first element matching `selector` with real mouse events, so the page's hit testing
	 * decides what gets the click (an element.click() would reach even a part with `pointer-events: none`).
	 */
	async clickAt(selector: string): Promise<void> {
		const box = await this.eval<{ x: number; y: number } | null>(
			`(() => { const b = document.querySelector(${JSON.stringify(selector)})?.getBoundingClientRect(); return b ? { x: b.x + b.width / 2, y: b.y + b.height / 2 } : null; })()`,
		);
		if (!box) throw new Error(`page: no ${selector} to click`);
		for (const type of ["mousePressed", "mouseReleased"]) {
			await this.send("Input.dispatchMouseEvent", { type, x: box.x, y: box.y, button: "left", clickCount: 1 });
		}
	}

	async screenshot(): Promise<Buffer> {
		const reply = await this.send("Page.captureScreenshot", { format: "png" });
		return Buffer.from(String(reply.data), "base64");
	}

	/** Closes the tab with its browser context. */
	async close(): Promise<void> {
		this.connection?.close();
		await this.dispose();
	}
}
