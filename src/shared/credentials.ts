/**
 * Windows Credential Manager (0430): generic credentials read and written through `advapi32.dll`'s `CredReadW`,
 * `CredWriteW` and `CredDeleteW` with Bun's FFI (no dependency). The calendar's secret iCal URL lives in target
 * `jocasta/calendar-ical`; the env var `JOCASTA_CALENDAR_ICAL_SECRET` stays a fallback (0301), taken out of this
 * process's environment at the service's start so no child (herdr and its panes, whiteboard, docker, omp) inherits
 * it (SEC-5). Off Windows there is no store: reads find nothing and writes refuse.
 */
import { dlopen, FFIType, type Pointer, ptr, read, toArrayBuffer } from "bun:ffi";

/** The calendar secret's Credential Manager target (0430). */
export const CALENDAR_CREDENTIAL = "jocasta/calendar-ical";

/** The env var the calendar secret may still come from (0301, now a fallback). */
export const CALENDAR_ENV = "JOCASTA_CALENDAR_ICAL_SECRET";

const CRED_TYPE_GENERIC = 1;
const CRED_PERSIST_LOCAL_MACHINE = 2;
/** `CREDENTIALW` on 64-bit Windows: offsets of the fields used, and its size. */
const CREDENTIAL = { type: 4, targetName: 8, blobSize: 32, blob: 40, persist: 48, userName: 72, size: 80 } as const;
/** `CRED_MAX_CREDENTIAL_BLOB_SIZE`. */
const MAX_BLOB = 5 * 512;

type Advapi = ReturnType<typeof openAdvapi>;
let advapi: Advapi | undefined;

function openAdvapi() {
	return dlopen("advapi32.dll", {
		CredReadW: { args: [FFIType.ptr, FFIType.u32, FFIType.u32, FFIType.ptr], returns: FFIType.i32 },
		CredWriteW: { args: [FFIType.ptr, FFIType.u32], returns: FFIType.i32 },
		CredDeleteW: { args: [FFIType.ptr, FFIType.u32, FFIType.u32], returns: FFIType.i32 },
		CredFree: { args: [FFIType.ptr], returns: FFIType.void },
	}).symbols;
}

function store(): Advapi | undefined {
	if (process.platform !== "win32" || process.arch !== "x64") return undefined;
	advapi ??= openAdvapi();
	return advapi;
}

/** A NUL-terminated UTF-16 string for a `LPCWSTR`. */
const wide = (text: string) => Buffer.from(`${text}\0`, "utf16le");

/** A blob as text: UTF-8 as written here, or UTF-16LE as `cmdkey /pass:` and the Control Panel store passwords. */
function blobText(bytes: Buffer): string {
	const utf16 =
		bytes.length >= 2 && bytes.length % 2 === 0 && bytes.subarray(1).every((b, i) => i % 2 === 1 || b === 0);
	return bytes.toString(utf16 ? "utf16le" : "utf8");
}

/** The secret of generic credential `target`, or undefined when there is none (or no store on this platform). */
export function readCredential(target: string): string | undefined {
	const api = store();
	if (!api) return undefined;
	const name = wide(target);
	const out = new BigUint64Array(1);
	if (!api.CredReadW(ptr(name), CRED_TYPE_GENERIC, 0, ptr(out))) return undefined;
	const credential = Number(out[0]) as Pointer;
	try {
		const size = read.u32(credential, CREDENTIAL.blobSize);
		const blob = read.ptr(credential, CREDENTIAL.blob) as Pointer;
		if (size === 0 || !blob) return undefined;
		const text = blobText(Buffer.from(toArrayBuffer(blob, 0, size).slice(0))).trim();
		return text || undefined;
	} finally {
		api.CredFree(credential);
	}
}

/** Writes `secret` as generic credential `target` for this user (replacing one already there). */
export function writeCredential(target: string, secret: string): void {
	const api = store();
	if (!api) throw new Error("Credential Manager is only on Windows");
	const blob = Buffer.from(secret, "utf8");
	if (blob.length === 0 || blob.length > MAX_BLOB) throw new Error(`the secret must be 1–${MAX_BLOB} bytes`);
	const name = wide(target);
	const user = wide("jocasta");
	const credential = Buffer.alloc(CREDENTIAL.size);
	credential.writeUInt32LE(CRED_TYPE_GENERIC, CREDENTIAL.type);
	credential.writeBigUInt64LE(BigInt(ptr(name)), CREDENTIAL.targetName);
	credential.writeUInt32LE(blob.length, CREDENTIAL.blobSize);
	credential.writeBigUInt64LE(BigInt(ptr(blob)), CREDENTIAL.blob);
	credential.writeUInt32LE(CRED_PERSIST_LOCAL_MACHINE, CREDENTIAL.persist);
	credential.writeBigUInt64LE(BigInt(ptr(user)), CREDENTIAL.userName);
	if (!api.CredWriteW(ptr(credential), 0)) throw new Error(`Credential Manager refused to save ${target}`);
}

/** Removes generic credential `target`; false when there was none. */
export function deleteCredential(target: string): boolean {
	const api = store();
	if (!api) return false;
	return api.CredDeleteW(ptr(wide(target)), CRED_TYPE_GENERIC, 0) !== 0;
}

/** The env fallback, once taken out of `process.env`. */
let calendarEnv: string | undefined;

/**
 * Moves `JOCASTA_CALENDAR_ICAL_SECRET` from `process.env` into this module (SEC-5) and returns it: every process
 * started afterwards (herdr's server and so every pane, whiteboard, docker, omp) inherits an environment without it.
 * The service and the `jocasta` command call it before they start anything; later calls return the value taken.
 */
export function takeCalendarEnv(): string | undefined {
	const value = process.env[CALENDAR_ENV];
	if (value !== undefined) {
		calendarEnv = value.trim() || undefined;
		delete process.env[CALENDAR_ENV];
	}
	return calendarEnv;
}

/** The calendar's secret iCal URL (0430): Credential Manager first, else the env var (0301); undefined when neither. */
export function calendarSecret(): string | undefined {
	return readCredential(CALENDAR_CREDENTIAL) ?? takeCalendarEnv();
}
