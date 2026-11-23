//! 0666: the global menu leaves through "quit Jocasta" only, which runs the `jocasta.quit` command as its key does.

use super::*;
use crate::api::schema::Method;
use crate::client::shell::global_menu::GLOBAL_MENU_ITEMS;

#[test]
fn the_menu_has_quit_jocasta_and_no_detach() {
    let labels = GLOBAL_MENU_ITEMS
        .iter()
        .map(|(label, _)| *label)
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        ["settings", "keybinds", "reload config", "quit Jocasta"]
    );
}

#[test]
fn quit_jocasta_invokes_the_quit_command() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let binding = crate::config::CustomCommandKeybind {
        bindings: crate::config::ActionKeybinds::prefix("alt+q"),
        label: "prefix+alt+q".into(),
        command: crate::embed::QUIT_COMMAND.into(),
        action: crate::config::CustomCommandAction::PluginAction,
        description: Some("Quit Jocasta".into()),
        width: None,
        height: None,
    };
    let mut projection = snapshot();
    projection
        .commands
        .push(crate::protocol::ClientShellCommand {
            command_id: "cmd_quit".into(),
            binding_label: binding.label.clone(),
            binding_labels: binding.bindings.labels(),
            action: crate::protocol::ClientShellCommandAction::PluginAction,
            description: None,
        });
    state.set_snapshot(Box::new(projection));
    state.config.keybinds.keybinds.custom_commands.push(binding);

    let quit = GLOBAL_MENU_ITEMS.len() - 1;
    state.toggle_global_menu();
    let mut outcome = ClientShellInput::default();
    state.activate_global_menu_item(quit, &mut outcome);

    assert!(state.overlay.is_none());
    assert!(outcome.actions.iter().any(|action| matches!(
        action,
        ClientShellAction::Endpoint { request, .. }
            if matches!(&request.method, Method::CommandInvoke(params) if params.command_id == "cmd_quit")
    )));
}
