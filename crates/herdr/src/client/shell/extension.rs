//! Fork seam (FORK.md): Jocasta's shell extension inside herdr's client shell. Its dock sits beside the panes of one
//! workspace and takes his keys once he clicks it or presses its key. Its question sits over everything and takes every
//! key while it is up. What they send goes to the server as `EndpointControl` messages.

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use super::*;
use crate::embed::Reply;

/// The dock's share of the pane area, in percent.
const DOCK_PERCENT: u32 = 35;
/// The narrowest dock worth drawing, and the least the panes keep beside it.
const DOCK_MIN: u16 = 24;
const PANES_MIN: u16 = 40;

/// Whether `workspace_id` is the boss's workspace, which Jocasta's extension says it docks beside (0665).
pub(super) fn is_boss_workspace(workspace_id: &str) -> bool {
    crate::embed::shell_docks(workspace_id)
}

impl ClientShellState {
    /// `layout` with the dock carved from the right of its pane area, where the focused workspace shows it.
    pub(super) fn with_dock(&self, mut layout: ClientShellLayout) -> ClientShellLayout {
        if !layout.mobile_header.is_empty() {
            return layout;
        }
        let Some(workspace_id) = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_workspace_id.as_deref())
        else {
            return layout;
        };
        if !crate::embed::shell_docks(workspace_id) {
            return layout;
        }
        let surface = layout.pane_surface;
        let width = u16::try_from(u32::from(surface.width) * DOCK_PERCENT / 100).unwrap_or(0);
        if width < DOCK_MIN || surface.width.saturating_sub(width) < PANES_MIN {
            return layout;
        }
        layout.pane_surface.width = surface.width - width;
        layout.dock = Rect::new(
            surface.x + layout.pane_surface.width,
            surface.y,
            width,
            surface.height,
        );
        layout
    }

    /// His key bound to [`crate::embed::DOCK_COMMAND`]: the dock takes his keys, where it shows.
    pub(super) fn focus_dock(&mut self, outcome: &mut ClientShellInput) {
        if !self.hits.dock.is_empty() && !self.dock_focused {
            self.dock_focused = true;
            outcome.repaint = true;
        }
    }

    /// 0665: his client may not close, split or add a tab to the boss's workspace (the one the dock shows beside);
    /// says so in a short notice and returns true when `workspace_id` is it. Jocasta's own calls don't pass here.
    pub(super) fn refuse_on_boss(
        &mut self,
        workspace_id: &str,
        outcome: &mut ClientShellInput,
    ) -> bool {
        if !is_boss_workspace(workspace_id) {
            return false;
        }
        outcome.repaint |= self.push_endpoint_notice(
            ClientEndpointNoticeKind::Rejected,
            "boss_workspace",
            "Jocasta's workspace",
            "Jocasta owns this workspace: it can't be closed, split or given tabs.",
        );
        true
    }

    /// The workspace `method` would close, split or add a tab to; `None` for any other method.
    pub(super) fn guarded_workspace(&self, method: &crate::api::schema::Method) -> Option<String> {
        use crate::api::schema::Method;
        let snapshot = self.snapshot.as_deref()?;
        let pane_workspace = |pane_id: &str| {
            snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == pane_id)
                .map(|pane| pane.workspace_id.clone())
        };
        match method {
            Method::WorkspaceClose(params) => Some(params.workspace_id.clone()),
            Method::TabCreate(params) => params
                .workspace_id
                .clone()
                .or_else(|| snapshot.focused_workspace_id.clone()),
            Method::PaneSplit(params) => params
                .workspace_id
                .clone()
                .or_else(|| params.target_pane_id.as_deref().and_then(pane_workspace)),
            Method::PaneClose(target) => pane_workspace(&target.pane_id),
            Method::TabClose(target) => snapshot
                .tabs
                .iter()
                .find(|tab| tab.tab_id == target.tab_id)
                .map(|tab| tab.workspace_id.clone()),
            _ => None,
        }
    }

    fn apply_reply(&mut self, reply: Reply, outcome: &mut ClientShellInput) {
        outcome.repaint |= reply.redraw;
        if (reply.release || reply.pass) && self.dock_focused {
            self.dock_focused = false;
            outcome.repaint = true;
        }
        for (kind, data) in reply.send {
            outcome
                .requests
                .push(ClientMessage::EndpointControl { kind, data });
        }
    }

    /// A key press for the question, while it is up, or for the dock, while it has his keys; false for one that goes on
    /// as if neither were there.
    pub(super) fn extension_takes_key(
        &mut self,
        key: &crate::input::TerminalKey,
        outcome: &mut ClientShellInput,
    ) -> bool {
        let event = KeyEvent::new(key.code, key.modifiers);
        if crate::embed::shell_asking() {
            let reply = crate::embed::shell_question_key(event);
            self.apply_reply(reply, outcome);
            return true;
        }
        if !self.dock_focused {
            return false;
        }
        let reply = crate::embed::shell_dock_key(event);
        let pass = reply.pass;
        self.apply_reply(reply, outcome);
        !pass
    }

    /// Typed or pasted text for the question, or typed text for the dock, as `extension_takes_key` takes keys.
    pub(super) fn extension_takes_text(
        &mut self,
        text: &str,
        paste: bool,
        outcome: &mut ClientShellInput,
    ) -> bool {
        if crate::embed::shell_asking() {
            if paste {
                let reply = crate::embed::shell_question_paste(text);
                self.apply_reply(reply, outcome);
            } else {
                for c in text.chars() {
                    let event =
                        KeyEvent::new(KeyCode::Char(c), crossterm::event::KeyModifiers::NONE);
                    let reply = crate::embed::shell_question_key(event);
                    self.apply_reply(reply, outcome);
                }
            }
            return true;
        }
        if !self.dock_focused || paste {
            if paste {
                self.dock_focused = false;
            }
            return false;
        }
        for c in text.chars() {
            let event = KeyEvent::new(KeyCode::Char(c), crossterm::event::KeyModifiers::NONE);
            let reply = crate::embed::shell_dock_key(event);
            let pass = reply.pass;
            self.apply_reply(reply, outcome);
            if pass {
                return false;
            }
        }
        true
    }

    /// A click in the dock gives it his keys and acts on the row under it (0660), and one elsewhere takes them back;
    /// the dock's own events stop there.
    pub(super) fn extension_takes_mouse(
        &mut self,
        mouse: MouseEvent,
        outcome: &mut ClientShellInput,
    ) -> bool {
        let in_dock = contains(self.hits.dock, (mouse.column, mouse.row));
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            if in_dock != self.dock_focused {
                self.dock_focused = in_dock;
                outcome.repaint = true;
            }
            if in_dock {
                let reply = crate::embed::shell_dock_mouse(mouse.column, mouse.row);
                self.apply_reply(reply, outcome);
            }
        } else if in_dock {
            let step = match mouse.kind {
                MouseEventKind::ScrollDown => Some(1),
                MouseEventKind::ScrollUp => Some(-1),
                _ => None,
            };
            if let Some(step) = step {
                let reply = crate::embed::shell_dock_scroll(mouse.column, mouse.row, step);
                self.apply_reply(reply, outcome);
            }
        }
        in_dock
    }

    /// A message from the server's extension: whether the frame needs drawing again, and the messages that go back.
    pub(crate) fn extension_message(
        &mut self,
        kind: &str,
        data: &str,
    ) -> (bool, Vec<ClientMessage>) {
        let reply = crate::embed::shell_message(kind, data);
        let back = reply
            .send
            .into_iter()
            .map(|(kind, data)| ClientMessage::EndpointControl { kind, data })
            .collect();
        (reply.redraw, back)
    }

    /// Whether the extension's drawing changed with time.
    pub(crate) fn tick_extension(&mut self) -> bool {
        crate::embed::shell_tick()
    }
}
