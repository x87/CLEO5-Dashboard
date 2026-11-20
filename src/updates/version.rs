use version_compare::{compare, Cmp};

use super::error::{Result, UpdaterError};

/// True when `latest` is strictly newer than `current`. Versions that cannot be ordered are
/// never treated as an update.
pub fn is_newer(current: &str, latest: &str) -> Result<bool> {
    let ordering = compare(current, latest).map_err(|_| {
        UpdaterError::VersionCompare(format!("cannot order {current} against {latest}"))
    })?;

    Ok(ordering == Cmp::Lt)
}
