use crate::error::{Result, UpdaterError};
use version_compare::{compare, Cmp};

/// True when `latest` is newer than `current`.
pub fn should_update(current: &str, latest: &str) -> Result<bool> {
    match compare(current, latest) {
        Ok(Cmp::Lt) | Ok(Cmp::Le) => {
            log::info!("Current version {} is older than latest {}", current, latest);
            Ok(true)
        }
        Ok(Cmp::Eq) => {
            log::info!("Current version {} is up to date", current);
            Ok(false)
        }
        Ok(Cmp::Gt) | Ok(Cmp::Ge) => {
            log::warn!("Current version {} is newer than latest {}?", current, latest);
            Ok(false)
        }
        Ok(Cmp::Ne) => {
            // The strings differ but no order is defined; do not update on a guess.
            log::warn!("Version comparison ambiguous: {} vs {}", current, latest);
            Ok(false)
        }
        Err(_) => Err(UpdaterError::VersionCompare(
            format!("Failed to compare versions: {} vs {}", current, latest)
        ))
    }
}
