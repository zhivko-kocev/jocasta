import { expect, test } from "bun:test";
import { fetchUsr } from "./api.ts";

test("fetchUsr finds a user by id", async () => {
	expect((await fetchUsr("u1"))?.name).toBe("Ada at acme");
	expect(await fetchUsr("nobody")).toBeUndefined();
});
