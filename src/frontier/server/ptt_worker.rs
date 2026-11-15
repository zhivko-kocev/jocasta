//! The service's push-to-talk key (0368, 0556): a WH_KEYBOARD_LL hook on a thread of its own, which blocks in
//! `GetMessageW` so Windows can call the hook (about 0.2 ms per key, measured in the 0367 spike). Every key passes on
//! unchanged. The thread first reports its id; the service ends the loop on its stop by posting WM_QUIT to it.
//! (The TypeScript runs this in a Bun Worker; here it is a std thread.) Windows only.

#![cfg(windows)]

use std::ptr::null_mut;
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
	CallNextHookEx, GetMessageW, MSG, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL,
	WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

use super::ptt_hold::{PushToTalkPhase, RightCtrlHold};
use crate::shared::errors::{Error, Result};

type Sink = Box<dyn Fn(PushToTalkPhase) + Send>;

/// What the hook procedure needs, which Windows gives it no way to carry: one hook at a time.
struct Active {
	hold: RightCtrlHold,
	sink: Sink,
}

static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);

fn active() -> MutexGuard<'static, Option<Active>> {
	ACTIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The hook procedure: every key event goes to the hold and on to the next hook unchanged.
unsafe extern "system" fn hook_proc(code: i32, message: WPARAM, info: LPARAM) -> LRESULT {
	if code == 0 && info != 0 {
		// SAFETY: for WH_KEYBOARD_LL Windows passes a pointer to a KBDLLHOOKSTRUCT in `info`, which starts with the
		// virtual-key code; it is valid for this call.
		let vk = unsafe { *(info as *const u32) };
		let down = message == WM_KEYDOWN as WPARAM || message == WM_SYSKEYDOWN as WPARAM;
		if let Some(active) = active().as_mut()
			&& let Some(phase) = active.hold.key(vk, down)
		{
			(active.sink)(phase);
		}
	}
	// SAFETY: the hook handle is ignored by CallNextHookEx; the rest is what Windows gave this procedure.
	unsafe { CallNextHookEx(null_mut(), code, message, info) }
}

/// A running push-to-talk hook; `stop` (or dropping it) ends the hook thread.
pub struct PushToTalkHook {
	thread: u32,
	join: Option<JoinHandle<()>>,
}

impl PushToTalkHook {
	/// The hook thread's id, which the TypeScript worker posts first.
	pub fn thread_id(&self) -> u32 {
		self.thread
	}

	/// Posts WM_QUIT to the thread, so its message loop ends, the hook goes and the thread exits; waits for it.
	pub fn stop(mut self) {
		self.end();
	}

	fn end(&mut self) {
		if let Some(join) = self.join.take() {
			// SAFETY: posting a message to a thread id is always sound; a thread that is gone fails the call.
			unsafe { PostThreadMessageW(self.thread, WM_QUIT, 0, 0) };
			let _ = join.join();
		}
	}
}

impl Drop for PushToTalkHook {
	fn drop(&mut self) {
		self.end();
	}
}

/// Starts the hook on a thread of its own; `on_phase` runs on that thread for every phase a key event causes, so it
/// must not block. Fails when the hook wasn't installed or one already runs.
pub fn start(on_phase: impl Fn(PushToTalkPhase) + Send + 'static) -> Result<PushToTalkHook> {
	{
		let mut slot = active();
		if slot.is_some() {
			return Err(Error::other("the push-to-talk keyboard hook already runs"));
		}
		*slot = Some(Active { hold: RightCtrlHold::new(), sink: Box::new(on_phase) });
	}
	let (ready, installed) = mpsc::channel::<Option<u32>>();
	let join = std::thread::spawn(move || run(&ready));
	match installed.recv() {
		Ok(Some(thread)) => Ok(PushToTalkHook { thread, join: Some(join) }),
		_ => {
			let _ = join.join();
			*active() = None;
			Err(Error::other("the push-to-talk keyboard hook wasn't installed"))
		}
	}
}

/// The thread's whole life: install the hook, report the thread id, pump messages until WM_QUIT, unhook.
fn run(ready: &mpsc::Sender<Option<u32>>) {
	// SAFETY: `hook_proc` has the HOOKPROC signature; a null module name is the calling executable.
	let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), GetModuleHandleW(std::ptr::null()), 0) };
	if hook.is_null() {
		let _ = ready.send(None);
		return;
	}
	// SAFETY: no arguments.
	let _ = ready.send(Some(unsafe { GetCurrentThreadId() }));
	// SAFETY: MSG is plain data; GetMessageW returns 0 on WM_QUIT (and -1 on error): the loop then ends.
	let mut msg: MSG = unsafe { std::mem::zeroed() };
	while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {}
	// SAFETY: `hook` is the handle SetWindowsHookExW returned on this thread.
	unsafe { UnhookWindowsHookEx(hook) };
	*active() = None;
}
