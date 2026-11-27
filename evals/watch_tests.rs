use super::linux::{child_pids, vm_rss_bytes};

#[test]
fn vm_rss_reads_the_kernels_kib_as_bytes() {
	let status = "Name:\tomp\nVmPeak:\t  900000 kB\nVmRSS:\t  123456 kB\nRssAnon:\t   100000 kB\n";
	assert_eq!(vm_rss_bytes(status), Some(123_456 * 1024));
}

#[test]
fn vm_rss_is_none_without_the_line() {
	// A kernel thread's status has no VmRSS; a process gone mid-read gives an empty text.
	assert_eq!(vm_rss_bytes("Name:\tkthreadd\nState:\tS (sleeping)\n"), None);
	assert_eq!(vm_rss_bytes(""), None);
	assert_eq!(vm_rss_bytes("VmRSS:\t  x kB\n"), None);
}

#[test]
fn child_pids_reads_the_space_separated_list() {
	assert_eq!(child_pids("2029890 2029891 \n"), vec![2_029_890, 2_029_891]);
	assert!(child_pids("").is_empty());
}

/// On Linux a watched run's peak is read: a shell with two children, sampled while they sleep.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn linux_peak_is_read() {
	use std::collections::HashMap;
	use std::time::Duration;

	let env: HashMap<String, String> = std::env::vars().filter(|(name, _)| name == "PATH").collect();
	let argv: Vec<String> = ["sh", "-c", "sleep 3 & sleep 3 & wait"].iter().map(|word| (*word).to_owned()).collect();
	let watched = super::run_watched(&argv, &std::env::temp_dir(), &env, Duration::from_secs(20)).await.unwrap();
	assert_eq!(watched.code, Some(0));
	assert!(watched.peak_tree_bytes.is_some_and(|peak| peak > 0), "{:?}", watched.peak_tree_bytes);
	assert!(super::linux::tree_rss_bytes(std::process::id()).is_some_and(|bytes| bytes > 0));
}
