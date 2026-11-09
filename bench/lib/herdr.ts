import { herdrRequestOnce } from "../../src/frontier/server/herdr.ts";

/**
 * One request to a herdr session, through the product's client (`herdrRequestOnce`): its framing, `HerdrError`s and
 * 0517's single retry of a dropped read. Only for the two calls the `Herdr` interface has no method for
 * (`pane.wait_for_output`, `tab.close`); everything else goes through the client (`openSandbox().herdr()`, BEN-18).
 */
export function herdrRequest<T>(session: string, method: string, params: object, timeoutMs = 10_000): Promise<T> {
	return herdrRequestOnce<T>(session, method, params, timeoutMs);
}
