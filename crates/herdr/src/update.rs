//! Version parsing and the update command the wire snapshot still carries.
//!
//! The self-updater, the version check and the install-manager detection are not part of
//! this fork. Nothing in it reaches the network or replaces the binary.

pub(crate) const HERDR_UPDATE_COMMAND: &str = "herdr update";

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
