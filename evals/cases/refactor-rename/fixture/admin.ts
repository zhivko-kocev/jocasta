import { fetchUsr } from "./api.ts";

/** Whether `id` names an admin. */
export async function isAdmin(id: string): Promise<boolean> {
	return (await fetchUsr(id))?.admin ?? false;
}
