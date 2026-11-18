use super::env::*;
use super::registry::*;
use super::targets::*;
use super::*;

use std::fs;
use std::path::PathBuf;

fn clear_integration_path_env() {
    std::env::remove_var(PI_CODING_AGENT_DIR_ENV_VAR);
    std::env::remove_var(OMP_CONFIG_DIR_ENV_VAR);
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_STATE_HOME");
    #[cfg(windows)]
    std::env::remove_var("APPDATA");
}

fn unique_base() -> PathBuf {
    clear_integration_path_env();
    std::env::temp_dir().join(format!(
        "herdr-integration-install-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn only_pi_and_omp_integrations_are_supported() {
    use crate::api::schema::IntegrationTarget;

    for target in IntegrationTarget::ALL {
        let expected = matches!(target, IntegrationTarget::Pi | IntegrationTarget::Omp);
        assert_eq!(integration_target_supported(target), expected);
    }

    let err = install_target(IntegrationTarget::Claude).unwrap_err();
    assert!(err.to_string().contains("not supported"));
    let err = uninstall_target(IntegrationTarget::Codex).unwrap_err();
    assert!(err.to_string().contains("not supported"));
}

#[cfg(windows)]
#[test]
fn home_dir_uses_userprofile_when_home_is_missing() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let previous_home = std::env::var_os("HOME");
    let previous_userprofile = std::env::var_os("USERPROFILE");
    std::env::remove_var("HOME");
    std::env::set_var("USERPROFILE", &base);

    assert_eq!(home_dir().unwrap(), base);

    if let Some(home) = previous_home {
        std::env::set_var("HOME", home);
    }
    if let Some(userprofile) = previous_userprofile {
        std::env::set_var("USERPROFILE", userprofile);
    } else {
        std::env::remove_var("USERPROFILE");
    }
}

#[test]
#[cfg(unix)]
fn command_available_requires_executable_file_on_path() {
    use std::os::unix::fs::PermissionsExt;

    let _lock = integration_env_lock();
    let base = unique_base();
    let bin = base.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", &bin);

    let command = bin.join("pi");
    fs::write(&command, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&command, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!command_available("pi"));

    fs::set_permissions(&command, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(command_available("pi"));

    if let Some(path) = original_path {
        std::env::set_var("PATH", path);
    } else {
        std::env::remove_var("PATH");
    }
    let _ = fs::remove_dir_all(base);
}

#[test]
#[cfg(windows)]
fn command_available_finds_windows_command_shims_on_path() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let bin = base.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", &bin);

    fs::write(bin.join("pi.cmd"), "@echo off\r\n").unwrap();
    assert!(command_available("pi"));

    fs::write(bin.join("omp.exe"), "").unwrap();
    assert!(command_available("omp"));

    assert!(!command_available("missing-agent"));

    if let Some(path) = original_path {
        std::env::set_var("PATH", path);
    } else {
        std::env::remove_var("PATH");
    }
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_pi_writes_embedded_asset_to_pi_extensions_dir() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join(".pi/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    std::env::set_var("HOME", &home);

    let path = install_pi().unwrap();
    let content = fs::read_to_string(&path).unwrap();

    assert_eq!(path, ext_dir.join(PI_EXTENSION_INSTALL_NAME));
    assert_eq!(content, PI_EXTENSION_ASSET);

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_pi_creates_extensions_dir_when_agent_dir_exists() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let agent_dir = home.join(".pi/agent");
    fs::create_dir_all(&agent_dir).unwrap();
    std::env::set_var("HOME", &home);

    let path = install_pi().unwrap();

    assert_eq!(
        path,
        agent_dir.join("extensions").join(PI_EXTENSION_INSTALL_NAME)
    );
    assert!(path.is_file());

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_pi_uses_pi_coding_agent_dir_env() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let agent_dir = base.join("custom-pi-agent");
    let ext_dir = agent_dir.join("extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    std::env::set_var(PI_CODING_AGENT_DIR_ENV_VAR, &agent_dir);

    let path = install_pi().unwrap();

    assert_eq!(path, ext_dir.join(PI_EXTENSION_INSTALL_NAME));

    clear_integration_path_env();
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_pi_expands_tilde_in_pi_coding_agent_dir_env() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join("custom-pi-agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    std::env::set_var("HOME", &home);
    std::env::set_var(PI_CODING_AGENT_DIR_ENV_VAR, "~/custom-pi-agent");

    let path = install_pi().unwrap();

    assert_eq!(path, ext_dir.join(PI_EXTENSION_INSTALL_NAME));

    std::env::remove_var("HOME");
    clear_integration_path_env();
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_omp_writes_embedded_asset_to_omp_extensions_dir() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join(".omp/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    std::env::set_var("HOME", &home);

    let installed = install_omp().unwrap();
    let content = fs::read_to_string(&installed.extension_path).unwrap();

    assert_eq!(
        installed.extension_path,
        ext_dir.join(OMP_EXTENSION_INSTALL_NAME)
    );
    assert!(!installed.removed_legacy_pi_extension);
    assert_eq!(content, OMP_EXTENSION_ASSET);

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_omp_removes_legacy_pi_integration_from_omp_extensions_dir() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join(".omp/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    let legacy_path = ext_dir.join(PI_EXTENSION_INSTALL_NAME);
    fs::write(&legacy_path, PI_EXTENSION_ASSET).unwrap();
    std::env::set_var("HOME", &home);

    let installed = install_omp().unwrap();

    assert_eq!(
        installed.extension_path,
        ext_dir.join(OMP_EXTENSION_INSTALL_NAME)
    );
    assert!(installed.removed_legacy_pi_extension);
    assert!(!legacy_path.exists());

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_omp_preserves_non_herdr_file_with_pi_install_name() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join(".omp/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    let user_path = ext_dir.join(PI_EXTENSION_INSTALL_NAME);
    fs::write(&user_path, "// user extension\n").unwrap();
    std::env::set_var("HOME", &home);

    let installed = install_omp().unwrap();

    assert_eq!(
        installed.extension_path,
        ext_dir.join(OMP_EXTENSION_INSTALL_NAME)
    );
    assert!(!installed.removed_legacy_pi_extension);
    assert_eq!(
        fs::read_to_string(user_path).unwrap(),
        "// user extension\n"
    );

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_omp_uses_pi_config_dir_env() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join("custom-omp/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    std::env::set_var("HOME", &home);
    std::env::set_var(OMP_CONFIG_DIR_ENV_VAR, "custom-omp");

    let installed = install_omp().unwrap();

    assert_eq!(
        installed.extension_path,
        ext_dir.join(OMP_EXTENSION_INSTALL_NAME)
    );
    assert!(!installed.removed_legacy_pi_extension);

    std::env::remove_var("HOME");
    clear_integration_path_env();
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_omp_refuses_shared_pi_extension_directory() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let agent_dir = base.join("shared-agent");
    let ext_dir = agent_dir.join("extensions");
    let pi_extension = ext_dir.join(PI_EXTENSION_INSTALL_NAME);
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(&pi_extension, PI_EXTENSION_ASSET).unwrap();
    std::env::set_var(PI_CODING_AGENT_DIR_ENV_VAR, &agent_dir);
    std::env::set_var(OMP_CONFIG_DIR_ENV_VAR, "ignored-omp-config");

    let err = install_omp().unwrap_err().to_string();

    assert!(err.contains("Pi and OMP resolve to the same extension directory"));
    assert!(err.contains(&ext_dir.display().to_string()));
    assert!(pi_extension.is_file());
    assert!(!ext_dir.join(OMP_EXTENSION_INSTALL_NAME).exists());

    clear_integration_path_env();
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_omp_creates_extensions_dir_when_agent_dir_exists() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let agent_dir = home.join(".omp/agent");
    let ext_dir = agent_dir.join("extensions");
    fs::create_dir_all(&agent_dir).unwrap();
    std::env::set_var("HOME", &home);

    let installed = install_omp().unwrap();

    assert_eq!(
        installed.extension_path,
        ext_dir.join(OMP_EXTENSION_INSTALL_NAME)
    );
    assert!(ext_dir.is_dir());
    assert!(!installed.removed_legacy_pi_extension);

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn uninstall_omp_removes_embedded_extension_when_present() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join(".omp/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(
        ext_dir.join(OMP_EXTENSION_INSTALL_NAME),
        OMP_EXTENSION_ASSET,
    )
    .unwrap();
    std::env::set_var("HOME", &home);

    let result = uninstall_omp().unwrap();

    assert_eq!(
        result.extension_path,
        ext_dir.join(OMP_EXTENSION_INSTALL_NAME)
    );
    assert!(result.removed_extension);
    assert!(!result.extension_path.exists());

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_omp_errors_when_extension_dir_missing() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    std::env::set_var("HOME", &home);

    let err = install_omp().unwrap_err().to_string();

    assert!(err.contains("omp extension directory not found"));

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn uninstall_pi_removes_embedded_extension_when_present() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join(".pi/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join(PI_EXTENSION_INSTALL_NAME), PI_EXTENSION_ASSET).unwrap();
    std::env::set_var("HOME", &home);

    let result = uninstall_pi().unwrap();

    assert_eq!(
        result.extension_path,
        ext_dir.join(PI_EXTENSION_INSTALL_NAME)
    );
    assert!(result.removed_extension);
    assert!(!result.extension_path.exists());

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn outdated_integrations_treat_missing_version_marker_as_legacy() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join(".pi/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    let extension_path = ext_dir.join(PI_EXTENSION_INSTALL_NAME);
    fs::write(&extension_path, "// installed by herdr\n").unwrap();
    std::env::set_var("HOME", &home);

    let outdated = outdated_installed_integrations();

    assert_eq!(outdated.len(), 1);
    assert_eq!(
        outdated[0].target,
        crate::api::schema::IntegrationTarget::Pi
    );
    assert_eq!(outdated[0].path, extension_path);
    assert_eq!(outdated[0].installed_version, None);
    assert_eq!(outdated[0].expected_version, PI_INTEGRATION_VERSION);

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn outdated_integrations_detect_previous_pi_version() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join(".pi/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    let extension_path = ext_dir.join(PI_EXTENSION_INSTALL_NAME);
    fs::write(
        &extension_path,
        "// HERDR_INTEGRATION_ID=pi\n// HERDR_INTEGRATION_VERSION=4\n",
    )
    .unwrap();
    std::env::set_var("HOME", &home);

    let outdated = outdated_installed_integrations();

    assert_eq!(outdated.len(), 1);
    assert_eq!(
        outdated[0].target,
        crate::api::schema::IntegrationTarget::Pi
    );
    assert_eq!(outdated[0].path, extension_path);
    assert_eq!(outdated[0].installed_version, Some(4));
    assert_eq!(outdated[0].expected_version, PI_INTEGRATION_VERSION);

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn outdated_integrations_detect_previous_omp_version() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join(".omp/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    let extension_path = ext_dir.join(OMP_EXTENSION_INSTALL_NAME);
    fs::write(
        &extension_path,
        "// HERDR_INTEGRATION_ID=omp\n// HERDR_INTEGRATION_VERSION=4\n",
    )
    .unwrap();
    std::env::set_var("HOME", &home);

    let outdated = outdated_installed_integrations();

    assert_eq!(outdated.len(), 1);
    assert_eq!(
        outdated[0].target,
        crate::api::schema::IntegrationTarget::Omp
    );
    assert_eq!(outdated[0].path, extension_path);
    assert_eq!(outdated[0].installed_version, Some(4));
    assert_eq!(outdated[0].expected_version, OMP_INTEGRATION_VERSION);

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn outdated_integrations_accept_current_version_marker() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    let ext_dir = home.join(".pi/agent/extensions");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join(PI_EXTENSION_INSTALL_NAME), PI_EXTENSION_ASSET).unwrap();
    std::env::set_var("HOME", &home);

    assert!(outdated_installed_integrations().is_empty());

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn install_pi_errors_when_extension_dir_missing() {
    let _lock = integration_env_lock();
    let base = unique_base();
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    std::env::set_var("HOME", &home);

    let err = install_pi().unwrap_err().to_string();

    assert!(err.contains("pi extension directory not found"));

    std::env::remove_var("HOME");
    let _ = fs::remove_dir_all(base);
}

#[test]
fn bundled_integration_asset_versions_match_expected_versions() {
    for (name, asset, expected_version) in [
        ("pi", PI_EXTENSION_ASSET, PI_INTEGRATION_VERSION),
        ("omp", OMP_EXTENSION_ASSET, OMP_INTEGRATION_VERSION),
    ] {
        assert_eq!(
            parse_integration_version(asset),
            Some(expected_version),
            "{name} asset version must match its integration version constant"
        );
    }
}

#[test]
fn process_owned_integration_assets_do_not_report_release() {
    for (name, asset) in [("pi", PI_EXTENSION_ASSET), ("omp", OMP_EXTENSION_ASSET)] {
        assert!(
            !asset.contains("pane.release_agent"),
            "{name} process exit should own lifecycle release"
        );
    }
}

#[test]
fn pi_extension_refreshes_session_ref_before_agent_start_state() {
    let agent_start = PI_EXTENSION_ASSET
        .find("pi.on(\"agent_start\", (_event, ctx)")
        .expect("pi extension should receive agent_start context");
    let handler = &PI_EXTENSION_ASSET[agent_start..];
    let update_session = handler
        .find("updateSessionRef(ctx);")
        .expect("pi extension should refresh the active session on agent_start");
    let report_session = handler
        .find("void reportSession();")
        .expect("pi extension should report the refreshed session before state");
    let publish_state = handler
        .find("publishState();")
        .expect("pi extension should publish working state after refreshing session");

    assert!(update_session < report_session);
    assert!(report_session < publish_state);
}

#[test]
fn omp_extension_refreshes_session_ref_before_agent_start_state() {
    let agent_start = OMP_EXTENSION_ASSET
        .find("pi.on(\"agent_start\", (_event, ctx)")
        .expect("omp extension should receive agent_start context");
    let handler = &OMP_EXTENSION_ASSET[agent_start..];
    let update_session = handler
        .find("updateSessionRef(ctx);")
        .expect("omp extension should refresh the active session on agent_start");
    let report_session = handler
        .find("void reportSession();")
        .expect("omp extension should report the refreshed session before state");
    let publish_state = handler
        .find("publishState();")
        .expect("omp extension should publish working state after refreshing session");

    assert!(update_session < report_session);
    assert!(report_session < publish_state);
}

fn omp_handler(event: &str) -> &'static str {
    let start = OMP_EXTENSION_ASSET
        .find(&format!("pi.on(\"{event}\""))
        .unwrap_or_else(|| panic!("omp extension registers {event} handler"));
    let rest = &OMP_EXTENSION_ASSET[start..];
    let end = rest[1..]
        .find("\n\n  pi.")
        .map(|offset| offset + 1)
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn omp_root_activation_requires_ui_context() {
    let activator = OMP_EXTENSION_ASSET
        .find("function activateRootSession(ctx: any, sessionStartSource = \"startup\"): boolean")
        .expect("omp extension should centralize root session activation");
    let helper = &OMP_EXTENSION_ASSET[activator..];
    let non_ui_guard = helper
        .find("ctx?.hasUI !== true")
        .expect("omp extension checks UI context before activating");
    let root_session = helper
        .find("rootSession = true;")
        .expect("omp extension activates root session after UI guard");
    let session_report = helper
        .find("void reportSession(sessionStartSource);")
        .expect("omp extension reports root session");

    assert!(non_ui_guard < root_session);
    assert!(root_session < session_report);
}

#[test]
fn omp_session_start_and_switch_use_root_activation() {
    let session_start = OMP_EXTENSION_ASSET
        .find("pi.on(\"session_start\", (_event, ctx)")
        .expect("omp extension registers session_start handler");
    let session_start_handler = &OMP_EXTENSION_ASSET[session_start..];
    session_start_handler
        .find("if (!activateRootSession(ctx))")
        .expect("omp session_start handler should activate root session");

    let session_switch = OMP_EXTENSION_ASSET
        .find("pi.on(\"session_switch\", (event, ctx)")
        .expect("omp extension registers session_switch handler");
    let session_switch_handler = &OMP_EXTENSION_ASSET[session_switch..];
    session_switch_handler
        .find("if (!activateRootSession(ctx, event?.reason || \"resume\"))")
        .expect("omp session_switch handler should activate root session with switch reason");
}

#[test]
fn omp_session_reports_include_start_source() {
    let report_session = OMP_EXTENSION_ASSET
        .find("function reportSession(sessionStartSource = \"startup\"): Promise<void>")
        .expect("omp extension should label session reports with a lifecycle source");
    let helper = &OMP_EXTENSION_ASSET[report_session..];
    let session_source = helper
        .find("session_start_source: sessionStartSource")
        .expect("omp session reports should include the lifecycle source");
    let session_ref = helper
        .find("...sessionRef")
        .expect("omp session reports should include the native session ref");

    assert!(session_source < session_ref);
}

#[test]
fn omp_socket_requests_are_serialized() {
    let queue = OMP_EXTENSION_ASSET
        .find("let requestQueue = Promise.resolve();")
        .expect("omp extension should keep socket reports ordered");
    let send_request = OMP_EXTENSION_ASSET[queue..]
        .find("function sendRequest(request: unknown): Promise<void>")
        .expect("omp extension should wrap socket sends in an ordered queue");
    let queued_send = OMP_EXTENSION_ASSET[queue + send_request..]
        .find("requestQueue = requestQueue.then(")
        .expect("omp extension should serialize socket requests through the queue");
    let raw_send = OMP_EXTENSION_ASSET[queue + send_request..]
        .find("sendRequestNow(request)")
        .expect("omp extension should enqueue the raw socket send");

    assert!(queued_send < raw_send);
}

#[test]
fn omp_runtime_events_can_activate_root_session_after_resume() {
    for event in [
        "agent_start",
        "tool_approval_requested",
        "tool_approval_resolved",
        "tool_execution_start",
        "tool_execution_end",
    ] {
        let handler = omp_handler(event);
        handler
            .find("!rootSession && !activateRootSession(ctx)")
            .unwrap_or_else(|| panic!("omp {event} handler should recover missing root session"));
    }
}

#[test]
fn omp_ask_and_approval_events_report_blocked_state() {
    let approval_handler = omp_handler("tool_approval_requested");
    approval_handler
        .find("activateBlocked(label);")
        .expect("approval requests should block the pane");

    let approval_resolved = omp_handler("tool_approval_resolved");
    approval_resolved
        .find("deactivateBlocked();")
        .expect("approval resolution should unblock the pane");

    let ask_handler = omp_handler("tool_execution_start");
    ask_handler
        .find("event?.toolName !== \"ask\"")
        .expect("tool execution handler should only treat Ask as blocked");
    ask_handler
        .find("activateBlocked(askBlockedMessage(event.args));")
        .expect("Ask start should block the pane");

    let ask_end_handler = omp_handler("tool_execution_end");
    ask_end_handler
        .find("event?.toolName !== \"ask\"")
        .expect("tool execution end should only treat Ask as blocked");
    ask_end_handler
        .find("deactivateBlocked();")
        .expect("Ask end should unblock the pane");
}
