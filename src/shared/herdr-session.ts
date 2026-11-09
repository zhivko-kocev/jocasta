import { UserError } from "./errors.ts";

/**
 * The herdr session Jocasta's work surface lives in (0392): `jocasta`, or `JOCASTA_HERDR_SESSION` for live checks in a
 * throwaway session (0441).
 */
export function herdrSession(): string {
	const value = process.env.JOCASTA_HERDR_SESSION;
	if (!value) return "jocasta";
	if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(value)) {
		throw new UserError("JOCASTA_HERDR_SESSION must be letters, digits, '.', '_' or '-'", 2);
	}
	return value;
}
