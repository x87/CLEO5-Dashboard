use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: String,
    pub assets: Vec<Asset>,
    pub published_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

impl GitHubRelease {
    /// `v1.2.3` becomes `1.2.3`.
    pub fn get_version(&self) -> String {
        self.tag_name.trim_start_matches('v').to_string()
    }
}

pub fn fetch_latest_release(api_url: &str) -> Result<GitHubRelease> {
    log::info!("Fetching latest release from: {}", api_url);

    let response = ureq::get(api_url)
        .set("User-Agent", "cleo5-dashboard/1.0")
        .call()?;

    let release: GitHubRelease = response.into_json()?;

    log::info!(
        "Latest release: {} with {} assets",
        release.tag_name,
        release.assets.len()
    );

    Ok(release)
}
