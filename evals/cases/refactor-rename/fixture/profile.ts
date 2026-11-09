import { fetchUsr } from "./api.ts";

/** The profile line for `id`. */
export async function profileLine(id: string): Promise<string> {
	const user = await fetchUsr(id);
	return user ? `${user.name} (${user.id})` : "unknown user";
}
