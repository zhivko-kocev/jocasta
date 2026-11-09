// Bundled by assets.test.ts the way the app's server bundle is (0367), then run from another working directory: prints
// whether the page and its script are served.
import { uiAsset } from "../../../../src/frontier/server/assets.ts";

const page = await uiAsset("/");
const html = (await page?.text()) ?? "";
// The page names its script relative to the HTML's place in the bundle; the browser resolves it to the root.
const script = html.match(/<script[^>]*src="[^"]*?([^"/]+\.js)"/)?.[1];
const chunk = script ? await uiAsset(`/${script}`) : undefined;
console.log(
	JSON.stringify({ page: page?.status, title: html.includes("<title>Jocasta</title>"), chunk: chunk?.status }),
);
