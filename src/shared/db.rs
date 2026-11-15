//! Jocasta's SQLite files: how one is opened and migrated (0099, 0100, 0101).

use std::fs;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, TransactionBehavior};

use crate::shared::errors::{Error, Result};

/// RFC3339 UTC with milliseconds (0100).
pub fn now_iso() -> String {
	chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

const BUSY_TIMEOUT_MS: u64 = 5000;

/// A DB that can't be opened, is newer than this Jocasta, or stays locked (0101): the TypeScript's `DbError`. Its
/// message is the one-line text after `jocasta: `; the caller prints it and exits 1.
fn db_error(message: String) -> Error {
	Error::user_with_code(message, 1)
}

fn read_version(db: &Connection) -> Result<usize> {
	let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
	Ok(usize::try_from(version).unwrap_or(0))
}

fn refuse_newer(path: &Path, version: usize, known: usize) -> Result<()> {
	if version > known {
		return Err(db_error(format!(
			"{} is schema {version}; this Jocasta knows up to {known}. Update Jocasta.",
			path.display()
		)));
	}
	Ok(())
}

/// Runs the migrations a file lacks, each in its own transaction.
fn migrate(db: &mut Connection, path: &Path, migrations: &[&str], version: usize) -> Result<()> {
	for (next, sql) in migrations.iter().enumerate().skip(version) {
		// Each step takes the write lock first and reads the version again under it: another process opening the same
		// old file at once may have run this step already (SHR-13), and then it is skipped.
		let step = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
		let now = read_version(&step)?;
		refuse_newer(path, now, migrations.len())?;
		if now != next {
			continue;
		}
		step.execute_batch(sql)?;
		step.execute_batch(&format!("PRAGMA user_version = {}", next + 1))?;
		step.commit()?;
	}
	Ok(())
}

fn open_and_migrate(path: &Path, migrations: &[&str], busy_timeout_ms: u64) -> Result<Connection> {
	let mut db = Connection::open(path)?;
	db.execute_batch(&format!("PRAGMA busy_timeout = {busy_timeout_ms}"))?;
	// The version is read before anything writes, so a newer file is refused untouched (0101).
	let version = read_version(&db)?;
	refuse_newer(path, version, migrations.len())?;
	db.execute_batch("PRAGMA journal_mode = WAL")?;
	db.execute_batch("PRAGMA synchronous = NORMAL")?;
	db.execute_batch("PRAGMA foreign_keys = ON")?;
	migrate(&mut db, path, migrations, version)?;
	Ok(db)
}

/// Opens a Jocasta DB file: the connection settings in 0099's order, then the numbered migrations (0100).
/// `migrations[i]` takes the file from `user_version` i to i + 1, each in its own transaction.
/// Never deletes, recreates or downgrades a file (0101). `busy_timeout_ms` (5000 by default) is for tests only.
pub fn open_db(path: &Path, migrations: &[&str], busy_timeout_ms: Option<u64>) -> Result<Connection> {
	if let Some(dir) = path.parent() {
		fs::create_dir_all(dir)?;
	}
	let busy_timeout_ms = busy_timeout_ms.unwrap_or(BUSY_TIMEOUT_MS);
	open_and_migrate(path, migrations, busy_timeout_ms).map_err(|error| db_failure(path, error, busy_timeout_ms))
}

/// Maps a driver error to 0101's messages; anything else passes through.
fn db_failure(path: &Path, error: Error, busy_timeout_ms: u64) -> Error {
	let Error::Sqlite(failure) = &error else { return error };
	let Some(code) = failure.sqlite_error_code() else { return error };
	if matches!(code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) {
		let waited = Duration::from_millis(busy_timeout_ms).as_secs_f64();
		return db_error(format!("{} is locked by another Jocasta process (waited {waited} s).", path.display()));
	}
	db_error(format!("{} can't be opened: {failure}. It was left as is.", path.display()))
}

#[cfg(test)]
mod tests {
	use std::sync::mpsc;

	use super::*;
	use crate::shared::schema::{CATALOG_MIGRATIONS, open_catalog};
	use crate::shared::testing::temp_jocasta;

	const MIGRATIONS: [&str; 2] = ["CREATE TABLE a (x INTEGER) STRICT;", "CREATE TABLE b (y TEXT) STRICT;"];

	fn pragma<T: rusqlite::types::FromSql>(db: &Connection, name: &str) -> T {
		db.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0)).unwrap()
	}

	fn tables(db: &Connection) -> Vec<String> {
		let mut statement = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").unwrap();
		statement.query_map([], |row| row.get(0)).unwrap().map(|name| name.unwrap()).collect()
	}

	fn message(error: Error) -> String {
		let Error::User { message, exit_code } = error else { panic!("not a DbError: {error:?}") };
		assert_eq!(exit_code, 1);
		message
	}

	#[test]
	fn runs_migrations_in_order_records_user_version_and_applies_0099s_connection_settings() {
		let temp = temp_jocasta();
		let db = open_db(&temp.root().join("x.db"), &MIGRATIONS, None).unwrap();
		assert_eq!(pragma::<i64>(&db, "user_version"), 2);
		assert_eq!(pragma::<String>(&db, "journal_mode"), "wal");
		assert_eq!(pragma::<i64>(&db, "foreign_keys"), 1);
		assert_eq!(pragma::<i64>(&db, "synchronous"), 1); // NORMAL
		assert_eq!(pragma::<i64>(&db, "busy_timeout"), 5000);
	}

	#[test]
	fn an_older_file_only_runs_the_migrations_it_lacks() {
		let temp = temp_jocasta();
		let path = temp.root().join("x.db");
		open_db(&path, &MIGRATIONS[..1], None).unwrap();
		let db = open_db(&path, &MIGRATIONS, None).unwrap();
		assert_eq!(pragma::<i64>(&db, "user_version"), 2);
		assert_eq!(tables(&db), ["a", "b"]);
	}

	#[test]
	fn a_failing_migration_rolls_back_and_leaves_the_version_where_it_was() {
		let temp = temp_jocasta();
		let path = temp.root().join("x.db");
		let failing = [MIGRATIONS[0], "CREATE TABLE c (z TEXT) STRICT; CREATE TABLE a (x INT);"];
		assert!(open_db(&path, &failing, None).is_err());
		let raw = Connection::open(&path).unwrap();
		assert_eq!(pragma::<i64>(&raw, "user_version"), 1);
		assert!(!tables(&raw).contains(&"c".to_owned()));
	}

	#[test]
	fn a_second_connection_migrating_the_same_old_file_at_once_makes_the_first_skip_that_step_not_fail() {
		// SHR-13. Another connection takes the write lock and runs migration 0 while this one has already read
		// user_version 0. It commits after a real delay: open_db blocks this thread on the lock, so nothing here can
		// tell it when to let go.
		let temp = temp_jocasta();
		let path = temp.root().join("x.db");
		open_db(&path, &[], None).unwrap();
		let (held, holding) = mpsc::channel();
		let holder_path = path.clone();
		let holder = std::thread::spawn(move || {
			let db = Connection::open(holder_path).unwrap();
			db.execute_batch("BEGIN IMMEDIATE").unwrap();
			db.execute_batch(MIGRATIONS[0]).unwrap();
			db.execute_batch("PRAGMA user_version = 1").unwrap();
			held.send(()).unwrap();
			std::thread::sleep(Duration::from_millis(500));
			db.execute_batch("COMMIT").unwrap();
		});
		holding.recv().unwrap();
		let db = open_db(&path, &MIGRATIONS, None).unwrap();
		assert_eq!(pragma::<i64>(&db, "user_version"), 2);
		assert_eq!(tables(&db), ["a", "b"]);
		holder.join().unwrap();
	}

	#[test]
	fn a_newer_file_is_refused_with_0101s_message_and_left_untouched() {
		let temp = temp_jocasta();
		let path = temp.root().join("catalog.db");
		let raw = Connection::open(&path).unwrap();
		raw.execute_batch("PRAGMA user_version = 7").unwrap();
		drop(raw);
		let before = fs::read(&path).unwrap();
		let error = open_db(&path, &MIGRATIONS, None).unwrap_err();
		assert_eq!(
			message(error),
			format!("{} is schema 7; this Jocasta knows up to 2. Update Jocasta.", path.display())
		);
		assert_eq!(fs::read(&path).unwrap(), before);
	}

	#[test]
	fn a_file_that_isnt_a_database_is_reported_and_never_rewritten() {
		let temp = temp_jocasta();
		let path = temp.root().join("project.db");
		fs::write(&path, "not a database, just text that is long enough to have a header ".repeat(4)).unwrap();
		let before = fs::read(&path).unwrap();
		let text = message(open_db(&path, &MIGRATIONS, None).unwrap_err());
		let re = regex::Regex::new(r"project\.db can't be opened: .*not a database.*\. It was left as is\.").unwrap();
		assert!(re.is_match(&text), "{text}");
		assert_eq!(fs::read(&path).unwrap(), before);
	}

	#[test]
	fn a_write_lock_held_past_busy_timeout_is_reported_as_locked() {
		let temp = temp_jocasta();
		let path = temp.root().join("catalog.db");
		open_db(&path, &[], None).unwrap();
		let holder = Connection::open(&path).unwrap();
		holder.execute_batch("PRAGMA journal_mode = WAL").unwrap();
		holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
		let started = std::time::Instant::now();
		let text = message(open_db(&path, &MIGRATIONS, Some(200)).unwrap_err());
		assert_eq!(text, format!("{} is locked by another Jocasta process (waited 0.2 s).", path.display()));
		assert!(started.elapsed() >= Duration::from_millis(150));
		holder.execute_batch("ROLLBACK").unwrap();
	}

	#[test]
	fn catalog_db_opens_at_its_migration_ledger_version_with_strict_tables() {
		let _temp = temp_jocasta();
		let catalog = open_catalog().unwrap();
		assert_eq!(pragma::<i64>(&catalog, "user_version"), CATALOG_MIGRATIONS.len() as i64);
		let strict = catalog
			.execute(
				"INSERT INTO project_paths (path, project_id, role, git_config_mtime, added_at) VALUES ('x', 'p', 'main', 'soon', 'now')",
				[],
			)
			.unwrap_err();
		assert!(strict.to_string().contains("cannot store TEXT value in INTEGER column"), "{strict}"); // STRICT (0100)
		// Only the home thread lives in catalog.db (0125).
		catalog
			.execute(
				"INSERT INTO threads (id, kind, title, state, created_at, last_active_at) VALUES ('t-1', 'project', 'x', 'open', 'a', 'a')",
				[],
			)
			.unwrap_err();
	}

	#[test]
	fn catalog_v9_queues_the_summaries_of_known_projects_to_be_indexed_and_leaves_the_rest_as_they_were() {
		let temp = temp_jocasta();
		let path = temp.root().join("catalog.db");
		let v8 = open_db(&path, &CATALOG_MIGRATIONS[..8], None).unwrap();
		let insert = "INSERT INTO summaries (session_file, entry_id, kind, cwd, at, summary, sent, project_id) VALUES (?1, ?2, 'compact', 'C:\\work', 1, 'A summary of the work.', 1, ?3)";
		v8.execute(insert, rusqlite::params!["a.jsonl", "known", "p-demo"]).unwrap();
		v8.execute(insert, rusqlite::params!["a.jsonl", "unknown", rusqlite::types::Null]).unwrap();
		drop(v8);
		let catalog = open_db(&path, &CATALOG_MIGRATIONS, None).unwrap();
		let mut statement = catalog.prepare("SELECT entry_id, indexed FROM summaries ORDER BY entry_id").unwrap();
		let rows: Vec<(String, i64)> =
			statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().map(|row| row.unwrap()).collect();
		assert_eq!(rows, [("known".to_owned(), 0), ("unknown".to_owned(), 1)]);
		let mut statement = catalog.prepare("SELECT name FROM sqlite_master WHERE type = 'index'").unwrap();
		let indexes: Vec<String> =
			statement.query_map([], |row| row.get(0)).unwrap().map(|name| name.unwrap()).collect();
		assert!(indexes.contains(&"summaries_unindexed".to_owned()));
		assert!(!indexes.contains(&"summaries_unsent".to_owned()));
		assert_eq!(pragma_count(&catalog), 0);
	}

	fn pragma_count(db: &Connection) -> i64 {
		db.query_row("SELECT count(*) AS n FROM history", [], |row| row.get(0)).unwrap()
	}
}
