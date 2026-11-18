//! The sandbox Jocasta as the live benchmarks read it, through the product's own helpers (BEN-18): herdr's panes from
//! the service's client, the service's state from `/health` as bare `jocasta` reads it, and the roots of Jocasta's
//! process trees.

use std::sync::Arc;

use tokio::sync::OnceCell;

use crate::lib::proctree::{JocastaRoots, ProcessInfo, surface_pids};
use crate::lib::sandbox::{Sandbox, require_sandbox};
use jocasta::frontier::server::herdr::{Herdr, HerdrOptions, connect_herdr};
use jocasta::frontier::server::launch::{ServiceState, service_state};
use jocasta::shared::errors::Result;
use jocasta::shared::service_file::{ServiceFile, read_service_file};

/// How long one `pane.process_info` read may take: what the bench's own reads waited before BEN-18.
const PANE_ROOT_MS: i64 = 10_000;

pub struct SandboxJocasta {
	pub sandbox: Sandbox,
	herdr_timeout_ms: i64,
	client: OnceCell<Arc<dyn Herdr>>,
}

/// herdr's server among `processes`: the parent of the first of `pane_roots` (the processes herdr started for its
/// panes, `Herdr::pane_root`) that is running; `None` when none is.
pub fn herdr_server_of(pane_roots: impl IntoIterator<Item = u32>, processes: &[ProcessInfo]) -> Option<u32> {
	pane_roots.into_iter().find_map(|root| {
		processes.iter().find(|info| info.pid == root).map(|info| info.ppid).filter(|server| *server != 0)
	})
}

/// The sandbox (`require_sandbox`, which refuses anything else) and its Jocasta; `herdr_timeout_ms` bounds each request
/// of its herdr client.
pub fn open_sandbox(herdr_timeout_ms: Option<i64>) -> Result<SandboxJocasta> {
	Ok(SandboxJocasta {
		sandbox: require_sandbox()?,
		herdr_timeout_ms: herdr_timeout_ms.unwrap_or(PANE_ROOT_MS),
		client: OnceCell::new(),
	})
}

impl SandboxJocasta {
	/// The product's client for the sandbox herdr session, connected on the first call that finds its server answering
	/// and kept from then on. It never starts a server (`start: false`, so no request revives one either): the service
	/// does, and H3's herdr fault depends on nobody else doing it. Fails while no client is connected and the server
	/// doesn't answer; a connected client's requests fail while the server is down and work again once it is back.
	pub async fn herdr(&self) -> Result<Arc<dyn Herdr>> {
		// A failed connect is tried again on the next call, once the server may answer.
		let client = self
			.client
			.get_or_try_init(|| async {
				connect_herdr(HerdrOptions {
					session: self.sandbox.herdr_session.clone(),
					start: Some(false),
					timeout_ms: Some(self.herdr_timeout_ms),
					..HerdrOptions::default()
				})
				.await
			})
			.await?;
		Ok(client.clone())
	}

	/// The service's state (`service_state`), the one `file` names (by default the one `<data>/jocasta-service.json`
	/// names).
	pub async fn health(&self, file: Option<&ServiceFile>) -> ServiceState {
		match file {
			Some(file) => service_state(Some(file), None).await,
			None => service_state(read_service_file().as_ref(), None).await,
		}
	}

	/// herdr's server by the first pane whose process is running: one pane at a time, so the lookup reads no more panes
	/// than it needs.
	async fn herdr_server(&self, processes: &[ProcessInfo]) -> Result<Option<u32>> {
		let herdr = self.herdr().await?;
		for pane in herdr.panes().await? {
			let root = herdr.pane_root(&pane.pane_id, Some(PANE_ROOT_MS)).await.ok().flatten();
			if let Some(server) = root.and_then(|root| herdr_server_of([root], processes)) {
				return Ok(Some(server));
			}
		}
		Ok(None)
	}

	/// The roots of Jocasta's process trees in `processes` (0552): the service `jocasta-service.json` names, herdr's
	/// server (`herdr_server_of` the given pane roots, else of each pane's process as herdr lists them) and the attached
	/// surfaces.
	pub async fn processes(&self, processes: &[ProcessInfo], pane_roots: Option<&[u32]>) -> Result<JocastaRoots> {
		Ok(JocastaRoots {
			service: read_service_file().map(|file| file.pid),
			herdr_server: match pane_roots {
				Some(roots) => herdr_server_of(roots.iter().copied(), processes),
				None => self.herdr_server(processes).await?,
			},
			surfaces: surface_pids(std::path::Path::new(&self.sandbox.data_dir)),
		})
	}
}
