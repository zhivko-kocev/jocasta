// Headless Edge driven over the Chrome DevTools Protocol, for page checks against a real Frontier server.
import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

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

export class Browser {
	/** Uncaught page exceptions, for the test to assert none. */
	readonly errors: string[] = [];
	private id = 0;
	private readonly pending = new Map<number, (reply: CdpReply) => void>();

	private constructor(
		private readonly proc: ChildProcess,
		private readonly profile: string,
		private readonly ws: WebSocket,
	) {
		ws.addEventListener("message", (event) => {
			const reply = JSON.parse(String(event.data)) as CdpReply;
			if (reply.id !== undefined) {
				this.pending.get(reply.id)?.(reply);
				this.pending.delete(reply.id);
			} else if (reply.method === "Runtime.exceptionThrown") {
				const details = reply.params?.exceptionDetails;
				this.errors.push(details?.exception?.description ?? details?.text ?? "exception");
			}
		});
	}

	/** Starts headless Edge on a throwaway profile with one blank page attached; `flags` add Edge switches. */
	static async launch(flags: readonly string[] = []): Promise<Browser> {
		const profile = mkdtempSync(join(tmpdir(), "jocasta-edge-"));
		const proc = spawn(EDGE, [
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
		]);
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
		const targets = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()) as {
			type: string;
			webSocketDebuggerUrl: string;
		}[];
		const target = targets.find((t) => t.type === "page");
		if (!target) throw new Error("headless Edge has no page");
		const ws = new WebSocket(target.webSocketDebuggerUrl);
		const { promise, resolve } = Promise.withResolvers<void>();
		ws.addEventListener("open", () => resolve(), { once: true });
		await promise;
		const browser = new Browser(proc, profile, ws);
		await browser.send("Page.enable");
		await browser.send("Runtime.enable");
		// The machine's own "show animations" setting (off on CI's Windows Server) never decides what a page test sees.
		await browser.send("Emulation.setEmulatedMedia", {
			features: [{ name: "prefers-reduced-motion", value: "no-preference" }],
		});
		// What the page's Content-Security-Policy blocks (0368), for tests to assert none.
		await browser.send("Page.addScriptToEvaluateOnNewDocument", {
			source: `window.__csp = []; document.addEventListener("securitypolicyviolation", (e) => window.__csp.push(e.effectiveDirective + " " + e.blockedURI + " " + (e.sourceFile || "") + ":" + e.lineNumber));`,
		});
		return browser;
	}

	/** Everything the page's Content-Security-Policy blocked since it loaded. */
	cspViolations(): Promise<string[]> {
		return this.eval<string[]>("window.__csp ?? []");
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

	/** Closes Edge through CDP (so its child processes exit too), then removes the profile. */
	async close(): Promise<void> {
		const exited = Promise.withResolvers<void>();
		if (this.proc.exitCode !== null) exited.resolve();
		else this.proc.once("exit", () => exited.resolve());
		await this.send("Browser.close").catch(() => this.proc.kill());
		await Promise.race([exited.promise, Bun.sleep(5000)]);
		this.ws.close();
		// Edge's helper processes let go of the profile some seconds after the browser exits, longer under load.
		const end = Date.now() + LAUNCH_TIMEOUT_MS;
		for (;;) {
			try {
				rmSync(this.profile, { recursive: true, force: true });
				return;
			} catch (error) {
				const code = error instanceof Error && "code" in error ? error.code : undefined;
				if ((code !== "EBUSY" && code !== "EPERM") || Date.now() > end) throw error;
				await Bun.sleep(250);
			}
		}
	}
}
