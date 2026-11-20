use serde::Deserialize;

use super::error::{Result, UpdaterError};

/// The parts of GitHub's `releases/latest` payload the dashboard uses.
#[derive(Debug, Deserialize)]
pub struct GitHubRelease {
    tag_name: String,
}

impl GitHubRelease {
    /// `v1.2.3` without its tag prefix.
    pub fn version(&self) -> String {
        self.tag_name.trim_start_matches('v').to_string()
    }
}

pub fn fetch_latest_release(api_url: &str) -> Result<GitHubRelease> {
    log::info!("Fetching latest release from {api_url}");

    let response = ureq::get(api_url)
        .set("User-Agent", "cleo5-dashboard/1.0")
        .call()
        .map_err(|err| UpdaterError::Http(err.to_string()))?;

    let release: GitHubRelease = response.into_json()?;

    log::info!("Latest release: {}", release.tag_name);
    Ok(release)
}
