use std::io;

use crate::api::client::ApiClient;

pub(super) fn api_client() -> io::Result<ApiClient> {
    Ok(ApiClient::local())
}

pub(super) fn server_status(
    client: &ApiClient,
) -> Result<crate::api::RuntimeStatus, crate::api::client::ApiClientError> {
    client.status()
}

pub(super) fn restart_guidance() -> String {
    crate::session::active_restart_after_update_guidance()
}

pub(super) fn socket_label() -> String {
    crate::api::socket_path().display().to_string()
}

pub(super) fn caller_pane_id() -> Option<String> {
    std::env::var("HERDR_PANE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
}
