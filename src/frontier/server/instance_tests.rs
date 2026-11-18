use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::shared::paths::data_dir;
use crate::shared::proctree::tests::sleeper_tree;
use crate::shared::proctree::{end_processes, pid_alive};
use crate::shared::testing::{temp_dir, temp_jocasta};

fn lock_file() -> PathBuf {
	let dir = data_dir();
	fs::create_dir_all(&dir).unwrap();
	dir.join("service.lock")
}

fn this_process() -> ProcessId {
	process_id_of(std::process::id()).expect("this process's start")
}

fn read_json(file: &Path) -> Value {
	serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
}

async fn down() -> ServiceState {
	ServiceState::Down
}

async fn nothing_killed(_holder: ProcessId, _spare: Vec<ProcessId>) {}

#[tokio::test]
async fn a_free_lock_is_taken_and_given_back_a_dead_holders_lock_is_taken_at_once() {
	let _j = temp_jocasta();
	let file = lock_file();
	let own = this_process();
	let mine = take_instance(InstanceOptions::new(&file, down, nothing_killed)).await.unwrap().expect("the lock");
	assert_eq!(read_json(&file), id_json(own));
	// The herdr server it starts is noted beside it (0582).
	mine.note_herdr(ProcessId { pid: 4242, start: 7.0 }).unwrap();
	let mut expected = id_json(own);
	expected["herdr"] = json!({ "pid": 4242, "start": 7 });
	assert_eq!(read_json(&file), expected);
	mine.release().unwrap();
	assert!(!file.exists());
	// A crashed holder (its pid runs no more, or another program reuses it with another start time).
	fs::write(&file, json!({ "pid": std::process::id(), "start": 1 }).to_string()).unwrap();
	let busy = || async { ServiceState::Busy };
	let again = take_instance(InstanceOptions::new(&file, busy, nothing_killed)).await.unwrap().expect("the lock");
	again.release().unwrap();
}

#[tokio::test]
async fn a_holder_that_answers_keeps_the_lock_this_start_leaves() {
	let _j = temp_jocasta();
	let file = lock_file();
	let other = this_process();
	fs::write(&file, id_json(other).to_string()).unwrap();
	let killed = Arc::new(AtomicUsize::new(0));
	let count = killed.clone();
	let mut options = InstanceOptions::new(
		&file,
		|| async { ServiceState::Up },
		move |_, _| {
			count.fetch_add(1, Ordering::SeqCst);
			async {}
		},
	);
	options.self_id = Some(ProcessId { pid: other.pid + 1, start: 1.0 });
	assert!(take_instance(options).await.unwrap().is_none());
	assert_eq!(killed.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_live_holder_that_doesnt_answer_within_the_bound_is_hung_its_tree_is_ended_and_the_lock_taken_over_0534() {
	let _j = temp_jocasta();
	let file = lock_file();
	let folder = temp_dir("instance-hung");
	// A hung service: a live process holding the lock whose /health never answers (`busy`).
	let tree = sleeper_tree(folder.path(), false).await;
	let holder = process_id_of(tree.root).expect("the stand-in service's start");
	fs::write(&file, id_json(holder).to_string()).unwrap();
	let asked = Arc::new(AtomicUsize::new(0));
	let count = asked.clone();
	let mut options = InstanceOptions::new(
		&file,
		move || {
			count.fetch_add(1, Ordering::SeqCst);
			async { ServiceState::Busy }
		},
		|holder, spare| async move {
			end_processes(&[holder], &spare).await;
		},
	);
	options.hung_ms = Some(300);
	options.sleep = Some(Box::new(|ms| Box::pin(tokio::time::sleep(Duration::from_millis(ms.min(50))))));
	let started = Instant::now();
	let mine = take_instance(options).await.unwrap().expect("the lock");
	assert!(started.elapsed() >= Duration::from_millis(300));
	assert!(asked.load(Ordering::SeqCst) > 1);
	assert!(!pid_alive(tree.root));
	assert_eq!(read_json(&file), id_json(this_process()));
	mine.release().unwrap();
}

#[tokio::test]
async fn a_hung_services_takeover_spares_the_herdr_server_it_started_and_what_runs_under_it_0582() {
	let _j = temp_jocasta();
	let file = lock_file();
	let folder = temp_dir("instance-spare");
	// A hung service that started a "herdr server" (its child), which runs a "pane" of its own (its child): the
	// sleeper tree's root, child and grandchild.
	let tree = sleeper_tree(folder.path(), true).await;
	let depth = |pid: u32| fs::read_to_string(folder.path().join("pids").join(pid.to_string())).unwrap();
	let at = |wanted: &str| *tree.all.iter().find(|pid| depth(**pid) == wanted).expect("a stand-in at that depth");
	let (service, herdr, pane) = (at("0"), at("1"), at("2"));
	let holder = process_id_of(service).expect("the service's start");
	let herdr_id = process_id_of(herdr).expect("herdr's start");
	let mut value = id_json(holder);
	value["herdr"] = id_json(herdr_id);
	fs::write(&file, value.to_string()).unwrap();
	let mut options = InstanceOptions::new(
		&file,
		|| async { ServiceState::Busy },
		|holder, spare| async move {
			end_processes(&[holder], &spare).await;
		},
	);
	options.hung_ms = Some(0);
	options.sleep = Some(Box::new(|_| Box::pin(async {})));
	let mine = take_instance(options).await.unwrap().expect("the lock");
	assert!(!pid_alive(service));
	assert!(pid_alive(herdr));
	assert!(pid_alive(pane));
	mine.release().unwrap();
}

/// A Windows process start is a FILETIME above 2^53, so the lock file holds it as a float; it must read back exactly,
/// or a live holder looks like another run (serde_json's `float_roundtrip`). This start once read back one ULP off.
#[test]
fn a_filetime_sized_start_reads_back_exactly_from_the_lock_file() {
	let folder = temp_dir("instance-start");
	let file = folder.path().join("service.lock");
	let id = ProcessId { pid: 31208, start: 1.343_923_469_574_569_1e17 };
	let mut value = id_json(id);
	value["herdr"] = id_json(ProcessId { pid: 4242, start: 1.343_923_469_574_569_3e17 });
	fs::write(&file, value.to_string()).unwrap();
	let Read::Held(holder) = read_holder(&file) else { panic!("the lock reads as held") };
	assert_eq!(holder.id.start.to_bits(), id.start.to_bits());
	assert_eq!(holder.herdr.expect("herdr's id").start.to_bits(), 1.343_923_469_574_569_3e17_f64.to_bits());
}
