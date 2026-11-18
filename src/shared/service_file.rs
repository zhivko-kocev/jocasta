//! `<data>/jocasta-service.json` (0254, 0584): where the running service listens, its token and its pid.

use std::error::Error as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::shared::errors::Result;
use crate::shared::fsx::{read_json_object, write_atomically};
use crate::shared::paths::data_dir;
use crate::shared::proctree::pid_alive;

/// `<data>/jocasta-service.json` (0254, 0584): where the running service listens, its token and its pid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceFile {
	pub port: u16,
	pub token: String,
	pub pid: u32,
}

/// The service named by `<data>/jocasta-service.json` while its process lives: its port and token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningService {
	pub port: u16,
	pub token: String,
}

/// The file's name before 0584; read when the new one isn't there (a service an older build started).
const OLD_SERVICE_FILE: &str = "frontier.json";

/// The file in `dir` (the data folder by default).
pub fn service_file_path(dir: Option<&Path>) -> PathBuf {
	dir.map_or_else(data_dir, Path::to_path_buf).join("jocasta-service.json")
}

/// Writes the file atomically, so a second `jocasta` never reads half of it, and private to this user (0600). `dir` is
/// the data folder (the benchmark's layer writes a trial's).
pub fn write_service_file(file: &ServiceFile, dir: Option<&Path>) -> Result<()> {
	write_atomically(&service_file_path(dir), format!("{}\n", serde_json::to_string(file)?), Some(0o600))
}

fn read_at(path: &Path) -> Option<ServiceFile> {
	let object = read_json_object(path)?;
	Some(ServiceFile {
		port: u16::try_from(object.get("port")?.as_u64()?).ok()?,
		token: object.get("token")?.as_str()?.to_owned(),
		pid: u32::try_from(object.get("pid")?.as_u64()?).ok()?,
	})
}

/// The file, else the old `frontier.json` (0584), or `None` when neither is one (a stale or torn file just means no
/// server).
pub fn read_service_file() -> Option<ServiceFile> {
	read_at(&service_file_path(None)).or_else(|| read_at(&data_dir().join(OLD_SERVICE_FILE)))
}

/// Removes the file when it still names this server's process.
pub fn remove_service_file(pid: u32) {
	let path = service_file_path(None);
	if read_at(&path).is_some_and(|file| file.pid == pid) {
		let _ = fs::remove_file(path);
	}
}

/// The service named by `<data>/jocasta-service.json` while its process lives: its port and token.
pub fn running_service() -> Option<RunningService> {
	read_service_file()
		.filter(|file| pid_alive(file.pid))
		.map(|file| RunningService { port: file.port, token: file.token })
}

/// The one text wherever Jocasta's service isn't running (0581 UX-13).
pub const NOT_RUNNING: &str = "Jocasta isn't running; start it with jocasta.";

/// Whether a request to the service failed because nothing listens on its port.
pub fn connection_refused(error: &reqwest::Error) -> bool {
	let mut cause = error.source();
	while let Some(inner) = cause {
		if inner.downcast_ref::<io::Error>().is_some_and(|io| io.kind() == io::ErrorKind::ConnectionRefused) {
			return true;
		}
		cause = inner.source();
	}
	false
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_jocasta;

	#[test]
	fn the_file_is_written_whole_with_its_keys_in_order_and_read_back() {
		let _j = temp_jocasta();
		let file = ServiceFile { port: 47400, token: "tok".to_owned(), pid: std::process::id() };
		write_service_file(&file, None).unwrap();
		assert_eq!(
			fs::read_to_string(service_file_path(None)).unwrap(),
			format!("{{\"port\":47400,\"token\":\"tok\",\"pid\":{}}}\n", std::process::id())
		);
		assert_eq!(read_service_file(), Some(file.clone()));
		assert_eq!(running_service(), Some(RunningService { port: 47400, token: "tok".to_owned() }));
	}

	#[test]
	fn a_torn_or_missing_file_means_no_server_and_the_old_name_is_read_when_the_new_one_is_not_there() {
		let _j = temp_jocasta();
		assert_eq!(read_service_file(), None);
		fs::create_dir_all(data_dir()).unwrap();
		fs::write(service_file_path(None), "{\"port\":1,\"tok").unwrap();
		assert_eq!(read_service_file(), None);
		fs::write(service_file_path(None), "{\"port\":\"1\",\"token\":\"t\",\"pid\":2}").unwrap();
		assert_eq!(read_service_file(), None);
		fs::remove_file(service_file_path(None)).unwrap();
		fs::write(data_dir().join(OLD_SERVICE_FILE), "{\"port\":7,\"token\":\"old\",\"pid\":3}").unwrap();
		assert_eq!(read_service_file(), Some(ServiceFile { port: 7, token: "old".to_owned(), pid: 3 }));
	}

	#[test]
	fn a_file_naming_a_dead_process_is_not_a_running_service() {
		let _j = temp_jocasta();
		write_service_file(&ServiceFile { port: 1, token: "t".to_owned(), pid: 2_u32.pow(31) - 2 }, None).unwrap();
		assert_eq!(running_service(), None);
	}

	#[test]
	fn the_file_is_removed_only_while_it_names_this_servers_process() {
		let _j = temp_jocasta();
		write_service_file(&ServiceFile { port: 1, token: "t".to_owned(), pid: 41 }, None).unwrap();
		remove_service_file(42);
		assert!(service_file_path(None).exists());
		remove_service_file(41);
		assert!(!service_file_path(None).exists());
	}

	#[test]
	fn a_given_folder_holds_the_file_there() {
		let dir = crate::shared::testing::temp_dir("service-file");
		write_service_file(&ServiceFile { port: 5, token: "t".to_owned(), pid: 6 }, Some(dir.path())).unwrap();
		assert!(dir.path().join("jocasta-service.json").exists());
	}

	#[tokio::test]
	async fn a_refused_connection_is_told_from_other_failures() {
		let port = crate::shared::fsx::free_port().unwrap();
		let refused = reqwest::Client::new().get(format!("http://127.0.0.1:{port}/")).send().await.unwrap_err();
		assert!(connection_refused(&refused));
	}
}
