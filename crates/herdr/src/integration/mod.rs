mod actions;
mod env;
mod registry;
mod targets;
mod types;

pub(crate) use actions::{install_target, uninstall_target};
#[cfg(test)]
pub(crate) use env::integration_env_lock;
pub(crate) use env::{
    apply_pane_base_env, HERDR_PANE_ID_ENV_VAR, HERDR_TAB_ID_ENV_VAR, HERDR_WORKSPACE_ID_ENV_VAR,
};
pub(crate) use registry::{
    installed_integration_statuses, integration_recommendations, integration_target_label,
    integration_target_supported, print_outdated_update_notice,
};
pub(crate) use types::{IntegrationRecommendation, IntegrationStatus, IntegrationStatusKind};

const PI_EXTENSION_INSTALL_NAME: &str = "herdr-agent-state.ts";
const PI_EXTENSION_ASSET: &str = include_str!("assets/pi/herdr-agent-state.ts");
const PI_INTEGRATION_VERSION: u32 = 9;
pub(crate) const OMP_EXTENSION_INSTALL_NAME: &str = "herdr-omp-agent-state.ts";
pub(crate) const OMP_EXTENSION_ASSET: &str = include_str!("assets/omp/herdr-agent-state.ts");
const OMP_INTEGRATION_VERSION: u32 = 10;
const INTEGRATION_VERSION_MARKER: &str = "HERDR_INTEGRATION_VERSION=";

#[cfg(test)]
mod tests;
