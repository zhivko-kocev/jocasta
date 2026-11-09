import { expect, test } from "bun:test";
import { type GraphitiChecks, graphitiHealth } from "../../../src/frontier/server/server.ts";

const checks = (health: "ok" | "refused" | "bad", starting: boolean, dockerRunning: boolean): GraphitiChecks => ({
	health: async () => health,
	starting: () => starting,
	dockerRunning: () => dockerRunning,
});

test("Graphiti on the services row: up, starting, idle while Docker runs, down with why (0444)", async () => {
	expect(await graphitiHealth(checks("ok", false, true))).toEqual({ state: "up" });
	// Answering wins over a start still in flight elsewhere in this process.
	expect(await graphitiHealth(checks("ok", true, true))).toEqual({ state: "up" });
	expect(await graphitiHealth(checks("refused", true, true))).toEqual({ state: "starting" });
	// Listening but not healthy yet while Jocasta starts it is still starting.
	expect(await graphitiHealth(checks("bad", true, true))).toEqual({ state: "starting" });
	expect(await graphitiHealth(checks("refused", false, true))).toEqual({ state: "idle", note: "starts when needed" });
	expect(await graphitiHealth(checks("refused", false, false))).toEqual({
		state: "down",
		note: "Docker isn't running",
	});
	expect(await graphitiHealth(checks("bad", false, true))).toEqual({ state: "down", note: "not answering" });
});
