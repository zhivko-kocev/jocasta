// Herdr's remote client is dropped from this fork. The client still reads these three
// environment variables, which only a remote launch ever set, so they stay named here.
pub(crate) const REMOTE_PREFERENCES_ENV_VAR: &str = "HERDR_REMOTE_PREFERENCES_IDENTITY";
pub(crate) const REATTACH_COMMAND_ENV_VAR: &str = "HERDR_REATTACH_COMMAND";
pub(crate) const REMOTE_KEYBINDINGS_ENV_VAR: &str = "HERDR_REMOTE_KEYBINDINGS";
