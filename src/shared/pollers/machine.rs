//! CPU, RAM and disk samples for the dashboard (0429). The TypeScript reads them through `node:os` and `statfsSync`; here
//! each platform reads its own source: Windows' `GetSystemTimes`, `GlobalMemoryStatusEx` and `GetDiskFreeSpaceExW`,
//! Linux's `/proc/stat` and `/proc/meminfo`, macOS' Mach `host_processor_info`, `sysctl` and `vm_stat`, and `df` for the
//! disk on POSIX.

use std::path::Path;

use crate::frontier::protocol::{Disk, Machine, Ram};
use crate::shared::errors::Result;

/// All cores' busy and total time so far, ms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
	pub busy: i64,
	pub total: i64,
}

/// The cores' busy and total time so far: user, nice, system and interrupt time are busy, idle is not (`node:os`'s
/// `cpus()` times, summed). All zero where the platform gives none.
pub fn cpu_times() -> CpuTimes {
	let (busy, total) = platform::cpu_busy_total().unwrap_or((0, 0));
	CpuTimes { busy, total }
}

/// The machine's installed memory in bytes (`os.totalmem()`; the benchmarks' conditions report it); `None` where the
/// platform doesn't tell.
pub fn total_memory_bytes() -> Option<u64> {
	platform::ram().ok().map(|(total, _)| total)
}

/// JavaScript's `Math.round`: halves go up.
fn js_round(x: f64) -> f64 {
	(x + 0.5).floor()
}

/// `part` of `total` rounded to whole percents of `total`, so a figure only changes on a whole percent (0429).
fn round_to_percent(part: u64, total: u64) -> u64 {
	if total == 0 {
		return 0;
	}
	let (part, total) = (part as f64, total as f64);
	js_round(js_round(part / total * 100.0) / 100.0 * total) as u64
}

/// One sample (0429): CPU busy fraction since `before` (all cores, to whole percents), RAM in use, and the free space
/// of the disk holding `dir`.
pub fn sample_machine(before: CpuTimes, now: CpuTimes, dir: &Path) -> Result<Machine> {
	let elapsed = now.total - before.total;
	let cpu =
		if elapsed > 0 { js_round((now.busy - before.busy) as f64 / elapsed as f64 * 100.0) / 100.0 } else { 0.0 };
	let (ram_total, ram_free) = platform::ram()?;
	let (disk_total, disk_avail) = platform::disk(dir)?;
	Ok(Machine {
		cpu: cpu.clamp(0.0, 1.0),
		ram: Ram { used: round_to_percent(ram_total.saturating_sub(ram_free), ram_total), total: ram_total },
		disk: Disk { free: round_to_percent(disk_avail, disk_total), total: disk_total },
	})
}

#[cfg(windows)]
mod platform {
	use std::os::windows::ffi::OsStrExt;
	use std::path::Path;

	use windows_sys::Win32::Foundation::FILETIME;
	use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
	use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
	use windows_sys::Win32::System::Threading::GetSystemTimes;

	use crate::shared::errors::Result;

	fn ticks(time: FILETIME) -> i64 {
		// 100 ns units to ms.
		i64::try_from((u64::from(time.dwHighDateTime) << 32 | u64::from(time.dwLowDateTime)) / 10_000)
			.unwrap_or(i64::MAX)
	}

	pub(super) fn cpu_busy_total() -> Option<(i64, i64)> {
		let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
		let (mut idle, mut kernel, mut user) = (zero, zero, zero);
		// SAFETY: three valid out pointers to FILETIMEs.
		if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
			return None;
		}
		// The kernel time holds the idle time.
		let total = ticks(kernel) + ticks(user);
		Some((total - ticks(idle), total))
	}

	pub(super) fn ram() -> Result<(u64, u64)> {
		// SAFETY: an all-zero MEMORYSTATUSEX is a valid value; `dwLength` is set before the call as it requires.
		let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
		status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
		// SAFETY: `status` is a valid, sized out pointer.
		if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
			return Err(std::io::Error::last_os_error().into());
		}
		Ok((status.ullTotalPhys, status.ullAvailPhys))
	}

	pub(super) fn disk(dir: &Path) -> Result<(u64, u64)> {
		let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
		let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
		// SAFETY: `wide` is NUL-terminated and the three out pointers are valid.
		if unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) } == 0 {
			return Err(std::io::Error::last_os_error().into());
		}
		Ok((total, avail))
	}
}

#[cfg(unix)]
mod platform {
	use std::path::Path;
	use std::process::Command;

	use crate::shared::env;
	use crate::shared::errors::{Error, Result};

	/// The output of `program`, as text.
	fn output(program: &str, args: &[&str]) -> Result<String> {
		let mut command = Command::new(program);
		command.args(args);
		env::apply_to(&mut command);
		let out = command.output()?;
		if !out.status.success() {
			return Err(Error::other(format!("{program} failed")));
		}
		Ok(String::from_utf8_lossy(&out.stdout).into_owned())
	}

	/// `df -Pk dir`: the disk's total and available bytes (block counts of 1024 bytes).
	pub(super) fn disk(dir: &Path) -> Result<(u64, u64)> {
		let text = output("df", &["-Pk", &dir.to_string_lossy()])?;
		let line = text.lines().nth(1).ok_or_else(|| Error::other("df gave no line"))?;
		let fields: Vec<&str> = line.split_whitespace().collect();
		let number = |index: usize| {
			fields.get(index).and_then(|f| f.parse::<u64>().ok()).ok_or_else(|| Error::other("df gave no number"))
		};
		Ok((number(1)? * 1024, number(3)? * 1024))
	}

