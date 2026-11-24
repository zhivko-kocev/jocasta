//! The git `pre-push` hook Jocasta puts in every pane's environment (0613): which pushes need an answer, the words git
//! runs, and what the hook says to the Jocasta service. The service's side is `frontier/server/push-gate.ts`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::shared::env;

/// The internal word of the `jocasta` command git runs as the hook (0613); not in the usage text.
pub const PUSH_HOOK_ROLE: &str = "push-hook";

/// The all-zero commit git gives for a ref that doesn't exist yet (a new branch) or is being deleted.
pub static NO_COMMIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^0+$").expect("a fixed pattern"));

/// The branches no pane pushes to without an answer (0613); the project's own trunk is another when it is named otherwise.
const PROTECTED: [&str; 3] = ["main", "master", "develop"];

/// One line git gives a `pre-push` hook on stdin: a ref the push updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushedRef {
	/// The remote's ref, `refs/heads/main`.
	pub r#ref: String,
	/// The commit the push puts there; all zeros when the push deletes the ref.
	pub local: String,
	/// The commit the ref has now; all zeros when it is new.
	pub remote: String,
}

/// What the hook sends the service (a `push` message on the bridge pipe, 0675).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
	/// The folder the push runs in, a checkout or worktree.
	pub dir: PathBuf,
	/// The remote's address as git gave it.
	pub remote: String,
	pub refs: Vec<PushedRef>,
	/// The pane's `JOCASTA_PANE_KEY`, which the hook inherits from its pane.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub pane_key: Option<String>,
}

/// The service's answer: pass, or refuse with a reason for the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushAnswer {
	pub allow: bool,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub reason: Option<String>,
}

/// What `PushHookDeps::ask` returns: the answer, or why the service didn't answer.
pub type PushReply = Result<PushAnswer, String>;

