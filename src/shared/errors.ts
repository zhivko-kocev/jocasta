/** A refusal Jocasta explains in one line after `jocasta: `; the CLI exits with `exitCode` (0140: 2 = invocation error). */
export class UserError extends Error {
	override name = "UserError";
	constructor(
		message: string,
		readonly exitCode = 2,
	) {
		super(message);
	}
}
