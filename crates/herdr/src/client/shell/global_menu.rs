use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClientGlobalMenuAction {
    Binding(crate::input::KeybindAction),
    /// Jocasta's `jocasta.quit`, run as its key (Ctrl+B Alt+Q) runs it (0666).
    Quit,
}

/// The menu's items (0666): herdr's own items that stay, then "quit Jocasta" (the only way out).
pub(super) const GLOBAL_MENU_ITEMS: &[(&str, ClientGlobalMenuAction)] = &[
    (
        "settings",
        ClientGlobalMenuAction::Binding(crate::input::KeybindAction::Settings),
    ),
    (
        "keybinds",
        ClientGlobalMenuAction::Binding(crate::input::KeybindAction::Help),
    ),
    (
        "reload config",
        ClientGlobalMenuAction::Binding(crate::input::KeybindAction::ReloadConfig),
    ),
    ("quit Jocasta", ClientGlobalMenuAction::Quit),
];

impl ClientShellState {
    pub(super) fn toggle_global_menu(&mut self) {
        if matches!(self.overlay, Some(ClientShellOverlay::GlobalMenu(_))) {
            self.overlay = None;
        } else {
            self.overlay = Some(ClientShellOverlay::GlobalMenu(ClientGlobalMenuOverlay {
                highlighted: 0,
            }));
        }
    }

    pub(super) fn move_global_menu_selection(&mut self, delta: isize) {
        let Some(ClientShellOverlay::GlobalMenu(menu)) = self.overlay.as_mut() else {
            return;
        };
        menu.highlighted = (menu.highlighted as isize + delta)
            .clamp(0, GLOBAL_MENU_ITEMS.len().saturating_sub(1) as isize)
            as usize;
    }

    pub(super) fn activate_global_menu_item(
        &mut self,
        index: usize,
        outcome: &mut ClientShellInput,
    ) {
        let Some((_, action)) = GLOBAL_MENU_ITEMS.get(index) else {
            return;
        };
        self.overlay = None;
        match action {
            ClientGlobalMenuAction::Binding(binding) => {
                self.record_binding(crate::input::KeybindMatch::Action(*binding), outcome)
            }
            ClientGlobalMenuAction::Quit => self.quit_jocasta(outcome),
        }
        outcome.repaint = true;
    }

    /// Runs the key bound to [`crate::embed::QUIT_COMMAND`], as pressing it would.
    fn quit_jocasta(&mut self, outcome: &mut ClientShellInput) {
        let command = self
            .config
            .keybinds
            .keybinds
            .custom_commands
            .iter()
            .find(|command| command.command == crate::embed::QUIT_COMMAND);
        match command {
            Some(command) => self.record_binding(
                crate::input::KeybindMatch::Command(command.clone()),
                outcome,
            ),
            None => self.set_endpoint_error(
                "Jocasta's quit key is missing from herdr.toml; run jocasta setup",
            ),
        }
    }
}
