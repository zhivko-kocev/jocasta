use std::io;

use super::registry::{integration_target_label, integration_target_supported};
use super::targets::{install_omp, install_pi, uninstall_omp, uninstall_pi};
use super::PI_EXTENSION_INSTALL_NAME;

fn unsupported(target: crate::api::schema::IntegrationTarget, action: &str) -> io::Error {
    io::Error::other(format!(
        "{} integration {action} is not supported in this build",
        integration_target_label(target)
    ))
}

pub(crate) fn install_target(
    target: crate::api::schema::IntegrationTarget,
) -> io::Result<Vec<String>> {
    let result = install_target_inner(target);
    let outcome = if result.is_ok() { "ok" } else { "error" };
    crate::logging::integration_action("install", integration_target_label(target), outcome);
    result
}

fn install_target_inner(target: crate::api::schema::IntegrationTarget) -> io::Result<Vec<String>> {
    if !integration_target_supported(target) {
        return Err(unsupported(target, "install"));
    }

    match target {
        crate::api::schema::IntegrationTarget::Pi => {
            let path = install_pi()?;
            Ok(vec![format!(
                "installed pi integration to {}",
                path.display()
            )])
        }
        crate::api::schema::IntegrationTarget::Omp => {
            let installed = install_omp()?;
            let mut messages = Vec::new();
            if installed.removed_legacy_pi_extension {
                messages.push(format!(
                    "removed legacy pi integration from omp extension directory at {}",
                    installed
                        .extension_path
                        .with_file_name(PI_EXTENSION_INSTALL_NAME)
                        .display()
                ));
            }
            messages.push(format!(
                "installed omp integration to {}",
                installed.extension_path.display()
            ));
            Ok(messages)
        }
        _ => Err(unsupported(target, "install")),
    }
}

pub(crate) fn uninstall_target(
    target: crate::api::schema::IntegrationTarget,
) -> io::Result<Vec<String>> {
    if !integration_target_supported(target) {
        return Err(unsupported(target, "uninstall"));
    }

    let messages = match target {
        crate::api::schema::IntegrationTarget::Pi => {
            let result = uninstall_pi()?;
            if result.removed_extension {
                vec![format!(
                    "removed pi integration extension at {}",
                    result.extension_path.display()
                )]
            } else {
                vec![format!(
                    "no pi integration extension found at {}",
                    result.extension_path.display()
                )]
            }
        }
        crate::api::schema::IntegrationTarget::Omp => {
            let result = uninstall_omp()?;
            if result.removed_extension {
                vec![format!(
                    "removed omp integration extension at {}",
                    result.extension_path.display()
                )]
            } else {
                vec![format!(
                    "no omp integration extension found at {}",
                    result.extension_path.display()
                )]
            }
        }
        _ => return Err(unsupported(target, "uninstall")),
    };

    crate::logging::integration_action("uninstall", integration_target_label(target), "ok");
    Ok(messages)
}