	#[cfg(target_os = "linux")]
	pub(super) fn cpu_busy_total() -> Option<(i64, i64)> {
		let text = std::fs::read_to_string("/proc/stat").ok()?;
		let line = text.lines().find(|line| line.starts_with("cpu "))?;
		// user nice system idle iowait irq: the fields `node:os` reads; a tick is 10 ms.
		let f: Vec<i64> = line.split_whitespace().skip(1).take(6).filter_map(|f| f.parse().ok()).collect();
		let [user, nice, sys, idle, _iowait, irq] = f[..] else { return None };
		let busy = (user + nice + sys + irq) * 10;
		Some((busy, busy + idle * 10))
	}

	#[cfg(target_os = "linux")]
	pub(super) fn ram() -> Result<(u64, u64)> {
		let text = std::fs::read_to_string("/proc/meminfo")?;
		let kb = |name: &str| {
			text.lines()
				.find_map(|line| {
					line.strip_prefix(name)?.trim_start_matches(':').split_whitespace().next()?.parse::<u64>().ok()
				})
				.map(|kb| kb * 1024)
				.ok_or_else(|| Error::other("/proc/meminfo lacks a figure"))
		};
		Ok((kb("MemTotal")?, kb("MemAvailable")?))
	}

	#[cfg(target_os = "macos")]
	pub(super) fn ram() -> Result<(u64, u64)> {
		let total =
			output("sysctl", &["-n", "hw.memsize"])?.trim().parse::<u64>().map_err(|_| Error::other("sysctl"))?;
		let text = output("vm_stat", &[])?;
		let page = text
			.lines()
			.next()
			.and_then(|line| line.split("page size of ").nth(1))
			.and_then(|rest| rest.split_whitespace().next())
			.and_then(|n| n.parse::<u64>().ok())
			.unwrap_or(4096);
		let free = text
			.lines()
			.find_map(|line| line.strip_prefix("Pages free:")?.trim().trim_end_matches('.').parse::<u64>().ok())
			.ok_or_else(|| Error::other("vm_stat lacks the free pages"))?;
		Ok((total, free * page))
	}

	#[cfg(target_os = "macos")]
	pub(super) fn cpu_busy_total() -> Option<(i64, i64)> {
		unsafe extern "C" {
			static mach_task_self_: u32;
			fn mach_host_self() -> u32;
			fn host_processor_info(host: u32, flavor: i32, count: *mut u32, info: *mut *mut i32, size: *mut u32)
			-> i32;
			fn vm_deallocate(task: u32, address: usize, size: usize) -> i32;
		}
		const PROCESSOR_CPU_LOAD_INFO: i32 = 2;
		let (mut cores, mut info, mut size) = (0u32, std::ptr::null_mut::<i32>(), 0u32);
		// SAFETY: the out pointers are valid; on success `info` points to `size` ints Mach allocated, read below and
		// released with `vm_deallocate`.
		unsafe {
			if host_processor_info(mach_host_self(), PROCESSOR_CPU_LOAD_INFO, &mut cores, &mut info, &mut size) != 0 {
				return None;
			}
			let ticks = std::slice::from_raw_parts(info, size as usize);
			let (mut busy, mut total) = (0i64, 0i64);
			// Per core: user, system, idle, nice; a tick is 10 ms.
			for core in ticks.chunks_exact(4) {
				let (user, sys, idle, nice) =
					(i64::from(core[0]), i64::from(core[1]), i64::from(core[2]), i64::from(core[3]));
				busy += (user + sys + nice) * 10;
				total += (user + sys + idle + nice) * 10;
			}
			vm_deallocate(mach_task_self_, info as usize, size as usize * std::mem::size_of::<i32>());
			Some((busy, total))
		}
	}

	#[cfg(not(any(target_os = "linux", target_os = "macos")))]
	pub(super) fn cpu_busy_total() -> Option<(i64, i64)> {
		None
	}

	#[cfg(not(any(target_os = "linux", target_os = "macos")))]
	pub(super) fn ram() -> Result<(u64, u64)> {
		Err(Error::other("memory isn't readable on this platform"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_dir;

	#[test]
	fn machine_cpu_since_the_last_reading_ram_and_disk_in_whole_percents() {
		let dir = temp_dir("machine");
		let sample =
			sample_machine(CpuTimes { busy: 100, total: 400 }, CpuTimes { busy: 160, total: 600 }, dir.path()).unwrap();
		assert_eq!(sample.cpu, 0.3);
		assert!(sample.ram.total > 0);
		assert!(sample.ram.used > 0);
		assert!(sample.ram.used <= sample.ram.total);
		assert!(sample.disk.total > 0);
		assert!(sample.disk.free <= sample.disk.total);
		// Rounded to a whole percent of the total.
		let percent = sample.ram.used as f64 / sample.ram.total as f64 * 100.0;
		assert!((percent - percent.round()).abs() < 1e-6);
		let now = cpu_times();
		assert_eq!(sample_machine(now, now, dir.path()).unwrap().cpu, 0.0);
	}
}
