//! Ported from the H5 cases of `test/bench/scripts.test.ts`.

use super::*;

fn workspace(id: &str, label: &str) -> WorkspaceInfo {
	WorkspaceInfo { id: id.to_owned(), label: label.to_owned(), cwd: None }
}

fn pane(pane_id: &str, workspace_id: &str, label: Option<&str>, agent: Option<&str>) -> PaneInfo {
	PaneInfo {
		pane_id: pane_id.to_owned(),
		workspace_id: workspace_id.to_owned(),
		label: label.map(str::to_owned),
		agent: agent.map(str::to_owned),
		..PaneInfo::default()
	}
}

fn old_row(service_ms: Option<Option<f64>>, companion_ms: Option<f64>, ready_ms: f64) -> H5Row {
	H5Row {
		tool: "jocasta".to_owned(),
		mode: "cold",
		run: 0,
		ready_ms: Some(ready_ms),
		boss_idle_ms: None,
		service_ms,
		companion_ms,
	}
}

#[test]
fn cold_then_warm_per_run() {
	assert_eq!(schedule(2), vec![("cold", 0), ("warm", 0), ("cold", 1), ("warm", 1)]);
}

#[test]
fn jocastas_ready_point_is_the_boss_pane_in_workspace_jocasta_old_rows_companion_ms_reads_as_service_ms() {
	let workspaces = [workspace("w1", "Jocasta"), workspace("w2", "acme")];
	let panes = [pane("w2:p1", "w2", None, Some("omp")), pane("w1:p1", "w1", Some("boss"), Some("omp"))];
	assert_eq!(boss_pane(&workspaces, &panes).map(|p| p.pane_id.as_str()), Some("w1:p1"));
	assert!(boss_pane(&[workspace("w2", "acme")], &panes).is_none());
	assert_eq!(row_service_ms(&old_row(None, Some(1450.0), 8123.0)), Some(1450.0));
	assert_eq!(row_service_ms(&old_row(Some(Some(1460.0)), None, 8410.0)), Some(1460.0));
}
