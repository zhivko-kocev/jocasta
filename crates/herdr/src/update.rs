//! Version parsing and the update instruction the release-notes view prints.
//!
//! The self-updater, the version check and the install-manager detection are not part of
//! this fork. Nothing in it reaches the network or replaces the binary.

pub(crate) const HERDR_UPDATE_COMMAND: &str = "herdr update";
const HOMEBREW_UPDATE_COMMAND: &str = "brew update && brew upgrade herdr";
const MISE_UPDATE_COMMAND: &str = "mise upgrade herdr";
const NIX_UPDATE_COMMAND: &str = "update through Nix";

/// Parsed semver version for comparison.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }

    pub fn current() -> Self {
        Self::parse(crate::build_info::BASE_VERSION).expect("invalid CARGO_PKG_VERSION")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

pub(crate) fn update_install_instruction(install_command: &str) -> String {
    match install_command {
        HERDR_UPDATE_COMMAND => {
            "detach, run `herdr update`, then run Herdr again to reconnect".to_string()
        }
        HOMEBREW_UPDATE_COMMAND => {
            "detach, run `brew update && brew upgrade herdr`, then run Herdr again to reconnect"
                .to_string()
        }
        MISE_UPDATE_COMMAND => {
            "detach, run `mise upgrade herdr`, then run Herdr again to reconnect".to_string()
        }
        NIX_UPDATE_COMMAND => {
            "detach, update through Nix, then run Herdr again to reconnect".to_string()
        }
        command => format!("detach, run `{command}`, then run Herdr again to reconnect"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_install_instruction_distinguishes_install_from_restart() {
        assert_eq!(
            update_install_instruction(HERDR_UPDATE_COMMAND),
            "detach, run `herdr update`, then run Herdr again to reconnect"
        );
        assert_eq!(
            update_install_instruction(HOMEBREW_UPDATE_COMMAND),
            "detach, run `brew update && brew upgrade herdr`, then run Herdr again to reconnect"
        );
        assert_eq!(
            update_install_instruction(MISE_UPDATE_COMMAND),
            "detach, run `mise upgrade herdr`, then run Herdr again to reconnect"
        );
    }
}
