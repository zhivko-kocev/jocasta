//! `proctree`'s Windows reads and kills, through windows-sys: Toolhelp for pid and parent, `GetProcessTimes` for start,
//! the PEB for a working folder, `TerminateProcess` to end one.

use std::collections::{BTreeMap, HashMap};
use std::ffi::c_void;
use std::path::PathBuf;

use windows_sys::Wdk::System::Threading::NtQueryInformationProcess;
use windows_sys::Win32::Foundation::{
	CloseHandle, ERROR_ACCESS_DENIED, FILETIME, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
	CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
	GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
	PROCESS_VM_READ, TerminateProcess,
};

use super::FolderProcess;

const STILL_ACTIVE: u32 = 259;

/// An open process handle, closed when dropped.
struct Handle(HANDLE);

impl Handle {
	/// The process opened with `access`, or `None` when it can't be (gone, or not this user's).
	fn open(pid: u32, access: u32) -> Option<Self> {
		// SAFETY: plain call; a null result is the failure.
		let handle = unsafe { OpenProcess(access, 0, pid) };
		(!handle.is_null()).then_some(Self(handle))
	}
}

impl Drop for Handle {
	fn drop(&mut self) {
		// SAFETY: the handle was opened by `open` and is closed once.
		unsafe { CloseHandle(self.0) };
	}
}

/// Whether `pid` is alive: its exit code is STILL_ACTIVE (the test libuv's `uv_kill` uses), or it can't be opened
/// because it belongs to someone else.
pub fn alive(pid: u32) -> bool {
	let Some(handle) = Handle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
		// SAFETY: plain call.
		return unsafe { GetLastError() } == ERROR_ACCESS_DENIED;
	};
	let mut code = 0u32;
	// SAFETY: `code` is a valid out pointer for the call.
	unsafe { GetExitCodeProcess(handle.0, &mut code) != 0 && code == STILL_ACTIVE }
}

/// Every process as `(pid, parent pid, exe file name)`, from one Toolhelp snapshot.
fn snapshot() -> Vec<(u32, u32, String)> {
	// SAFETY: plain call; the invalid handle is the failure.
	let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
	if snap.is_null() || snap == INVALID_HANDLE_VALUE {
		return Vec::new();
	}
	let snap = Handle(snap);
	let mut out = Vec::new();
	let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..PROCESSENTRY32W::default() };
	// SAFETY: `entry` has its size set and is valid for each call.
	let mut ok = unsafe { Process32FirstW(snap.0, &mut entry) };
	while ok != 0 {
		let name = &entry.szExeFile;
		let end = name.iter().position(|unit| *unit == 0).unwrap_or(name.len());
		out.push((entry.th32ProcessID, entry.th32ParentProcessID, String::from_utf16_lossy(&name[..end])));
		// SAFETY: as above.
		ok = unsafe { Process32NextW(snap.0, &mut entry) };
	}
	out
}

/// `size` bytes of the process's memory at `address`, or `None` when they can't be read.
fn read(handle: &Handle, address: u64, size: usize) -> Option<Vec<u8>> {
	let mut buffer = vec![0u8; size];
	// SAFETY: `buffer` is `size` bytes long; the address is another process's, which the call checks.
	let ok = unsafe {
		ReadProcessMemory(
			handle.0,
			address as usize as *const c_void,
			buffer.as_mut_ptr().cast(),
			size,
			std::ptr::null_mut(),
		)
	};
	(ok != 0).then_some(buffer)
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
	u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap_or([0; 8]))
}

/// A process's current directory from its PEB (x64 layouts), or `None` when it can't be read.
fn cwd_of(pid: u32) -> Option<String> {
	// RTL_USER_PROCESS_PARAMETERS.CurrentDirectory.DosPath (UNICODE_STRING) @0x38.
	parameter_string(pid, 0x38)
}

/// A process's command line from its PEB (x64 layouts), or `None` when it can't be read.
fn command_line_of(pid: u32) -> Option<String> {
	// RTL_USER_PROCESS_PARAMETERS.CommandLine (UNICODE_STRING) @0x70.
	parameter_string(pid, 0x70)
}

/// The UNICODE_STRING at `offset` of the process's RTL_USER_PROCESS_PARAMETERS, read from its PEB (x64 layouts).
fn parameter_string(pid: u32, offset: u64) -> Option<String> {
	let handle = Handle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ)?;
	// PROCESS_BASIC_INFORMATION (48 bytes): PebBaseAddress @8.
	let mut basic = [0u64; 6];
	// SAFETY: `basic` is the 48 bytes the call fills.
	let status = unsafe { NtQueryInformationProcess(handle.0, 0, basic.as_mut_ptr().cast(), 48, std::ptr::null_mut()) };
	if status != 0 || basic[1] == 0 {
		return None;
	}
	// PEB.ProcessParameters @0x20; a UNICODE_STRING is Length (u16) @0, Buffer @8.
	let params = u64_at(&read(&handle, basic[1] + 0x20, 8)?, 0);
	let string = read(&handle, params + offset, 16)?;
	let length = usize::from(u16::from_le_bytes([string[0], string[1]]));
	let buffer = u64_at(&string, 8);
	if length == 0 || buffer == 0 {
		return None;
	}
	let text = read(&handle, buffer, length)?;
	let units: Vec<u16> = text.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
	Some(String::from_utf16_lossy(&units))
}

