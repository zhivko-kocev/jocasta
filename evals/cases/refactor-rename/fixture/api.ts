export interface User {
	id: string;
	name: string;
	admin: boolean;
}

const users: Record<string, User> = {
	u1: { id: "u1", name: "Ada at acme", admin: true },
	u2: { id: "u2", name: "Ben at client-a", admin: false },
};

/** The user with `id`, or undefined. */
export async function fetchUsr(id: string): Promise<User | undefined> {
	return users[id];
}
