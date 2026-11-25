//! The sandbox Jocasta as the live benchmarks read it, through the product's own pipe (BEN-18, 0683): the shell's panes
//! from the service's `panes` and `pane_text` commands on the bridge, the service's state from `/health` as bare
//! `jocasta` reads it, and the root of Jocasta's process tree.

use serde_json::{Map, Value};

use crate::lib::proctree::ProcessInfo;
use crate::lib::sandbox::{Sandbox, cli_request, require_sandbox};
use jocasta::frontier::server::launch::{ServiceState, service_state};
use jocasta::frontier::server::surface::{AgentState, PaneInfo};
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::service_file::{ServiceFile, read_service_file};

pub struct SandboxJocasta {
	pub sandbox: Sandbox,
}

/// One pane of the shell as the service's `panes` command lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BenchPane {
	pub info: PaneInfo,
	/// The label of the pane's workspace.
	pub workspace: Option<String>,
	/// The pid of the pane's process (omp in Jocasta's panes), `None` when it has none.
	pub root: Option<u32>,
}

/// The `panes` command's JSON text as panes.
pub fn parse_panes(text: &str) -> Result<Vec<BenchPane>> {
	let listed: Value = serde_json::from_str(text).map_err(|error| Error::other(format!("panes: {error}")))?;
	let Some(rows) = listed.as_array() else {
		return Err(Error::other("panes: the service didn't answer a list"));
	};
	let text_of = |row: &Value, key: &str| row.get(key).and_then(Value::as_str).map(str::to_owned);
	Ok(rows
		.iter()
		.map(|row| BenchPane {
			info: PaneInfo {
				pane_id: text_of(row, "pane_id").unwrap_or_default(),
				tab_id: text_of(row, "tab_id").unwrap_or_default(),
				workspace_id: text_of(row, "workspace_id").unwrap_or_default(),
				cwd: text_of(row, "cwd").map(Into::into),
				agent_state: row
					.get("agent_state")
					.and_then(|state| serde_json::from_value::<AgentState>(state.clone()).ok())
					.unwrap_or_default(),
				agent: text_of(row, "agent"),
				session_path: text_of(row, "session_path").map(Into::into),
				label: text_of(row, "label"),
				..PaneInfo::default()
			},
			workspace: text_of(row, "workspace"),
			root: row.get("root").and_then(Value::as_u64).and_then(|root| u32::try_from(root).ok()),
		})
		.collect())
}

/// The sandbox (`require_sandbox`, which refuses anything else) and its Jocasta.
pub fn open_sandbox() -> Result<SandboxJocasta> {
	Ok(SandboxJocasta { sandbox: require_sandbox()? })
}

impl SandboxJocasta {
	/// One `cli` command of the running service, on the bridge with its token; its text, or why not.
	async fn cli(&self, command: &str, args: Map<String, Value>) -> Result<String> {
		let reply = cli_request(command, args).await?;
		if reply.is_error == Some(true) {
			return Err(Error::other(format!("{command}: {}", reply.text)));
		}
		Ok(reply.text)
	}

	/// The shell's panes (`panes`). Fails while no service answers; works again once it is back.
	pub async fn panes(&self) -> Result<Vec<PaneInfo>> {
		Ok(self.bench_panes().await?.into_iter().map(|pane| pane.info).collect())
	}

	/// The shell's panes with their workspaces and process ids.
	pub async fn bench_panes(&self) -> Result<Vec<BenchPane>> {
		parse_panes(&self.cli("panes", Map::new()).await?)
	}

	/// The service's state (`service_state`), the one `file` names (by default the one `<data>/jocasta-service.json`
	/// names).
	pub async fn health(&self, file: Option<&ServiceFile>) -> ServiceState {
		match file {
			Some(file) => service_state(Some(file), None).await,
			None => service_state(read_service_file().as_ref(), None).await,
		}
	}

	/// Jocasta's processes in `processes` (0552): the tree of the one process `jocasta-service.json` names, which holds
	/// the service, the shell and every pane's omp (0683). Empty while no service file exists.
	pub fn process_tree(&self, processes: &[ProcessInfo]) -> Vec<ProcessInfo> {
		read_service_file().map_or_else(Vec::new, |file| crate::lib::proctree::tree_of(file.pid, processes))
	}
}
