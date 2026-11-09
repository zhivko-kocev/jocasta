import { expect, test } from "bun:test";
import { isAdmin } from "./admin.ts";
import { cachedUser } from "./cache.ts";
import { profileLine } from "./profile.ts";

test("callers of fetchUsr", async () => {
	expect(await profileLine("u2")).toBe("Ben at client-a (u2)");
	expect(await profileLine("u9")).toBe("unknown user");
	expect(await isAdmin("u1")).toBe(true);
	expect((await cachedUser("u2"))?.id).toBe("u2");
});