/// The updated refs of a hook's stdin: `<local ref> <local sha> <remote ref> <remote sha>` a line.
pub fn parse_pushed_refs(stdin: &str) -> Vec<PushedRef> {
	let mut refs = Vec::new();
	for line in stdin.lines() {
		let mut parts = line.split_whitespace().skip(1);
		if let (Some(local), Some(r#ref), Some(remote)) = (parts.next(), parts.next(), parts.next()) {
			refs.push(PushedRef { r#ref: r#ref.to_owned(), local: local.to_owned(), remote: remote.to_owned() });
		}
	}
	refs
}

/// Whether `address` is a folder on this disk: a path or `file://`. Everything git would reach over a network
/// (`https://`, `http://`, `ssh://`, `git://`, `user@host:path`, `host:path`) is not: git's own rule, a colon before
/// any slash, except a drive letter.
pub fn is_folder_remote(address: &str) -> bool {
	if address.get(..7).is_some_and(|head| head.eq_ignore_ascii_case("file://")) {
		return true;
	}
	let Some(colon) = address.find(':').filter(|colon| *colon > 0) else { return true };
	if colon == 1 && address.as_bytes()[0].is_ascii_alphabetic() {
		return true;
	}
	let before = &address[..colon];
	before.contains('/') || before.contains('\\')
}

/// Whether `ref` is a protected branch: main, master, develop, or `trunk` (the project's own, when named otherwise).
pub fn is_protected(r#ref: &str, trunk: Option<&str>) -> bool {
	r#ref.strip_prefix("refs/heads/").is_some_and(|branch| PROTECTED.contains(&branch) || trunk == Some(branch))
}

/// `path` as git's shell reads it on this system: forward slashes, in single quotes.
fn shell_word(path: &str) -> String {
	format!("'{}'", path.replace('\\', "/").replace('\'', "'\\''"))
}

/// JavaScript's `Number.parseInt(text, 10)` for the digits it reads: leading white space, a sign, then the digits it
/// can; `None` where JavaScript gives NaN.
fn parse_int(text: &str) -> Option<i64> {
	let text = text.trim_start();
	let (negative, digits) = match text.as_bytes().first() {
		Some(b'-') => (true, &text[1..]),
		Some(b'+') => (false, &text[1..]),
		_ => (false, text),
	};
	let end = digits.bytes().take_while(u8::is_ascii_digit).count();
	let value: i64 = digits[..end].parse().ok()?;
	Some(if negative { -value } else { value })
}

/// The environment that gives git the hook (0613): `GIT_CONFIG_COUNT` and its `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n`
/// pairs for `hook.jocasta.*`, added after the entries `base` already holds (`None`: the process's own environment).
/// `command` is how `jocasta` starts (`selfCommand()`); git adds the remote's name and address.
pub fn push_hook_env(command: &[String], base: Option<&BTreeMap<String, String>>) -> BTreeMap<String, String> {
	let held = match base {
		Some(base) => base.get("GIT_CONFIG_COUNT").cloned(),
		None => env::var("GIT_CONFIG_COUNT"),
	};
	let first = held.as_deref().and_then(parse_int).filter(|held| *held > 0).unwrap_or(0);
	let words: Vec<String> = command.iter().map(|word| shell_word(word)).chain([PUSH_HOOK_ROLE.to_owned()]).collect();
	let entries = [
		("hook.jocasta.event", "pre-push".to_owned()),
		("hook.jocasta.command", words.join(" ")),
		("hook.jocasta.enabled", "true".to_owned()),
	];
	let mut out = BTreeMap::new();
	out.insert("GIT_CONFIG_COUNT".to_owned(), (first + entries.len() as i64).to_string());
	for (i, (key, value)) in entries.into_iter().enumerate() {
		out.insert(format!("GIT_CONFIG_KEY_{}", first + i as i64), key.to_owned());
		out.insert(format!("GIT_CONFIG_VALUE_{}", first + i as i64), value);
	}
	out
}

/// What the hook needs from outside.
pub trait PushHookDeps {
	/// The folder the push runs in.
	fn cwd(&self) -> &Path;
	/// The pane's key, when its environment has one.
	fn pane_key(&self) -> Option<&str>;
	/// The branch the remote called `remote` has as its default (`origin/HEAD`), when git knows it.
	fn trunk(&self, remote: &str) -> Option<String>;
	/// Asks the service; an `Err` is why it didn't answer.
	fn ask(&self, request: &PushRequest) -> impl Future<Output = PushReply>;
	/// A line to stderr, for the agent.
	fn say(&self, line: &str);
}

/// The hook's verdict as its exit code (0613): `args` are the remote's name and address. A push that updates no
/// protected branch, or goes to a folder on this disk, passes without asking anything; the hook knows the three fixed
/// names and the remote's default branch, and the service adds the project's own trunk to every push it is sent. No
/// answer from the service refuses.
pub async fn run_push_hook(args: &[String], stdin: &str, deps: &impl PushHookDeps) -> i32 {
	let name = args.first().map_or("", String::as_str);
	let address = args.get(1).map_or(name, String::as_str);
	if is_folder_remote(address) {
		return 0;
	}
	let refs = parse_pushed_refs(stdin);
	let trunk = deps.trunk(name);
	if !refs.iter().any(|pushed| is_protected(&pushed.r#ref, trunk.as_deref())) {
		return 0;
	}
	let request = PushRequest {
		dir: deps.cwd().to_path_buf(),
		remote: address.to_owned(),
		refs,
		pane_key: deps.pane_key().filter(|key| !key.is_empty()).map(str::to_owned),
	};
	match deps.ask(&request).await {
		Err(why) => {
			deps.say(&format!("jocasta: this push to a protected branch is refused: {why}"));
			1
		}
		Ok(answer) if answer.allow => 0,
		Ok(answer) => {
			deps.say(&format!(
				"jocasta: {}",
				answer.reason.as_deref().unwrap_or("this push to a protected branch is refused.")
			));
			1
		}
	}
}

#[cfg(test)]
mod tests {
	use std::cell::RefCell;

	use super::*;

	const SHA: &str = "1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b";

	fn none() -> String {
		"0".repeat(40)
	}

	fn pushed(r#ref: &str, local: &str, remote: &str) -> PushedRef {
		PushedRef { r#ref: r#ref.to_owned(), local: local.to_owned(), remote: remote.to_owned() }
	}

	/// git's stdin for a hook: one line per updated remote ref.
	fn line(remote_ref: &str, local: &str, remote: &str) -> String {
		format!("refs/heads/work {local} {remote_ref} {remote}\n")
	}

	fn new_ref(remote_ref: &str) -> String {
		line(remote_ref, SHA, &none())
	}

	fn strings(items: &[&str]) -> Vec<String> {
		items.iter().map(|item| (*item).to_owned()).collect()
	}

	/// A fake service: `calls` collects what it asked, `said` what it printed.
	struct Fake {
		answer: PushReply,
		trunk: Option<String>,
		calls: RefCell<Vec<PushRequest>>,
		said: RefCell<Vec<String>>,
	}

	impl PushHookDeps for Fake {
		fn cwd(&self) -> &Path {
			Path::new("/work/acme")
		}
		fn pane_key(&self) -> Option<&str> {
			Some("key-1")
		}
		fn trunk(&self, _remote: &str) -> Option<String> {
			self.trunk.clone()
		}
		async fn ask(&self, request: &PushRequest) -> PushReply {
			self.calls.borrow_mut().push(request.clone());
			self.answer.clone()
		}
		fn say(&self, line: &str) {
			self.said.borrow_mut().push(line.to_owned());
		}
	}

	struct Ran {
		code: i32,
		calls: Vec<PushRequest>,
		said: Vec<String>,
	}

	/// Runs the hook with a fake service.
	fn hook(args: &[&str], stdin: &str, answer: PushReply, trunk: Option<&str>) -> Ran {
		let fake = Fake {
			answer,
			trunk: trunk.map(str::to_owned),
			calls: RefCell::new(Vec::new()),
			said: RefCell::new(Vec::new()),
		};
		let code = futures_util::FutureExt::now_or_never(run_push_hook(&strings(args), stdin, &fake))
			.expect("the fake never waits");
		Ran { code, calls: fake.calls.into_inner(), said: fake.said.into_inner() }
	}

	fn allow() -> PushReply {
		Ok(PushAnswer { allow: true, reason: None })
	}

	#[test]
	fn gits_stdin_lines_become_the_updated_refs_blank_lines_skipped() {
		let stdin = format!("refs/heads/work {SHA} refs/heads/main {}\n\nHEAD {SHA} refs/heads/dev {SHA}\n", none());
		assert_eq!(
			parse_pushed_refs(&stdin),
			vec![pushed("refs/heads/main", SHA, &none()), pushed("refs/heads/dev", SHA, SHA)]
		);
		assert_eq!(parse_pushed_refs("a b c d\r\n").len(), 1);
	}

	#[test]
	fn the_zero_commit_is_no_commit() {
		assert!(NO_COMMIT.is_match(&none()));
		assert!(!NO_COMMIT.is_match(SHA));
		assert!(!NO_COMMIT.is_match(""));
	}

	#[test]
	fn a_network_address_is_each_of_https_http_ssh_git_user_at_host_path_and_host_path_a_path_or_file_is_a_folder() {
		for network in [
			"https://example.org/acme/app.git",
			"http://example.org/acme/app.git",
			"ssh://git@example.org/acme/app.git",
			"git://example.org/acme/app.git",
			"git@example.org:acme/app.git",
			"example.org:acme/app.git",
		] {
			assert_eq!((network, is_folder_remote(network)), (network, false));
		}
		for folder in [
			"/srv/git/app.git",
			"../origin.git",
			"./origin.git",
			"origin.git",
			"C:\\srv\\app.git",
			"C:/srv/app.git",
			"\\\\server\\share\\app.git",
			"file:///srv/git/app.git",
			"file://C:/srv/app.git",
		] {
			assert_eq!((folder, is_folder_remote(folder)), (folder, true));
		}
	}

	#[test]
	fn main_master_develop_and_the_projects_own_trunk_are_protected_feature_branches_and_tags_are_not() {
		for branch in ["main", "master", "develop"] {
			assert!(is_protected(&format!("refs/heads/{branch}"), None));
		}
		assert!(!is_protected("refs/heads/trunk", None));
		assert!(is_protected("refs/heads/trunk", Some("trunk")));
		for other in [
			"refs/heads/feature/main",
			"refs/heads/maintenance",
			"refs/heads/develop2",
			"refs/tags/main",
			"refs/heads/Main",
		] {
			assert_eq!((other, is_protected(other, None)), (other, false));
		}
	}

	#[test]
	fn a_push_that_updates_no_protected_branch_passes_without_asking_anything_even_with_the_service_down() {
		let down = || Err("Jocasta isn't running".to_owned());
		let result =
			hook(&["origin", "https://example.org/acme/app.git"], &new_ref("refs/heads/feature/x"), down(), None);
		assert_eq!((result.code, result.calls, result.said), (0, vec![], Vec::<String>::new()));
		let tag = hook(&["origin", "https://example.org/a.git"], &new_ref("refs/tags/main"), down(), None);
		assert!(tag.calls.is_empty());
	}

	#[test]
	fn a_push_to_a_folder_remote_passes_without_asking_whatever_branch_it_updates() {
		for address in ["/srv/git/app.git", "file:///srv/git/app.git", "C:\\srv\\app.git"] {
			let result = hook(&["origin", address], &new_ref("refs/heads/main"), Err("down".to_owned()), None);
			assert_eq!((result.code, result.calls, result.said), (0, vec![], Vec::<String>::new()));
		}
	}

	#[test]
	fn a_push_updating_main_master_or_develop_on_a_network_remote_asks_the_service_with_every_updated_ref_and_the_panes_key()
	 {
		for branch in ["main", "master", "develop"] {
			let stdin = new_ref("refs/heads/feature/x") + &line(&format!("refs/heads/{branch}"), SHA, SHA);
			let result = hook(&["origin", "git@example.org:acme/app.git"], &stdin, allow(), None);
			assert_eq!(result.code, 0);
			assert_eq!(
				result.calls,
				vec![PushRequest {
					dir: PathBuf::from("/work/acme"),
					remote: "git@example.org:acme/app.git".to_owned(),
					refs: vec![
						pushed("refs/heads/feature/x", SHA, &none()),
						pushed(&format!("refs/heads/{branch}"), SHA, SHA),
					],
					pane_key: Some("key-1".to_owned()),
				}]
			);
		}
	}

	#[test]
	fn deleting_a_protected_branch_asks_too_the_remotes_own_default_branch_is_protected_under_another_name() {
		let url = ["origin", "https://example.org/a.git"];
		let del = hook(&url, &line("refs/heads/main", &none(), SHA), allow(), None);
		let refs: Vec<_> = del.calls.iter().map(|call| call.refs.clone()).collect();
		assert_eq!(refs, vec![vec![pushed("refs/heads/main", &none(), SHA)]]);
		assert_eq!(hook(&url, &new_ref("refs/heads/stable"), allow(), Some("stable")).calls.len(), 1);
		assert_eq!(hook(&url, &new_ref("refs/heads/stable"), allow(), None).calls.len(), 0);
	}

	#[test]
	fn the_services_refusal_is_printed_for_the_agent_and_exits_1_no_answer_refuses_with_its_reason() {
		let url = ["origin", "https://example.org/a.git"];
		let refused = hook(
			&url,
			&new_ref("refs/heads/main"),
			Ok(PushAnswer { allow: false, reason: Some("acme takes pull requests.".to_owned()) }),
			None,
		);
		assert_eq!((refused.code, refused.said), (1, vec!["jocasta: acme takes pull requests.".to_owned()]));
		let unreasoned = hook(&url, &new_ref("refs/heads/main"), Ok(PushAnswer { allow: false, reason: None }), None);
		assert_eq!(unreasoned.said, vec!["jocasta: this push to a protected branch is refused.".to_owned()]);
		let down = hook(
			&url,
			&new_ref("refs/heads/main"),
			Err("Jocasta isn't running; start it with jocasta.".to_owned()),
			None,
		);
		assert_eq!(down.code, 1);
		assert_eq!(
			down.said,
			vec![
				"jocasta: this push to a protected branch is refused: Jocasta isn't running; start it with jocasta."
					.to_owned()
			]
		);
	}

	fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
		pairs.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect()
	}

	#[test]
	fn the_hooks_environment_holds_the_three_keys_an_existing_git_config_count_keeps_its_entries_and_ours_follow() {
		let empty = BTreeMap::new();
		assert_eq!(
			push_hook_env(&strings(&["C:\\Program Files\\Jocasta\\jocasta.exe"]), Some(&empty)),
			map(&[
				("GIT_CONFIG_COUNT", "3"),
				("GIT_CONFIG_KEY_0", "hook.jocasta.event"),
				("GIT_CONFIG_VALUE_0", "pre-push"),
				("GIT_CONFIG_KEY_1", "hook.jocasta.command"),
				("GIT_CONFIG_VALUE_1", "'C:/Program Files/Jocasta/jocasta.exe' push-hook"),
				("GIT_CONFIG_KEY_2", "hook.jocasta.enabled"),
				("GIT_CONFIG_VALUE_2", "true"),
			])
		);
		let held = map(&[("GIT_CONFIG_COUNT", "2"), ("GIT_CONFIG_KEY_0", "user.name"), ("GIT_CONFIG_VALUE_0", "x")]);
		assert_eq!(
			push_hook_env(&strings(&["bun", "D:/code/jocasta/src/entry.ts"]), Some(&held)),
			map(&[
				("GIT_CONFIG_COUNT", "5"),
				("GIT_CONFIG_KEY_2", "hook.jocasta.event"),
				("GIT_CONFIG_VALUE_2", "pre-push"),
				("GIT_CONFIG_KEY_3", "hook.jocasta.command"),
				("GIT_CONFIG_VALUE_3", "'bun' 'D:/code/jocasta/src/entry.ts' push-hook"),
				("GIT_CONFIG_KEY_4", "hook.jocasta.enabled"),
				("GIT_CONFIG_VALUE_4", "true"),
			])
		);
		let nope = map(&[("GIT_CONFIG_COUNT", "nope")]);
		assert_eq!(push_hook_env(&strings(&["jocasta"]), Some(&nope))["GIT_CONFIG_COUNT"], "3");
	}

	#[test]
	fn a_quote_in_the_command_is_escaped_for_gits_shell_and_a_count_reads_like_parse_int() {
		let env = push_hook_env(&strings(&["it's"]), Some(&BTreeMap::new()));
		assert_eq!(env["GIT_CONFIG_VALUE_1"], "'it'\\''s' push-hook");
		assert_eq!(parse_int("2abc"), Some(2));
		assert_eq!(parse_int("  -3"), Some(-3));
		assert_eq!(parse_int("x"), None);
		let held = map(&[("GIT_CONFIG_COUNT", "-1")]);
		assert_eq!(push_hook_env(&strings(&["j"]), Some(&held))["GIT_CONFIG_COUNT"], "3");
	}

	#[test]
	fn the_request_goes_over_the_wire_with_gits_names() {
		let request = PushRequest {
			dir: PathBuf::from("/work/acme"),
			remote: "origin".to_owned(),
			refs: vec![pushed("refs/heads/main", SHA, &none())],
			pane_key: None,
		};
		assert_eq!(
			serde_json::to_value(&request).unwrap(),
			serde_json::json!({"dir": "/work/acme", "remote": "origin", "refs": [{"ref": "refs/heads/main", "local": SHA, "remote": none()}]})
		);
		let keyed = PushRequest { pane_key: Some("k".to_owned()), ..request };
		assert_eq!(serde_json::to_value(&keyed).unwrap()["paneKey"], "k");
		assert_eq!(
			serde_json::to_value(PushAnswer { allow: true, reason: None }).unwrap(),
			serde_json::json!({"allow": true})
		);
	}
}