/// A Windows command line as the arguments the C runtime (and Bun) read from it: whitespace separates them outside
/// double quotes, `\"` is a quote, `2n` backslashes before a quote are `n` and open or close one, `2n+1` are `n` and a
/// quote.
pub fn split_command_line(line: &str) -> Vec<String> {
	let mut args = Vec::new();
	let mut current = String::new();
	let mut started = false;
	let mut quoted = false;
	let mut chars = line.chars().peekable();
	while let Some(c) = chars.next() {
		match c {
			'\\' => {
				let mut slashes = 1;
				while chars.next_if_eq(&'\\').is_some() {
					slashes += 1;
				}
				started = true;
				if chars.next_if_eq(&'"').is_some() {
					current.extend(std::iter::repeat_n('\\', slashes / 2));
					if slashes % 2 == 1 {
						current.push('"');
					} else {
						quoted = !quoted;
					}
				} else {
					current.extend(std::iter::repeat_n('\\', slashes));
				}
			}
			'"' => {
				started = true;
				quoted = !quoted;
			}
			c if c.is_whitespace() && !quoted => {
				if started {
					args.push(std::mem::take(&mut current));
					started = false;
				}
			}
			c => {
				started = true;
				current.push(c);
			}
		}
	}
	if started {
		args.push(current);
	}
	args
}

pub fn parents(_groups: Option<&mut HashMap<u32, u32>>) -> BTreeMap<u32, u32> {
	snapshot().into_iter().map(|(pid, ppid, _)| (pid, ppid)).collect()
}

/// The arguments of each of `pids` from its command line (the first is the program); one this user can't read is left
/// out.
pub fn args(pids: &[u32]) -> HashMap<u32, Vec<String>> {
	pids.iter().filter_map(|pid| Some((*pid, split_command_line(&command_line_of(*pid)?)))).collect()
}

pub fn cwds() -> Vec<FolderProcess> {
	let mut found = Vec::new();
	for (pid, _, name) in snapshot() {
		if pid == 0 || pid == 4 {
			continue;
		}
		if let Some(cwd) = cwd_of(pid) {
			found.push(FolderProcess {
				pid,
				name: if name.is_empty() { "?".into() } else { name },
				cwd: PathBuf::from(cwd),
			});
		}
	}
	found
}

/// A process's creation time (FILETIME: 100 ns units since 1601); `Some(None)` when it ended, `None` when it can't be
/// opened.
fn start(pid: u32) -> Option<Option<f64>> {
	let handle = Handle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
	// Ended: an exit code other than STILL_ACTIVE, the test `pid_alive` (libuv) uses, which TerminateProcess sets
	// before the exit time appears; or an exit time (an open handle to an ended process still opens).
	let mut code = 0u32;
	// SAFETY: `code` is a valid out pointer for the call.
	if unsafe { GetExitCodeProcess(handle.0, &mut code) } != 0 && code != STILL_ACTIVE {
		return Some(None);
	}
	// Creation, exit, kernel and user time.
	let mut times = [FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }; 4];
	// SAFETY: each pointer is one FILETIME of `times`.
	let ok = unsafe {
		let [creation, exit, kernel, user] = &mut times;
		GetProcessTimes(handle.0, creation, exit, kernel, user)
	};
	if ok == 0 {
		return None;
	}
	let filetime = |time: &FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
	if filetime(&times[1]) != 0 {
		return Some(None);
	}
	let created = filetime(&times[0]);
	(created != 0).then_some(Some(created as f64))
}

pub fn starts(pids: &[u32]) -> HashMap<u32, Option<f64>> {
	pids.iter().filter_map(|pid| start(*pid).map(|start| (*pid, start))).collect()
}

/// `TerminateProcess` on `pid` (exit code 1, as Node's `process.kill(pid, "SIGKILL")` does).
pub fn kill(pid: u32) {
	if let Some(handle) = Handle::open(pid, PROCESS_TERMINATE) {
		// SAFETY: the handle is open with PROCESS_TERMINATE.
		unsafe { TerminateProcess(handle.0, 1) };
	}
}

/// Windows has no process groups here.
pub fn kill_group(_pid: u32) {}
