use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::env::{omp_extension_dir, pi_extension_dir};
use super::types::{OmpInstallPaths, OmpUninstallResult, PiUninstallResult};
use super::{
    OMP_EXTENSION_ASSET, OMP_EXTENSION_INSTALL_NAME, PI_EXTENSION_ASSET, PI_EXTENSION_INSTALL_NAME,
};

fn remove_file_if_exists(path: &Path) -> io::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn ensure_extension_dir(dir: &Path, agent: &str) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    if dir.parent().is_some_and(|parent| parent.is_dir()) {
        return fs::create_dir_all(dir);
    }
    Err(io::Error::other(format!(
        "{agent} extension directory not found at {}. install {agent} first",
        dir.display()
    )))
}

pub(crate) fn install_pi() -> io::Result<PathBuf> {
    let dir = pi_extension_dir()?;
    ensure_extension_dir(&dir, "pi")?;

    let path = dir.join(PI_EXTENSION_INSTALL_NAME);
    fs::write(&path, PI_EXTENSION_ASSET)?;
    Ok(path)
}

pub(crate) fn install_omp() -> io::Result<OmpInstallPaths> {
    let dir = omp_extension_dir()?;
    let pi_dir = pi_extension_dir()?;
    if dir == pi_dir {
        return Err(io::Error::other(format!(
            "Pi and OMP resolve to the same extension directory at {}; configure separate agent directories before installing OMP",
            dir.display()
        )));
    }
    ensure_extension_dir(&dir, "omp")?;

    let removed_legacy_pi_extension = remove_legacy_pi_extension_from_omp_dir(&dir)?;
    let extension_path = dir.join(OMP_EXTENSION_INSTALL_NAME);
    fs::write(&extension_path, OMP_EXTENSION_ASSET)?;
    Ok(OmpInstallPaths {
        extension_path,
        removed_legacy_pi_extension,
    })
}

fn remove_legacy_pi_extension_from_omp_dir(dir: &Path) -> io::Result<bool> {
    let legacy_path = dir.join(PI_EXTENSION_INSTALL_NAME);
    if !legacy_path.is_file() {
        return Ok(false);
    }

    let content = fs::read_to_string(&legacy_path)?;
    if content.contains("HERDR_INTEGRATION_ID=pi") {
        fs::remove_file(legacy_path)?;
        return Ok(true);
    }

    Ok(false)
}

pub(crate) fn uninstall_pi() -> io::Result<PiUninstallResult> {
    let extension_path = pi_extension_dir()?.join(PI_EXTENSION_INSTALL_NAME);
    let removed_extension = remove_file_if_exists(&extension_path)?;

    Ok(PiUninstallResult {
        extension_path,
        removed_extension,
    })
}

pub(crate) fn uninstall_omp() -> io::Result<OmpUninstallResult> {
    let extension_path = omp_extension_dir()?.join(OMP_EXTENSION_INSTALL_NAME);
    let removed_extension = remove_file_if_exists(&extension_path)?;

    Ok(OmpUninstallResult {
        extension_path,
        removed_extension,
    })
}
