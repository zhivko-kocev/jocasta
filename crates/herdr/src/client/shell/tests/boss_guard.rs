//! 0665: his client may not close, split or add a tab to the boss's workspace; every other workspace keeps all four.
//! The boss's workspace is the one Jocasta's shell extension docks beside.

use super::*;
use crate::api::schema::Method;
use crate::embed::{Reply, ShellExtension};
use crate::input::{KeybindAction, KeybindMatch};
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;

const BOSS: &str = "ws_boss";

struct BossOnly;

impl ShellExtension for BossOnly {
    fn message(&mut self, _kind: &str, _data: &str) -> Reply {
        Reply::default()
    }
    fn docks(&self, workspace_id: &str) -> bool {
        workspace_id == BOSS
    }
    fn draw_dock(&mut self, _buffer: &mut Buffer, _area: Rect, _focused: bool) {}
    fn dock_key(&mut self, _key: KeyEvent) -> Reply {
        Reply::default()
    }
    fn dock_mouse(&mut self, _column: u16, _row: u16) -> Reply {
        Reply::default()
    }
    fn asking(&self) -> bool {
        false
    }
    fn draw_question(&mut self, _buffer: &mut Buffer, _area: Rect) -> Rect {
        Rect::default()
    }
    fn question_key(&mut self, _key: KeyEvent) -> Reply {
        Reply::default()
    }
    fn question_paste(&mut self, _text: &str) -> Reply {
        Reply::default()
    }
    fn tick(&mut self) -> bool {
        false
    }
    fn status(&self) -> Option<String> {
        None
    }
}

/// A state focused on workspace `workspace_id`, the boss's when it is [`BOSS`].
fn state_on(workspace_id: &str) -> ClientShellState {
    crate::embed::set_shell_extension(Box::new(BossOnly));
    let mut projected = snapshot();
    projected.focused_workspace_id = Some(workspace_id.into());
    projected.workspaces[0].workspace_id = workspace_id.into();
    for tab in &mut projected.tabs {
        tab.workspace_id = workspace_id.into();
    }
    for pane in &mut projected.panes {
        pane.workspace_id = workspace_id.into();
    }
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.config.confirm_close = false;
    state.config.prompt_new_tab_name = false;
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.compose(106, 24).unwrap();
    state
}

fn endpoint_methods(outcome: &ClientShellInput) -> Vec<&Method> {
    outcome
        .actions
        .iter()
        .filter_map(|action| match action {
            ClientShellAction::Endpoint { request, .. } => Some(&request.method),
            _ => None,
        })
        .collect()
}

fn press(state: &mut ClientShellState, action: KeybindAction) -> ClientShellInput {
    let mut outcome = ClientShellInput::default();
    state.record_binding(KeybindMatch::Action(action), &mut outcome);
    outcome
}

fn guarded(method: &Method) -> bool {
    matches!(
        method,
        Method::WorkspaceClose(_)
            | Method::TabCreate(_)
            | Method::TabClose(_)
            | Method::PaneSplit(_)
            | Method::PaneClose(_)
    )
}

const REFUSED: [KeybindAction; 6] = [
    KeybindAction::CloseWorkspace,
    KeybindAction::NewTab,
    KeybindAction::CloseTab,
    KeybindAction::SplitVertical,
    KeybindAction::SplitHorizontal,
    KeybindAction::ClosePane,
];

#[test]
fn the_boss_workspace_refuses_close_split_and_new_tab_from_his_client() {
    for action in REFUSED {
        let mut state = state_on(BOSS);
        let outcome = press(&mut state, action);
        assert!(
            endpoint_methods(&outcome).iter().all(|m| !guarded(m)),
            "{action:?} reached the server on the boss's workspace"
        );
        assert!(outcome.requests.is_empty(), "{action:?}");
        assert!(
            state
                .visible_endpoint_notice
                .as_ref()
                .is_some_and(|notice| notice.key.code == "boss_workspace"),
            "{action:?} gave no notice"
        );
        assert!(state.overlay.is_none(), "{action:?} opened a prompt");
    }
}

#[test]
fn another_workspace_keeps_close_split_and_new_tab() {
    for action in REFUSED {
        let mut state = state_on("ws_other");
        let outcome = press(&mut state, action);
        assert!(
            endpoint_methods(&outcome).iter().any(|m| guarded(m)),
            "{action:?} did nothing on an ordinary workspace"
        );
    }
}

#[test]
fn the_boss_workspace_has_no_new_tab_button_and_ordinary_ones_do() {
    let boss = state_on(BOSS);
    assert!(boss.hits.new_tab.is_empty());
    let other = state_on("ws_other");
    assert!(!other.hits.new_tab.is_empty());
}
