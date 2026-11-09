/** Longest link kept: a longer one is no link a person clicks (SE-6). */
const MAX_LINK = 300;

/**
 * A link from outside (a calendar entry, a CI check) as one to open: an http(s) URL of at most 300 characters,
 * normalized; anything else is undefined (SE-4, SE-6).
 */
export function webLink(url: string | null | undefined): string | undefined {
	if (!url || !URL.canParse(url)) return undefined;
	const parsed = new URL(url);
	if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return undefined;
	return parsed.href.length <= MAX_LINK ? parsed.href : undefined;
}

/** Opens `url` in his default browser, without waiting (Windows' URL handler; `open`; `xdg-open`). */
export function openInBrowser(url: string): void {
	const argv =
		process.platform === "win32"
			? ["rundll32.exe", "url.dll,FileProtocolHandler", url]
			: [process.platform === "darwin" ? "open" : "xdg-open", url];
	Bun.spawn(argv, { stdio: ["ignore", "ignore", "ignore"], windowsHide: true }).unref();
}
