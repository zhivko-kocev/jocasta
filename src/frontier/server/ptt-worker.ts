// The service's push-to-talk key (0368, 0556): a WH_KEYBOARD_LL hook on this worker's own thread, which blocks in
// GetMessageW so Windows can call the hook (about 0.2 ms per key, measured in the 0367 spike). Every key passes on
// unchanged. The worker first posts its thread id; the service ends the loop on its stop by posting WM_QUIT to it.
import { dlopen, FFIType, JSCallback, type Pointer, ptr, read } from "bun:ffi";
import { RightCtrlHold } from "./ptt-hold.ts";

declare const self: Worker;

const WH_KEYBOARD_LL = 13;
const WM_KEYDOWN = 0x100;
const WM_SYSKEYDOWN = 0x104;

const user32 = dlopen("user32.dll", {
	SetWindowsHookExW: { args: [FFIType.i32, FFIType.function, FFIType.ptr, FFIType.u32], returns: FFIType.ptr },
	CallNextHookEx: { args: [FFIType.ptr, FFIType.i32, FFIType.u64, FFIType.ptr], returns: FFIType.i64 },
	GetMessageW: { args: [FFIType.ptr, FFIType.ptr, FFIType.u32, FFIType.u32], returns: FFIType.i32 },
	UnhookWindowsHookEx: { args: [FFIType.ptr], returns: FFIType.i32 },
});
const kernel32 = dlopen("kernel32.dll", {
	GetModuleHandleW: { args: [FFIType.ptr], returns: FFIType.ptr },
	GetCurrentThreadId: { args: [], returns: FFIType.u32 },
});

const hold = new RightCtrlHold();
/** The hook handle; Bun types an FFI pointer return this way. */
let hook: Pointer | bigint | null = null;
const proc = new JSCallback(
	(code: number, message: number | bigint, info: Pointer) => {
		// KBDLLHOOKSTRUCT starts with the virtual-key code.
		if (code === 0 && info) {
			const kind = Number(message);
			const phase = hold.key(read.u32(info, 0), kind === WM_KEYDOWN || kind === WM_SYSKEYDOWN);
			if (phase) self.postMessage(phase);
		}
		return user32.symbols.CallNextHookEx(hook, code, BigInt(message), info);
	},
	{ args: [FFIType.i32, FFIType.u64, FFIType.ptr], returns: FFIType.i64 },
);
hook = user32.symbols.SetWindowsHookExW(WH_KEYBOARD_LL, proc.ptr, kernel32.symbols.GetModuleHandleW(null), 0);
if (!hook) throw new Error("the push-to-talk keyboard hook wasn't installed");
self.postMessage({ thread: kernel32.symbols.GetCurrentThreadId() });
const msg = new Uint8Array(48);
// GetMessageW returns 0 on WM_QUIT (and -1 on error): the hook goes, and the worker ends on its own.
while (user32.symbols.GetMessageW(ptr(msg), null, 0, 0) > 0) {}
user32.symbols.UnhookWindowsHookEx(hook);
proc.close();
user32.close();
kernel32.close();
