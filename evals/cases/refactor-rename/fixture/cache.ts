import { fetchUsr, type User } from "./api.ts";

const cached = new Map<string, User | undefined>();

/** `fetchUsr`, remembered per id. */
export async function cachedUser(id: string): Promise<User | undefined> {
	if (!cached.has(id)) cached.set(id, await fetchUsr(id));
	return cached.get(id);
}
