//! `fs.mkdtempSync(join(tmpdir(), prefix))`: a fresh folder under the system's temp folder, its name `prefix` and six
//! random characters.

use std::fs;
use std::path::PathBuf;

use jocasta::shared::errors::Result;

pub fn mkdtemp(prefix: &str) -> Result<PathBuf> {
	let suffix = uuid::Uuid::new_v4().simple().to_string();
	let dir = std::env::temp_dir().join(format!("{prefix}{}", &suffix[..6]));
	fs::create_dir_all(&dir)?;
	Ok(dir)
}
